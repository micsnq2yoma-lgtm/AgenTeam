//! K16 local knowledge embedding model pack manager (`KnowledgeModelPackManager`).
//!
//! Reuses the speech model pack trust & publication model:
//! - Compiled-in source lock (`embedding-model-pack-source-lock.json`)
//! - Detached Minisign signature verification via `crate::resource_signature_core`
//! - Strict official download host pinning (`download.myagents.io`, HTTPS only)
//! - SHA-256 + byte-size verification for every file in the manifest
//! - Private staging directory (`private/<uuid>/`) cleaned up on startup
//! - Atomic `active.json` pointer publication and clean uninstall (BM25 fallback)

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::knowledge::types::KnowledgeErrorCode;
use crate::resource_signature;
use crate::workspace_files::path_safety::{
    ensure_workspace_directory_no_follow, metadata_is_link_like,
    open_workspace_regular_file_no_follow_classified, read_workspace_file_no_follow,
    write_workspace_file_no_follow,
};
use KnowledgeErrorCode::{KbAccessDenied, KbInvalidInput, KbLimitExceeded, KbStoreUnavailable};

pub const EMBEDDING_MODEL_PACK_SOURCE_LOCK: &str =
    include_str!("embedding-model-pack-source-lock.json");

const ACTIVE_POINTER_SCHEMA_VERSION: u32 = 1;
const MANIFEST_SCHEMA_VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 16 * 1024;
const MAX_ACTIVE_POINTER_BYTES: u64 = 32 * 1024;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingSourceLock {
    pub schema_version: u32,
    pub pack_id: String,
    pub pack_revision: String,
    pub display_name: String,
    pub dimension: usize,
    pub max_tokens: usize,
    pub pooling: String,
    pub normalize: bool,
    pub download_host: String,
    pub manifest_base_url: String,
    pub download_hard_limit_bytes: u64,
    pub signature_policy: EmbeddingSignaturePolicy,
    pub expected_files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingSignaturePolicy {
    pub algorithm: String,
    pub trust_root: String,
    pub detached_signature_required: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeModelPackStatusKind {
    NotInstalled,
    Checking,
    Downloading,
    Verifying,
    Installing,
    Removing,
    Ready,
    UpdateAvailable,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeModelPackStatus {
    pub status: KnowledgeModelPackStatusKind,
    pub usable: bool,
    pub pack_id: String,
    pub display_name: String,
    pub dimension: usize,
    pub max_tokens: usize,
    pub active_revision: Option<String>,
    pub available_revision: String,
    pub downloaded_bytes: u64,
    pub total_download_bytes: u64,
    pub installed_model_bytes: u64,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivatedEmbeddingPack {
    pub pack_id: String,
    pub revision: String,
    pub dimension: usize,
    pub max_tokens: usize,
    pub pooling: String,
    pub normalize: bool,
    pub pack_dir: PathBuf,
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedEmbeddingManifest {
    pub schema_version: u32,
    pub pack_id: String,
    pub pack_revision: String,
    pub dimension: usize,
    pub max_tokens: usize,
    pub pooling: String,
    pub normalize: bool,
    pub files: Vec<SignedEmbeddingFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedEmbeddingFileEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivePointer {
    schema_version: u32,
    pack_id: String,
    pack_revision: String,
    directory_name: String,
    manifest_sha256: String,
    manifest_signature: String,
    activated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Idle,
    Downloading,
    Verifying,
    Installing,
    Removing,
}

#[cfg(test)]
pub(crate) type SignatureVerifier =
    Arc<dyn Fn(&[u8], &str) -> Result<(), String> + Send + Sync>;

struct PackState {
    operation: Operation,
    active: Option<ActivatedEmbeddingPack>,
    active_pointer: Option<ActivePointer>,
    downloaded_bytes: u64,
    total_download_bytes: u64,
    installed_model_bytes: u64,
    last_error_code: Option<String>,
}

pub struct KnowledgeModelPackManager {
    models_root: PathBuf,
    source_lock: EmbeddingSourceLock,
    #[cfg(test)]
    test_verifier: Option<SignatureVerifier>,
    state: Mutex<PackState>,
}

pub fn parse_source_lock() -> Result<EmbeddingSourceLock, &'static str> {
    let lock: EmbeddingSourceLock = serde_json::from_str(EMBEDDING_MODEL_PACK_SOURCE_LOCK)
        .map_err(|_| "KB_MODEL_SOURCE_LOCK_INVALID")?;
    if lock.schema_version != 1
        || lock.pack_id.trim().is_empty()
        || lock.pack_revision.trim().is_empty()
        || lock.dimension == 0
        || lock.max_tokens == 0
        || lock.download_hard_limit_bytes == 0
        || lock.signature_policy.algorithm != "minisign"
        || !lock.signature_policy.detached_signature_required
        || lock.expected_files.is_empty()
    {
        return Err("KB_MODEL_SOURCE_LOCK_INVALID");
    }
    for rel in &lock.expected_files {
        validate_pack_relative_path(rel)?;
    }
    Ok(lock)
}

fn validate_pack_relative_path(rel: &str) -> Result<PathBuf, &'static str> {
    if rel.is_empty() || rel.contains('\\') || rel.starts_with('/') {
        return Err("KB_MODEL_UNSAFE_PATH");
    }
    let path = Path::new(rel);
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let s = part.to_str().ok_or("KB_MODEL_UNSAFE_PATH")?;
                if s.is_empty() || s == "." || s == ".." {
                    return Err("KB_MODEL_UNSAFE_PATH");
                }
                normalized.push(s);
            }
            _ => return Err("KB_MODEL_UNSAFE_PATH"),
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err("KB_MODEL_UNSAFE_PATH");
    }
    Ok(normalized)
}

fn is_safe_revision_dir_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !name.starts_with('.')
        && !name.contains("..")
}

fn is_valid_sha256_hex(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sha256_file_no_follow(root: &Path, rel: &str, expected_size: u64) -> Result<String, &'static str> {
    let (_, mut file) = open_workspace_regular_file_no_follow_classified(root, rel, "model pack file")
        .map_err(|_| "KB_MODEL_FILE_MISSING")?;
    let meta = file.metadata().map_err(|_| "KB_MODEL_FILE_MISSING")?;
    if !meta.is_file() || meta.len() != expected_size {
        return Err("KB_MODEL_FILE_SIZE_MISMATCH");
    }
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut read_total = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|_| "KB_MODEL_FILE_READ_ERROR")?;
        if n == 0 {
            break;
        }
        read_total = read_total.saturating_add(n as u64);
        if read_total > expected_size {
            return Err("KB_MODEL_FILE_SIZE_MISMATCH");
        }
        hasher.update(&buf[..n]);
    }
    if read_total != expected_size {
        return Err("KB_MODEL_FILE_SIZE_MISMATCH");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_official_download_url(raw_url: &str, allowed_host: &str) -> Result<reqwest::Url, &'static str> {
    let url = reqwest::Url::parse(raw_url).map_err(|_| "KB_MODEL_DOWNLOAD_URL_INVALID")?;
    if url.scheme() != "https" {
        return Err("KB_MODEL_DOWNLOAD_URL_INVALID");
    }
    if url.host_str() != Some(allowed_host) {
        return Err("KB_MODEL_DOWNLOAD_HOST_REJECTED");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("KB_MODEL_DOWNLOAD_URL_INVALID");
    }
    Ok(url)
}

