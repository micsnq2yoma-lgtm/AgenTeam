//! K12 linked folders: read-only directory scanning, debounced filesystem watching,
//! and fingerprint/hash move recognition.
//!
//! Safety invariants:
//! - Reuses `file_indexer::SKIP_DIRS` and excludes hidden entries and credential-like files.
//! - Rejects symlinks, Windows junctions, and reparse points at every traversal step and
//!   opens every leaf file handle-relative with no-follow semantics (`open_workspace_regular_file_no_follow_classified`).
//! - Performs bounded reads and post-read metadata verification (TOCTOU guard).
//! - NEVER deletes, moves, or mutates any file inside a user's linked folder.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::commands::validate_file_path as system_blacklist_check;
use crate::document_processing::{ManagedDocumentProcessing, MAX_OUTPUT_BYTES};
use crate::knowledge::ingest::conversion::{convert, decode_text, source_limit};
use crate::knowledge::store::linked::{
    normalize_linked_path, platform_path_key, LinkedFolder, LinkedSourceSnapshot,
};
use crate::knowledge::store::{KnowledgeStore, SourceFingerprint, StoreResult};
use crate::knowledge::types::KnowledgeErrorCode;
use crate::search::FILE_INDEX_SKIP_DIRS as SKIP_DIRS;
use crate::workspace_files::path_safety::{
    metadata_is_link_like, open_workspace_regular_file_no_follow_classified,
    read_workspace_file_no_follow, validate_workspace_root,
};
use crate::{ulog_info, ulog_warn};

const DEFAULT_DEBOUNCE_WINDOW: Duration = Duration::from_secs(2);
const TEXT_LIMIT: u64 = 8 * 1024 * 1024;
const MAX_SCAN_DEPTH: usize = 32;
const MAX_SCAN_ENTRIES: usize = 50_000;

/// Credential-like file extensions excluded from linked folder scanning by default.
const CREDENTIAL_EXTENSIONS: &[&str] = &[
    "pem", "key", "p12", "pfx", "jks", "keystore", "kdbx", "crt", "cer", "der", "ovpn", "ppk",
    "netrc", "pgpass", "npmrc", "pypirc", "htpasswd", "env",
];

/// Credential-like exact filenames (case-insensitive) excluded by default.
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedFolderScanSummary {
    pub kb_id: String,
    pub path: PathBuf,
    pub added: usize,
    pub updated: usize,
    pub moved: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub rechunked: usize,
}

#[derive(Debug, Clone)]
struct ScannedFile {
    rel_path: String,
    abs_path: PathBuf,
    path_key: String,
    extension: String,
    limit: u64,
    fingerprint: SourceFingerprint,
}

#[derive(Debug)]
struct LoadedFile {
    scanned: ScannedFile,
    original_sha256: String,
    bytes: Vec<u8>,
}

fn should_skip_dir_name(name: &str) -> bool {
    name.starts_with('.')
        || SKIP_DIRS
            .iter()
            .any(|skip| name.eq_ignore_ascii_case(skip))
}

fn is_credential_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with('.')
        || lower.starts_with(".env")
        || CREDENTIAL_FILENAMES.contains(&lower.as_str())
    {
        return true;
    }
    Path::new(&lower)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| CREDENTIAL_EXTENSIONS.contains(&ext))
}

fn fingerprint_from_meta(meta: &fs::Metadata) -> Option<SourceFingerprint> {
    let modified = meta.modified().ok()?;
    Some(SourceFingerprint {
        size: meta.len(),
        mtime_ms: DateTime::<Utc>::from(modified).timestamp_millis(),
    })
}

