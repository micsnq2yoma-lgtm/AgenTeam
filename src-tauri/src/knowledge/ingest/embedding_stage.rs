//! K21-b Post-`ready` online embedding stage owned by `KnowledgeIngestManager`.
//!
//! Responsibilities:
//! - Tracks per-source embedding status (`pending | embedding | ready | failed | skipped`),
//!   chunk progress (`embedded_chunks / total_chunks`), and token estimates.
//! - Splits uncached chunks into batches of <= 64 items and <= 100,000 chars, executing
//!   up to `EMBED_CONCURRENCY` (2) batches concurrently.
//! - Reuses cached `(model_revision, chunk_content_hash)` vectors across restarts so
//!   resumption is idempotent and never re-requests existing vectors.
//! - On model / host / dimension change, builds the new revision for the library, publishes
//!   `active.json` atomically with a higher `generation`, and cleans up stale revisions.
//! - Cancels in-flight/pending batches on source or library deletion and on auth/balance errors.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::knowledge::chunker::KnowledgeChunk;
use crate::knowledge::online_embedding::{
    format_online_model_revision, EmbeddingError, EmbeddingErrorKind, KnowledgeEmbeddingService,
    ResolvedEmbeddingEndpoint, MAX_ONLINE_EMBED_BATCH_CHARS, MAX_ONLINE_EMBED_BATCH_ITEMS,
    MAX_ONLINE_EMBED_ITEM_CHARS,
};
use crate::knowledge::store::{KnowledgeSource, KnowledgeStore, SourceStatus};
use crate::knowledge::vector_index::{KnowledgeVectorIndex, VectorChunkEntry};
use crate::ulog_warn;

