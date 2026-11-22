//! Markdown is authoritative. Ingest and citation lookup use the same digest
//! and chunker, without keeping a second mutable copy of the chunk list.
use super::*;
use crate::document_processing::MAX_OUTPUT_BYTES;
use crate::knowledge::chunker::{chunk_markdown, ChunkError, ChunkPosition, KnowledgeChunk};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ResolvedFragment {
    Available { chunk: Box<KnowledgeChunk> },
    Unavailable,
}

struct ReadFailure {
    code: KnowledgeErrorCode,
    position: Option<ChunkPosition>,
}

impl From<KnowledgeErrorCode> for ReadFailure {
    fn from(code: KnowledgeErrorCode) -> Self {
        Self {
            code,
            position: None,
        }
    }
}

impl KnowledgeStore {
    /// Only the same attempt, phase and Markdown version may settle this job.
    /// Failure keeps the published Markdown and its managed original for retry.
    pub(crate) fn finish_chunking(&self, job_id: &str, cancelled: &AtomicBool) -> StoreResult<()> {
        let (kb, snapshot) = self.locked(|files, registry| {
            let (kb, record) = files.current(registry, job_id)?;
            if record.source.facts.status != SourceStatus::Chunking
                || cancelled.load(Ordering::Acquire)
            {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            Ok((kb, record.source))
        })?;
        let result = self.read_content_chunks(&kb, &snapshot).map(drop);
        self.locked(|files, registry| {
            let (_, mut record) = files.current(registry, job_id)?;
            if cancelled.load(Ordering::Acquire)
                || record.source.facts.status != SourceStatus::Chunking
                || record.source.facts.content_hash != snapshot.facts.content_hash
            {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            match result {
                Ok(()) => {
                    record.source.facts.status = SourceStatus::Ready;
                    // A wiki page is curator output, never curator input.
                    record.curation_pending = record.source.facts.kind != SourceKind::Wiki;
                }
                Err(failure) => {
                    let import = record.ingest.as_mut().ok_or(KbStoreCorrupt)?;
                    import.error = Some(failure.code);
                    import.suggestion = Some(
                        if failure.position.is_some() {
                            "simplifyGraphemeAndRetry"
                        } else {
                            "checkStorageAndRetry"
                        }
                        .to_string(),
                    );
                    import.position = failure.position;
                    record.source.facts.status = SourceStatus::Failed;
                }
            }
            files.save_record(&kb, &record)?;
            self.source_upserted(&kb, &record.source);
            Ok(())
        })
    }

    fn read_content_chunks(
        &self,
        kb: &str,
        source: &KnowledgeSource,
    ) -> Result<Vec<KnowledgeChunk>, ReadFailure> {
        let markdown = self.read_published_content(kb, source)?;
        let locale = crate::i18n::locale_from_config(&self.app_data_dir.join("config.json"));
        chunk_markdown(kb, &source.id, source.display_title(), &markdown, locale).map_err(|err| {
            match err {
                ChunkError::GraphemeExceedsBudget(position) => ReadFailure {
                    code: KnowledgeErrorCode::KbChunkUnsplittable,
                    position: Some(position),
                },
                ChunkError::InvalidReference => KbStoreCorrupt.into(),
                ChunkError::InsufficientContextSpace => KbLimitExceeded.into(),
            }
        })
    }

    pub(crate) fn read_published_content(
        &self,
        kb: &str,
        source: &KnowledgeSource,
    ) -> StoreResult<String> {
        let expected = source.facts.content_hash.as_deref().ok_or(KbStoreCorrupt)?;
        let relative = format!("knowledge/{kb}/sources/{}/content.md", source.id);
        let (_, bytes) =
            read_workspace_file_no_follow(&self.app_data_dir, &relative, MAX_OUTPUT_BYTES)
                .map_err(|_| KbStoreUnavailable)?;
        if format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Err(KbStoreCorrupt);
        }
        String::from_utf8(bytes).map_err(|_| KbStoreCorrupt)
    }

    /// Project one committed source for a derived reader. If its content or
    /// metadata changes while chunking, the consumer retries with the new record.
    pub(crate) fn search_document(
        &self,
        kb: &str,
        source_id: &str,
    ) -> StoreResult<Option<(KnowledgeSource, Vec<KnowledgeChunk>)>> {
        let source = self.get_source(kb, source_id)?;
        if source.facts.status != SourceStatus::Ready {
            return Ok(None);
        }
        let result = self.read_content_chunks(kb, &source);
        let current = self.get_source(kb, source_id)?;
        if current != source {
            return Err(KnowledgeErrorCode::KbJobConflict);
        }
        let chunks = result.map_err(|failure| failure.code)?;
        Ok(Some((source, chunks)))
    }

    pub(super) fn resolve_fragment(
        &self,
        reference: &KnowledgeRef,
        source: &KnowledgeSource,
    ) -> StoreResult<Option<ResolvedFragment>> {
        let Some(id) = &reference.chunk_id else {
            return Ok(None);
        };
        if source.facts.status != SourceStatus::Ready || source.facts.content_hash.is_none() {
            return Ok(Some(ResolvedFragment::Unavailable));
        }
        let result = self.read_content_chunks(&reference.kb_id, source);
        let current = self.get_source(&reference.kb_id, &reference.source_id)?;
        if current.facts.content_hash != source.facts.content_hash
            || current.facts.status != SourceStatus::Ready
        {
            return Err(KnowledgeErrorCode::KbJobConflict);
        }
        let chunk = result
            .map_err(|err| err.code)?
            .into_iter()
            .find(|c| &c.chunk_id == id);
        Ok(Some(match chunk {
            Some(c) => ResolvedFragment::Available { chunk: Box::new(c) },
            None => ResolvedFragment::Unavailable,
        }))
    }
}
