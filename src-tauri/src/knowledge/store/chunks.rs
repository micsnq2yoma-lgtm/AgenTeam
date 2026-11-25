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

    /// Return the derived chunks for a ready source, or an empty list if not yet ready.
    pub fn list_chunks(&self, kb: &str, source_id: &str) -> StoreResult<Vec<KnowledgeChunk>> {
        let source = self.get_source(kb, source_id)?;
        if source.facts.status != SourceStatus::Ready || source.facts.content_hash.is_none() {
            return Ok(Vec::new());
        }
        let result = self.read_content_chunks(kb, &source);
        let current = self.get_source(kb, source_id)?;
        if current.facts.content_hash != source.facts.content_hash
            || current.facts.status != SourceStatus::Ready
        {
            return Err(KnowledgeErrorCode::KbJobConflict);
        }
        result.map_err(|failure| failure.code)
    }

    pub(super) fn resolve_citation_details(
        &self,
        reference: &KnowledgeRef,
        source: &KnowledgeSource,
        original_extension: Option<&str>,
        linked_folder_root: Option<&Path>,
        source_dir: &Path,
    ) -> StoreResult<(
        Option<ResolvedFragment>,
        Option<String>,
        Option<ResolvedPreview>,
    )> {
        let ready_content = if source.facts.status == SourceStatus::Ready
            && source.facts.content_hash.is_some()
        {
            let markdown = self.read_published_content(&reference.kb_id, source)?;
            let locale = crate::i18n::locale_from_config(&self.app_data_dir.join("config.json"));
            let chunks = chunk_markdown(
                &reference.kb_id,
                &source.id,
                source.display_title(),
                &markdown,
                locale,
            )
            .map_err(|err| match err {
                ChunkError::GraphemeExceedsBudget(_) => KnowledgeErrorCode::KbChunkUnsplittable,
                ChunkError::InvalidReference => KbStoreCorrupt,
                ChunkError::InsufficientContextSpace => KbLimitExceeded,
            })?;
            let current = self.get_source(&reference.kb_id, &reference.source_id)?;
            if current.facts.content_hash != source.facts.content_hash
                || current.facts.status != SourceStatus::Ready
            {
                return Err(KnowledgeErrorCode::KbJobConflict);
            }
            Some((markdown, chunks))
        } else {
            None
        };

        let mut matched_chunk: Option<KnowledgeChunk> = None;
        let fragment = match &reference.chunk_id {
            None => None,
            Some(id) => match &ready_content {
                None => Some(ResolvedFragment::Unavailable),
                Some((_, chunks)) => match chunks.iter().find(|c| &c.chunk_id == id).cloned() {
                    Some(c) => {
                        matched_chunk = Some(c.clone());
                        Some(ResolvedFragment::Available { chunk: Box::new(c) })
                    }
                    None => Some(ResolvedFragment::Unavailable),
                },
            },
        };

        let page_chunk = ready_content.as_ref().and_then(|(_, chunks)| {
            reference
                .page
                .and_then(|p| chunks.iter().find(|c| c.page == Some(p)))
        });

        let summary_text = source
            .metadata
            .get(&MetadataKey::Summary)
            .and_then(|field| field.value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty());

        let excerpt = matched_chunk
            .as_ref()
            .map(|c| truncate_excerpt(&c.text, 300))
            .or_else(|| page_chunk.map(|c| truncate_excerpt(&c.text, 300)))
            .or_else(|| summary_text.map(|s| truncate_excerpt(s, 300)))
            .or_else(|| {
                ready_content
                    .as_ref()
                    .and_then(|(_, chunks)| chunks.first())
                    .map(|c| truncate_excerpt(&c.text, 300))
            })
            .filter(|s| !s.is_empty());

        let text_target_chunk = matched_chunk.as_ref().or(page_chunk);
        let (utf16_range, line_number) = match (&ready_content, text_target_chunk) {
            (Some((markdown, _)), Some(chunk)) => (
                Some(chunk.utf16_range),
                Some(utf16_offset_to_line(markdown, chunk.utf16_range[0])),
            ),
            _ => (None, None),
        };

        let resolved_page = matched_chunk
            .as_ref()
            .and_then(|c| c.page)
            .or(reference.page);

        let preview = resolve_preview_target(
            source,
            original_extension,
            linked_folder_root,
            source_dir,
            resolved_page,
            utf16_range,
            line_number,
        );

        Ok((fragment, excerpt, preview))
    }
}

const MAX_EXCERPT_CHARS: usize = 300;

fn truncate_excerpt(text: &str, max_chars: usize) -> String {
    let limit = max_chars.min(MAX_EXCERPT_CHARS);
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(limit).collect();
    while out.ends_with(char::is_whitespace) {
        out.pop();
    }
    out.push('…');
    out
}

