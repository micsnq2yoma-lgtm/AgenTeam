//! Managed imports. source.json is the commit record for each attempt. The
//! small queue and lookup files contain identities and can precede publication.
use super::super::chunker::ChunkPosition;
use super::*;
use crate::document_processing::{
    copy_knowledge_original, reject_link_ancestors, MAX_OUTPUT_BYTES,
};
use crate::knowledge::ingest::conversion::{source_limit, ConversionError};
use crate::knowledge::types::{create_knowledge_job_id, is_knowledge_job_id};
use crate::workspace_files::path_safety::{
    metadata_is_link_like, open_workspace_file_strict,
    open_workspace_regular_file_no_follow_classified, validate_item_name,
};
use chrono::{DateTime, Local};
use tempfile::TempDir;
use KnowledgeErrorCode::{KbInsufficientSpace, KbJobConflict, KbSourceInUse};

const MAX_SOURCES: usize = 10_000;
const MAX_ACTIVE: usize = 64;
const ACTIVE: &str = "active-imports.json";
const INCOMING: &str = ".incoming-";
const PREVIOUS: &str = ".previous-";
const WORK: &str = ".ingest-work-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestJob {
    pub id: String,
    pub kb_id: String,
    pub source_id: String,
    pub status: SourceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<KnowledgeErrorCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<ChunkPosition>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReceipt {
    pub source_id: String,
    pub job: Option<IngestJob>,
    pub duplicate: bool,
}

pub struct IngestInput {
    pub job: IngestJob,
    pub original: PathBuf,
    pub stage: TempDir,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ImportRecord {
    id: String,
    extension: String,
    original_sha256: String,
    document_job_id: Option<String>,
    pub(super) error: Option<KnowledgeErrorCode>,
    pub(super) suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) position: Option<ChunkPosition>,
}

impl ImportRecord {
    pub(super) fn validate(&self, source: &KnowledgeSource) -> StoreResult<()> {
        if !is_knowledge_job_id(&self.id)
            || !valid_digest(&self.original_sha256)
            || self.extension.is_empty()
            || !self.extension.bytes().all(|c| c.is_ascii_alphanumeric())
            || source.facts.kind != SourceKind::File
            || source.facts.file_fingerprint.is_none()
        {
            return Err(KbStoreCorrupt);
        }
        Ok(())
    }
}

