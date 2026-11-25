//! Store-owned linked folder registry and read-only linked source projections.
//!
//! Linked folders keep the user's original files in place. The Store only persists
//! normalized `content.md` and `source.json` projections inside `<app-data>/knowledge/`,
//! never copying `original.<ext>` and never deleting or mutating user originals.

use super::*;
use crate::knowledge::chunker::{chunk_markdown, ChunkError};
use crate::workspace_files::path_safety::{metadata_is_link_like, validate_workspace_root};
use sha2::{Digest, Sha256};

const MACOS_DATA_VOLUME_PREFIX: &str = "/System/Volumes/Data";
const MAX_LINKED_FOLDERS_PER_KB: usize = 32;
const MAX_LINKED_CONTENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedFolder {
    pub path: PathBuf,
    #[serde(default)]
    pub organize_allowed: bool,
    #[serde(default)]
    pub git_confirmed: bool,
    #[serde(default)]
    pub is_git_work_tree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LinkedSourceMeta {
    pub(super) folder_root: PathBuf,
    pub(super) original_sha256: String,
    pub(super) extension: String,
    /// Incremented only when `content.md` changes (never on move/rename or fingerprint-only updates).
    /// P1 embedding caches can key on `(source_id, content_hash)` / `content_revision` to skip work.
    #[serde(default = "default_content_revision")]
    pub(super) content_revision: u64,
}

fn default_content_revision() -> u64 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedSourceSnapshot {
    pub source_id: String,
    pub current_path: PathBuf,
    pub display_name: Option<String>,
    pub file_fingerprint: Option<SourceFingerprint>,
    pub content_hash: Option<String>,
    pub original_sha256: String,
    pub extension: String,
    pub content_revision: u64,
}

/// Strip macOS APFS Data volume firmlink prefix (`/System/Volumes/Data/Users/...` -> `/Users/...`)
/// so watcher events and canonical paths match the user-visible path cleanly.
pub(crate) fn strip_macos_firmlink_prefix(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    let normalized = raw.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix(MACOS_DATA_VOLUME_PREFIX) {
        if rest.starts_with('/') && rest.len() > 1 {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

/// Strip Windows verbatim prefix (`\\?\C:\...` or `//?/C:/...`) portably across OSes.
fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    let stripped = raw
        .strip_prefix(r"\\?\")
        .or_else(|| raw.strip_prefix("//?/"));
    if let Some(rest) = stripped {
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

/// Normalize a linked folder or file path across platforms:
/// - Strip Windows `\\?\` verbatim prefix.
/// - Strip macOS `/System/Volumes/Data` APFS firmlink prefix.
pub(crate) fn normalize_linked_path(path: &Path) -> PathBuf {
    let external = crate::sidecar::normalize_external_path(path.to_path_buf());
    let no_verbatim = strip_windows_verbatim_prefix(&external);
    strip_macos_firmlink_prefix(&no_verbatim)
}

/// Deterministic comparison key for path identity matching with configurable case folding.
pub(crate) fn path_key_with_case_folding(path: &Path, case_insensitive: bool) -> String {
    let normalized = normalize_linked_path(path);
    let mut text = normalized.to_string_lossy().replace('\\', "/");
    while text.len() > 1 && text.ends_with('/') && !text.ends_with(":/") {
        text.pop();
    }
    if case_insensitive {
        text.to_lowercase()
    } else {
        text
    }
}

/// Platform-aware path comparison key (case-insensitive on Windows and macOS APFS default,
/// case-sensitive on Linux).
pub(crate) fn platform_path_key(path: &Path) -> String {
    path_key_with_case_folding(path, cfg!(any(target_os = "windows", target_os = "macos")))
}

/// Walk upward from `dir` looking for `.git` (directory or worktree file) without following links.
pub(crate) fn detect_git_work_tree(dir: &Path) -> bool {
    let mut cursor = Some(dir);
    while let Some(current) = cursor {
        let dot_git = current.join(".git");
        if let Ok(meta) = fs::symlink_metadata(&dot_git) {
            if meta.is_dir() || meta.is_file() {
                return true;
            }
        }
        cursor = current.parent();
    }
    false
}

fn validate_linked_folder_root(raw_path: &Path, store_root: &Path) -> StoreResult<PathBuf> {
    if !raw_path.is_absolute() {
        return Err(KbInvalidInput);
    }
    let leaf_meta = fs::symlink_metadata(raw_path).map_err(|_| KbInvalidInput)?;
    if !leaf_meta.is_dir() || metadata_is_link_like(&leaf_meta) {
        return Err(KbInvalidInput);
    }
    let raw_str = raw_path.to_str().ok_or(KbInvalidInput)?;
    let validated = validate_workspace_root(raw_str).map_err(|_| KbInvalidInput)?;
    let canonical = normalize_linked_path(&validated);

    // Refuse self-linking into the managed KnowledgeStore root (or vice versa).
    let store_canonical = fs::canonicalize(store_root)
        .map(|p| normalize_linked_path(&p))
        .unwrap_or_else(|_| normalize_linked_path(store_root));
    let folder_key = platform_path_key(&canonical);
    let store_key = platform_path_key(&store_canonical);
    let store_prefix = format!("{store_key}/");
    let folder_prefix = format!("{folder_key}/");
    if folder_key == store_key
        || folder_key.starts_with(&store_prefix)
        || store_key.starts_with(&folder_prefix)
    {
        return Err(KbInvalidInput);
    }
    Ok(canonical)
}

fn valid_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

impl KnowledgeStore {
    /// Register a local folder on a knowledge base in read-only mode.
    /// `organizeAllowed` and `gitConfirmed` default to `false`; `isGitWorkTree` is detected live.
    pub fn add_linked_folder(&self, kb_id: &str, folder_path: &Path) -> StoreResult<LinkedFolder> {
        let canonical =
            validate_linked_folder_root(folder_path, &self.app_data_dir.join("knowledge"))?;
        let is_git_work_tree = detect_git_work_tree(&canonical);
        let new_key = platform_path_key(&canonical);

        self.locked_kb(kb_id, None, |files, _| {
            let mut kb = files.read_kb(kb_id)?;
            if kb.linked_folders.len() >= MAX_LINKED_FOLDERS_PER_KB {
                return Err(KbLimitExceeded);
            }
            if kb
                .linked_folders
                .iter()
                .any(|existing| platform_path_key(&existing.path) == new_key)
            {
                return Err(KbInvalidInput);
            }
            let entry = LinkedFolder {
                path: canonical.clone(),
                organize_allowed: false,
                git_confirmed: false,
                is_git_work_tree,
            };
            kb.linked_folders.push(entry.clone());
            files.write_json(&format!("{kb_id}/kb.json"), &kb)?;
            self.emit_ui("kb-upserted", kb_id, None);
            Ok(entry)
        })
    }

    /// Update `organizeAllowed` / `gitConfirmed` flags for a linked folder and refresh `isGitWorkTree`.
    pub fn update_linked_folder(
        &self,
        kb_id: &str,
        folder_path: &Path,
        organize_allowed: Option<bool>,
        git_confirmed: Option<bool>,
    ) -> StoreResult<LinkedFolder> {
        let target_key = platform_path_key(folder_path);
        self.locked_kb(kb_id, None, |files, _| {
            let mut kb = files.read_kb(kb_id)?;
            let folder = kb
                .linked_folders
                .iter_mut()
                .find(|entry| platform_path_key(&entry.path) == target_key)
                .ok_or(KbNotFound)?;
            folder.is_git_work_tree = detect_git_work_tree(&folder.path);
            if let Some(allowed) = organize_allowed {
                folder.organize_allowed = allowed;
            }
            if let Some(confirmed) = git_confirmed {
                folder.git_confirmed = confirmed;
            }
            let updated = folder.clone();
            files.write_json(&format!("{kb_id}/kb.json"), &kb)?;
            self.emit_ui("kb-upserted", kb_id, None);
            Ok(updated)
        })
    }

    /// Unlink a folder from `kb.json` and remove its Store-owned source projections.
    /// NEVER deletes or modifies any user file inside `folder_path`.
    pub fn remove_linked_folder(&self, kb_id: &str, folder_path: &Path) -> StoreResult<usize> {
        let target_key = platform_path_key(folder_path);
        self.locked_kb(kb_id, None, |files, _| {
            let mut kb = files.read_kb(kb_id)?;
            let before = kb.linked_folders.len();
            kb.linked_folders
                .retain(|entry| platform_path_key(&entry.path) != target_key);
            if kb.linked_folders.len() == before {
                return Err(KbNotFound);
            }
            files.write_json(&format!("{kb_id}/kb.json"), &kb)?;

            let mut removed_ids = Vec::new();
            if let Some(parent) = files.source_parent(kb_id)? {
                files.recover_source_publications(&parent)?;
                for id in ingest::valid_kb_source_folders(&parent)? {
                    if let Some(record) = files.read_record(kb_id, &id)? {
                        let belongs = record.source.facts.kind == SourceKind::LinkedFile
                            && record
                                .linked
                                .as_ref()
                                .is_some_and(|meta| platform_path_key(&meta.folder_root) == target_key);
                        if belongs {
                            removed_ids.push(id);
                        }
                    }
                }
                for id in &removed_ids {
                    let tombstone = files.detach(&parent, id)?;
                    let _ = self.changes.send(KnowledgeChange::SourceRemoved {
                        kb_id: kb_id.into(),
                        source_id: id.clone(),
                    });
                    self.emit_ui("source-removed", kb_id, Some(id));
                    files.cleanup(&tombstone);
                }
            }
            self.emit_ui("kb-upserted", kb_id, None);
            Ok(removed_ids.len())
        })
    }

    /// List all linked-file source snapshots currently registered under `folder_path`.
    pub fn list_linked_sources_for_folder(
        &self,
        kb_id: &str,
        folder_path: &Path,
    ) -> StoreResult<Vec<LinkedSourceSnapshot>> {
        let target_key = platform_path_key(folder_path);
        self.locked_kb(kb_id, None, |files, _| {
            let kb = files.read_kb(kb_id)?;
            if !kb
                .linked_folders
                .iter()
                .any(|entry| platform_path_key(&entry.path) == target_key)
            {
                return Err(KbNotFound);
            }
            let Some(parent) = files.source_parent(kb_id)? else {
                return Ok(Vec::new());
            };
            files.recover_source_publications(&parent)?;
            let mut snapshots = Vec::new();
            for id in ingest::valid_kb_source_folders(&parent)? {
                if let Some(record) = files.read_record(kb_id, &id)? {
                    if record.source.facts.kind == SourceKind::LinkedFile {
                        if let (Some(meta), Some(current_path)) =
                            (record.linked, record.source.facts.current_path)
                        {
                            if platform_path_key(&meta.folder_root) == target_key {
                                snapshots.push(LinkedSourceSnapshot {
                                    source_id: record.source.id,
                                    current_path,
                                    display_name: record.source.display_name,
                                    file_fingerprint: record.source.facts.file_fingerprint,
                                    content_hash: record.source.facts.content_hash,
                                    original_sha256: meta.original_sha256,
                                    extension: meta.extension,
                                    content_revision: meta.content_revision,
                                });
                            }
                        }
                    }
                }
            }
            snapshots.sort_by(|a, b| a.source_id.cmp(&b.source_id));
            Ok(snapshots)
        })
    }

    /// Publish or update a linked file's normalized `content.md` and `source.json`
    /// without ever copying `original.<ext>` into the managed Store.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn publish_linked_source(
        &self,
        kb_id: &str,
        existing_source_id: Option<&str>,
        folder_root: &Path,
        file_path: &Path,
        extension: &str,
        original_sha256: &str,
        fingerprint: SourceFingerprint,
        markdown: &str,
    ) -> StoreResult<KnowledgeSource> {
        if markdown.len() > MAX_LINKED_CONTENT_BYTES
            || !valid_hex_sha256(original_sha256)
            || extension.is_empty()
            || !extension.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(KbInvalidInput);
        }
        let normalized_root = normalize_linked_path(folder_root);
        let normalized_file = normalize_linked_path(file_path);
        let display_name = normalized_file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .ok_or(KbInvalidInput)?;
        let content_hash = format!("{:x}", Sha256::digest(markdown.as_bytes()));
        let folder_key = platform_path_key(&normalized_root);

        // Pre-validate chunking outside the write lock using a valid synthetic or existing ID.
        let check_source_id = existing_source_id.unwrap_or("src_0000000000000000");
        let locale = crate::i18n::locale_from_config(&self.app_data_dir.join("config.json"));
        chunk_markdown(kb_id, check_source_id, &display_name, markdown, locale).map_err(
            |err| match err {
                ChunkError::GraphemeExceedsBudget(_) => KnowledgeErrorCode::KbChunkUnsplittable,
                ChunkError::InvalidReference => KbStoreCorrupt,
                ChunkError::InsufficientContextSpace => KbLimitExceeded,
            },
        )?;

        self.locked_kb(kb_id, existing_source_id, |files, _| {
            let kb = files.read_kb(kb_id)?;
            if !kb
                .linked_folders
                .iter()
                .any(|entry| platform_path_key(&entry.path) == folder_key)
            {
                return Err(KbNotFound);
            }
            if let Some(parent) = files.source_parent(kb_id)? {
                files.recover_source_publications(&parent)?;
            }
            let parent = files.root.join(kb_id).join("sources");

            if let Some(source_id) = existing_source_id {
                let mut record = files.require_record(kb_id, source_id)?;
                if record.source.facts.kind != SourceKind::LinkedFile {
                    return Err(KbInvalidInput);
                }
                let prev_rev = record
                    .linked
                    .as_ref()
                    .map(|meta| meta.content_revision)
                    .unwrap_or(1);
                let hash_changed =
                    record.source.facts.content_hash.as_deref() != Some(content_hash.as_str());
                let next_rev = if hash_changed { prev_rev + 1 } else { prev_rev };

                record.source.display_name = Some(display_name.clone());
                record.source.facts.current_path = Some(normalized_file.clone());
                record.source.facts.content_hash = Some(content_hash.clone());
                record.source.facts.file_fingerprint = Some(fingerprint);
                record.source.facts.status = SourceStatus::Ready;
                record.linked = Some(LinkedSourceMeta {
                    folder_root: normalized_root,
                    original_sha256: original_sha256.to_owned(),
                    extension: extension.to_ascii_lowercase(),
                    content_revision: next_rev,
                });
                if hash_changed {
                    record.curation_pending = true;
                    files.write_text(&format!("{kb_id}/sources/{source_id}/content.md"), markdown)?;
                }
                files.write_json(&source_path(kb_id, source_id), &record)?;
                self.source_upserted(kb_id, &record.source);
                return Ok(record.source);
            }

            ingest::check_source_limit(&parent)?;
            let source_id = allocate_id(&parent, |_| false, create_knowledge_source_id)?;
            let record = SourceRecord {
                source: KnowledgeSource {
                    id: source_id.clone(),
                    display_name: Some(display_name),
                    facts: SourceFacts {
                        kind: SourceKind::LinkedFile,
                        current_path: Some(normalized_file),
                        content_hash: Some(content_hash),
                        file_fingerprint: Some(fingerprint),
                        status: SourceStatus::Ready,
                    },
                    metadata: BTreeMap::new(),
                },
                ingest: None,
                linked: Some(LinkedSourceMeta {
                    folder_root: normalized_root,
                    original_sha256: original_sha256.to_owned(),
                    extension: extension.to_ascii_lowercase(),
                    content_revision: 1,
                }),
                curation_pending: true,
            };

            let staged_id = format!("{STAGING}{source_id}");
            let staged = parent.join(&staged_id);
            files.write_text(&format!("{kb_id}/sources/{staged_id}/content.md"), markdown)?;
            files.write_json(&source_path(kb_id, &staged_id), &record)?;
            rename_directory_noreplace(&staged, &parent.join(&source_id))
                .map_err(|_| KbStoreUnavailable)?;
            sync_directory(&parent).map_err(|_| KbStoreUnavailable)?;
            sync_directory(&files.root.join(kb_id)).map_err(|_| KbStoreUnavailable)?;
            self.source_upserted(kb_id, &record.source);
            Ok(record.source)
        })
    }

    /// Update only path and fingerprint for a linked file recognized as moved or renamed.
    /// Preserves `source_id`, `content.md`, `content_hash`, `content_revision`, and all metadata
    /// without triggering re-chunking or embedding recomputation.
    pub(crate) fn relink_moved_source(
        &self,
        kb_id: &str,
        source_id: &str,
        new_path: &Path,
        new_fingerprint: SourceFingerprint,
    ) -> StoreResult<KnowledgeSource> {
        let normalized_file = normalize_linked_path(new_path);
        let display_name = normalized_file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .ok_or(KbInvalidInput)?;

        self.locked_kb(kb_id, Some(source_id), |files, _| {
            let mut record = files.require_record(kb_id, source_id)?;
            if record.source.facts.kind != SourceKind::LinkedFile || record.linked.is_none() {
                return Err(KbInvalidInput);
            }
            record.source.display_name = Some(display_name);
            record.source.facts.current_path = Some(normalized_file);
            record.source.facts.file_fingerprint = Some(new_fingerprint);
            files.write_json(&source_path(kb_id, source_id), &record)?;
            // Broadcast MetadataChanged so search updates the title projection without
            // invalidating content_hash / content_revision for future P1 embedding caches.
            let _ = self.changes.send(KnowledgeChange::MetadataChanged {
                kb_id: kb_id.into(),
                source_id: source_id.into(),
            });
            self.emit_ui("metadata-changed", kb_id, Some(source_id));
            Ok(record.source)
        })
    }

    /// Update `current_path` (and `display_name`) for every `LinkedFile` source inside
    /// `folder_root` matching `old_abs` (exact file or moved directory prefix) in one Store lock.
    pub(crate) fn relink_moved_paths_under(
        &self,
        kb_id: &str,
        folder_root: &Path,
        old_abs: &Path,
        new_abs: &Path,
    ) -> StoreResult<usize> {
        let root_key = platform_path_key(folder_root);
        let old_norm = normalize_linked_path(old_abs);
        let new_norm = normalize_linked_path(new_abs);
        let old_key = platform_path_key(&old_norm);
        let old_prefix = format!("{old_key}/");

        self.locked_kb(kb_id, None, |files, _| {
            let Some(parent) = files.source_parent(kb_id)? else {
                return Ok(0);
            };
            files.recover_source_publications(&parent)?;
            let mut updated = 0usize;
            for id in ingest::valid_kb_source_folders(&parent)? {
                let Some(mut record) = files.read_record(kb_id, &id)? else {
                    continue;
                };
                if record.source.facts.kind != SourceKind::LinkedFile {
                    continue;
                }
                let Some(meta) = &record.linked else {
                    continue;
                };
                if platform_path_key(&meta.folder_root) != root_key {
                    continue;
                }
                let Some(cur_path) = record.source.facts.current_path.as_deref() else {
                    continue;
                };
                let cur_norm = normalize_linked_path(cur_path);
                let cur_key = platform_path_key(&cur_norm);
                let next_path = if cur_key == old_key {
                    Some(new_norm.clone())
                } else if cur_key.starts_with(&old_prefix) {
                    let tail = cur_norm
                        .strip_prefix(&old_norm)
                        .ok()
                        .map(Path::to_path_buf)
                        .or_else(|| {
                            let cur_slash = cur_norm.to_string_lossy().replace('\\', "/");
                            let old_slash = old_norm.to_string_lossy().replace('\\', "/");
                            let suffix = cur_slash
                                .get(old_slash.len()..)
                                .and_then(|s| s.strip_prefix('/'))?;
                            Some(PathBuf::from(suffix))
                        });
                    tail.map(|rel| normalize_linked_path(&new_norm.join(rel)))
                } else {
                    None
                };

                if let Some(next_path) = next_path {
                    if let Some(name) = next_path.file_name().map(|n| n.to_string_lossy().into_owned()) {
                        record.source.display_name = Some(name);
                    }
                    record.source.facts.current_path = Some(next_path);
                    files.write_json(&source_path(kb_id, &id), &record)?;
                    let _ = self.changes.send(KnowledgeChange::MetadataChanged {
                        kb_id: kb_id.into(),
                        source_id: id.clone(),
                    });
                    self.emit_ui("metadata-changed", kb_id, Some(&id));
                    updated += 1;
                }
            }
            Ok(updated)
        })
    }

    /// Update only the file fingerprint when mtime changed without modifying file bytes.
    pub(crate) fn refresh_linked_fingerprint(
        &self,
        kb_id: &str,
        source_id: &str,
        new_fingerprint: SourceFingerprint,
    ) -> StoreResult<()> {
        self.locked_kb(kb_id, Some(source_id), |files, _| {
            let mut record = files.require_record(kb_id, source_id)?;
            if record.source.facts.kind != SourceKind::LinkedFile {
                return Err(KbInvalidInput);
            }
            record.source.facts.file_fingerprint = Some(new_fingerprint);
            files.write_json(&source_path(kb_id, source_id), &record)
        })
    }
}