fn scan_linked_directory(folder_root: &Path) -> StoreResult<(PathBuf, Vec<ScannedFile>, usize)> {
    let root_str = folder_root
        .to_str()
        .ok_or(KnowledgeErrorCode::KbInvalidInput)?;
    let validated =
        validate_workspace_root(root_str).map_err(|_| KnowledgeErrorCode::KbInvalidInput)?;
    let canonical_root = normalize_linked_path(&validated);

    let mut discovered = Vec::new();
    let mut skipped = 0usize;
    let mut visited_entries = 0usize;
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(canonical_root.clone(), String::new(), 0)];

    while let Some((dir, rel_prefix, depth)) = stack.pop() {
        if depth > MAX_SCAN_DEPTH {
            skipped += 1;
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            skipped += 1;
            continue;
        };
        let mut children: Vec<fs::DirEntry> = entries.flatten().collect();
        children.sort_by_key(|entry| entry.file_name());

        for entry in children {
            visited_entries += 1;
            if visited_entries > MAX_SCAN_ENTRIES {
                skipped += 1;
                break;
            }
            let entry_path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&entry_path) else {
                skipped += 1;
                continue;
            };
            // Hard security boundary: never follow symlinks, junctions, or reparse points.
            if metadata_is_link_like(&meta) {
                skipped += 1;
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                skipped += 1;
                continue;
            };
            let rel_path = if rel_prefix.is_empty() {
                name.clone()
            } else {
                format!("{rel_prefix}/{name}")
            };

            if meta.is_dir() {
                if should_skip_dir_name(&name) {
                    skipped += 1;
                    continue;
                }
                if entry_path
                    .to_str()
                    .is_some_and(|s| system_blacklist_check(s).is_err())
                {
                    skipped += 1;
                    continue;
                }
                stack.push((entry_path, rel_path, depth + 1));
                continue;
            }

            if !meta.is_file() {
                skipped += 1;
                continue;
            }
            if is_credential_file(&name) {
                skipped += 1;
                continue;
            }
            let Ok(limit) = source_limit(&entry_path) else {
                skipped += 1;
                continue;
            };
            if meta.len() > limit {
                skipped += 1;
                continue;
            }
            let Some(fingerprint) = fingerprint_from_meta(&meta) else {
                skipped += 1;
                continue;
            };
            let Some(extension) = entry_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase())
            else {
                skipped += 1;
                continue;
            };

            let abs_path = normalize_linked_path(&entry_path);
            let path_key = platform_path_key(&abs_path);
            discovered.push(ScannedFile {
                rel_path,
                abs_path,
                path_key,
                extension,
                limit,
                fingerprint,
            });
        }
    }

    Ok((canonical_root, discovered, skipped))
}