fn cleanup_private_dir(private_dir: &Path) {
    if let Ok(entries) = fs::read_dir(private_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(meta) = fs::symlink_metadata(&path) {
                if meta.is_dir() && !metadata_is_link_like(&meta) {
                    let _ = fs::remove_dir_all(&path);
                } else {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}

impl KnowledgeModelPackManager {
    pub fn initialize(models_root: PathBuf) -> Result<Arc<Self>, String> {
        let source_lock = parse_source_lock().map_err(str::to_owned)?;
        fs::create_dir_all(&models_root).map_err(|_| "KB_MODEL_DIR_CREATE_FAILED".to_string())?;
        let _ = ensure_workspace_directory_no_follow(&models_root, "packs")
            .map_err(|_| "KB_MODEL_DIR_CREATE_FAILED".to_string())?;
        let _ = ensure_workspace_directory_no_follow(&models_root, "private")
            .map_err(|_| "KB_MODEL_DIR_CREATE_FAILED".to_string())?;
        cleanup_private_dir(&models_root.join("private"));

        let manager = Arc::new(Self {
            models_root,
            source_lock,
            #[cfg(test)]
            test_verifier: None,
            state: Mutex::new(PackState {
                operation: Operation::Idle,
                active: None,
                active_pointer: None,
                downloaded_bytes: 0,
                total_download_bytes: 0,
                installed_model_bytes: 0,
                last_error_code: None,
            }),
        });
        manager.reload_active_from_disk();
        Ok(manager)
    }

    #[cfg(test)]
    pub(crate) fn initialize_with_test_verifier(
        models_root: PathBuf,
        verifier: SignatureVerifier,
    ) -> Result<Arc<Self>, String> {
        let source_lock = parse_source_lock().map_err(str::to_owned)?;
        fs::create_dir_all(&models_root).map_err(|_| "KB_MODEL_DIR_CREATE_FAILED".to_string())?;
        let _ = ensure_workspace_directory_no_follow(&models_root, "packs")
            .map_err(|_| "KB_MODEL_DIR_CREATE_FAILED".to_string())?;
        let _ = ensure_workspace_directory_no_follow(&models_root, "private")
            .map_err(|_| "KB_MODEL_DIR_CREATE_FAILED".to_string())?;
        cleanup_private_dir(&models_root.join("private"));

        let manager = Arc::new(Self {
            models_root,
            source_lock,
            test_verifier: Some(verifier),
            state: Mutex::new(PackState {
                operation: Operation::Idle,
                active: None,
                active_pointer: None,
                downloaded_bytes: 0,
                total_download_bytes: 0,
                installed_model_bytes: 0,
                last_error_code: None,
            }),
        });
        manager.reload_active_from_disk();
        Ok(manager)
    }

    fn verify_signature(&self, bytes: &[u8], signature_value: &str) -> Result<(), &'static str> {
        #[cfg(test)]
        if let Some(verifier) = &self.test_verifier {
            return verifier(bytes, signature_value).map_err(|_| "KB_MODEL_SIGNATURE_INVALID");
        }
        resource_signature::verify_minisign_bytes(
            bytes,
            signature_value,
            "knowledge embedding manifest",
        )
        .map_err(|_| "KB_MODEL_SIGNATURE_INVALID")
    }

    fn validate_manifest(&self, manifest: &SignedEmbeddingManifest) -> Result<u64, &'static str> {
        if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err("KB_MODEL_MANIFEST_SCHEMA_UNSUPPORTED");
        }
        if manifest.pack_id != self.source_lock.pack_id {
            return Err("KB_MODEL_PACK_ID_MISMATCH");
        }
        if !is_safe_revision_dir_name(&manifest.pack_revision) {
            return Err("KB_MODEL_REVISION_INVALID");
        }
        if manifest.dimension != self.source_lock.dimension
            || manifest.max_tokens != self.source_lock.max_tokens
            || manifest.pooling != self.source_lock.pooling
            || manifest.normalize != self.source_lock.normalize
        {
            return Err("KB_MODEL_PROFILE_MISMATCH");
        }
        let expected: BTreeSet<&str> = self
            .source_lock
            .expected_files
            .iter()
            .map(String::as_str)
            .collect();
        let mut seen = BTreeSet::new();
        let mut total_size = 0u64;
        for entry in &manifest.files {
            validate_pack_relative_path(&entry.path)?;
            if !is_valid_sha256_hex(&entry.sha256) {
                return Err("KB_MODEL_MANIFEST_HASH_INVALID");
            }
            if !seen.insert(entry.path.as_str()) {
                return Err("KB_MODEL_MANIFEST_DUPLICATE_FILE");
            }
            total_size = total_size.saturating_add(entry.size);
            if total_size > self.source_lock.download_hard_limit_bytes {
                return Err("KB_MODEL_SIZE_LIMIT_EXCEEDED");
            }
        }
        if seen != expected {
            return Err("KB_MODEL_INVENTORY_MISMATCH");
        }
        Ok(total_size)
    }

    fn verify_pack_dir(
        &self,
        pack_dir: &Path,
        manifest: &SignedEmbeddingManifest,
    ) -> Result<ActivatedEmbeddingPack, &'static str> {
        let meta = fs::symlink_metadata(pack_dir).map_err(|_| "KB_MODEL_PACK_DIR_MISSING")?;
        if !meta.is_dir() || metadata_is_link_like(&meta) {
            return Err("KB_MODEL_UNSAFE_PATH");
        }
        let mut file_map = HashMap::new();
        for entry in &manifest.files {
            let actual_hash = sha256_file_no_follow(pack_dir, &entry.path, entry.size)?;
            if actual_hash != entry.sha256 {
                return Err("KB_MODEL_FILE_HASH_MISMATCH");
            }
            file_map.insert(entry.path.as_str(), pack_dir.join(&entry.path));
        }
        let model_path = file_map
            .get("models/embedding/model.int8.onnx")
            .cloned()
            .ok_or("KB_MODEL_INVENTORY_MISMATCH")?;
        let tokenizer_path = file_map
            .get("models/embedding/tokenizer.json")
            .cloned()
            .ok_or("KB_MODEL_INVENTORY_MISMATCH")?;
        Ok(ActivatedEmbeddingPack {
            pack_id: manifest.pack_id.clone(),
            revision: manifest.pack_revision.clone(),
            dimension: manifest.dimension,
            max_tokens: manifest.max_tokens,
            pooling: manifest.pooling.clone(),
            normalize: manifest.normalize,
            pack_dir: pack_dir.to_path_buf(),
            model_path,
            tokenizer_path,
            manifest_path: pack_dir.join("manifest.json"),
        })
    }

    fn reload_active_from_disk(&self) {
        let active_rel = "active.json";
        let active_bytes = match read_workspace_file_no_follow(
            &self.models_root,
            active_rel,
            MAX_ACTIVE_POINTER_BYTES,
        ) {
            Ok((_, bytes)) => bytes,
            Err(_) => {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                state.active = None;
                state.active_pointer = None;
                state.installed_model_bytes = 0;
                state.downloaded_bytes = 0;
                state.last_error_code = None;
                return;
            }
        };

        let result: Result<(ActivatedEmbeddingPack, ActivePointer, u64), &'static str> = (|| {
            let pointer: ActivePointer = serde_json::from_slice(&active_bytes)
                .map_err(|_| "KB_MODEL_ACTIVE_POINTER_CORRUPT")?;
            if pointer.schema_version != ACTIVE_POINTER_SCHEMA_VERSION
                || pointer.pack_id != self.source_lock.pack_id
                || !is_safe_revision_dir_name(&pointer.directory_name)
                || !is_valid_sha256_hex(&pointer.manifest_sha256)
            {
                return Err("KB_MODEL_ACTIVE_POINTER_CORRUPT");
            }
            let manifest_rel = format!("packs/{}/manifest.json", pointer.directory_name);
            let (_, manifest_bytes) = read_workspace_file_no_follow(
                &self.models_root,
                &manifest_rel,
                MAX_MANIFEST_BYTES,
            )
            .map_err(|_| "KB_MODEL_MANIFEST_MISSING")?;
            if sha256_hex_bytes(&manifest_bytes) != pointer.manifest_sha256 {
                return Err("KB_MODEL_MANIFEST_HASH_MISMATCH");
            }
            self.verify_signature(&manifest_bytes, &pointer.manifest_signature)?;
            let manifest: SignedEmbeddingManifest = serde_json::from_slice(&manifest_bytes)
                .map_err(|_| "KB_MODEL_MANIFEST_CORRUPT")?;
            if manifest.pack_revision != pointer.pack_revision {
                return Err("KB_MODEL_ACTIVE_POINTER_CORRUPT");
            }
            let total_size = self.validate_manifest(&manifest)?;
            let pack_dir = self.models_root.join("packs").join(&pointer.directory_name);
            let activated = self.verify_pack_dir(&pack_dir, &manifest)?;
            Ok((activated, pointer, total_size))
        })();

        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok((activated, pointer, total_size)) => {
                state.active = Some(activated);
                state.active_pointer = Some(pointer);
                state.installed_model_bytes = total_size;
                state.total_download_bytes = total_size;
                state.downloaded_bytes = total_size;
                state.last_error_code = None;
            }
            Err(code) => {
                state.active = None;
                state.active_pointer = None;
                state.installed_model_bytes = 0;
                state.last_error_code = Some(code.to_string());
            }
        }
    }

    pub fn active_pack(&self) -> Option<ActivatedEmbeddingPack> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active
            .clone()
    }

    pub fn status(&self) -> KnowledgeModelPackStatus {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let status = match state.operation {
            Operation::Downloading => KnowledgeModelPackStatusKind::Downloading,
            Operation::Verifying => KnowledgeModelPackStatusKind::Verifying,
            Operation::Installing => KnowledgeModelPackStatusKind::Installing,
            Operation::Removing => KnowledgeModelPackStatusKind::Removing,
            Operation::Idle if state.last_error_code.is_some() => {
                KnowledgeModelPackStatusKind::Error
            }
            Operation::Idle
                if state
                    .active_pointer
                    .as_ref()
                    .is_some_and(|p| p.pack_revision != self.source_lock.pack_revision) =>
            {
                KnowledgeModelPackStatusKind::UpdateAvailable
            }
            Operation::Idle if state.active.is_some() => KnowledgeModelPackStatusKind::Ready,
            Operation::Idle => KnowledgeModelPackStatusKind::NotInstalled,
        };
        KnowledgeModelPackStatus {
            status,
            usable: state.active.is_some() && state.operation == Operation::Idle,
            pack_id: self.source_lock.pack_id.clone(),
            display_name: self.source_lock.display_name.clone(),
            dimension: self.source_lock.dimension,
            max_tokens: self.source_lock.max_tokens,
            active_revision: state.active.as_ref().map(|a| a.revision.clone()),
            available_revision: self.source_lock.pack_revision.clone(),
            downloaded_bytes: state.downloaded_bytes,
            total_download_bytes: state.total_download_bytes,
            installed_model_bytes: state.installed_model_bytes,
            last_error_code: state.last_error_code.clone(),
        }
    }

    /// Install and atomically activate a signed embedding model pack from a local directory
    /// containing `manifest.json`, `manifest.json.minisig`, and all manifest-listed files.
    /// Enforces the exact same Minisign + source-lock + SHA-256 checks as remote installs.
    pub fn install_from_local_bundle(
        &self,
        bundle_dir: &Path,
    ) -> Result<KnowledgeModelPackStatus, KnowledgeErrorCode> {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.operation != Operation::Idle {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            state.operation = Operation::Verifying;
            state.last_error_code = None;
        }

        let outcome = self.install_from_local_bundle_inner(bundle_dir);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.operation = Operation::Idle;
        match outcome {
            Ok((activated, pointer, total_size)) => {
                state.active = Some(activated);
                state.active_pointer = Some(pointer);
                state.installed_model_bytes = total_size;
                state.total_download_bytes = total_size;
                state.downloaded_bytes = total_size;
                state.last_error_code = None;
                drop(state);
                Ok(self.status())
            }
            Err((err_code, detail)) => {
                state.last_error_code = Some(detail.to_string());
                drop(state);
                Err(err_code)
            }
        }
    }

    fn install_from_local_bundle_inner(
        &self,
        bundle_dir: &Path,
    ) -> Result<(ActivatedEmbeddingPack, ActivePointer, u64), (KnowledgeErrorCode, &'static str)>
    {
        let bundle_meta =
            fs::symlink_metadata(bundle_dir).map_err(|_| (KbInvalidInput, "KB_MODEL_BUNDLE_MISSING"))?;
        if !bundle_meta.is_dir() || metadata_is_link_like(&bundle_meta) {
            return Err((KbInvalidInput, "KB_MODEL_UNSAFE_PATH"));
        }

        let (_, manifest_bytes) =
            read_workspace_file_no_follow(bundle_dir, "manifest.json", MAX_MANIFEST_BYTES)
                .map_err(|_| (KbInvalidInput, "KB_MODEL_MANIFEST_MISSING"))?;
        let (_, sig_bytes) =
            read_workspace_file_no_follow(bundle_dir, "manifest.json.minisig", MAX_SIGNATURE_BYTES)
                .map_err(|_| (KbAccessDenied, "KB_MODEL_SIGNATURE_MISSING"))?;
        let signature_text = std::str::from_utf8(&sig_bytes)
            .map_err(|_| (KbAccessDenied, "KB_MODEL_SIGNATURE_INVALID"))?
            .trim()
            .to_string();

        self.verify_signature(&manifest_bytes, &signature_text)
            .map_err(|code| (KbAccessDenied, code))?;

        let manifest: SignedEmbeddingManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| (KbInvalidInput, "KB_MODEL_MANIFEST_CORRUPT"))?;
        let total_size = self
            .validate_manifest(&manifest)
            .map_err(|code| (KbInvalidInput, code))?;

        let stage_id = Uuid::new_v4().to_string();
        let stage_dir = self.models_root.join("private").join(&stage_id);
        fs::create_dir_all(&stage_dir)
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_CREATE_FAILED"))?;

        let stage_result = (|| -> Result<ActivatedEmbeddingPack, (KnowledgeErrorCode, &'static str)> {
            write_workspace_file_no_follow(&stage_dir, "manifest.json", &manifest_bytes)
                .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
            for entry in &manifest.files {
                let (_, mut src) = open_workspace_regular_file_no_follow_classified(
                    bundle_dir,
                    &entry.path,
                    "model pack bundle file",
                )
                .map_err(|_| (KbInvalidInput, "KB_MODEL_FILE_MISSING"))?;
                let meta = src
                    .metadata()
                    .map_err(|_| (KbInvalidInput, "KB_MODEL_FILE_MISSING"))?;
                if !meta.is_file() || meta.len() != entry.size {
                    return Err((KbInvalidInput, "KB_MODEL_FILE_SIZE_MISMATCH"));
                }
                let dst_path = stage_dir.join(&entry.path);
                if let Some(parent) = dst_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
                }
                let mut dst = File::create(&dst_path)
                    .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
                let mut hasher = Sha256::new();
                let mut buf = [0u8; 64 * 1024];
                let mut copied = 0u64;
                loop {
                    let n = src
                        .read(&mut buf)
                        .map_err(|_| (KbInvalidInput, "KB_MODEL_FILE_READ_ERROR"))?;
                    if n == 0 {
                        break;
                    }
                    copied = copied.saturating_add(n as u64);
                    if copied > entry.size {
                        return Err((KbInvalidInput, "KB_MODEL_FILE_SIZE_MISMATCH"));
                    }
                    hasher.update(&buf[..n]);
                    dst.write_all(&buf[..n])
                        .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
                }
                dst.sync_all()
                    .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
                if copied != entry.size || format!("{:x}", hasher.finalize()) != entry.sha256 {
                    return Err((KbInvalidInput, "KB_MODEL_FILE_HASH_MISMATCH"));
                }
            }
            self.verify_pack_dir(&stage_dir, &manifest)
                .map_err(|code| (KbInvalidInput, code))
        })();

        if let Err(err) = stage_result {
            let _ = fs::remove_dir_all(&stage_dir);
            return Err(err);
        }

        self.publish_staged_pack(
            &stage_dir,
            &manifest,
            &manifest_bytes,
            signature_text,
            total_size,
        )
    }

    /// Download and install the configured revision from the official content-addressed R2 host.
    pub async fn install_remote(&self) -> Result<KnowledgeModelPackStatus, KnowledgeErrorCode> {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.operation != Operation::Idle {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            state.operation = Operation::Downloading;
            state.downloaded_bytes = 0;
            state.total_download_bytes = 0;
            state.last_error_code = None;
        }

        let outcome = self.install_remote_inner().await;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.operation = Operation::Idle;
        match outcome {
            Ok((activated, pointer, total_size)) => {
                state.active = Some(activated);
                state.active_pointer = Some(pointer);
                state.installed_model_bytes = total_size;
                state.total_download_bytes = total_size;
                state.downloaded_bytes = total_size;
                state.last_error_code = None;
                drop(state);
                Ok(self.status())
            }
            Err((err_code, detail)) => {
                state.last_error_code = Some(detail.to_string());
                drop(state);
                Err(err_code)
            }
        }
    }

    async fn install_remote_inner(
        &self,
    ) -> Result<(ActivatedEmbeddingPack, ActivePointer, u64), (KnowledgeErrorCode, &'static str)>
    {
        // External R2 download host (`download.myagents.io`), intentionally uses system proxy
        // per `clippy.toml` and disables redirects.
        #[allow(clippy::disallowed_methods)]
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_HTTP_CLIENT_FAILED"))?;

        let manifest_url = format!(
            "{}/{}/manifest.json",
            self.source_lock.manifest_base_url.trim_end_matches('/'),
            self.source_lock.pack_revision
        );
        let sig_url = format!("{manifest_url}.minisig");
        let manifest_url = validate_official_download_url(&manifest_url, &self.source_lock.download_host)
            .map_err(|c| (KbAccessDenied, c))?;
        let sig_url = validate_official_download_url(&sig_url, &self.source_lock.download_host)
            .map_err(|c| (KbAccessDenied, c))?;

        let manifest_bytes = fetch_bounded_bytes(&client, manifest_url, MAX_MANIFEST_BYTES)
            .await
            .map_err(|c| (KbStoreUnavailable, c))?;
        let sig_bytes = fetch_bounded_bytes(&client, sig_url, MAX_SIGNATURE_BYTES)
            .await
            .map_err(|c| (KbStoreUnavailable, c))?;
        let signature_text = std::str::from_utf8(&sig_bytes)
            .map_err(|_| (KbAccessDenied, "KB_MODEL_SIGNATURE_INVALID"))?
            .trim()
            .to_string();

        self.verify_signature(&manifest_bytes, &signature_text)
            .map_err(|c| (KbAccessDenied, c))?;

        let manifest: SignedEmbeddingManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| (KbInvalidInput, "KB_MODEL_MANIFEST_CORRUPT"))?;
        let total_size = self
            .validate_manifest(&manifest)
            .map_err(|c| (KbInvalidInput, c))?;
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.total_download_bytes = total_size;
        }

        let stage_id = Uuid::new_v4().to_string();
        let stage_dir = self.models_root.join("private").join(&stage_id);
        fs::create_dir_all(&stage_dir)
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_CREATE_FAILED"))?;
        write_workspace_file_no_follow(&stage_dir, "manifest.json", &manifest_bytes)
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;

        for entry in &manifest.files {
            let raw_url = entry
                .url
                .as_deref()
                .ok_or((KbInvalidInput, "KB_MODEL_DOWNLOAD_URL_MISSING"))?;
            let file_url = validate_official_download_url(raw_url, &self.source_lock.download_host)
                .map_err(|c| (KbAccessDenied, c))?;
            let dst_path = stage_dir.join(&entry.path);
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
            }
            let download_res = self
                .download_file_verified(&client, file_url, &dst_path, entry.size, &entry.sha256)
                .await;
            if let Err(err) = download_res {
                let _ = fs::remove_dir_all(&stage_dir);
                return Err(err);
            }
        }

        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.operation = Operation::Installing;
        }
        self.publish_staged_pack(
            &stage_dir,
            &manifest,
            &manifest_bytes,
            signature_text,
            total_size,
        )
    }

    async fn download_file_verified(
        &self,
        client: &reqwest::Client,
        url: reqwest::Url,
        dst_path: &Path,
        expected_size: u64,
        expected_sha256: &str,
    ) -> Result<(), (KnowledgeErrorCode, &'static str)> {
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_DOWNLOAD_FAILED"))?;
        if !resp.status().is_success() {
            return Err((KbStoreUnavailable, "KB_MODEL_DOWNLOAD_HTTP_ERROR"));
        }
        let mut dst = File::create(dst_path)
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| (KbStoreUnavailable, "KB_MODEL_DOWNLOAD_FAILED"))?;
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > expected_size {
                return Err((KbLimitExceeded, "KB_MODEL_FILE_SIZE_MISMATCH"));
            }
            hasher.update(&chunk);
            dst.write_all(&chunk)
                .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.downloaded_bytes = state.downloaded_bytes.saturating_add(chunk.len() as u64);
        }
        dst.sync_all()
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_STAGE_WRITE_FAILED"))?;
        if downloaded != expected_size || format!("{:x}", hasher.finalize()) != expected_sha256 {
            return Err((KbInvalidInput, "KB_MODEL_FILE_HASH_MISMATCH"));
        }
        Ok(())
    }

    fn publish_staged_pack(
        &self,
        stage_dir: &Path,
        manifest: &SignedEmbeddingManifest,
        manifest_bytes: &[u8],
        signature_text: String,
        total_size: u64,
    ) -> Result<(ActivatedEmbeddingPack, ActivePointer, u64), (KnowledgeErrorCode, &'static str)>
    {
        let dir_name = manifest.pack_revision.clone();
        let target_dir = self.models_root.join("packs").join(&dir_name);
        if target_dir.exists() {
            let backup_dir = self
                .models_root
                .join("private")
                .join(format!("old-{}", Uuid::new_v4()));
            fs::rename(&target_dir, &backup_dir)
                .map_err(|_| (KbStoreUnavailable, "KB_MODEL_PUBLISH_FAILED"))?;
            if let Err(err) = fs::rename(stage_dir, &target_dir) {
                let _ = fs::rename(&backup_dir, &target_dir);
                let _ = err;
                return Err((KbStoreUnavailable, "KB_MODEL_PUBLISH_FAILED"));
            }
            let _ = fs::remove_dir_all(&backup_dir);
        } else {
            fs::rename(stage_dir, &target_dir)
                .map_err(|_| (KbStoreUnavailable, "KB_MODEL_PUBLISH_FAILED"))?;
        }

        let pointer = ActivePointer {
            schema_version: ACTIVE_POINTER_SCHEMA_VERSION,
            pack_id: manifest.pack_id.clone(),
            pack_revision: manifest.pack_revision.clone(),
            directory_name: dir_name,
            manifest_sha256: sha256_hex_bytes(manifest_bytes),
            manifest_signature: signature_text,
            activated_at: Utc::now().to_rfc3339(),
        };
        let pointer_bytes = serde_json::to_vec_pretty(&pointer)
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_POINTER_ENCODE_FAILED"))?;
        write_workspace_file_no_follow(&self.models_root, "active.json", &pointer_bytes)
            .map_err(|_| (KbStoreUnavailable, "KB_MODEL_POINTER_WRITE_FAILED"))?;

        let activated = self
            .verify_pack_dir(&target_dir, manifest)
            .map_err(|c| (KbStoreUnavailable, c))?;
        Ok((activated, pointer, total_size))
    }

    /// Uninstall all installed embedding model packs and clear `active.json`.
    /// Retrieval automatically degrades to BM25 without errors.
    pub fn remove(&self) -> Result<KnowledgeModelPackStatus, KnowledgeErrorCode> {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.operation != Operation::Idle {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            state.operation = Operation::Removing;
        }

        let active_path = self.models_root.join("active.json");
        if active_path.exists() {
            let _ = fs::remove_file(&active_path);
        }
        let packs_dir = self.models_root.join("packs");
        if packs_dir.exists() {
            let _ = fs::remove_dir_all(&packs_dir);
        }
        let _ = fs::create_dir_all(&packs_dir);
        cleanup_private_dir(&self.models_root.join("private"));

        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.operation = Operation::Idle;
        state.active = None;
        state.active_pointer = None;
        state.downloaded_bytes = 0;
        state.total_download_bytes = 0;
        state.installed_model_bytes = 0;
        state.last_error_code = None;
        drop(state);
        Ok(self.status())
    }
}

async fn fetch_bounded_bytes(
    client: &reqwest::Client,
    url: reqwest::Url,
    max_bytes: u64,
) -> Result<Vec<u8>, &'static str> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|_| "KB_MODEL_DOWNLOAD_FAILED")?;
    if !resp.status().is_success() {
        return Err("KB_MODEL_DOWNLOAD_HTTP_ERROR");
    }
    let mut out = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "KB_MODEL_DOWNLOAD_FAILED")?;
        if (out.len() as u64).saturating_add(chunk.len() as u64) > max_bytes {
            return Err("KB_MODEL_MANIFEST_TOO_LARGE");
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}
