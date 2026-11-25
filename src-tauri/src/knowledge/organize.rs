//! K13 physical file organize kernel (`KnowledgeOrganizeManager`).
//!
//! Owns plan validation, precondition fingerprinting, state machine transitions
//! (`pending_review -> approved -> executing -> completed | partially_completed | interrupted`,
//! plus `rejected` and `stale`), checksummed durable journaling (`DurableRecordJournal`),
//! strict workspace execution (under `acquire_edit_mutation`), startup crash recovery,
//! reverse undo with post-execution modification protection, and 7-day plan expiration.
//!
//! Security boundary:
//! - Only `mkdir`, `move`, and `rename` are supported (never delete, never cross-volume).
//! - Approval, execution, and undo are ONLY exposed via user-facing Tauri commands,
//!   NEVER registered in Management API or CLI routes.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::durable_journal::{recover_and_read, DurableRecordJournal};
use crate::knowledge::store::linked::{
    detect_git_work_tree, normalize_linked_path, platform_path_key,
};
use crate::knowledge::store::{KnowledgeStore, StoreResult};
use crate::knowledge::types::{
    create_knowledge_plan_id, is_knowledge_base_id, is_knowledge_plan_id, KnowledgeErrorCode,
};
use crate::utils::file_lock::{with_file_lock_blocking, FileLockOptions};
use crate::workspace_files::acquire_edit_mutation;
use crate::workspace_files::crud::{
    strict_mkdir, strict_move_or_rename, validate_strict_relative_path, StrictFsError,
};
use crate::workspace_files::path_safety::{
    ensure_workspace_directory_no_follow, metadata_is_link_like,
    open_workspace_regular_file_no_follow_classified, read_workspace_file_no_follow,
    resolve_inside_workspace, write_workspace_file_no_follow,
};
use crate::ulog_warn;
use KnowledgeErrorCode::{KbAccessDenied, KbInvalidInput, KbLimitExceeded, KbNotFound, KbStoreUnavailable};

const MAX_PLAN_ITEMS: usize = 500;
const MAX_PRECONDITION_HASH_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PLAN_JSON_BYTES: u64 = 4 * 1024 * 1024;
const JOURNAL_SCHEMA_VERSION: u32 = 1;
const JOURNAL_MAX_LINE_BYTES: usize = 64 * 1024;
const PLAN_EXPIRY_DAYS: i64 = 7;

const CREDENTIAL_FILENAMES: &[&str] = &[
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    "id_dsa",
    "credentials",
    "credentials.json",
    "credentials.yaml",
    "credentials.yml",
    "secrets.json",
    "secrets.yaml",
    "secrets.yml",
    "service-account.json",
    "service_account.json",
    "master.key",
    "private.key",
    "token.json",
    "kubeconfig",
    "shadow",
    "passwd",
];