/// Open, double-check metadata, bounded-read, and re-verify handle stability for one linked file.
fn load_linked_file_checked(canonical_root: &Path, scanned: &ScannedFile) -> Option<LoadedFile> {
    let (_, mut file) = open_workspace_regular_file_no_follow_classified(
        canonical_root,
        &scanned.rel_path,
        "linked knowledge file",
    )
    .ok()?;
    let before = file.metadata().ok()?;
    if !before.is_file() || before.len() > scanned.limit {
        return None;
    }
    let mut bytes = Vec::with_capacity(before.len().min(65_536) as usize);
    file.by_ref()
        .take(scanned.limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > scanned.limit {
        return None;
    }
    let after = file.metadata().ok()?;
    if after.len() != before.len() || after.modified().ok() != before.modified().ok() {
        return None;
    }
    let fingerprint = fingerprint_from_meta(&after)?;
    let original_sha256 = format!("{:x}", Sha256::digest(&bytes));
    let mut updated_scanned = scanned.clone();
    updated_scanned.fingerprint = fingerprint;
    Some(LoadedFile {
        scanned: updated_scanned,
        original_sha256,
        bytes,
    })
}

fn convert_loaded_to_markdown(
    loaded: &LoadedFile,
    documents: Option<&ManagedDocumentProcessing>,
) -> Option<String> {
    if loaded.scanned.limit == TEXT_LIMIT {
        return decode_text(loaded.bytes.clone()).ok();
    }
    let documents = documents?;
    let temp = tempfile::tempdir().ok()?;
    let input_path = temp
        .path()
        .join(format!("linked.{}", loaded.scanned.extension));
    fs::write(&input_path, &loaded.bytes).ok()?;
    let stage = tempfile::tempdir().ok()?;
    let cancelled = AtomicBool::new(false);
    convert(
        &input_path,
        stage.path(),
        Some(documents),
        &cancelled,
        |_| Ok(()),
    )
    .ok()?;
    let (_, markdown_bytes) =
        read_workspace_file_no_follow(stage.path(), "content.md", MAX_OUTPUT_BYTES).ok()?;
    String::from_utf8(markdown_bytes).ok()
}

/// Reconcile a single linked folder against `KnowledgeStore` in read-only mode.
///
/// Recognizes external moves and renames via `(size, mtime_ms)` fingerprint or `original_sha256`
/// content hash: updates only `source.json` path metadata without re-chunking or invalidating
/// `content_hash` / `content_revision`.
pub fn reconcile_linked_folder(
    store: &KnowledgeStore,
    documents: Option<&ManagedDocumentProcessing>,
    kb_id: &str,
    folder_path: &Path,
) -> StoreResult<LinkedFolderScanSummary> {
    let (canonical_root, discovered, mut skipped) = scan_linked_directory(folder_path)?;
    let existing = store.list_linked_sources_for_folder(kb_id, &canonical_root)?;

    let mut existing_by_key: HashMap<String, LinkedSourceSnapshot> = HashMap::new();
    for snap in existing {
        existing_by_key.insert(platform_path_key(&snap.current_path), snap);
    }

    let mut summary = LinkedFolderScanSummary {
        kb_id: kb_id.to_owned(),
        path: canonical_root.clone(),
        ..Default::default()
    };

    let mut newly_seen: Vec<ScannedFile> = Vec::new();
    let mut seen_keys: BTreeSet<String> = BTreeSet::new();

    for scanned in discovered {
        seen_keys.insert(scanned.path_key.clone());
        if let Some(prev) = existing_by_key.get(&scanned.path_key) {
            if prev.file_fingerprint.as_ref() == Some(&scanned.fingerprint) {
                // On case-insensitive platforms (Windows / macOS), handle case-only renames
                // (e.g. `Notes.md` -> `notes.md`) without re-reading or re-chunking.
                if prev.current_path != scanned.abs_path {
                    store.relink_moved_source(
                        kb_id,
                        &prev.source_id,
                        &scanned.abs_path,
                        scanned.fingerprint,
                    )?;
                    summary.moved += 1;
                } else {
                    summary.unchanged += 1;
                }
                continue;
            }
            // Fingerprint changed at the same path -> read safely and compare content hash.
            let Some(loaded) = load_linked_file_checked(&canonical_root, &scanned) else {
                skipped += 1;
                continue;
            };
            if loaded.original_sha256 == prev.original_sha256 {
                if prev.current_path != loaded.scanned.abs_path {
                    store.relink_moved_source(
                        kb_id,
                        &prev.source_id,
                        &loaded.scanned.abs_path,
                        loaded.scanned.fingerprint,
                    )?;
                    summary.moved += 1;
                } else {
                    store.refresh_linked_fingerprint(
                        kb_id,
                        &prev.source_id,
                        loaded.scanned.fingerprint,
                    )?;
                    summary.unchanged += 1;
                }
                continue;
            }
            let Some(markdown) = convert_loaded_to_markdown(&loaded, documents) else {
                skipped += 1;
                continue;
            };
            store.publish_linked_source(
                kb_id,
                Some(&prev.source_id),
                &canonical_root,
                &loaded.scanned.abs_path,
                &loaded.scanned.extension,
                &loaded.original_sha256,
                loaded.scanned.fingerprint,
                &markdown,
            )?;
            summary.updated += 1;
            summary.rechunked += 1;
        } else {
            newly_seen.push(scanned);
        }
    }

    // Collect vanished existing sources not seen at their previous path key.
    let mut vanished: Vec<LinkedSourceSnapshot> = existing_by_key
        .into_iter()
        .filter(|(key, _)| !seen_keys.contains(key))
        .map(|(_, snap)| snap)
        .collect();
    vanished.sort_by(|a, b| a.source_id.cmp(&b.source_id));

    // Match newly seen files against vanished sources by fingerprint or original_sha256.
    let mut matched_vanished_ids: BTreeSet<String> = BTreeSet::new();
    for scanned in newly_seen {
        let Some(loaded) = load_linked_file_checked(&canonical_root, &scanned) else {
            skipped += 1;
            continue;
        };

        let matched_idx = vanished
            .iter()
            .position(|prev| {
                !matched_vanished_ids.contains(&prev.source_id)
                    && prev.file_fingerprint.as_ref() == Some(&loaded.scanned.fingerprint)
                    && prev.original_sha256 == loaded.original_sha256
            })
            .or_else(|| {
                vanished.iter().position(|prev| {
                    !matched_vanished_ids.contains(&prev.source_id)
                        && prev.original_sha256 == loaded.original_sha256
                })
            })
            .or_else(|| {
                vanished.iter().position(|prev| {
                    !matched_vanished_ids.contains(&prev.source_id)
                        && prev.file_fingerprint.as_ref() == Some(&loaded.scanned.fingerprint)
                })
            });

        if let Some(idx) = matched_idx {
            let prev = &vanished[idx];
            matched_vanished_ids.insert(prev.source_id.clone());
            store.relink_moved_source(
                kb_id,
                &prev.source_id,
                &loaded.scanned.abs_path,
                loaded.scanned.fingerprint,
            )?;
            summary.moved += 1;
            continue;
        }

        let Some(markdown) = convert_loaded_to_markdown(&loaded, documents) else {
            skipped += 1;
            continue;
        };
        store.publish_linked_source(
            kb_id,
            None,
            &canonical_root,
            &loaded.scanned.abs_path,
            &loaded.scanned.extension,
            &loaded.original_sha256,
            loaded.scanned.fingerprint,
            &markdown,
        )?;
        summary.added += 1;
        summary.rechunked += 1;
    }

    // Remove truly deleted linked sources from Store ONLY (never touches user files).
    for prev in vanished {
        if matched_vanished_ids.contains(&prev.source_id) {
            continue;
        }
        store.remove_source(kb_id, &prev.source_id)?;
        summary.removed += 1;
    }

    summary.skipped = skipped;
    Ok(summary)
}

#[derive(Default)]
struct WatchState {
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
    /// Keyed by `(kb_id, platform_path_key(folder_root))` -> canonical `PathBuf`.
    watched: BTreeMap<(String, String), PathBuf>,
    /// Ref-count per watched path key so two KBs watching the same folder share one OS watch.
    path_refs: HashMap<String, (PathBuf, usize)>,
}

/// Manages linked folder registration, manual scans, and debounced OS filesystem watching.
pub struct KnowledgeLinkedManager {
    store: Arc<KnowledgeStore>,
    documents: Option<ManagedDocumentProcessing>,
    debounce: Duration,
    reconcile_lock: Mutex<()>,
    watch_state: Mutex<WatchState>,
    shutdown: Arc<AtomicBool>,
}

impl KnowledgeLinkedManager {
    pub fn new(store: Arc<KnowledgeStore>, documents: Option<ManagedDocumentProcessing>) -> Self {
        Self::new_with_debounce(store, documents, DEFAULT_DEBOUNCE_WINDOW)
    }

    pub fn new_with_debounce(
        store: Arc<KnowledgeStore>,
        documents: Option<ManagedDocumentProcessing>,
        debounce: Duration,
    ) -> Self {
        Self {
            store,
            documents,
            debounce,
            reconcile_lock: Mutex::new(()),
            watch_state: Mutex::new(WatchState::default()),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the debounced filesystem watcher thread and attach watches for all existing linked folders.
    pub fn start(self: &Arc<Self>) -> StoreResult<()> {
        let mut state = self
            .watch_state
            .lock()
            .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?;
        if state.debouncer.is_some() {
            return Ok(());
        }

        let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();
        let debouncer = new_debouncer(self.debounce, None, tx)
            .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?;
        state.debouncer = Some(debouncer);
        drop(state);

        let manager = Arc::clone(self);
        let shutdown = Arc::clone(&self.shutdown);
        std::thread::Builder::new()
            .name("kb-linked-watcher".to_string())
            .spawn(move || {
                while !shutdown.load(Ordering::Acquire) {
                    let result = match rx.recv_timeout(Duration::from_millis(250)) {
                        Ok(result) => result,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    let events = match result {
                        Ok(events) => events,
                        Err(errors) => {
                            for err in errors {
                                ulog_warn!("[knowledge] linked watcher error: {}", err);
                            }
                            continue;
                        }
                    };
                    let changed_paths: Vec<PathBuf> = events
                        .into_iter()
                        .flat_map(|ev| ev.event.paths)
                        .map(|p| normalize_linked_path(&p))
                        .collect();
                    manager.handle_watch_batch(&changed_paths);
                }
            })
            .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?;

        self.sync_watches()?;
        ulog_info!(
            "[knowledge] linked folder watcher started (debounce={:?})",
            self.debounce
        );
        Ok(())
    }

    /// Stop the watcher thread and drop all OS watches cleanly.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        if let Ok(mut state) = self.watch_state.lock() {
            state.debouncer = None;
            state.watched.clear();
            state.path_refs.clear();
        }
    }

    /// Synchronize OS watches with all linked folders currently registered across `KnowledgeStore`.
    pub fn sync_watches(&self) -> StoreResult<()> {
        let kbs = self.store.list()?;
        let mut desired: BTreeMap<(String, String), PathBuf> = BTreeMap::new();
        for kb in kbs {
            for folder in kb.linked_folders {
                let normalized = normalize_linked_path(&folder.path);
                let key = platform_path_key(&normalized);
                desired.insert((kb.id.clone(), key), normalized);
            }
        }

        let mut state = self
            .watch_state
            .lock()
            .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?;

        let mut desired_paths: HashMap<String, (PathBuf, usize)> = HashMap::new();
        for ((_, path_key), path) in &desired {
            desired_paths
                .entry(path_key.clone())
                .and_modify(|(_, count)| *count += 1)
                .or_insert_with(|| (path.clone(), 1));
        }

        let prev_keys: Vec<String> = state.path_refs.keys().cloned().collect();
        for key in prev_keys {
            if !desired_paths.contains_key(&key) {
                if let Some((path, _)) = state.path_refs.remove(&key) {
                    if let Some(debouncer) = state.debouncer.as_mut() {
                        let _ = debouncer.unwatch(&path);
                    }
                }
            }
        }

        for (key, (path, count)) in desired_paths {
            let already_watched = state.path_refs.contains_key(&key);
            if !already_watched && path.is_dir() {
                if let Some(debouncer) = state.debouncer.as_mut() {
                    if let Err(err) = debouncer.watch(&path, RecursiveMode::Recursive) {
                        ulog_warn!("[knowledge] failed to watch linked folder: {}", err);
                        continue;
                    }
                }
            }
            state.path_refs.insert(key, (path, count));
        }

        state.watched = desired;
        Ok(())
    }

    fn handle_watch_batch(&self, changed_paths: &[PathBuf]) {
        if changed_paths.is_empty() {
            return;
        }
        let changed_keys: Vec<String> = changed_paths
            .iter()
            .map(|path| platform_path_key(path))
            .collect();

        let targets: Vec<(String, PathBuf)> = {
            let Ok(state) = self.watch_state.lock() else {
                return;
            };
            state
                .watched
                .iter()
                .filter(|((_, root_key), _)| {
                    let prefix = format!("{root_key}/");
                    changed_keys
                        .iter()
                        .any(|ev_key| ev_key == root_key || ev_key.starts_with(&prefix))
                })
                .map(|((kb_id, _), root_path)| (kb_id.clone(), root_path.clone()))
                .collect()
        };

        for (kb_id, root_path) in targets {
            let _guard = match self.reconcile_lock.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if let Err(code) =
                reconcile_linked_folder(&self.store, self.documents.as_ref(), &kb_id, &root_path)
            {
                if code != KnowledgeErrorCode::KbNotFound {
                    ulog_warn!(
                        "[knowledge] linked folder reconcile failed kb={} code={:?}",
                        kb_id,
                        code
                    );
                }
            }
        }
    }

    pub fn add_linked_folder(
        &self,
        kb_id: &str,
        folder_path: &Path,
    ) -> StoreResult<(LinkedFolder, LinkedFolderScanSummary)> {
        let folder = self.store.add_linked_folder(kb_id, folder_path)?;
        let summary = {
            let _guard = self
                .reconcile_lock
                .lock()
                .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?;
            reconcile_linked_folder(&self.store, self.documents.as_ref(), kb_id, &folder.path)?
        };
        let _ = self.sync_watches();
        Ok((folder, summary))
    }

    pub fn update_linked_folder(
        &self,
        kb_id: &str,
        folder_path: &Path,
        organize_allowed: Option<bool>,
        git_confirmed: Option<bool>,
    ) -> StoreResult<LinkedFolder> {
        self.store
            .update_linked_folder(kb_id, folder_path, organize_allowed, git_confirmed)
    }

    pub fn remove_linked_folder(&self, kb_id: &str, folder_path: &Path) -> StoreResult<usize> {
        let removed = {
            let _guard = self
                .reconcile_lock
                .lock()
                .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?;
            self.store.remove_linked_folder(kb_id, folder_path)?
        };
        let _ = self.sync_watches();
        Ok(removed)
    }

    pub fn scan_linked_folder(
        &self,
        kb_id: &str,
        folder_path: &Path,
    ) -> StoreResult<LinkedFolderScanSummary> {
        let _guard = self
            .reconcile_lock
            .lock()
            .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?;
        reconcile_linked_folder(&self.store, self.documents.as_ref(), kb_id, folder_path)
    }
}
