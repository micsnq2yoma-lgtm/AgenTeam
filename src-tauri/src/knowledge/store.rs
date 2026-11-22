use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

mod chunks;
pub use chunks::ResolvedFragment;
pub mod ingest;

use crate::durable_fs::{rename_directory_noreplace, sync_directory};
use crate::utils::file_lock::{with_file_lock_blocking, FileLockOptions};
use crate::workspace_files::path_safety::{
    ensure_workspace_directory_no_follow, read_workspace_file_no_follow,
    write_workspace_file_no_follow,
};

use super::types::{
    create_knowledge_base_id, create_knowledge_source_id, is_knowledge_base_id,
    is_knowledge_source_id, parse_knowledge_ref, KnowledgeBaseId, KnowledgeErrorCode,
    KnowledgeMetadataField, KnowledgeMetadataOrigin, KnowledgeRef, KnowledgeSourceId,
};

use KnowledgeErrorCode::{
    KbFieldLocked, KbInvalidInput, KbLimitExceeded, KbNotFound, KbStoreCorrupt, KbStoreUnavailable,
};

pub type StoreResult<T> = Result<T, KnowledgeErrorCode>;

const REGISTRY: &str = "kbs.json";
const REMOVED: &str = ".removed-";
const STAGING: &str = ".staging-";
const MAX_JSON_BYTES: u64 = 2 * 1024 * 1024;
const DEFAULT_CURATION_INTERVAL: u32 = 4320;
/// User-editable curation rules seeded into every new library. The curator
/// Skill owns the template text so the rules and the workflow that reads them
/// ship together; the Store only copies them before the library is published.
pub(crate) const LIBRARY_TEMPLATES: [(&str, &str); 2] = [
    (
        "ORGANIZE_RULES.md",
        include_str!(
            "../../../bundled-skills/agenteam-knowledge-curator/references/ORGANIZE_RULES.md"
        ),
    ),
    (
        "WIKI_SCHEMA.md",
        include_str!(
            "../../../bundled-skills/agenteam-knowledge-curator/references/WIKI_SCHEMA.md"
        ),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CurationMode {
    Auto,
    Manual,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentWriteAccess {
    CuratorOnly,
    BoundSessions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeBase {
    pub id: KnowledgeBaseId,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curator_agent_id: Option<String>,
    pub curation_mode: CurationMode,
    pub curation_interval_minutes: u32,
    pub agent_write_access: AgentWriteAccess,
    pub wiki_enabled: bool,
    pub pending_categories: Vec<String>,
}

/// Missing fields retain their values. An explicit null clears the curator.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeBasePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_curator")]
    pub curator_agent_id: Option<Option<String>>,
    pub curation_mode: Option<CurationMode>,
    pub curation_interval_minutes: Option<u32>,
    pub agent_write_access: Option<AgentWriteAccess>,
    pub wiki_enabled: Option<bool>,
    pub pending_categories: Option<Vec<String>>,
}

fn deserialize_curator<'de, D>(value: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(value).map(Some)
}

impl KnowledgeBase {
    fn from_patch(id: KnowledgeBaseId, patch: KnowledgeBasePatch) -> StoreResult<Self> {
        let default_mode = if patch.curator_agent_id.as_ref().is_some_and(Option::is_some) {
            CurationMode::Auto
        } else {
            CurationMode::Manual
        };
        let mut kb = Self {
            id,
            name: String::new(),
            description: String::new(),
            curator_agent_id: None,
            curation_mode: default_mode,
            curation_interval_minutes: DEFAULT_CURATION_INTERVAL,
            agent_write_access: AgentWriteAccess::CuratorOnly,
            wiki_enabled: true,
            pending_categories: Vec::new(),
        };
        kb.apply_patch(patch)?;
        Ok(kb)
    }

    fn apply_patch(&mut self, patch: KnowledgeBasePatch) -> StoreResult<()> {
        if let Some(name) = patch.name {
            self.name = name;
        }
        if let Some(description) = patch.description {
            self.description = description;
        }
        if let Some(curator_agent_id) = patch.curator_agent_id {
            self.curator_agent_id = curator_agent_id;
        }
        if let Some(mode) = patch.curation_mode {
            self.curation_mode = mode;
        }
        if let Some(interval) = patch.curation_interval_minutes {
            self.curation_interval_minutes = interval;
        }
        if let Some(access) = patch.agent_write_access {
            self.agent_write_access = access;
        }
        if let Some(enabled) = patch.wiki_enabled {
            self.wiki_enabled = enabled;
        }
        if let Some(categories) = patch.pending_categories {
            self.pending_categories = categories;
        }
        self.validate()
    }

    fn validate(&self) -> StoreResult<()> {
        if self.name.trim().is_empty()
            || self.curation_interval_minutes == 0
            || self
                .curator_agent_id
                .as_ref()
                .is_some_and(|id| id.trim().is_empty())
            || (self.curation_mode == CurationMode::Auto && self.curator_agent_id.is_none())
        {
            Err(KbInvalidInput)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    File,
    Note,
    Wiki,
    LinkedFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceStatus {
    Queued,
    Converting,
    Chunking,
    Ready,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFingerprint {
    pub size: u64,
    pub mtime_ms: i64,
}

/// Facts owned by ingestion. Paths are descriptive and never used for Store I/O.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFacts {
    pub kind: SourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_fingerprint: Option<SourceFingerprint>,
    pub status: SourceStatus,
}

impl SourceFacts {
    fn validate(&self) -> StoreResult<()> {
        if self.content_hash.as_ref().is_some_and(|hash| {
            hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        }) {
            return Err(KbInvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataKey {
    Summary,
    Tags,
    Category,
    DocType,
    Date,
    Entities,
    Language,
    DuplicateOf,
}

pub type MetadataPatch = BTreeMap<MetadataKey, Value>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSource {
    pub id: KnowledgeSourceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(flatten)]
    pub facts: SourceFacts,
    pub metadata: BTreeMap<MetadataKey, KnowledgeMetadataField<Value>>,
}

impl KnowledgeSource {
    pub(crate) fn display_title(&self) -> &str {
        self.facts
            .current_path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .or(self.display_name.as_deref())
            .unwrap_or(&self.id)
    }
}

// Ingestion metadata shares source.json's commit with the public source facts.
// Tauri returns only the public projection.
#[derive(Serialize, Deserialize)]
struct SourceRecord {
    #[serde(flatten)]
    source: KnowledgeSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ingest: Option<ingest::ImportRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum MetadataOutcome {
    Updated,
    Rejected { code: KnowledgeErrorCode },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedKnowledgeRef {
    pub reference: KnowledgeRef,
    pub source: KnowledgeSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment: Option<ResolvedFragment>,
}

/// An internal index notification. Only identities and content hashes cross this channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeChange {
    SourceUpserted {
        kb_id: KnowledgeBaseId,
        source_id: KnowledgeSourceId,
        content_hash: Option<String>,
    },
    SourceRemoved {
        kb_id: KnowledgeBaseId,
        source_id: KnowledgeSourceId,
    },
    KbRemoved {
        kb_id: KnowledgeBaseId,
    },
    MetadataChanged {
        kb_id: KnowledgeBaseId,
        source_id: KnowledgeSourceId,
    },
}

pub struct KnowledgeStore {
    app_data_dir: PathBuf,
    changes: broadcast::Sender<KnowledgeChange>,
}

impl KnowledgeStore {
    /// Construction registers the owner without touching the disk on the UI thread.
    pub fn new(app_data_dir: PathBuf) -> Self {
        let (changes, _) = broadcast::channel(256);
        Self {
            app_data_dir,
            changes,
        }
    }

    pub fn subscribe_changes(&self) -> broadcast::Receiver<KnowledgeChange> {
        self.changes.subscribe()
    }

    /// All reads and writes take the same on-disk lock. Commands call this on
    /// the blocking pool, and the index can re-read from disk after lagging.
    fn locked<T>(
        &self,
        action: impl FnOnce(&StoreFiles, &mut BTreeSet<KnowledgeBaseId>) -> StoreResult<T>,
    ) -> StoreResult<T> {
        fs::create_dir_all(&self.app_data_dir).map_err(|_| KbStoreUnavailable)?;
        let app = fs::canonicalize(&self.app_data_dir).map_err(|_| KbStoreUnavailable)?;
        let (root, _handle) = ensure_workspace_directory_no_follow(&app, "knowledge")
            .map_err(|_| KbStoreUnavailable)?;
        let files = StoreFiles { app, root };
        with_file_lock_blocking(
            &files.root.join(".store.lock"),
            FileLockOptions::default(),
            || {
                Ok(files.read_registry().and_then(|mut registry| {
                    files.recover_removed_kbs(&registry)?;
                    action(&files, &mut registry)
                }))
            },
        )
        .map_err(|_| KbStoreUnavailable)?
    }

    fn locked_kb<T>(
        &self,
        kb_id: &str,
        source_id: Option<&str>,
        action: impl FnOnce(&StoreFiles, &mut BTreeSet<KnowledgeBaseId>) -> StoreResult<T>,
    ) -> StoreResult<T> {
        if !is_knowledge_base_id(kb_id) || source_id.is_some_and(|id| !is_knowledge_source_id(id)) {
            return Err(KbInvalidInput);
        }
        self.locked(|files, registry| {
            if !registry.contains(kb_id) {
                return Err(KbNotFound);
            }
            action(files, registry)
        })
    }

    pub fn list(&self) -> StoreResult<Vec<KnowledgeBase>> {
        self.locked(|files, registry| registry.iter().map(|id| files.read_kb(id)).collect())
    }

    pub fn create(&self, patch: KnowledgeBasePatch) -> StoreResult<KnowledgeBase> {
        self.locked(|files, registry| {
            let id = allocate_id(
                &files.root,
                |id| registry.contains(id),
                |entropy| create_knowledge_base_id(entropy[..6].try_into().expect("six bytes")),
            )?;
            let kb = KnowledgeBase::from_patch(id, patch)?;
            // Publication of kbs.json is the visibility boundary. A staged
            // directory has no readers until its settings and default rules
            // are fully synced, so a library never appears without them.
            for (name, template) in LIBRARY_TEMPLATES {
                files.write_text(&format!("{}/{name}", kb.id), template)?;
            }
            files.write_json(&format!("{}/kb.json", kb.id), &kb)?;
            sync_directory(&files.root).map_err(|_| KbStoreUnavailable)?;
            registry.insert(kb.id.clone());
            files.write_json(REGISTRY, registry)?;
            Ok(kb)
        })
    }

    pub fn update(&self, kb_id: &str, patch: KnowledgeBasePatch) -> StoreResult<KnowledgeBase> {
        self.locked_kb(kb_id, None, |files, _| {
            let mut kb = files.read_kb(kb_id)?;
            kb.apply_patch(patch)?;
            files.write_json(&format!("{kb_id}/kb.json"), &kb)?;
            Ok(kb)
        })
    }

    pub fn delete(&self, kb_id: &str) -> StoreResult<()> {
        self.locked_kb(kb_id, None, |files, registry| {
            files.read_kb(kb_id)?;
            // kbs.json decides recovery if interrupted before or after publication.
            let tombstone = files.detach(&files.root, kb_id)?;
            registry.remove(kb_id);
            files.write_json(REGISTRY, registry)?;
            let _ = self.changes.send(KnowledgeChange::KbRemoved {
                kb_id: kb_id.into(),
            });
            files.cleanup(&tombstone);
            Ok(())
        })
    }

    pub fn list_sources(&self, kb_id: &str) -> StoreResult<Vec<KnowledgeSource>> {
        self.locked_kb(kb_id, None, |files, _| {
            files.read_kb(kb_id)?;
            let Some(parent) = files.source_parent(kb_id)? else {
                return Ok(Vec::new());
            };
            files.recover_source_publications(&parent)?;
            files.cleanup_sources(&parent)?;
            let mut sources = Vec::new();
            for entry in fs::read_dir(parent).map_err(|_| KbStoreUnavailable)? {
                let entry = entry.map_err(|_| KbStoreUnavailable)?;
                let name = entry.file_name();
                let Some(id) = name.to_str() else { continue };
                if is_knowledge_source_id(id) {
                    sources.push(files.read_source(kb_id, id)?.ok_or(KbStoreCorrupt)?);
                }
            }
            sources.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(sources)
        })
    }

    pub fn get_source(&self, kb_id: &str, source_id: &str) -> StoreResult<KnowledgeSource> {
        self.locked_kb(kb_id, Some(source_id), |files, _| {
            files.require_source(kb_id, source_id)
        })
    }

    pub fn create_source(&self, kb_id: &str, facts: SourceFacts) -> StoreResult<KnowledgeSource> {
        facts.validate()?;
        self.locked_kb(kb_id, None, |files, _| {
            files.read_kb(kb_id)?;
            if let Some(parent) = files.source_parent(kb_id)? {
                files.recover_source_publications(&parent)?;
            }
            let parent = files.root.join(kb_id).join("sources");
            ingest::check_source_limit(&parent)?;
            let id = allocate_id(&parent, |_| false, create_knowledge_source_id)?;
            let source = KnowledgeSource {
                id,
                display_name: None,
                facts,
                metadata: BTreeMap::new(),
            };
            let staged_id = format!("{STAGING}{}", source.id);
            let staged = parent.join(&staged_id);
            files.write_json(&source_path(kb_id, &staged_id), &source)?;
            rename_directory_noreplace(&staged, &parent.join(&source.id))
                .map_err(|_| KbStoreUnavailable)?;
            sync_directory(&parent).map_err(|_| KbStoreUnavailable)?;
            sync_directory(&files.root.join(kb_id)).map_err(|_| KbStoreUnavailable)?;
            self.source_upserted(kb_id, &source);
            Ok(source)
        })
    }

    pub fn update_source(
        &self,
        kb_id: &str,
        source_id: &str,
        facts: SourceFacts,
    ) -> StoreResult<KnowledgeSource> {
        facts.validate()?;
        self.locked_kb(kb_id, Some(source_id), |files, _| {
            let mut record = files.require_record(kb_id, source_id)?;
            if record.ingest.is_some() {
                return Err(KbInvalidInput);
            }
            record.source.facts = facts;
            files.write_json(&source_path(kb_id, source_id), &record)?;
            self.source_upserted(kb_id, &record.source);
            Ok(record.source)
        })
    }

    pub fn remove_source(&self, kb_id: &str, source_id: &str) -> StoreResult<()> {
        self.locked_kb(kb_id, Some(source_id), |files, _| {
            files.require_source(kb_id, source_id)?;
            let parent = files.root.join(kb_id).join("sources");
            let tombstone = files.detach(&parent, source_id)?;
            let _ = self.changes.send(KnowledgeChange::SourceRemoved {
                kb_id: kb_id.into(),
                source_id: source_id.into(),
            });
            files.cleanup(&tombstone);
            Ok(())
        })
    }

    pub fn set_metadata(
        &self,
        kb_id: &str,
        source_id: &str,
        actor: KnowledgeMetadataOrigin,
        fields: MetadataPatch,
    ) -> StoreResult<BTreeMap<MetadataKey, MetadataOutcome>> {
        self.write_metadata(kb_id, source_id, actor, fields, |_| Ok(()))
    }

    pub(crate) fn set_agent_metadata<G>(
        &self,
        kb_id: &str,
        source_id: &str,
        fields: MetadataPatch,
        authorize: impl FnOnce(&KnowledgeBase) -> StoreResult<G>,
    ) -> StoreResult<BTreeMap<MetadataKey, MetadataOutcome>> {
        self.write_metadata(
            kb_id,
            source_id,
            KnowledgeMetadataOrigin::Agent,
            fields,
            authorize,
        )
    }

    pub(crate) fn check_agent_write<G>(
        &self,
        kb_id: &str,
        authorize: impl FnOnce(&KnowledgeBase) -> StoreResult<G>,
    ) -> StoreResult<()> {
        self.locked_kb(kb_id, None, |files, _| {
            authorize(&files.read_kb(kb_id)?).map(drop)
        })
    }

    fn write_metadata<G>(
        &self,
        kb_id: &str,
        source_id: &str,
        actor: KnowledgeMetadataOrigin,
        fields: MetadataPatch,
        authorize: impl FnOnce(&KnowledgeBase) -> StoreResult<G>,
    ) -> StoreResult<BTreeMap<MetadataKey, MetadataOutcome>> {
        self.locked_kb(kb_id, Some(source_id), |files, _| {
            let _permit = authorize(&files.read_kb(kb_id)?)?;
            let mut record = files.require_record(kb_id, source_id)?;
            let source = &mut record.source;
            let updated_at = Utc::now().to_rfc3339();
            let mut outcomes = BTreeMap::new();
            for (key, value) in fields {
                if actor == KnowledgeMetadataOrigin::Agent
                    && source
                        .metadata
                        .get(&key)
                        .is_some_and(|field| field.origin == KnowledgeMetadataOrigin::User)
                {
                    outcomes.insert(
                        key,
                        MetadataOutcome::Rejected {
                            code: KbFieldLocked,
                        },
                    );
                    continue;
                }
                source.metadata.insert(
                    key,
                    KnowledgeMetadataField {
                        value,
                        origin: actor,
                        updated_at: updated_at.clone(),
                    },
                );
                outcomes.insert(key, MetadataOutcome::Updated);
            }
            if outcomes
                .values()
                .any(|outcome| *outcome == MetadataOutcome::Updated)
            {
                files.write_json(&source_path(kb_id, source_id), &record)?;
                let _ = self.changes.send(KnowledgeChange::MetadataChanged {
                    kb_id: kb_id.into(),
                    source_id: source_id.into(),
                });
            }
            Ok(outcomes)
        })
    }

    pub fn resolve_ref(&self, input: &str) -> StoreResult<ResolvedKnowledgeRef> {
        let reference = parse_knowledge_ref(input).ok_or(KbInvalidInput)?;
        let source = self.get_source(&reference.kb_id, &reference.source_id)?;
        let fragment = self.resolve_fragment(&reference, &source)?;
        Ok(ResolvedKnowledgeRef {
            reference,
            source,
            fragment,
        })
    }

    fn source_upserted(&self, kb_id: &str, source: &KnowledgeSource) {
        let _ = self.changes.send(KnowledgeChange::SourceUpserted {
            kb_id: kb_id.into(),
            source_id: source.id.clone(),
            content_hash: source.facts.content_hash.clone(),
        });
    }
}

fn allocate_id(
    parent: &Path,
    reserved: impl Fn(&str) -> bool,
    format: impl Fn([u8; 8]) -> String,
) -> StoreResult<String> {
    for _ in 0..16 {
        let uuid = Uuid::new_v4();
        let mut entropy = [0; 8];
        // Avoid the bytes containing UUID version and variant bits.
        entropy[..6].copy_from_slice(&uuid.as_bytes()[..6]);
        entropy[6..].copy_from_slice(&uuid.as_bytes()[9..11]);
        let id = format(entropy);
        if reserved(&id) {
            continue;
        }
        let mut free = true;
        for name in [&id, &format!("{STAGING}{id}"), &format!("{REMOVED}{id}")] {
            match fs::symlink_metadata(parent.join(name)) {
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Ok(_) => free = false,
                Err(_) => return Err(KbStoreUnavailable),
            }
        }
        if free {
            return Ok(id);
        }
    }
    Err(KbStoreUnavailable)
}

fn source_path(kb_id: &str, source_id: &str) -> String {
    format!("{kb_id}/sources/{source_id}/source.json")
}

struct StoreFiles {
    app: PathBuf,
    root: PathBuf,
}

impl StoreFiles {
    fn read_json<T: DeserializeOwned>(&self, relative: &str) -> StoreResult<Option<T>> {
        let path = self.root.join(relative);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(KbStoreUnavailable),
            Ok(_) => {}
        }
        let (_, bytes) = read_workspace_file_no_follow(
            &self.app,
            &format!("knowledge/{relative}"),
            MAX_JSON_BYTES,
        )
        .map_err(|_| KbStoreUnavailable)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| KbStoreCorrupt)
    }

    fn write_json<T: Serialize>(&self, relative: &str, value: &T) -> StoreResult<()> {
        let bytes = serde_json::to_vec_pretty(value).map_err(|_| KbStoreUnavailable)?;
        if bytes.len() as u64 > MAX_JSON_BYTES {
            return Err(KbLimitExceeded);
        }
        let target =
            write_workspace_file_no_follow(&self.app, &format!("knowledge/{relative}"), &bytes)
                .map_err(|_| KbStoreUnavailable)?;
        sync_directory(target.parent().expect("Store file has parent"))
            .map_err(|_| KbStoreUnavailable)
    }

    fn write_text(&self, relative: &str, text: &str) -> StoreResult<()> {
        if text.len() as u64 > MAX_JSON_BYTES {
            return Err(KbLimitExceeded);
        }
        let target = write_workspace_file_no_follow(
            &self.app,
            &format!("knowledge/{relative}"),
            text.as_bytes(),
        )
        .map_err(|_| KbStoreUnavailable)?;
        sync_directory(target.parent().expect("Store file has parent"))
            .map_err(|_| KbStoreUnavailable)
    }

    fn read_registry(&self) -> StoreResult<BTreeSet<KnowledgeBaseId>> {
        let ids: Vec<KnowledgeBaseId> = match self.read_json(REGISTRY)? {
            Some(ids) => ids,
            None => {
                // An absent registry is a new Store only when it has no
                // live or detached libraries. Initialize it before staging
                // the first library, so later loss cannot publish an empty view.
                for entry in fs::read_dir(&self.root).map_err(|_| KbStoreUnavailable)? {
                    let entry = entry.map_err(|_| KbStoreUnavailable)?;
                    if entry.file_name().to_str().is_some_and(|name| {
                        is_knowledge_base_id(name)
                            || name.strip_prefix(REMOVED).is_some_and(is_knowledge_base_id)
                    }) {
                        return Err(KbStoreCorrupt);
                    }
                }
                let registry = BTreeSet::new();
                self.write_json(REGISTRY, &registry)?;
                return Ok(registry);
            }
        };
        let mut registry = BTreeSet::new();
        for id in ids {
            if !is_knowledge_base_id(&id) || !registry.insert(id) {
                return Err(KbStoreCorrupt);
            }
        }
        Ok(registry)
    }

    fn read_kb(&self, id: &str) -> StoreResult<KnowledgeBase> {
        let kb: KnowledgeBase = self
            .read_json(&format!("{id}/kb.json"))?
            .ok_or(KbStoreCorrupt)?;
        if kb.id != id || kb.validate().is_err() {
            return Err(KbStoreCorrupt);
        }
        Ok(kb)
    }

    fn require_source(&self, kb_id: &str, source_id: &str) -> StoreResult<KnowledgeSource> {
        self.require_record(kb_id, source_id)
            .map(|record| record.source)
    }

    fn require_record(&self, kb_id: &str, source_id: &str) -> StoreResult<SourceRecord> {
        self.read_kb(kb_id)?;
        self.source_parent(kb_id)?;
        self.read_record(kb_id, source_id)?.ok_or(KbNotFound)
    }

    fn read_source(&self, kb_id: &str, id: &str) -> StoreResult<Option<KnowledgeSource>> {
        Ok(self.read_record(kb_id, id)?.map(|record| record.source))
    }

    fn read_record(&self, kb_id: &str, id: &str) -> StoreResult<Option<SourceRecord>> {
        let path = source_path(kb_id, id);
        let parent = self.root.join(kb_id).join("sources");
        self.recover_source_publication(&parent, id)?;
        let record: Option<SourceRecord> = self.read_json(&path)?;
        if let Some(record) = &record {
            let source = &record.source;
            if source.id != id || source.facts.validate().is_err() {
                return Err(KbStoreCorrupt);
            }
            if let Some(import) = &record.ingest {
                import.validate(source)?;
            }
        } else {
            let target = self.root.join(path);
            match fs::symlink_metadata(target.parent().expect("source file has a parent")) {
                Ok(_) => return Err(KbStoreCorrupt),
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(_) => return Err(KbStoreUnavailable),
            }
        }
        Ok(record)
    }

    fn source_parent(&self, kb_id: &str) -> StoreResult<Option<PathBuf>> {
        let parent = self.root.join(kb_id).join("sources");
        if !Self::is_plain_directory(&parent)? {
            return Ok(None);
        }
        Ok(Some(parent))
    }

    fn cleanup_sources(&self, parent: &Path) -> StoreResult<()> {
        for path in Self::reserved_directories(parent, REMOVED, is_knowledge_source_id)?
            .into_iter()
            .chain(Self::reserved_directories(
                parent,
                STAGING,
                is_knowledge_source_id,
            )?)
        {
            self.cleanup(&path);
        }
        Ok(())
    }

    fn is_plain_directory(path: &Path) -> StoreResult<bool> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(KbStoreUnavailable),
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(KbStoreCorrupt);
        }
        // The exact canonical path also rejects ancestor redirects and junctions.
        if fs::canonicalize(path).map_err(|_| KbStoreUnavailable)? != path {
            return Err(KbStoreCorrupt);
        }
        Ok(true)
    }

    fn reserved_directories(
        parent: &Path,
        prefix: &str,
        valid_id: fn(&str) -> bool,
    ) -> StoreResult<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(parent).map_err(|_| KbStoreUnavailable)? {
            let entry = entry.map_err(|_| KbStoreUnavailable)?;
            if entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix(prefix))
                .is_some_and(valid_id)
            {
                paths.push(entry.path());
            }
        }
        Ok(paths)
    }

    fn detach(&self, parent: &Path, id: &str) -> StoreResult<PathBuf> {
        let live = parent.join(id);
        if !Self::is_plain_directory(&live)? {
            return Err(KbStoreCorrupt);
        }
        let tombstone = parent.join(format!("{REMOVED}{id}"));
        rename_directory_noreplace(&live, &tombstone).map_err(|_| KbStoreUnavailable)?;
        sync_directory(parent).map_err(|_| KbStoreUnavailable)?;
        Ok(tombstone)
    }

    fn recover_removed_kbs(&self, registry: &BTreeSet<KnowledgeBaseId>) -> StoreResult<()> {
        for tombstone in Self::reserved_directories(&self.root, REMOVED, is_knowledge_base_id)? {
            let id = tombstone
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix(REMOVED))
                .expect("validated removed name");
            if registry.contains(id) {
                if !Self::is_plain_directory(&tombstone)? {
                    return Err(KbStoreCorrupt);
                }
                rename_directory_noreplace(&tombstone, &self.root.join(id))
                    .map_err(|_| KbStoreUnavailable)?;
                sync_directory(&self.root).map_err(|_| KbStoreUnavailable)?;
            } else {
                self.cleanup(&tombstone);
            }
        }
        Ok(())
    }

    fn cleanup(&self, tombstone: &Path) {
        let Some(parent) = tombstone.parent() else {
            return;
        };
        // First make the namespace change durable. Deleting the detached tree
        // is safe even if a linked source points at an external original.
        if matches!(Self::is_plain_directory(tombstone), Ok(true))
            && sync_directory(parent).is_ok()
            && fs::remove_dir_all(tombstone).is_ok()
        {
            let _ = sync_directory(parent);
        }
    }
}

#[cfg(test)]
mod tests;