impl SourceRecord {
    fn job(&self, kb_id: &str) -> StoreResult<IngestJob> {
        let import = self.ingest.as_ref().ok_or(KbNotFound)?;
        Ok(IngestJob {
            id: import.id.clone(),
            kb_id: kb_id.into(),
            source_id: self.source.id.clone(),
            status: self.source.facts.status,
            error: import.error,
            suggestion: import.suggestion.clone(),
            position: import.position,
        })
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn pending(status: SourceStatus) -> bool {
    matches!(
        status,
        SourceStatus::Queued | SourceStatus::Converting | SourceStatus::Chunking
    )
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobLink {
    source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archived: Option<IngestJob>,
}

fn locator(kb_id: &str, job_id: &str) -> String {
    format!("{kb_id}/jobs/{job_id}.json")
}

pub(super) fn valid_kb_source_folders(parent: &Path) -> StoreResult<impl Iterator<Item = String>> {
    let mut ids = Vec::new();
    match fs::read_dir(parent) {
        Ok(entries) => {
            for entry in entries {
                let name = entry.map_err(|_| KbStoreUnavailable)?.file_name();
                if let Some(id) = name.to_str().filter(|id| is_knowledge_source_id(id)) {
                    ids.push(id.to_owned());
                }
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(_) => return Err(KbStoreUnavailable),
    }
    Ok(ids.into_iter())
}

pub(super) fn check_source_limit(parent: &Path) -> StoreResult<()> {
    if valid_kb_source_folders(parent)?.count() >= MAX_SOURCES {
        Err(KbLimitExceeded)
    } else {
        Ok(())
    }
}

fn check_space(parent: &Path, needed: u64) -> StoreResult<()> {
    if crate::filesystem_capacity::available_space(parent).map_err(|_| KbStoreUnavailable)? < needed
    {
        Err(KbInsufficientSpace)
    } else {
        Ok(())
    }
}

fn checked_input(
    path: &Path,
    limit: u64,
    workspace: Option<&Path>,
) -> StoreResult<(fs::File, fs::Metadata)> {
    let requested = path.to_str().ok_or(KbInvalidInput)?;
    let file = match workspace {
        Some(root) => open_workspace_file_strict(root, requested, "knowledge input"),
        None => {
            if !path.is_absolute() {
                return Err(KbInvalidInput);
            }
            reject_link_ancestors(path, false).map_err(|_| KbInvalidInput)?;
            let meta = fs::symlink_metadata(path).map_err(|_| KbInvalidInput)?;
            if !meta.is_file() || metadata_is_link_like(&meta) {
                return Err(KbInvalidInput);
            }
            let parent = path.parent().ok_or(KbInvalidInput)?;
            open_workspace_regular_file_no_follow_classified(parent, requested, "knowledge input")
                .map(|(_, file)| file)
        }
    }
    .map_err(|error| {
        if error.is_in_use() {
            KbSourceInUse
        } else {
            KbInvalidInput
        }
    })?;
    let meta = file.metadata().map_err(|_| KbInvalidInput)?;
    if meta.len() > limit {
        return Err(KbLimitExceeded);
    }
    Ok((file, meta))
}

fn copy_checked(file: &mut fs::File, dest: &Path, size: u64) -> StoreResult<String> {
    copy_knowledge_original(file, dest, size).map_err(|code| match code.as_str() {
        "DOCUMENT_RESOURCE_LIMIT" => KbLimitExceeded,
        "DOCUMENT_SOURCE_IN_USE" => KbSourceInUse,
        "DOCUMENT_INPUT_READ_FAILED" | "DOCUMENT_SOURCE_CHANGED" => KbInvalidInput,
        _ => KbStoreUnavailable,
    })
}

impl StoreFiles {
    fn source_root(&self, kb: &str, source: &str) -> PathBuf {
        self.root.join(kb).join("sources").join(source)
    }

    pub(super) fn prepare_import(&self, kb: &str, bytes: u64) -> StoreResult<TempDir> {
        self.read_kb(kb)?;
        let (sources, _handle) =
            ensure_workspace_directory_no_follow(&self.root, &format!("{kb}/sources"))
                .map_err(|_| KbStoreUnavailable)?;
        check_space(&sources, bytes)?;
        tempfile::Builder::new()
            .prefix(INCOMING)
            .tempdir_in(sources)
            .map_err(|_| KbStoreUnavailable)
    }

    pub(super) fn publish_source_version(
        &self,
        kb: &str,
        record: &SourceRecord,
        stage: &TempDir,
        replace: bool,
    ) -> StoreResult<()> {
        let live = self.source_root(kb, &record.source.id);
        let parent = live.parent().ok_or(KbStoreCorrupt)?;
        let previous = parent.join(format!("{PREVIOUS}{}", record.source.id));
        let stage_name = stage
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(KbStoreCorrupt)?;
        self.write_json(&source_path(kb, stage_name), record)?;
        if replace {
            rename_directory_noreplace(&live, &previous).map_err(|_| KbStoreUnavailable)?;
            sync_directory(parent).map_err(|_| KbStoreUnavailable)?;
        }
        rename_directory_noreplace(stage.path(), &live).map_err(|_| KbStoreUnavailable)?;
        sync_directory(parent).map_err(|_| KbStoreUnavailable)?;
        if replace {
            self.cleanup(&previous);
        }
        Ok(())
    }

    // A crash before the new source directory is visible restores the prior
    // record, original and attempt. After publication only that old copy is garbage.
    pub(super) fn recover_source_publications(&self, parent: &Path) -> StoreResult<()> {
        for previous in Self::reserved_directories(parent, PREVIOUS, is_knowledge_source_id)? {
            let name = previous
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or(KbStoreCorrupt)?;
            self.recover_source_publication(
                parent,
                name.strip_prefix(PREVIOUS).ok_or(KbStoreCorrupt)?,
            )?;
        }
        Ok(())
    }

    pub(super) fn recover_source_publication(&self, parent: &Path, id: &str) -> StoreResult<()> {
        let previous = parent.join(format!("{PREVIOUS}{id}"));
        if !Self::is_plain_directory(&previous)? {
            return Ok(());
        }
        let live = parent.join(id);
        if Self::is_plain_directory(&live)? {
            self.cleanup(&previous);
        } else {
            rename_directory_noreplace(&previous, &live).map_err(|_| KbStoreUnavailable)?;
            sync_directory(parent).map_err(|_| KbStoreUnavailable)?;
        }
        Ok(())
    }
    fn locate(&self, registry: &BTreeSet<String>, id: &str) -> StoreResult<(String, JobLink)> {
        if !is_knowledge_job_id(id) {
            return Err(KbInvalidInput);
        }
        for kb in registry {
            if let Some(link) = self.read_json::<JobLink>(&locator(kb, id))? {
                if !is_knowledge_source_id(&link.source_id) {
                    return Err(KbStoreCorrupt);
                }
                return Ok((kb.clone(), link));
            }
        }
        Err(KbNotFound)
    }

    pub(super) fn current(
        &self,
        registry: &BTreeSet<String>,
        id: &str,
    ) -> StoreResult<(String, SourceRecord)> {
        let (kb, link) = self.locate(registry, id)?;
        let record = self.require_record(&kb, &link.source_id)?;
        if record.ingest.as_ref().is_none_or(|import| import.id != id) {
            return Err(KbJobConflict);
        }
        Ok((kb, record))
    }

    fn get_job(&self, registry: &BTreeSet<String>, id: &str) -> StoreResult<IngestJob> {
        let (kb, link) = self.locate(registry, id)?;
        let record = self.require_record(&kb, &link.source_id)?;
        let current = record.job(&kb)?;
        if current.id == id {
            return Ok(current);
        }
        link.archived
            .filter(|job| job.id == id && job.kb_id == kb && job.source_id == link.source_id)
            .ok_or(KbNotFound)
    }

    fn allocate_job(&self, registry: &BTreeSet<String>) -> StoreResult<String> {
        let date = Local::now().format("%Y%m%d").to_string();
        for _ in 0..16 {
            let entropy = Uuid::new_v4();
            let id = create_knowledge_job_id(
                &date,
                entropy.as_bytes()[..6].try_into().expect("six bytes"),
            )
            .ok_or(KbStoreUnavailable)?;
            match self.locate(registry, &id) {
                Err(KbNotFound) => return Ok(id),
                Ok(_) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(KbStoreUnavailable)
    }

    fn active_jobs(&self, registry: &BTreeSet<String>) -> StoreResult<Vec<String>> {
        let ids = self.read_json::<Vec<String>>(ACTIVE)?.unwrap_or_default();
        let mut active = Vec::new();
        for id in ids {
            match self.get_job(registry, &id) {
                Ok(job) if pending(job.status) => active.push(id),
                Ok(_) | Err(KbNotFound) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(active)
    }

    fn reserve_job(
        &self,
        registry: &BTreeSet<String>,
        kb: &str,
        source: &str,
        id: &str,
    ) -> StoreResult<()> {
        let mut active = self.active_jobs(registry)?;
        if active.len() >= MAX_ACTIVE {
            return Err(KbLimitExceeded);
        }
        self.write_json(
            &locator(kb, id),
            &JobLink {
                source_id: source.into(),
                archived: None,
            },
        )?;
        // Reserve before making the queued state visible. Stale reservations
        // are filtered against source.json on the next admission or recovery.
        active.push(id.into());
        self.write_json(ACTIVE, &active)
    }

    pub(super) fn save_record(&self, kb: &str, record: &SourceRecord) -> StoreResult<()> {
        self.write_json(&source_path(kb, &record.source.id), record)
    }
}

impl KnowledgeStore {
    /// Invalidate outstanding attempts before waiting for workers. Every final
    /// publication checks the current attempt and phase under this same lock.
    pub fn interrupt_ingests(&self) -> StoreResult<Vec<String>> {
        self.locked(|files, registry| {
            let active = files.active_jobs(registry)?;
            let mut documents = Vec::new();
            let mut resumable = Vec::new();
            for id in active {
                let (kb, mut record) = files.current(registry, &id)?;
                if record.source.facts.status == SourceStatus::Chunking {
                    resumable.push(id);
                    continue;
                }
                let import = record.ingest.as_ref().ok_or(KbStoreCorrupt)?;
                if let Some(document) = &import.document_job_id {
                    documents.push(document.clone());
                }
                record.source.facts.status = SourceStatus::Interrupted;
                files.save_record(&kb, &record)?;
                self.source_upserted(&kb, &record.source);
            }
            files.write_json(ACTIVE, &resumable)?;
            Ok(documents)
        })
    }

    /// Only reclaim private directories after producers relinquish them or
    /// during startup, before admission opens.
    pub fn reclaim_ingest_staging(&self) -> StoreResult<()> {
        self.recover_ingest_staging().map(drop)
    }

    // K2 could already have removed a completed conversion from active-imports.
    // Discover published Markdown by its authoritative source record on startup.
    pub(crate) fn recover_ingest_staging(&self) -> StoreResult<Vec<String>> {
        self.locked(|files, registry| {
            let mut owners = BTreeMap::new();
            for kb in registry.iter() {
                files.read_kb(kb)?;
                files.recover_wiki_publication(kb)?;
                let Some(parent) = files.source_parent(kb)? else {
                    continue;
                };
                files.recover_source_publications(&parent)?;
                for stage in StoreFiles::reserved_directories(&parent, INCOMING, valid_temp_suffix)?
                {
                    files.cleanup(&stage);
                }
                for source in valid_kb_source_folders(&parent)? {
                    let dir = parent.join(&source);
                    if !StoreFiles::is_plain_directory(&dir)? {
                        return Err(KbStoreCorrupt);
                    }
                    let record = files.require_record(kb, &source)?;
                    if record.source.facts.status == SourceStatus::Chunking {
                        if let Some(job) = record.ingest {
                            if owners.insert(job.id, (kb.as_str(), source)).is_some() {
                                return Err(KbStoreCorrupt);
                            }
                        }
                    }
                    for stage in StoreFiles::reserved_directories(&dir, WORK, valid_work_suffix)? {
                        files.cleanup(&stage);
                    }
                }
            }
            // K2 could remove a Chunking job from the active index. Restore its
            // entry so new admissions also account for the resumed backlog.
            // Older publications may be discoverable only through source.json. Recreate
            // a missing locator for its unique current attempt, never an archived one.
            for (id, (kb, source)) in &owners {
                match files.locate(registry, id) {
                    Ok((owner, link))
                        if owner == *kb
                            && link.source_id == source.as_str()
                            && link.archived.is_none() => {}
                    Ok(_) => return Err(KbStoreCorrupt),
                    Err(KbNotFound) => files.write_json(
                        &locator(kb, id),
                        &JobLink {
                            source_id: source.clone(),
                            archived: None,
                        },
                    )?,
                    Err(error) => return Err(error),
                }
            }
            let resumable: Vec<_> = owners.into_keys().collect();
            let mut active = files.active_jobs(registry)?;
            let before = active.len();
            let mut seen: BTreeSet<_> = active.iter().cloned().collect();
            active.extend(
                resumable
                    .iter()
                    .filter(|id| seen.insert((*id).clone()))
                    .cloned(),
            );
            if active.len() != before {
                files.write_json(ACTIVE, &active)?;
            }
            Ok(resumable)
        })
    }

    /// Build the complete next generation beside the source, including the
    /// original. Only the final directory swap holds the registry lock.
    pub fn publish_ingest(&self, id: &str, stage: &Path) -> StoreResult<IngestJob> {
        if !is_knowledge_job_id(id) {
            return Err(KbInvalidInput);
        }
        let (output, original, expected_hash) = self.locked(|files, registry| {
            let (kb, record) = files.current(registry, id)?;
            if record.source.facts.status != SourceStatus::Converting {
                return Err(KbJobConflict);
            }
            let dir = files.source_root(&kb, &record.source.id);
            if stage.parent() != Some(dir.as_path())
                || !stage
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|name| name.starts_with(&format!("{WORK}{id}-")))
                || !StoreFiles::is_plain_directory(stage)?
            {
                return Err(KbInvalidInput);
            }
            let import = record.ingest.as_ref().ok_or(KbStoreCorrupt)?;
            let size = record
                .source
                .facts
                .file_fingerprint
                .as_ref()
                .ok_or(KbStoreCorrupt)?
                .size;
            Ok((
                files.prepare_import(&kb, size)?,
                dir.join(format!("original.{}", import.extension)),
                import.original_sha256.clone(),
            ))
        })?;
        let mut used = 0;
        let markdown = stage.join("content.md");
        let markdown_hash = copy_artifact(&markdown, &output.path().join("content.md"), &mut used)?;
        let assets = stage.join("assets");
        if StoreFiles::is_plain_directory(&assets)? {
            let (destination, _handle) =
                ensure_workspace_directory_no_follow(output.path(), "assets")
                    .map_err(|_| KbStoreUnavailable)?;
            for entry in fs::read_dir(&assets).map_err(|_| KbStoreUnavailable)? {
                let entry = entry.map_err(|_| KbStoreUnavailable)?;
                let name = entry.file_name();
                let name = name.to_str().ok_or(KbInvalidInput)?;
                validate_item_name(name).map_err(|_| KbInvalidInput)?;
                copy_artifact(&entry.path(), &destination.join(name), &mut used)?;
            }
            sync_directory(&destination).map_err(|_| KbStoreUnavailable)?;
        }
        let (mut file, original_metadata) = checked_input(
            &original,
            source_limit(&original).map_err(|_| KbStoreCorrupt)?,
            None,
        )?;
        let original_name = original.file_name().ok_or(KbStoreCorrupt)?;
        let new_original = output.path().join(original_name);
        if copy_checked(&mut file, &new_original, original_metadata.len())? != expected_hash {
            return Err(KbStoreCorrupt);
        }
        drop(file);
        self.locked(|files, registry| {
            let (kb, mut record) = files.current(registry, id)?;
            if record.source.facts.status != SourceStatus::Converting {
                return Err(KbJobConflict);
            }
            record.source.facts.status = SourceStatus::Chunking;
            record.source.facts.content_hash = Some(markdown_hash);
            files.publish_source_version(&kb, &record, &output, true)?;
            self.source_upserted(&kb, &record.source);
            record.job(&kb)
        })
    }

    pub fn enqueue_file(&self, kb: &str, path: &Path) -> StoreResult<ImportReceipt> {
        self.enqueue_input(kb, path, None, |_| Ok(()))
    }

    pub(crate) fn enqueue_input<G>(
        &self,
        kb: &str,
        path: &Path,
        workspace: Option<&Path>,
        authorize: impl Fn(&KnowledgeBase) -> StoreResult<G>,
    ) -> StoreResult<ImportReceipt> {
        if !is_knowledge_base_id(kb) {
            return Err(KbInvalidInput);
        }
        if workspace.is_some() {
            self.check_agent_write(kb, &authorize)?;
        }
        let limit = source_limit(path).map_err(|error| match error {
            ConversionError::Failed(code, _) => code,
            _ => KbStoreUnavailable,
        })?;
        let (mut file, metadata) = checked_input(path, limit, workspace)?;
        let modified = metadata.modified().map_err(|_| KbInvalidInput)?;
        let fingerprint = SourceFingerprint {
            size: metadata.len(),
            mtime_ms: DateTime::<Utc>::from(modified).timestamp_millis(),
        };
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or(KbInvalidInput)?
            .to_ascii_lowercase();
        let stage = self.locked_kb(kb, None, |files, _| {
            files.prepare_import(kb, fingerprint.size)
        })?;
        let hash = copy_checked(
            &mut file,
            &stage.path().join(format!("original.{extension}")),
            fingerprint.size,
        )?;
        let copied = file.metadata().map_err(|_| KbInvalidInput)?;
        if copied.len() != metadata.len()
            || copied.modified().map_err(|_| KbInvalidInput)? != modified
        {
            return Err(KbInvalidInput);
        }
        drop(file);
        self.locked_kb(kb, None, |files, registry| {
            let _permit = authorize(&files.read_kb(kb)?)?;
            let parent = files.source_parent(kb)?.ok_or(KbStoreUnavailable)?;
            files.recover_source_publications(&parent)?;
            let hash_path = format!("{kb}/original-hashes/{hash}.json");
            if let Some(id) = files.read_json::<String>(&hash_path)? {
                if !is_knowledge_source_id(&id) {
                    return Err(KbStoreCorrupt);
                }
                if let Some(record) = files.read_record(kb, &id)?.filter(|r| {
                    r.ingest
                        .as_ref()
                        .is_some_and(|import| import.original_sha256 == hash)
                }) {
                    return Ok(ImportReceipt {
                        source_id: id,
                        job: Some(record.job(kb)?),
                        duplicate: true,
                    });
                }
            }
            check_source_limit(&parent)?;
            let source = allocate_id(&parent, |_| false, create_knowledge_source_id)?;
            let id = files.allocate_job(registry)?;
            let record = SourceRecord {
                source: KnowledgeSource {
                    id: source.clone(),
                    display_name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned()),
                    metadata: BTreeMap::new(),
                    facts: SourceFacts {
                        kind: SourceKind::File,
                        current_path: None,
                        content_hash: None,
                        file_fingerprint: Some(fingerprint),
                        status: SourceStatus::Queued,
                    },
                },
                curation_pending: false,
                ingest: Some(ImportRecord {
                    id: id.clone(),
                    extension,
                    original_sha256: hash,
                    document_job_id: None,
                    error: None,
                    suggestion: None,
                    position: None,
                }),
            };
            files.reserve_job(registry, kb, &source, &id)?;
            files.write_json(&hash_path, &source)?;
            let staged = stage
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(KbStoreUnavailable)?;
            files.write_json(&source_path(kb, staged), &record)?;
            // The hash and job lookup directories must already be durable
            // when the queued source first becomes visible.
            sync_directory(&files.root.join(kb)).map_err(|_| KbStoreUnavailable)?;
            rename_directory_noreplace(stage.path(), &parent.join(&source))
                .map_err(|_| KbStoreUnavailable)?;
            sync_directory(&parent).map_err(|_| KbStoreUnavailable)?;
            self.source_upserted(kb, &record.source);
            Ok(ImportReceipt {
                source_id: source,
                job: Some(record.job(kb)?),
                duplicate: false,
            })
        })
    }

    pub fn ingest_job(&self, id: &str) -> StoreResult<IngestJob> {
        if !is_knowledge_job_id(id) {
            return Err(KbInvalidInput);
        }
        self.locked(|files, registry| files.get_job(registry, id))
    }

    pub fn begin_ingest(&self, id: &str) -> StoreResult<IngestInput> {
        if !is_knowledge_job_id(id) {
            return Err(KbInvalidInput);
        }
        self.locked(|files, registry| {
            let (kb, mut record) = files.current(registry, id)?;
            if record.source.facts.status != SourceStatus::Queued {
                return Err(KbJobConflict);
            }
            let dir = files.source_root(&kb, &record.source.id);
            let original = dir.join(format!(
                "original.{}",
                record.ingest.as_ref().ok_or(KbStoreCorrupt)?.extension
            ));
            let stage = tempfile::Builder::new()
                .prefix(&format!("{WORK}{id}-"))
                .tempdir_in(&dir)
                .map_err(|_| KbStoreUnavailable)?;
            record.source.facts.status = SourceStatus::Converting;
            files.save_record(&kb, &record)?;
            self.source_upserted(&kb, &record.source);
            Ok(IngestInput {
                job: record.job(&kb)?,
                original,
                stage,
            })
        })
    }

    pub fn attach_conversion(&self, id: &str, document_id: &str) -> StoreResult<()> {
        if !is_knowledge_job_id(id) {
            return Err(KbInvalidInput);
        }
        self.locked(|files, registry| {
            let (kb, mut record) = files.current(registry, id)?;
            if record.source.facts.status != SourceStatus::Converting {
                return Err(KbJobConflict);
            }
            let import = record.ingest.as_mut().ok_or(KbStoreCorrupt)?;
            if import
                .document_job_id
                .as_deref()
                .is_some_and(|old| old != document_id)
            {
                return Err(KbJobConflict);
            }
            import.document_job_id = Some(document_id.into());
            files.save_record(&kb, &record)
        })
    }

    pub fn retry_ingest(&self, kb: &str, source: &str) -> StoreResult<IngestJob> {
        self.locked_kb(kb, Some(source), |files, registry| {
            let mut record = files.require_record(kb, source)?;
            if !matches!(
                record.source.facts.status,
                SourceStatus::Cancelled | SourceStatus::Failed | SourceStatus::Interrupted
            ) {
                return Err(KbJobConflict);
            }
            let old = record.job(kb)?;
            let old_link = locator(kb, &old.id);
            let id = files.allocate_job(registry)?;
            files.reserve_job(registry, kb, source, &id)?;
            files.write_json(
                &old_link,
                &JobLink {
                    source_id: source.into(),
                    archived: Some(old),
                },
            )?;
            let import = record.ingest.as_mut().ok_or(KbStoreCorrupt)?;
            import.id = id;
            import.document_job_id = None;
            import.error = None;
            import.suggestion = None;
            import.position = None;
            record.source.facts.status = SourceStatus::Queued;
            files.save_record(kb, &record)?;
            self.source_upserted(kb, &record.source);
            record.job(kb)
        })
    }

    pub fn cancel_ingest(&self, id: &str) -> StoreResult<IngestJob> {
        self.stop_ingest(id, SourceStatus::Cancelled, None, None)
    }

    pub fn interrupt_ingest(&self, id: &str) -> StoreResult<IngestJob> {
        self.stop_ingest(id, SourceStatus::Interrupted, None, None)
    }

    pub fn fail_ingest(
        &self,
        id: &str,
        code: KnowledgeErrorCode,
        suggestion: &'static str,
    ) -> StoreResult<IngestJob> {
        self.stop_ingest(id, SourceStatus::Failed, Some(code), Some(suggestion))
    }

    fn stop_ingest(
        &self,
        id: &str,
        status: SourceStatus,
        error: Option<KnowledgeErrorCode>,
        suggestion: Option<&str>,
    ) -> StoreResult<IngestJob> {
        if !is_knowledge_job_id(id) {
            return Err(KbInvalidInput);
        }
        self.locked(|files, registry| {
            let (kb, mut record) = files.current(registry, id)?;
            if record.source.facts.status == status {
                return record.job(&kb);
            }
            if !pending(record.source.facts.status) {
                return Err(KbJobConflict);
            }
            record.source.facts.status = status;
            let import = record.ingest.as_mut().ok_or(KbStoreCorrupt)?;
            import.error = error;
            import.suggestion = suggestion.map(str::to_owned);
            import.position = None;
            files.save_record(&kb, &record)?;
            self.source_upserted(&kb, &record.source);
            record.job(&kb)
        })
    }
}

fn valid_temp_suffix(name: &str) -> bool {
    name.len() == 6 && name.bytes().all(|c| c.is_ascii_alphanumeric())
}

fn valid_work_suffix(name: &str) -> bool {
    name.rsplit_once('-')
        .is_some_and(|(id, suffix)| is_knowledge_job_id(id) && valid_temp_suffix(suffix))
}

fn copy_artifact(source: &Path, dest: &Path, used: &mut u64) -> StoreResult<String> {
    let (mut file, metadata) = checked_input(source, MAX_OUTPUT_BYTES.saturating_sub(*used), None)?;
    check_space(dest.parent().ok_or(KbInvalidInput)?, metadata.len())?;
    let hash = copy_checked(&mut file, dest, metadata.len())?;
    *used += metadata.len();
    Ok(hash)
}

#[cfg(test)]
mod tests;