pub const EMBED_CONCURRENCY: usize = 2;
const EMBED_BATCH_TIMEOUT: Duration = Duration::from_secs(30);
const SNIPPET_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEmbeddingStatusKind {
    Pending,
    Embedding,
    Ready,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceEmbeddingProgress {
    pub kb_id: String,
    pub source_id: String,
    pub status: SourceEmbeddingStatusKind,
    pub embedded_chunks: usize,
    pub total_chunks: usize,
    pub estimated_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<EmbeddingErrorKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEmbeddingSummary {
    pub kb_id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_model_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_dimension: Option<usize>,
    pub embedded_chunks: usize,
    pub total_chunks: usize,
    pub estimated_tokens: u64,
    pub ready_sources: usize,
    pub pending_sources: usize,
    pub embedding_sources: usize,
    pub failed_sources: usize,
    pub skipped_sources: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_kind: Option<EmbeddingErrorKind>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkEmbedSpec {
    pub chunk_id: String,
    pub chunk_content_hash: String,
    pub title: String,
    pub prefix: String,
    pub page: Option<u32>,
    pub snippet: String,
    pub embed_text: String,
    pub char_count: usize,
}

pub(crate) fn chunk_embed_text(chunk: &KnowledgeChunk) -> String {
    if chunk.prefix.trim().is_empty() {
        chunk.text.clone()
    } else {
        format!("{}\n{}", chunk.prefix, chunk.text)
    }
}

pub(crate) fn chunk_content_hash(embed_text: &str) -> String {
    format!("{:x}", Sha256::digest(embed_text.as_bytes()))
}

pub(crate) fn estimate_tokens_from_chars(chars: usize) -> u64 {
    // Bilingual zh/en heuristic: ~3 chars per token (ceiling).
    (chars as u64).div_ceil(3)
}

fn truncate_snippet(text: &str) -> String {
    if text.chars().count() <= SNIPPET_MAX_CHARS {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(SNIPPET_MAX_CHARS).collect();
        s.push('…');
        s
    }
}

fn build_chunk_specs(source: &KnowledgeSource, chunks: &[KnowledgeChunk]) -> Vec<ChunkEmbedSpec> {
    let title = source.display_title().to_string();
    chunks
        .iter()
        .map(|chunk| {
            let embed_text = chunk_embed_text(chunk);
            let hash = chunk_content_hash(&embed_text);
            let char_count = embed_text.chars().count().min(MAX_ONLINE_EMBED_ITEM_CHARS);
            ChunkEmbedSpec {
                chunk_id: chunk.chunk_id.clone(),
                chunk_content_hash: hash,
                title: title.clone(),
                prefix: chunk.prefix.clone(),
                page: chunk.page,
                snippet: truncate_snippet(&chunk.text),
                embed_text,
                char_count,
            }
        })
        .collect()
}

/// Split uncached chunk indices into batches respecting both item count (<= 64)
/// and character budget (<= 100,000 chars).
pub(crate) fn split_embedding_batches(specs: &[ChunkEmbedSpec], indices: &[usize]) -> Vec<Vec<usize>> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_chars = 0usize;

    for &idx in indices {
        let item_chars = specs[idx].char_count.max(1);
        let exceeds_items = current.len() >= MAX_ONLINE_EMBED_BATCH_ITEMS;
        let exceeds_chars =
            !current.is_empty() && current_chars.saturating_add(item_chars) > MAX_ONLINE_EMBED_BATCH_CHARS;
        if exceeds_items || exceeds_chars {
            batches.push(std::mem::take(&mut current));
            current_chars = 0;
        }
        current.push(idx);
        current_chars = current_chars.saturating_add(item_chars);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

fn revision_prefix_for_endpoint(endpoint: &ResolvedEmbeddingEndpoint) -> String {
    let probe = format_online_model_revision(&endpoint.host, &endpoint.model, 0);
    probe.strip_suffix('0').unwrap_or(&probe).to_string()
}

fn expected_revision_for_kb(
    vector_index: &KnowledgeVectorIndex,
    kb_id: &str,
    endpoint: &ResolvedEmbeddingEndpoint,
) -> Option<String> {
    if let Some(dim) = endpoint.dimensions {
        return Some(format_online_model_revision(&endpoint.host, &endpoint.model, dim));
    }
    let active = vector_index.active_generation(kb_id)?;
    let prefix = revision_prefix_for_endpoint(endpoint);
    active.model_revision.starts_with(&prefix).then_some(active.model_revision)
}

#[derive(Default)]
struct StageInner {
    sources: HashMap<(String, String), SourceEmbeddingProgress>,
    source_cancels: HashMap<(String, String), Arc<AtomicBool>>,
    kb_cancels: HashMap<String, Arc<AtomicBool>>,
}

/// Embedding stage state owned by `KnowledgeIngestManager`.
pub struct EmbeddingStage {
    store: Arc<KnowledgeStore>,
    vector_index: Arc<KnowledgeVectorIndex>,
    service: Arc<KnowledgeEmbeddingService>,
    inner: Mutex<StageInner>,
    closed: AtomicBool,
}

impl EmbeddingStage {
    pub fn new(
        store: Arc<KnowledgeStore>,
        vector_index: Arc<KnowledgeVectorIndex>,
        service: Arc<KnowledgeEmbeddingService>,
    ) -> Self {
        Self {
            store,
            vector_index,
            service,
            inner: Mutex::default(),
            closed: AtomicBool::new(false),
        }
    }

    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut guard) = self.inner.lock() {
            for flag in guard.source_cancels.values() {
                flag.store(true, Ordering::Release);
            }
            for flag in guard.kb_cancels.values() {
                flag.store(true, Ordering::Release);
            }
            guard.source_cancels.clear();
            guard.kb_cancels.clear();
        }
    }

    pub fn cancel_source(&self, kb_id: &str, source_id: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(flag) = guard
                .source_cancels
                .remove(&(kb_id.to_string(), source_id.to_string()))
            {
                flag.store(true, Ordering::Release);
            }
            guard.sources.remove(&(kb_id.to_string(), source_id.to_string()));
        }
        self.vector_index.remove_source_entries(kb_id, source_id);
    }

    pub fn cancel_library(&self, kb_id: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(flag) = guard.kb_cancels.remove(kb_id) {
                flag.store(true, Ordering::Release);
            }
            let matching_keys: Vec<(String, String)> = guard
                .source_cancels
                .keys()
                .filter(|(k, _)| k == kb_id)
                .cloned()
                .collect();
            for key in matching_keys {
                if let Some(flag) = guard.source_cancels.remove(&key) {
                    flag.store(true, Ordering::Release);
                }
            }
            guard.sources.retain(|(k, _), _| k != kb_id);
        }
        self.vector_index.clear_kb(kb_id);
    }

    fn set_progress(&self, progress: SourceEmbeddingProgress) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.sources.insert(
                (progress.kb_id.clone(), progress.source_id.clone()),
                progress,
            );
        }
    }

    fn register_source_cancel(&self, kb_id: &str, source_id: &str) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(prev) = guard.source_cancels.insert(
                (kb_id.to_string(), source_id.to_string()),
                Arc::clone(&cancel),
            ) {
                prev.store(true, Ordering::Release);
            }
        }
        cancel
    }

    fn register_kb_cancel(&self, kb_id: &str) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(prev) = guard.kb_cancels.insert(kb_id.to_string(), Arc::clone(&cancel)) {
                prev.store(true, Ordering::Release);
            }
        }
        cancel
    }

    fn is_cancelled(&self, source_cancel: &AtomicBool, kb_cancel: Option<&AtomicBool>) -> bool {
        self.closed.load(Ordering::Acquire)
            || source_cancel.load(Ordering::Acquire)
            || kb_cancel.is_some_and(|c| c.load(Ordering::Acquire))
    }

    /// Snapshot progress for one source, computing live fallback from Store + VectorIndex
    /// when not currently tracked in memory (e.g. immediately after app restart).
    pub fn source_progress(&self, kb_id: &str, source_id: &str) -> SourceEmbeddingProgress {
        if let Ok(guard) = self.inner.lock() {
            if let Some(p) = guard.sources.get(&(kb_id.to_string(), source_id.to_string())) {
                return p.clone();
            }
        }
        let endpoint = self.service.resolve_active_endpoint().ok();
        let expected_rev = endpoint
            .as_ref()
            .and_then(|ep| expected_revision_for_kb(&self.vector_index, kb_id, ep));

        let Ok(Some((source, chunks))) = self.store.search_document(kb_id, source_id) else {
            return SourceEmbeddingProgress {
                kb_id: kb_id.to_string(),
                source_id: source_id.to_string(),
                status: SourceEmbeddingStatusKind::Skipped,
                embedded_chunks: 0,
                total_chunks: 0,
                estimated_tokens: 0,
                model_revision: expected_rev,
                error_kind: None,
            };
        };

        let specs = build_chunk_specs(&source, &chunks);
        let total_chunks = specs.len();
        let total_chars: usize = specs.iter().map(|s| s.char_count).sum();
        let estimated_tokens = estimate_tokens_from_chars(total_chars);

        let Some(_) = endpoint else {
            return SourceEmbeddingProgress {
                kb_id: kb_id.to_string(),
                source_id: source_id.to_string(),
                status: SourceEmbeddingStatusKind::Skipped,
                embedded_chunks: 0,
                total_chunks,
                estimated_tokens,
                model_revision: None,
                error_kind: None,
            };
        };

        let embedded_chunks = expected_rev
            .as_deref()
            .map(|rev| {
                specs
                    .iter()
                    .filter(|s| {
                        self.vector_index
                            .lookup_cached(kb_id, rev, &s.chunk_content_hash)
                            .is_some()
                    })
                    .count()
            })
            .unwrap_or(0);

        let status = if total_chunks == 0 || embedded_chunks == total_chunks {
            SourceEmbeddingStatusKind::Ready
        } else {
            SourceEmbeddingStatusKind::Pending
        };

        SourceEmbeddingProgress {
            kb_id: kb_id.to_string(),
            source_id: source_id.to_string(),
            status,
            embedded_chunks,
            total_chunks,
            estimated_tokens,
            model_revision: expected_rev,
            error_kind: None,
        }
    }

    /// Aggregate embedding summary for a knowledge base (shown in Library Settings).
    pub fn library_summary(&self, kb_id: &str) -> LibraryEmbeddingSummary {
        let enabled = self
            .service
            .current_config()
            .is_some_and(|cfg| cfg.enabled);
        let active = self.vector_index.active_generation(kb_id);
        let sources = self.store.list_sources(kb_id).unwrap_or_default();

        let mut summary = LibraryEmbeddingSummary {
            kb_id: kb_id.to_string(),
            enabled,
            active_model_revision: active.as_ref().map(|p| p.model_revision.clone()),
            active_dimension: active.as_ref().map(|p| p.dimension),
            embedded_chunks: 0,
            total_chunks: 0,
            estimated_tokens: 0,
            ready_sources: 0,
            pending_sources: 0,
            embedding_sources: 0,
            failed_sources: 0,
            skipped_sources: 0,
            last_error_kind: None,
        };

        for src in sources.into_iter().filter(|s| s.facts.status == SourceStatus::Ready) {
            let progress = self.source_progress(kb_id, &src.id);
            summary.embedded_chunks += progress.embedded_chunks;
            summary.total_chunks += progress.total_chunks;
            summary.estimated_tokens += progress.estimated_tokens;
            match progress.status {
                SourceEmbeddingStatusKind::Ready => summary.ready_sources += 1,
                SourceEmbeddingStatusKind::Pending => summary.pending_sources += 1,
                SourceEmbeddingStatusKind::Embedding => summary.embedding_sources += 1,
                SourceEmbeddingStatusKind::Failed => {
                    summary.failed_sources += 1;
                    if summary.last_error_kind.is_none() {
                        summary.last_error_kind = progress.error_kind;
                    }
                }
                SourceEmbeddingStatusKind::Skipped => summary.skipped_sources += 1,
            }
        }
        summary
    }

    /// Spawn background post-`ready` embedding for a single source.
    pub fn spawn_source(self: &Arc<Self>, kb_id: String, source_id: String) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let stage = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let _ = stage.embed_source_now(&kb_id, &source_id).await;
        });
    }

    /// Spawn background library embedding / resume / model-switch rebuild.
    pub fn spawn_library(self: &Arc<Self>, kb_id: String) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let stage = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let _ = stage.embed_library_now(&kb_id).await;
        });
    }

    /// Resume all libraries on startup (idempotent via cache key `(model_revision, chunk_content_hash)`).
    pub fn spawn_resume_all(self: &Arc<Self>) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        if self.service.resolve_active_endpoint().is_err() {
            return;
        }
        let Ok(kbs) = self.store.list() else {
            return;
        };
        for kb in kbs {
            self.spawn_library(kb.id);
        }
    }

    /// Embed a single `Ready` source and upsert its vectors into `KnowledgeVectorIndex`.
    /// If the active model changed compared to the library's active generation, delegates
    /// to `embed_library_now` so all sources switch atomically under a new generation.
    pub async fn embed_source_now(
        &self,
        kb_id: &str,
        source_id: &str,
    ) -> Result<SourceEmbeddingProgress, EmbeddingError> {
        let endpoint = match self.service.resolve_active_endpoint() {
            Ok(ep) => ep,
            Err(err)
                if matches!(
                    err.kind,
                    EmbeddingErrorKind::Disabled | EmbeddingErrorKind::NotConfigured
                ) =>
            {
                let skipped = SourceEmbeddingProgress {
                    kb_id: kb_id.to_string(),
                    source_id: source_id.to_string(),
                    status: SourceEmbeddingStatusKind::Skipped,
                    embedded_chunks: 0,
                    total_chunks: 0,
                    estimated_tokens: 0,
                    model_revision: None,
                    error_kind: None,
                };
                self.set_progress(skipped.clone());
                return Ok(skipped);
            }
            Err(err) => {
                let failed = SourceEmbeddingProgress {
                    kb_id: kb_id.to_string(),
                    source_id: source_id.to_string(),
                    status: SourceEmbeddingStatusKind::Failed,
                    embedded_chunks: 0,
                    total_chunks: 0,
                    estimated_tokens: 0,
                    model_revision: None,
                    error_kind: Some(err.kind),
                };
                self.set_progress(failed);
                return Err(err);
            }
        };

        // If the library already has vectors under a DIFFERENT model revision, rebuild
        // the entire library atomically so generation flips cleanly and old vectors are removed.
        if let Some(active) = self.vector_index.active_generation(kb_id) {
            let expected = expected_revision_for_kb(&self.vector_index, kb_id, &endpoint);
            if expected.as_deref() != Some(active.model_revision.as_str()) {
                let _ = self.embed_library_now(kb_id).await;
                return Ok(self.source_progress(kb_id, source_id));
            }
        }

        let source_cancel = self.register_source_cancel(kb_id, source_id);
        let outcome = self
            .compute_source_entries(kb_id, source_id, &endpoint, &source_cancel, None)
            .await;

        match outcome {
            Ok(Some((model_revision, dimension, entries, progress))) => {
                if !self.is_cancelled(&source_cancel, None) {
                    if let Err(code) = self.vector_index.upsert_source_entries(
                        kb_id,
                        &model_revision,
                        dimension,
                        source_id,
                        entries,
                    ) {
                        ulog_warn!(
                            "[knowledge:embedding] failed to publish source vectors kb={} code={:?}",
                            kb_id,
                            code
                        );
                    }
                    self.set_progress(progress.clone());
                }
                Ok(progress)
            }
            Ok(None) => Ok(self.source_progress(kb_id, source_id)),
            Err(err) => Err(err),
        }
    }

    /// Embed all `Ready` sources in a library (idempotent resume or atomic model-switch rebuild).
    pub async fn embed_library_now(
        &self,
        kb_id: &str,
    ) -> Result<LibraryEmbeddingSummary, EmbeddingError> {
        let kb_cancel = self.register_kb_cancel(kb_id);
        let sources = self.store.list_sources(kb_id).unwrap_or_default();
        let ready_sources: Vec<KnowledgeSource> = sources
            .into_iter()
            .filter(|s| s.facts.status == SourceStatus::Ready)
            .collect();

        let endpoint = match self.service.resolve_active_endpoint() {
            Ok(ep) => ep,
            Err(err)
                if matches!(
                    err.kind,
                    EmbeddingErrorKind::Disabled | EmbeddingErrorKind::NotConfigured
                ) =>
            {
                for src in &ready_sources {
                    self.set_progress(SourceEmbeddingProgress {
                        kb_id: kb_id.to_string(),
                        source_id: src.id.clone(),
                        status: SourceEmbeddingStatusKind::Skipped,
                        embedded_chunks: 0,
                        total_chunks: 0,
                        estimated_tokens: 0,
                        model_revision: None,
                        error_kind: None,
                    });
                }
                return Ok(self.library_summary(kb_id));
            }
            Err(err) => {
                for src in &ready_sources {
                    self.set_progress(SourceEmbeddingProgress {
                        kb_id: kb_id.to_string(),
                        source_id: src.id.clone(),
                        status: SourceEmbeddingStatusKind::Failed,
                        embedded_chunks: 0,
                        total_chunks: 0,
                        estimated_tokens: 0,
                        model_revision: None,
                        error_kind: Some(err.kind),
                    });
                }
                return Err(err);
            }
        };

        let mut staged_entries: Vec<VectorChunkEntry> = Vec::new();
        let mut target_revision: Option<String> = None;
        let mut target_dimension: Option<usize> = None;
        let mut abort_error: Option<EmbeddingError> = None;

        for src in &ready_sources {
            if self.is_cancelled(&kb_cancel, None) {
                break;
            }
            let source_cancel = self.register_source_cancel(kb_id, &src.id);
            match self
                .compute_source_entries(
                    kb_id,
                    &src.id,
                    &endpoint,
                    &source_cancel,
                    Some(&kb_cancel),
                )
                .await
            {
                Ok(Some((rev, dim, entries, progress))) => {
                    target_revision.get_or_insert(rev);
                    target_dimension.get_or_insert(dim);
                    staged_entries.extend(entries);
                    self.set_progress(progress);
                }
                Ok(None) => {}
                Err(err) => {
                    // Auth / balance errors abort the whole library pass immediately (K21 §5.4).
                    if matches!(
                        err.kind,
                        EmbeddingErrorKind::AuthFailed | EmbeddingErrorKind::InsufficientBalance
                    ) {
                        abort_error = Some(err);
                        break;
                    }
                }
            }
        }

        if let (Some(rev), Some(dim)) = (target_revision, target_dimension) {
            if !self.is_cancelled(&kb_cancel, None)
                && self
                    .vector_index
                    .publish_revision(kb_id, &rev, dim, staged_entries)
                    .is_ok()
            {
                self.vector_index.cleanup_stale_revisions(kb_id, &rev);
            }
        }

        if let Some(err) = abort_error {
            return Err(err);
        }
        Ok(self.library_summary(kb_id))
    }

    async fn compute_source_entries(
        &self,
        kb_id: &str,
        source_id: &str,
        endpoint: &ResolvedEmbeddingEndpoint,
        source_cancel: &Arc<AtomicBool>,
        kb_cancel: Option<&Arc<AtomicBool>>,
    ) -> Result<Option<(String, usize, Vec<VectorChunkEntry>, SourceEmbeddingProgress)>, EmbeddingError>
    {
        if self.is_cancelled(source_cancel, kb_cancel.map(Arc::as_ref)) {
            return Ok(None);
        }

        let Ok(Some((source, chunks))) = self.store.search_document(kb_id, source_id) else {
            return Ok(None);
        };
        let specs = build_chunk_specs(&source, &chunks);
        let total_chunks = specs.len();
        let total_chars: usize = specs.iter().map(|s| s.char_count).sum();
        let estimated_tokens = estimate_tokens_from_chars(total_chars);

        if total_chunks == 0 {
            let progress = SourceEmbeddingProgress {
                kb_id: kb_id.to_string(),
                source_id: source_id.to_string(),
                status: SourceEmbeddingStatusKind::Ready,
                embedded_chunks: 0,
                total_chunks: 0,
                estimated_tokens: 0,
                model_revision: expected_revision_for_kb(&self.vector_index, kb_id, endpoint),
                error_kind: None,
            };
            self.set_progress(progress);
            return Ok(None);
        }

        let cached_rev = expected_revision_for_kb(&self.vector_index, kb_id, endpoint);
        let mut vectors: Vec<Option<(Vec<i8>, f32)>> = vec![None; total_chunks];
        let mut uncached_indices = Vec::new();
        let mut cached_count = 0usize;
        let mut resolved_dim = endpoint.dimensions;

        if let Some(ref rev) = cached_rev {
            for (idx, spec) in specs.iter().enumerate() {
                if let Some((v, scale)) =
                    self.vector_index
                        .lookup_cached(kb_id, rev, &spec.chunk_content_hash)
                {
                    if resolved_dim.is_none() {
                        resolved_dim = Some(v.len());
                    }
                    vectors[idx] = Some((v, scale));
                    cached_count += 1;
                } else {
                    uncached_indices.push(idx);
                }
            }
        } else {
            uncached_indices.extend(0..total_chunks);
        }

        let mut progress = SourceEmbeddingProgress {
            kb_id: kb_id.to_string(),
            source_id: source_id.to_string(),
            status: SourceEmbeddingStatusKind::Embedding,
            embedded_chunks: cached_count,
            total_chunks,
            estimated_tokens,
            model_revision: cached_rev.clone(),
            error_kind: None,
        };
        self.set_progress(progress.clone());

        let mut final_revision = cached_rev;
        let mut final_dimension = resolved_dim;

        if !uncached_indices.is_empty() {
            let batches = split_embedding_batches(&specs, &uncached_indices);
            let semaphore = Arc::new(Semaphore::new(EMBED_CONCURRENCY));
            let batch_abort = Arc::new(AtomicBool::new(false));
            let mut handles = Vec::with_capacity(batches.len());

            for batch_indices in batches {
                if self.is_cancelled(source_cancel, kb_cancel.map(Arc::as_ref))
                    || batch_abort.load(Ordering::Acquire)
                {
                    break;
                }
                let permit = semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| EmbeddingError::new(EmbeddingErrorKind::NetworkUnreachable))?;
                let items: Vec<(String, String)> = batch_indices
                    .iter()
                    .map(|&i| (specs[i].chunk_id.clone(), specs[i].embed_text.clone()))
                    .collect();
                let service = Arc::clone(&self.service);
                let ep = endpoint.clone();
                let sc = Arc::clone(source_cancel);
                let kc = kb_cancel.cloned();
                let ab = Arc::clone(&batch_abort);

                handles.push(tauri::async_runtime::spawn(async move {
                    let _permit = permit;
                    if sc.load(Ordering::Acquire)
                        || kc.as_ref().is_some_and(|c| c.load(Ordering::Acquire))
                        || ab.load(Ordering::Acquire)
                    {
                        return Ok(None);
                    }
                    match service
                        .embed_with_endpoint(
                            &ep,
                            &items,
                            EMBED_BATCH_TIMEOUT,
                            crate::knowledge::online_embedding::MAX_ONLINE_EMBED_RETRIES,
                        )
                        .await
                    {
                        Ok(out) => Ok(Some((batch_indices, out))),
                        Err(err) => {
                            if matches!(
                                err.kind,
                                EmbeddingErrorKind::AuthFailed
                                    | EmbeddingErrorKind::InsufficientBalance
                            ) {
                                ab.store(true, Ordering::Release);
                            }
                            Err(err)
                        }
                    }
                }));
            }

            let mut first_err: Option<EmbeddingError> = None;
            for handle in handles {
                match handle.await {
                    Ok(Ok(Some((batch_indices, out)))) => {
                        final_revision = Some(out.model_revision);
                        final_dimension = Some(out.dimension);
                        for (idx, vec) in batch_indices.into_iter().zip(out.vectors.into_iter()) {
                            vectors[idx] = Some((vec.vector, vec.scale));
                            cached_count += 1;
                        }
                        progress.embedded_chunks = cached_count;
                        progress.model_revision = final_revision.clone();
                        self.set_progress(progress.clone());
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(err)) => {
                        if first_err.is_none() {
                            first_err = Some(err);
                        }
                    }
                    Err(_) => {
                        if first_err.is_none() {
                            first_err = Some(EmbeddingError::new(EmbeddingErrorKind::NetworkUnreachable));
                        }
                    }
                }
            }

            if let Some(err) = first_err {
                progress.status = SourceEmbeddingStatusKind::Failed;
                progress.error_kind = Some(err.kind);
                self.set_progress(progress);
                return Err(err);
            }
        }

        if self.is_cancelled(source_cancel, kb_cancel.map(Arc::as_ref)) {
            return Ok(None);
        }

        let Some(dimension) = final_dimension else {
            return Ok(None);
        };
        let model_revision = final_revision
            .unwrap_or_else(|| format_online_model_revision(&endpoint.host, &endpoint.model, dimension));

        let mut entries = Vec::with_capacity(total_chunks);
        for (spec, maybe_vec) in specs.into_iter().zip(vectors.into_iter()) {
            let Some((vector, scale)) = maybe_vec else {
                progress.status = SourceEmbeddingStatusKind::Failed;
                progress.error_kind = Some(EmbeddingErrorKind::InvalidResponse);
                self.set_progress(progress);
                return Err(EmbeddingError::new(EmbeddingErrorKind::InvalidResponse));
            };
            entries.push(VectorChunkEntry {
                kb_id: kb_id.to_string(),
                source_id: source_id.to_string(),
                chunk_id: spec.chunk_id,
                chunk_content_hash: spec.chunk_content_hash,
                title: spec.title,
                prefix: spec.prefix,
                page: spec.page,
                snippet: spec.snippet,
                vector,
                scale,
            });
        }

        progress.status = SourceEmbeddingStatusKind::Ready;
        progress.embedded_chunks = entries.len();
        progress.model_revision = Some(model_revision.clone());
        progress.error_kind = None;
        Ok(Some((model_revision, dimension, entries, progress)))
    }
}