fn utf16_offset_to_line(text: &str, utf16_offset: usize) -> usize {
    let mut line = 1usize;
    let mut units = 0usize;
    for ch in text.chars() {
        if units >= utf16_offset {
            break;
        }
        units += ch.len_utf16();
        if ch == '\n' {
            line += 1;
        }
    }
    line
}

fn is_plain_text_extension(ext: &str) -> bool {
    ["md", "markdown", "txt"]
        .iter()
        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
}

fn regular_child_no_follow(parent: &Path, filename: &str) -> Option<PathBuf> {
    let candidate = parent.join(filename);
    let meta = fs::symlink_metadata(&candidate).ok()?;
    if !meta.is_file() || crate::workspace_files::path_safety::metadata_is_link_like(&meta) {
        return None;
    }
    let canonical = fs::canonicalize(&candidate).ok()?;
    if canonical.parent() != Some(parent) {
        return None;
    }
    Some(canonical)
}

fn file_sha256_matches(path: &Path, expected_hex: &str) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    format!("{:x}", Sha256::digest(&bytes)) == expected_hex
}

fn normalize_preview_path(path: PathBuf) -> String {
    crate::sidecar::normalize_external_path(path)
        .to_string_lossy()
        .into_owned()
}

fn resolve_linked_original_no_follow(
    folder_root: &Path,
    current_path: &Path,
) -> Option<PathBuf> {
    let rel = current_path.strip_prefix(folder_root).ok()?;
    let rel_str = rel.to_str()?;
    let (resolved, _file) = crate::workspace_files::path_safety::open_workspace_regular_file_no_follow_classified(
        folder_root,
        rel_str,
        "linked knowledge preview",
    )
    .ok()?;
    Some(resolved)
}

fn resolve_preview_target(
    source: &KnowledgeSource,
    original_extension: Option<&str>,
    linked_folder_root: Option<&Path>,
    source_dir: &Path,
    page: Option<u32>,
    utf16_range: Option<[usize; 2]>,
    line_number: Option<usize>,
) -> Option<ResolvedPreview> {
    let valid_ext = original_extension
        .filter(|ext| !ext.is_empty() && ext.bytes().all(|b| b.is_ascii_alphanumeric()));
    let content_path = regular_child_no_follow(source_dir, "content.md");
    let display_name = source.display_title().to_string();

    if source.facts.kind == SourceKind::LinkedFile {
        if let (Some(root), Some(current)) = (linked_folder_root, source.facts.current_path.as_deref())
        {
            if let Some(verified) = resolve_linked_original_no_follow(root, current) {
                let ext = valid_ext.or_else(|| {
                    verified.extension().and_then(|e| e.to_str())
                });
                if ext.is_some_and(|e| !is_plain_text_extension(e)) {
                    return Some(ResolvedPreview {
                        path: normalize_preview_path(verified),
                        name: display_name,
                        is_content_fallback: false,
                        page,
                        utf16_range: None,
                        line_number: None,
                    });
                }
                let coordinates_match = source
                    .facts
                    .content_hash
                    .as_deref()
                    .is_some_and(|hash| file_sha256_matches(&verified, hash));
                if coordinates_match || content_path.is_none() {
                    return Some(ResolvedPreview {
                        path: normalize_preview_path(verified),
                        name: display_name,
                        is_content_fallback: false,
                        page,
                        utf16_range: if coordinates_match { utf16_range } else { None },
                        line_number: if coordinates_match { line_number } else { None },
                    });
                }
            }
        }
        return content_path.map(|content| ResolvedPreview {
            path: normalize_preview_path(content),
            name: display_name,
            is_content_fallback: true,
            page,
            utf16_range,
            line_number,
        });
    }

    let original_path =
        valid_ext.and_then(|ext| regular_child_no_follow(source_dir, &format!("original.{ext}")));

    if let (Some(ext), Some(original)) = (valid_ext, original_path) {
        if !is_plain_text_extension(ext) {
            return Some(ResolvedPreview {
                path: normalize_preview_path(original),
                name: display_name,
                is_content_fallback: false,
                page,
                utf16_range: None,
                line_number: None,
            });
        }
        let coordinates_match = source
            .facts
            .content_hash
            .as_deref()
            .is_some_and(|hash| file_sha256_matches(&original, hash));
        if coordinates_match || content_path.is_none() {
            return Some(ResolvedPreview {
                path: normalize_preview_path(original),
                name: display_name,
                is_content_fallback: false,
                page,
                utf16_range: if coordinates_match { utf16_range } else { None },
                line_number: if coordinates_match { line_number } else { None },
            });
        }
    }

    content_path.map(|content| ResolvedPreview {
        path: normalize_preview_path(content),
        name: display_name,
        is_content_fallback: source.facts.kind == SourceKind::File,
        page,
        utf16_range,
        line_number,
    })
}