const CREDENTIAL_EXTENSIONS: &[&str] = &[
    "pem", "key", "p12", "pfx", "jks", "keystore", "kdbx", "crt", "cer", "der", "ovpn", "ppk",
    "netrc", "pgpass", "npmrc", "pypirc", "htpasswd", "env",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizePlanStatus {
    PendingReview,
    Approved,
    Executing,
    Completed,
    PartiallyCompleted,
    Interrupted,
    Rejected,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeOpKind {
    Mkdir,
    Move,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeItemStatus {
    Pending,
    Succeeded,
    Conflict,
    Failed,
    Skipped,
    Undone,
    UndoSkippedModified,
    UndoConflict,
    UndoFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePrecondition {
    pub is_dir: bool,
    pub size: u64,
    pub mtime_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlanItem {
    pub id: String,
    pub kind: OrganizeOpKind,
    pub folder_root: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_rel: Option<String>,
    pub target_rel: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precondition: Option<FilePrecondition>,
    #[serde(default)]
    pub affected_markdown_links: usize,
    #[serde(default = "default_true")]
    pub default_selected: bool,
    #[serde(default)]
    pub approved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_target_rel: Option<String>,
    pub status: OrganizeItemStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_fingerprint: Option<FilePrecondition>,
}

fn default_true() -> bool {
    true
}

impl OrganizePlanItem {
    pub fn effective_target_rel(&self) -> &str {
        self.override_target_rel
            .as_deref()
            .unwrap_or(self.target_rel.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub id: String,
    pub kb_id: String,
    pub status: OrganizePlanStatus,
    pub created_at: String,
    pub expires_at: String,
    pub updated_at: String,
    pub items: Vec<OrganizePlanItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposeItemInput {
    pub kind: OrganizeOpKind,
    pub folder_root: PathBuf,
    pub source_rel: Option<String>,
    pub target_rel: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposePlanInput {
    pub items: Vec<ProposeItemInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemApprovalDecision {
    pub item_id: String,
    pub approved: bool,
    pub override_target_rel: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalDecision {
    #[serde(default)]
    pub reject: bool,
    #[serde(default)]
    pub items: Vec<ItemApprovalDecision>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoReport {
    pub plan_id: String,
    pub undone: Vec<String>,
    pub skipped_modified: Vec<String>,
    pub conflicts: Vec<String>,
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OrganizeJournalEvent {
    Intent {
        op_id: String,
        kind: OrganizeOpKind,
        folder_root: PathBuf,
        source_rel: Option<String>,
        target_rel: String,
        precondition: Option<FilePrecondition>,
    },
    Done {
        op_id: String,
        post_fingerprint: Option<FilePrecondition>,
    },
    Skipped {
        op_id: String,
        status: OrganizeItemStatus,
        reason: String,
    },
    Undone {
        op_id: String,
    },
    UndoSkipped {
        op_id: String,
        status: OrganizeItemStatus,
        reason: String,
    },
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrashPoint {
    IntentRecorded,
    FsMoved,
    StoreRelinked,
    DoneRecorded,
}

struct RecordedIntent {
    kind: OrganizeOpKind,
    folder_root: PathBuf,
    source_rel: Option<String>,
    target_rel: String,
    precondition: Option<FilePrecondition>,
}

fn is_hidden_or_credential_rel(segments: &[String]) -> bool {
    for seg in segments {
        let lower = seg.to_ascii_lowercase();
        if lower.starts_with('.') || CREDENTIAL_FILENAMES.contains(&lower.as_str()) {
            return true;
        }
        if Path::new(&lower)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| CREDENTIAL_EXTENSIONS.contains(&ext))
        {
            return true;
        }
    }
    false
}

fn capture_precondition(folder_root: &Path, rel: &str) -> StoreResult<FilePrecondition> {
    let resolved = resolve_inside_workspace(folder_root, rel).map_err(|_| KbInvalidInput)?;
    let meta = fs::symlink_metadata(&resolved).map_err(|_| KbNotFound)?;
    if metadata_is_link_like(&meta) {
        return Err(KbInvalidInput);
    }
    let mtime_ms = meta
        .modified()
        .ok()
        .map(|t| DateTime::<Utc>::from(t).timestamp_millis())
        .unwrap_or(0);
    if meta.is_dir() {
        return Ok(FilePrecondition {
            is_dir: true,
            size: 0,
            mtime_ms,
            sha256: None,
        });
    }
    if !meta.is_file() {
        return Err(KbInvalidInput);
    }
    let (_, mut file) = open_workspace_regular_file_no_follow_classified(
        folder_root,
        rel,
        "organize precondition",
    )
    .map_err(|err| {
        if err.is_in_use() {
            KnowledgeErrorCode::KbSourceInUse
        } else {
            KbInvalidInput
        }
    })?;
    let fmeta = file.metadata().map_err(|_| KbInvalidInput)?;
    let size = fmeta.len();
    let fmtime_ms = fmeta
        .modified()
        .ok()
        .map(|t| DateTime::<Utc>::from(t).timestamp_millis())
        .unwrap_or(mtime_ms);
    let sha256 = if size <= MAX_PRECONDITION_HASH_BYTES {
        let mut buf = Vec::with_capacity(size.min(65_536) as usize);
        file.by_ref()
            .take(MAX_PRECONDITION_HASH_BYTES.saturating_add(1))
            .read_to_end(&mut buf)
            .map_err(|_| KbStoreUnavailable)?;
        if buf.len() as u64 <= MAX_PRECONDITION_HASH_BYTES {
            Some(format!("{:x}", Sha256::digest(&buf)))
        } else {
            None
        }
    } else {
        None
    };
    Ok(FilePrecondition {
        is_dir: false,
        size,
        mtime_ms: fmtime_ms,
        sha256,
    })
}

fn check_precondition(
    folder_root: &Path,
    rel: &str,
    expected: &FilePrecondition,
) -> Result<bool, KnowledgeErrorCode> {
    let actual = capture_precondition(folder_root, rel)?;
    if actual.is_dir != expected.is_dir {
        return Ok(false);
    }
    if actual.is_dir {
        return Ok(true);
    }
    if actual.size != expected.size {
        return Ok(false);
    }
    Ok(match (&actual.sha256, &expected.sha256) {
        (Some(a), Some(b)) => a == b,
        _ => actual.mtime_ms == expected.mtime_ms,
    })
}

fn precondition_matches(folder_root: &Path, rel: &str, expected: &FilePrecondition) -> bool {
    matches!(check_precondition(folder_root, rel, expected), Ok(true))
}

fn count_relative_markdown_links(folder_root: &Path, rel: &str) -> usize {
    if !rel.to_ascii_lowercase().ends_with(".md")
        && !rel.to_ascii_lowercase().ends_with(".markdown")
    {
        return 0;
    }
    let Ok((_, bytes)) = read_workspace_file_no_follow(folder_root, rel, 1024 * 1024) else {
        return 0;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return 0;
    };
    let mut count = 0usize;
    let mut rest = text;
    while let Some(idx) = rest.find("](") {
        let after = &rest[idx + 2..];
        if let Some(end) = after.find(')') {
            let target = after[..end].trim();
            if !target.is_empty()
                && !target.starts_with("http://")
                && !target.starts_with("https://")
                && !target.starts_with("kb://")
                && !target.starts_with('#')
                && !target.starts_with("mailto:")
            {
                count += 1;
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    count
}

fn ensure_no_link_in_rel(folder_root: &Path, rel: &str) -> StoreResult<()> {
    let resolved = resolve_inside_workspace(folder_root, rel).map_err(|_| KbInvalidInput)?;
    let stripped = resolved
        .strip_prefix(folder_root)
        .map_err(|_| KbInvalidInput)?;
    let mut cursor = folder_root.to_path_buf();
    for comp in stripped.components() {
        cursor.push(comp);
        match fs::symlink_metadata(&cursor) {
            Ok(meta) => {
                if metadata_is_link_like(&meta) {
                    return Err(KbInvalidInput);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return Err(KbStoreUnavailable),
        }
    }
    Ok(())
}

fn is_case_only_rel_rename(src_rel: &str, dst_rel: &str) -> bool {
    let src_p = Path::new(src_rel);
    let dst_p = Path::new(dst_rel);
    if src_p.parent() != dst_p.parent() {
        return false;
    }
    match (
        src_p.file_name().and_then(|s| s.to_str()),
        dst_p.file_name().and_then(|s| s.to_str()),
    ) {
        (Some(a), Some(b)) => a != b && a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

fn strict_error_code(err: &StrictFsError) -> (OrganizeItemStatus, String) {
    match err {
        StrictFsError::Conflict(msg) => (OrganizeItemStatus::Conflict, format!("TARGET_EXISTS: {msg}")),
        StrictFsError::InUse(msg) => (OrganizeItemStatus::Failed, format!("FILE_IN_USE: {msg}")),
        StrictFsError::CrossDevice(msg) => {
            (OrganizeItemStatus::Failed, format!("CROSS_VOLUME: {msg}"))
        }
        StrictFsError::NotFound(msg) => {
            (OrganizeItemStatus::Conflict, format!("SOURCE_MISSING: {msg}"))
        }
        StrictFsError::InvalidPath(msg) => {
            (OrganizeItemStatus::Failed, format!("INVALID_PATH: {msg}"))
        }
        StrictFsError::Io(msg) => (OrganizeItemStatus::Failed, format!("IO_ERROR: {msg}")),
    }
}

pub struct KnowledgeOrganizeManager {
    store: Arc<KnowledgeStore>,
    knowledge_root: PathBuf,
}

impl KnowledgeOrganizeManager {
    pub fn new(store: Arc<KnowledgeStore>, app_data_dir: PathBuf) -> Self {
        Self {
            store,
            knowledge_root: app_data_dir.join("knowledge"),
        }
    }

    fn plan_lock_path(&self, kb_id: &str) -> PathBuf {
        self.knowledge_root
            .join(kb_id)
            .join("organize")
            .join(".organize.lock")
    }

    fn plan_rel_json(kb_id: &str, plan_id: &str) -> String {
        format!("{kb_id}/organize/plans/{plan_id}/plan.json")
    }

    fn journal_abs_path(&self, kb_id: &str, plan_id: &str) -> PathBuf {
        self.knowledge_root
            .join(kb_id)
            .join("organize")
            .join("plans")
            .join(plan_id)
            .join("journal.jsonl")
    }

    fn with_kb_organize_lock<T>(
        &self,
        kb_id: &str,
        op: impl FnOnce() -> StoreResult<T>,
    ) -> StoreResult<T> {
        if !is_knowledge_base_id(kb_id) {
            return Err(KbInvalidInput);
        }
        let _ = ensure_workspace_directory_no_follow(
            &self.knowledge_root,
            &format!("{kb_id}/organize/plans"),
        )
        .map_err(|_| KbStoreUnavailable)?;
        with_file_lock_blocking(
            &self.plan_lock_path(kb_id),
            FileLockOptions::default(),
            || Ok(op()),
        )
        .map_err(|_| KbStoreUnavailable)?
    }

    fn save_plan_locked(&self, plan: &OrganizePlan) -> StoreResult<()> {
        let bytes = serde_json::to_vec_pretty(plan).map_err(|_| KbStoreUnavailable)?;
        if bytes.len() as u64 > MAX_PLAN_JSON_BYTES {
            return Err(KbLimitExceeded);
        }
        write_workspace_file_no_follow(
            &self.knowledge_root,
            &Self::plan_rel_json(&plan.kb_id, &plan.id),
            &bytes,
        )
        .map_err(|_| KbStoreUnavailable)?;
        Ok(())
    }

    fn load_plan_locked(&self, kb_id: &str, plan_id: &str) -> StoreResult<OrganizePlan> {
        if !is_knowledge_base_id(kb_id) || !is_knowledge_plan_id(plan_id) {
            return Err(KbInvalidInput);
        }
        let (_, bytes) = read_workspace_file_no_follow(
            &self.knowledge_root,
            &Self::plan_rel_json(kb_id, plan_id),
            MAX_PLAN_JSON_BYTES,
        )
        .map_err(|_| KbNotFound)?;
        let plan: OrganizePlan = serde_json::from_slice(&bytes).map_err(|_| KbStoreUnavailable)?;
        if plan.id != plan_id || plan.kb_id != kb_id {
            return Err(KbStoreUnavailable);
        }
        Ok(plan)
    }

    fn allocate_plan_id_locked(&self, kb_id: &str) -> StoreResult<String> {
        let date = Utc::now().format("%Y%m%d").to_string();
        let plans_dir = self.knowledge_root.join(kb_id).join("organize").join("plans");
        for _ in 0..32 {
            let raw = uuid::Uuid::new_v4();
            let entropy: [u8; 6] = raw.as_bytes()[..6].try_into().map_err(|_| KbStoreUnavailable)?;
            let id = create_knowledge_plan_id(&date, entropy).ok_or(KbStoreUnavailable)?;
            if !plans_dir.join(&id).exists() {
                return Ok(id);
            }
        }
        Err(KbStoreUnavailable)
    }

    /// Validate and create a new `pending_review` physical organize plan.
    /// Does NOT touch any user file on disk.
    pub fn propose_plan(&self, kb_id: &str, input: ProposePlanInput) -> StoreResult<OrganizePlan> {
        if !is_knowledge_base_id(kb_id) {
            return Err(KbInvalidInput);
        }
        if input.items.is_empty() {
            return Err(KbInvalidInput);
        }
        if input.items.len() > MAX_PLAN_ITEMS {
            return Err(KbLimitExceeded);
        }

        let kb = self.store.get(kb_id)?;
        let authorized_folders: BTreeMap<String, _> = kb
            .linked_folders
            .iter()
            .map(|f| (platform_path_key(&f.path), f.clone()))
            .collect();

        // Track virtual created directories and claimed targets within the same proposal.
        let mut planned_mkdirs: BTreeSet<(String, String)> = BTreeSet::new();
        let mut planned_targets: BTreeSet<(String, String)> = BTreeSet::new();
        let mut items = Vec::with_capacity(input.items.len());

        for (idx, raw_item) in input.items.into_iter().enumerate() {
            if raw_item.reason.trim().is_empty() {
                return Err(KbInvalidInput);
            }
            let root_norm = normalize_linked_path(&raw_item.folder_root);
            let root_key = platform_path_key(&root_norm);
            let linked = authorized_folders
                .get(&root_key)
                .ok_or(KbAccessDenied)?;

            // Hard authorization boundary: folder must have organizeAllowed == true,
            // and if it is a Git work tree (live OR stored), gitConfirmed must be true.
            if !linked.organize_allowed {
                return Err(KbAccessDenied);
            }
            let live_git = linked.is_git_work_tree || detect_git_work_tree(&linked.path);
            if live_git && !linked.git_confirmed {
                return Err(KbAccessDenied);
            }
            let canonical_root = linked.path.clone();

            let dst_segments =
                validate_strict_relative_path(&raw_item.target_rel).map_err(|_| KbInvalidInput)?;
            if is_hidden_or_credential_rel(&dst_segments) {
                return Err(KbInvalidInput);
            }
            let target_rel = dst_segments.join("/");
            let resolved_dst =
                resolve_inside_workspace(&canonical_root, &target_rel).map_err(|_| KbInvalidInput)?;
            if resolved_dst.to_string_lossy().chars().count() > 240 {
                return Err(KbInvalidInput);
            }
            ensure_no_link_in_rel(&canonical_root, &target_rel)?;

            let target_key = (
                root_key.clone(),
                platform_path_key(&resolved_dst),
            );
            if !planned_targets.insert(target_key.clone()) {
                return Err(KbInvalidInput);
            }

            let mut source_rel_norm = None;
            let mut precondition = None;
            let mut affected_markdown_links = 0usize;
            let mut matched_source_id = None;

            match raw_item.kind {
                OrganizeOpKind::Mkdir => {
                    if raw_item.source_rel.is_some() {
                        return Err(KbInvalidInput);
                    }
                    if fs::symlink_metadata(&resolved_dst).is_ok() {
                        return Err(KbInvalidInput);
                    }
                    planned_mkdirs.insert(target_key);
                }
                OrganizeOpKind::Move | OrganizeOpKind::Rename => {
                    let raw_src = raw_item.source_rel.as_deref().ok_or(KbInvalidInput)?;
                    let src_segments =
                        validate_strict_relative_path(raw_src).map_err(|_| KbInvalidInput)?;
                    if is_hidden_or_credential_rel(&src_segments) {
                        return Err(KbInvalidInput);
                    }
                    let src_rel = src_segments.join("/");
                    if src_rel == target_rel {
                        return Err(KbInvalidInput);
                    }
                    let resolved_src = resolve_inside_workspace(&canonical_root, &src_rel)
                        .map_err(|_| KbInvalidInput)?;
                    ensure_no_link_in_rel(&canonical_root, &src_rel)?;
                    if resolved_dst.starts_with(&resolved_src) {
                        return Err(KbInvalidInput);
                    }
                    if raw_item.kind == OrganizeOpKind::Rename
                        && resolved_src.parent() != resolved_dst.parent()
                    {
                        return Err(KbInvalidInput);
                    }
                    let case_only = is_case_only_rel_rename(&src_rel, &target_rel);
                    if !case_only && fs::symlink_metadata(&resolved_dst).is_ok() {
                        return Err(KbInvalidInput);
                    }
                    if let Some(dst_parent) = resolved_dst.parent() {
                        let parent_key = (root_key.clone(), platform_path_key(dst_parent));
                        if !dst_parent.is_dir() && !planned_mkdirs.contains(&parent_key) {
                            return Err(KbInvalidInput);
                        }
                    }

                    let pre = capture_precondition(&canonical_root, &src_rel)?;
                    affected_markdown_links = count_relative_markdown_links(&canonical_root, &src_rel);
                    if let Ok(snaps) = self
                        .store
                        .list_linked_sources_for_folder(kb_id, &canonical_root)
                    {
                        let src_key = platform_path_key(&resolved_src);
                        matched_source_id = snaps
                            .into_iter()
                            .find(|s| platform_path_key(&s.current_path) == src_key)
                            .map(|s| s.source_id);
                    }

                    source_rel_norm = Some(src_rel);
                    precondition = Some(pre);
                }
            }

            let default_selected = affected_markdown_links == 0;
            items.push(OrganizePlanItem {
                id: format!("op_{}", idx + 1),
                kind: raw_item.kind,
                folder_root: canonical_root,
                source_rel: source_rel_norm,
                target_rel,
                reason: raw_item.reason.trim().to_string(),
                source_id: matched_source_id,
                precondition,
                affected_markdown_links,
                default_selected,
                approved: default_selected,
                override_target_rel: None,
                status: OrganizeItemStatus::Pending,
                error: None,
                post_fingerprint: None,
            });
        }

        self.with_kb_organize_lock(kb_id, || {
            let plan_id = self.allocate_plan_id_locked(kb_id)?;
            let now = Utc::now();
            let expires = now + ChronoDuration::days(PLAN_EXPIRY_DAYS);
            let plan = OrganizePlan {
                id: plan_id,
                kb_id: kb_id.to_owned(),
                status: OrganizePlanStatus::PendingReview,
                created_at: now.to_rfc3339(),
                expires_at: expires.to_rfc3339(),
                updated_at: now.to_rfc3339(),
                items,
            };
            self.save_plan_locked(&plan)?;
            Ok(plan)
        })
    }

    fn check_and_mark_stale_locked(&self, plan: &mut OrganizePlan) -> StoreResult<bool> {
        if plan.status != OrganizePlanStatus::PendingReview {
            return Ok(false);
        }
        let now = Utc::now();
        let expired = DateTime::parse_from_rfc3339(&plan.expires_at)
            .map(|dt| now > dt.with_timezone(&Utc))
            .unwrap_or(true);
        let precondition_changed = plan.items.iter().any(|item| {
            if let (Some(src), Some(pre)) = (item.source_rel.as_deref(), item.precondition.as_ref())
            {
                !precondition_matches(&item.folder_root, src, pre)
            } else {
                false
            }
        });
        if expired || precondition_changed {
            plan.status = OrganizePlanStatus::Stale;
            plan.updated_at = now.to_rfc3339();
            self.save_plan_locked(plan)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn get_plan(&self, kb_id: &str, plan_id: &str) -> StoreResult<OrganizePlan> {
        self.with_kb_organize_lock(kb_id, || {
            let mut plan = self.load_plan_locked(kb_id, plan_id)?;
            let _ = self.check_and_mark_stale_locked(&mut plan)?;
            Ok(plan)
        })
    }

    pub fn list_plans(&self, kb_id: &str) -> StoreResult<Vec<OrganizePlan>> {
        self.with_kb_organize_lock(kb_id, || {
            let plans_dir = self.knowledge_root.join(kb_id).join("organize").join("plans");
            let mut plans = Vec::new();
            if let Ok(entries) = fs::read_dir(&plans_dir) {
                for entry in entries.flatten() {
                    let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                        continue;
                    };
                    if is_knowledge_plan_id(&name) {
                        if let Ok(mut plan) = self.load_plan_locked(kb_id, &name) {
                            let _ = self.check_and_mark_stale_locked(&mut plan);
                            plans.push(plan);
                        }
                    }
                }
            }
            plans.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
            Ok(plans)
        })
    }

    /// User-only approval step (only exposed via Tauri command `cmd_kb_organize_approve`).
    pub fn approve_plan(
        &self,
        kb_id: &str,
        plan_id: &str,
        decision: ApprovalDecision,
    ) -> StoreResult<OrganizePlan> {
        self.with_kb_organize_lock(kb_id, || {
            let mut plan = self.load_plan_locked(kb_id, plan_id)?;
            if plan.status != OrganizePlanStatus::PendingReview {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            if self.check_and_mark_stale_locked(&mut plan)? {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            if decision.reject {
                plan.status = OrganizePlanStatus::Rejected;
                for item in &mut plan.items {
                    item.approved = false;
                }
                plan.updated_at = Utc::now().to_rfc3339();
                self.save_plan_locked(&plan)?;
                return Ok(plan);
            }

            let decision_map: BTreeMap<&str, &ItemApprovalDecision> = decision
                .items
                .iter()
                .map(|d| (d.item_id.as_str(), d))
                .collect();

            for item in &mut plan.items {
                if let Some(d) = decision_map.get(item.id.as_str()) {
                    item.approved = d.approved;
                    if let Some(override_rel) = d.override_target_rel.as_deref() {
                        let segs = validate_strict_relative_path(override_rel)
                            .map_err(|_| KbInvalidInput)?;
                        if is_hidden_or_credential_rel(&segs) {
                            return Err(KbInvalidInput);
                        }
                        let norm = segs.join("/");
                        ensure_no_link_in_rel(&item.folder_root, &norm)?;
                        item.override_target_rel = Some(norm);
                    }
                }
            }

            if !plan.items.iter().any(|item| item.approved) {
                plan.status = OrganizePlanStatus::Rejected;
            } else {
                plan.status = OrganizePlanStatus::Approved;
            }
            plan.updated_at = Utc::now().to_rfc3339();
            self.save_plan_locked(&plan)?;
            Ok(plan)
        })
    }

    /// User-only execution step (only exposed via Tauri command `cmd_kb_organize_execute`).
    /// Acquires `workspace_files::acquire_edit_mutation` for every involved folder root.
    pub async fn execute_plan(&self, kb_id: &str, plan_id: &str) -> StoreResult<OrganizePlan> {
        self.execute_plan_inner(kb_id, plan_id, None).await
    }

    #[cfg(test)]
    pub(crate) async fn execute_plan_with_crash(
        &self,
        kb_id: &str,
        plan_id: &str,
        crash: Option<(String, CrashPoint)>,
    ) -> StoreResult<OrganizePlan> {
        self.execute_plan_inner(kb_id, plan_id, crash).await
    }

    async fn execute_plan_inner(
        &self,
        kb_id: &str,
        plan_id: &str,
        #[cfg(test)] crash: Option<(String, CrashPoint)>,
        #[cfg(not(test))] _crash: Option<()>,
    ) -> StoreResult<OrganizePlan> {
        // 1. Transition Approved (or Interrupted resume) -> Executing and collect involved roots.
        let mut plan = self.with_kb_organize_lock(kb_id, || {
            let mut plan = self.load_plan_locked(kb_id, plan_id)?;
            if !matches!(
                plan.status,
                OrganizePlanStatus::Approved | OrganizePlanStatus::Interrupted
            ) {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            plan.status = OrganizePlanStatus::Executing;
            plan.updated_at = Utc::now().to_rfc3339();
            self.save_plan_locked(&plan)?;
            Ok(plan)
        })?;

        // 2. Acquire edit_mutation guards for all involved linked folders in sorted order.
        let mut roots: Vec<PathBuf> = plan
            .items
            .iter()
            .filter(|i| i.approved)
            .map(|i| i.folder_root.clone())
            .collect();
        roots.sort_by_key(|p| platform_path_key(p));
        roots.dedup_by(|a, b| platform_path_key(a) == platform_path_key(b));

        let mut _guards = Vec::with_capacity(roots.len());
        for root in &roots {
            _guards.push(acquire_edit_mutation(root).await);
        }

        // 3. Open durable journal and execute each approved pending item.
        let journal_path = self.journal_abs_path(kb_id, plan_id);
        let mut journal = DurableRecordJournal::<OrganizeJournalEvent>::open(
            journal_path,
            plan_id,
            JOURNAL_SCHEMA_VERSION,
            JOURNAL_MAX_LINE_BYTES,
        )
        .map_err(|_| KbStoreUnavailable)?;

        for item in &mut plan.items {
            if !item.approved {
                if item.status == OrganizeItemStatus::Pending {
                    item.status = OrganizeItemStatus::Skipped;
                }
                continue;
            }
            if item.status != OrganizeItemStatus::Pending {
                continue;
            }

            let target_rel = item.effective_target_rel().to_string();
            let now_ms = Utc::now().timestamp_millis();

            // Step 3.1: Write journal Intent + fsync
            journal
                .append(
                    now_ms,
                    0,
                    OrganizeJournalEvent::Intent {
                        op_id: item.id.clone(),
                        kind: item.kind,
                        folder_root: item.folder_root.clone(),
                        source_rel: item.source_rel.clone(),
                        target_rel: target_rel.clone(),
                        precondition: item.precondition.clone(),
                    },
                )
                .map_err(|_| KbStoreUnavailable)?;

            #[cfg(test)]
            if crash
                .as_ref()
                .is_some_and(|(id, pt)| id == &item.id && *pt == CrashPoint::IntentRecorded)
            {
                return Err(KbStoreUnavailable);
            }

            // Step 3.2: Re-verify precondition before touching disk
            if let (Some(src_rel), Some(pre)) =
                (item.source_rel.as_deref(), item.precondition.as_ref())
            {
                match check_precondition(&item.folder_root, src_rel, pre) {
                    Ok(true) => {}
                    Err(KnowledgeErrorCode::KbSourceInUse) => {
                        item.status = OrganizeItemStatus::Failed;
                        item.error = Some("FILE_IN_USE".to_string());
                        let _ = journal.append(
                            Utc::now().timestamp_millis(),
                            0,
                            OrganizeJournalEvent::Skipped {
                                op_id: item.id.clone(),
                                status: OrganizeItemStatus::Failed,
                                reason: "FILE_IN_USE".to_string(),
                            },
                        );
                        continue;
                    }
                    Ok(false) | Err(_) => {
                        item.status = OrganizeItemStatus::Conflict;
                        item.error = Some("PRECONDITION_MISMATCH".to_string());
                        let _ = journal.append(
                            Utc::now().timestamp_millis(),
                            0,
                            OrganizeJournalEvent::Skipped {
                                op_id: item.id.clone(),
                                status: OrganizeItemStatus::Conflict,
                                reason: "PRECONDITION_MISMATCH".to_string(),
                            },
                        );
                        continue;
                    }
                }
            }

            // Step 3.3: Execute via strict workspace helper
            let fs_result = match item.kind {
                OrganizeOpKind::Mkdir => strict_mkdir(&item.folder_root, &target_rel),
                OrganizeOpKind::Move | OrganizeOpKind::Rename => {
                    let src_rel = item.source_rel.as_deref().unwrap_or_default();
                    strict_move_or_rename(&item.folder_root, src_rel, &target_rel)
                }
            };

            let new_abs = match fs_result {
                Ok(path) => path,
                Err(err) => {
                    let (status, reason) = strict_error_code(&err);
                    item.status = status;
                    item.error = Some(reason.clone());
                    let _ = journal.append(
                        Utc::now().timestamp_millis(),
                        0,
                        OrganizeJournalEvent::Skipped {
                            op_id: item.id.clone(),
                            status,
                            reason,
                        },
                    );
                    continue;
                }
            };

            #[cfg(test)]
            if crash
                .as_ref()
                .is_some_and(|(id, pt)| id == &item.id && *pt == CrashPoint::FsMoved)
            {
                return Err(KbStoreUnavailable);
            }

            // Step 3.4: Update source.json path inside Store lock
            if let Some(src_rel) = item.source_rel.as_deref() {
                if let Ok(old_abs) = resolve_inside_workspace(&item.folder_root, src_rel) {
                    let _ = self.store.relink_moved_paths_under(
                        kb_id,
                        &item.folder_root,
                        &old_abs,
                        &new_abs,
                    );
                }
            }

            #[cfg(test)]
            if crash
                .as_ref()
                .is_some_and(|(id, pt)| id == &item.id && *pt == CrashPoint::StoreRelinked)
            {
                return Err(KbStoreUnavailable);
            }

            // Step 3.5: Record post-execution fingerprint & write Done journal + fsync
            let post_fp = capture_precondition(&item.folder_root, &target_rel).ok();
            journal
                .append(
                    Utc::now().timestamp_millis(),
                    0,
                    OrganizeJournalEvent::Done {
                        op_id: item.id.clone(),
                        post_fingerprint: post_fp.clone(),
                    },
                )
                .map_err(|_| KbStoreUnavailable)?;
            item.status = OrganizeItemStatus::Succeeded;
            item.error = None;
            item.post_fingerprint = post_fp;

            #[cfg(test)]
            if crash.as_ref().is_some_and(|(id, pt)| {
                id == &item.id && *pt == CrashPoint::DoneRecorded
            }) {
                return Err(KbStoreUnavailable);
            }
        }

        // 4. Finalize plan status
        let any_failed_or_conflict = plan.items.iter().any(|item| {
            item.approved
                && matches!(
                    item.status,
                    OrganizeItemStatus::Conflict | OrganizeItemStatus::Failed
                )
        });
        plan.status = if any_failed_or_conflict {
            OrganizePlanStatus::PartiallyCompleted
        } else {
            OrganizePlanStatus::Completed
        };
        plan.updated_at = Utc::now().to_rfc3339();
        self.with_kb_organize_lock(kb_id, || self.save_plan_locked(&plan))?;
        Ok(plan)
    }

    /// Startup crash recovery: scans all KBs for plans stuck in `executing`, replays their
    /// durable journal, reconciles any dangling `Intent` without `Done` against on-disk
    /// state and `source.json`, and settles them to `interrupted`.
    pub fn recover_interrupted_plans(&self) -> StoreResult<Vec<String>> {
        let kbs = self.store.list()?;
        let mut recovered = Vec::new();
        for kb in kbs {
            let plans_dir = self
                .knowledge_root
                .join(&kb.id)
                .join("organize")
                .join("plans");
            let Ok(entries) = fs::read_dir(&plans_dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Some(plan_id) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if !is_knowledge_plan_id(&plan_id) {
                    continue;
                }
                let settled = self.recover_one_plan(&kb.id, &plan_id)?;
                if settled {
                    recovered.push(plan_id);
                }
            }
        }
        Ok(recovered)
    }

    fn recover_one_plan(&self, kb_id: &str, plan_id: &str) -> StoreResult<bool> {
        self.with_kb_organize_lock(kb_id, || {
            let mut plan = self.load_plan_locked(kb_id, plan_id)?;
            if plan.status != OrganizePlanStatus::Executing {
                return Ok(false);
            }

            let journal_path = self.journal_abs_path(kb_id, plan_id);
            let entries = recover_and_read::<OrganizeJournalEvent>(
                &journal_path,
                plan_id,
                JOURNAL_SCHEMA_VERSION,
                JOURNAL_MAX_LINE_BYTES,
            )
            .map_err(|_| KbStoreUnavailable)?;
            let mut journal = DurableRecordJournal::<OrganizeJournalEvent>::open(
                journal_path,
                plan_id,
                JOURNAL_SCHEMA_VERSION,
                JOURNAL_MAX_LINE_BYTES,
            )
            .map_err(|_| KbStoreUnavailable)?;

            let mut intents: BTreeMap<String, RecordedIntent> = BTreeMap::new();
            let mut done_map: BTreeMap<String, Option<FilePrecondition>> = BTreeMap::new();
            let mut skipped_map: BTreeMap<String, (OrganizeItemStatus, String)> = BTreeMap::new();

            for entry in entries {
                match entry.event {
                    OrganizeJournalEvent::Intent {
                        op_id,
                        kind,
                        folder_root,
                        source_rel,
                        target_rel,
                        precondition,
                    } => {
                        intents.insert(
                            op_id,
                            RecordedIntent {
                                kind,
                                folder_root,
                                source_rel,
                                target_rel,
                                precondition,
                            },
                        );
                    }
                    OrganizeJournalEvent::Done {
                        op_id,
                        post_fingerprint,
                    } => {
                        done_map.insert(op_id, post_fingerprint);
                    }
                    OrganizeJournalEvent::Skipped {
                        op_id,
                        status,
                        reason,
                    } => {
                        skipped_map.insert(op_id, (status, reason));
                    }
                    OrganizeJournalEvent::Undone { .. }
                    | OrganizeJournalEvent::UndoSkipped { .. } => {}
                }
            }

            for item in &mut plan.items {
                if let Some(post_fp) = done_map.get(&item.id) {
                    item.status = OrganizeItemStatus::Succeeded;
                    item.post_fingerprint = post_fp.clone();
                    item.error = None;
                    continue;
                }
                if let Some((status, reason)) = skipped_map.get(&item.id) {
                    item.status = *status;
                    item.error = Some(reason.clone());
                    continue;
                }
                let Some(RecordedIntent {
                    kind,
                    folder_root,
                    source_rel,
                    target_rel,
                    precondition,
                }) = intents.get(&item.id)
                else {
                    continue;
                };

                // Dangling Intent without Done -> determine actual filesystem outcome!
                match kind {
                    OrganizeOpKind::Mkdir => {
                        let target_exists = resolve_inside_workspace(folder_root, target_rel)
                            .ok()
                            .is_some_and(|p| p.is_dir());
                        if target_exists {
                            let post_fp = capture_precondition(folder_root, target_rel).ok();
                            let _ = journal.append(
                                Utc::now().timestamp_millis(),
                                0,
                                OrganizeJournalEvent::Done {
                                    op_id: item.id.clone(),
                                    post_fingerprint: post_fp.clone(),
                                },
                            );
                            item.status = OrganizeItemStatus::Succeeded;
                            item.post_fingerprint = post_fp;
                        } else {
                            item.status = OrganizeItemStatus::Pending;
                        }
                    }
                    OrganizeOpKind::Move | OrganizeOpKind::Rename => {
                        let src_rel = source_rel.as_deref().unwrap_or_default();
                        let src_path = resolve_inside_workspace(folder_root, src_rel).ok();
                        let dst_path = resolve_inside_workspace(folder_root, target_rel).ok();
                        let src_exists = src_path
                            .as_ref()
                            .is_some_and(|p| fs::symlink_metadata(p).is_ok());
                        let dst_exists = dst_path
                            .as_ref()
                            .is_some_and(|p| fs::symlink_metadata(p).is_ok());

                        let target_matches = precondition
                            .as_ref()
                            .is_some_and(|pre| precondition_matches(folder_root, target_rel, pre));
                        let source_matches = precondition
                            .as_ref()
                            .is_some_and(|pre| precondition_matches(folder_root, src_rel, pre));

                        if !src_exists && dst_exists && target_matches {
                            // FS move already succeeded -> ensure source.json is updated and write Done!
                            if let (Some(old_abs), Some(new_abs)) = (src_path, dst_path) {
                                let _ = self.store.relink_moved_paths_under(
                                    kb_id,
                                    folder_root,
                                    &old_abs,
                                    &new_abs,
                                );
                            }
                            let post_fp = capture_precondition(folder_root, target_rel).ok();
                            let _ = journal.append(
                                Utc::now().timestamp_millis(),
                                0,
                                OrganizeJournalEvent::Done {
                                    op_id: item.id.clone(),
                                    post_fingerprint: post_fp.clone(),
                                },
                            );
                            item.status = OrganizeItemStatus::Succeeded;
                            item.post_fingerprint = post_fp;
                        } else if src_exists && !dst_exists && source_matches {
                            // Crash happened before FS move -> source intact, keep Pending.
                            item.status = OrganizeItemStatus::Pending;
                        } else {
                            let reason = "RECOVERY_AMBIGUOUS_STATE".to_string();
                            item.status = OrganizeItemStatus::Conflict;
                            item.error = Some(reason.clone());
                            let _ = journal.append(
                                Utc::now().timestamp_millis(),
                                0,
                                OrganizeJournalEvent::Skipped {
                                    op_id: item.id.clone(),
                                    status: OrganizeItemStatus::Conflict,
                                    reason,
                                },
                            );
                        }
                    }
                }
            }

            plan.status = OrganizePlanStatus::Interrupted;
            plan.updated_at = Utc::now().to_rfc3339();
            self.save_plan_locked(&plan)?;
            Ok(true)
        })
    }

    /// User-only undo step (only exposed via Tauri command `cmd_kb_organize_undo`).
    /// Reverses completed journal entries in reverse order with strict precondition checks.
    /// Files modified after execution are NEVER overwritten/undone and are listed in `skipped_modified`.
    pub async fn undo_plan(&self, kb_id: &str, plan_id: &str) -> StoreResult<UndoReport> {
        let mut plan = self.with_kb_organize_lock(kb_id, || {
            let plan = self.load_plan_locked(kb_id, plan_id)?;
            if !matches!(
                plan.status,
                OrganizePlanStatus::Completed
                    | OrganizePlanStatus::PartiallyCompleted
                    | OrganizePlanStatus::Interrupted
            ) {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            Ok(plan)
        })?;

        let mut roots: Vec<PathBuf> = plan
            .items
            .iter()
            .filter(|i| i.status == OrganizeItemStatus::Succeeded)
            .map(|i| i.folder_root.clone())
            .collect();
        roots.sort_by_key(|p| platform_path_key(p));
        roots.dedup_by(|a, b| platform_path_key(a) == platform_path_key(b));

        let mut _guards = Vec::with_capacity(roots.len());
        for root in &roots {
            _guards.push(acquire_edit_mutation(root).await);
        }

        let journal_path = self.journal_abs_path(kb_id, plan_id);
        let entries = recover_and_read::<OrganizeJournalEvent>(
            &journal_path,
            plan_id,
            JOURNAL_SCHEMA_VERSION,
            JOURNAL_MAX_LINE_BYTES,
        )
        .map_err(|_| KbStoreUnavailable)?;
        let mut journal = DurableRecordJournal::<OrganizeJournalEvent>::open(
            journal_path,
            plan_id,
            JOURNAL_SCHEMA_VERSION,
            JOURNAL_MAX_LINE_BYTES,
        )
        .map_err(|_| KbStoreUnavailable)?;

        let mut done_order: Vec<(String, Option<FilePrecondition>)> = Vec::new();
        let mut already_undone: BTreeSet<String> = BTreeSet::new();
        for entry in entries {
            match entry.event {
                OrganizeJournalEvent::Done {
                    op_id,
                    post_fingerprint,
                } => done_order.push((op_id, post_fingerprint)),
                OrganizeJournalEvent::Undone { op_id } => {
                    already_undone.insert(op_id);
                }
                _ => {}
            }
        }

        let mut report = UndoReport {
            plan_id: plan_id.to_owned(),
            ..Default::default()
        };

        for (op_id, done_fp) in done_order.into_iter().rev() {
            if already_undone.contains(&op_id) {
                continue;
            }
            let Some(item) = plan.items.iter_mut().find(|i| i.id == op_id) else {
                continue;
            };
            let target_rel = item.effective_target_rel().to_string();

            match item.kind {
                OrganizeOpKind::Mkdir => {
                    let Ok(target_abs) = resolve_inside_workspace(&item.folder_root, &target_rel)
                    else {
                        item.status = OrganizeItemStatus::UndoFailed;
                        report.failed.push(op_id);
                        continue;
                    };
                    match fs::remove_dir(&target_abs) {
                        Ok(()) => {
                            item.status = OrganizeItemStatus::Undone;
                            item.error = None;
                            let _ = journal.append(
                                Utc::now().timestamp_millis(),
                                0,
                                OrganizeJournalEvent::Undone {
                                    op_id: op_id.clone(),
                                },
                            );
                            report.undone.push(op_id);
                        }
                        Err(err) => {
                            // Non-empty directory (e.g., contains a modified file that wasn't undone)
                            item.status = OrganizeItemStatus::UndoSkippedModified;
                            item.error = Some(format!("DIR_NOT_EMPTY: {err}"));
                            let _ = journal.append(
                                Utc::now().timestamp_millis(),
                                0,
                                OrganizeJournalEvent::UndoSkipped {
                                    op_id: op_id.clone(),
                                    status: OrganizeItemStatus::UndoSkippedModified,
                                    reason: "DIR_NOT_EMPTY".to_string(),
                                },
                            );
                            report.skipped_modified.push(op_id);
                        }
                    }
                }
                OrganizeOpKind::Move | OrganizeOpKind::Rename => {
                    let Some(src_rel) = item.source_rel.clone() else {
                        item.status = OrganizeItemStatus::UndoFailed;
                        report.failed.push(op_id);
                        continue;
                    };
                    let expected_fp = done_fp.as_ref().or(item.post_fingerprint.as_ref());
                    if let Some(fp) = expected_fp {
                        if !precondition_matches(&item.folder_root, &target_rel, fp) {
                            item.status = OrganizeItemStatus::UndoSkippedModified;
                            item.error = Some("MODIFIED_AFTER_EXECUTION".to_string());
                            let _ = journal.append(
                                Utc::now().timestamp_millis(),
                                0,
                                OrganizeJournalEvent::UndoSkipped {
                                    op_id: op_id.clone(),
                                    status: OrganizeItemStatus::UndoSkippedModified,
                                    reason: "MODIFIED_AFTER_EXECUTION".to_string(),
                                },
                            );
                            report.skipped_modified.push(op_id);
                            continue;
                        }
                    }

                    match strict_move_or_rename(&item.folder_root, &target_rel, &src_rel) {
                        Ok(restored_abs) => {
                            if let Ok(old_target_abs) =
                                resolve_inside_workspace(&item.folder_root, &target_rel)
                            {
                                let _ = self.store.relink_moved_paths_under(
                                    kb_id,
                                    &item.folder_root,
                                    &old_target_abs,
                                    &restored_abs,
                                );
                            }
                            item.status = OrganizeItemStatus::Undone;
                            item.error = None;
                            let _ = journal.append(
                                Utc::now().timestamp_millis(),
                                0,
                                OrganizeJournalEvent::Undone {
                                    op_id: op_id.clone(),
                                },
                            );
                            report.undone.push(op_id);
                        }
                        Err(StrictFsError::Conflict(msg)) => {
                            item.status = OrganizeItemStatus::UndoConflict;
                            item.error = Some(format!("UNDO_CONFLICT: {msg}"));
                            report.conflicts.push(op_id);
                        }
                        Err(err) => {
                            ulog_warn!("[knowledge] organize undo failed op={} err={}", op_id, err);
                            item.status = OrganizeItemStatus::UndoFailed;
                            item.error = Some(err.to_string());
                            report.failed.push(op_id);
                        }
                    }
                }
            }
        }

        plan.updated_at = Utc::now().to_rfc3339();
        self.with_kb_organize_lock(kb_id, || self.save_plan_locked(&plan))?;
        Ok(report)
    }
}
