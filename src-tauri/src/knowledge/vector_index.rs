//! K18 persistent int8 vector index and BM25 + Vector RRF hybrid fusion.
//!
//! Layout per knowledge base:
//!   `<app-data>/knowledge/<kb-id>/vectors/<model-revision>/entries.json`
//!   `<app-data>/knowledge/<kb-id>/vectors/active.json` (atomic generation pointer)
//!
//! Properties:
//! - Cache key = `(model_revision, chunk_content_hash)`: moving/renaming a file
//!   keeps `chunk_content_hash` unchanged and never triggers re-embedding.
//! - Switching embedding model revisions builds the new revision directory and
//!   atomically flips `active.json` with a monotonic `generation` counter.
//! - Derived disposable data: missing/corrupt vector files or an uninstalled
//!   model pack degrade cleanly to `mode = "bm25"` with zero errors.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::knowledge::embedding::cosine_similarity_int8;
use crate::knowledge::types::{is_knowledge_base_id, KnowledgeErrorCode};
use crate::search::KnowledgeSearchHit;
use crate::workspace_files::path_safety::{
    ensure_workspace_directory_no_follow, read_workspace_file_no_follow,
    write_workspace_file_no_follow,
};
use KnowledgeErrorCode::{KbInvalidInput, KbStoreUnavailable};

const MAX_VECTOR_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_POINTER_BYTES: u64 = 16 * 1024;
pub const HYBRID_RRF_K: f64 = 60.0;
const SOURCE_CAP: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorChunkEntry {
    pub kb_id: String,
    pub source_id: String,
    pub chunk_id: String,
    pub chunk_content_hash: String,
    pub title: String,
    pub prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    pub snippet: String,
    pub vector: Vec<i8>,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorGenerationPointer {
    pub schema_version: u32,
    pub model_revision: String,
    pub dimension: usize,
    pub generation: u64,
    pub entry_count: usize,
}

#[derive(Debug, Clone)]
struct LoadedRevision {
    pointer: VectorGenerationPointer,
    entries: Vec<VectorChunkEntry>,
    by_content_hash: HashMap<String, usize>,
}

pub struct KnowledgeVectorIndex {
    knowledge_root: PathBuf,
    cache: RwLock<HashMap<String, LoadedRevision>>,
}

fn is_safe_revision(rev: &str) -> bool {
    !rev.is_empty()
        && rev.len() <= 128
        && rev
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !rev.starts_with('.')
        && !rev.contains("..")
}

impl KnowledgeVectorIndex {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            knowledge_root: app_data_dir.join("knowledge"),
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn pointer_rel(kb_id: &str) -> String {
        format!("{kb_id}/vectors/active.json")
    }

    fn entries_rel(kb_id: &str, revision: &str) -> String {
        format!("{kb_id}/vectors/{revision}/entries.json")
    }

    /// Look up cached quantized vector by `(model_revision, chunk_content_hash)`.
    pub fn lookup_cached(
        &self,
        kb_id: &str,
        model_revision: &str,
        chunk_content_hash: &str,
    ) -> Option<(Vec<i8>, f32)> {
        self.ensure_loaded(kb_id);
        let guard = self.cache.read().ok()?;
        let loaded = guard.get(kb_id)?;
        if loaded.pointer.model_revision != model_revision {
            return None;
        }
        let &idx = loaded.by_content_hash.get(chunk_content_hash)?;
        let entry = loaded.entries.get(idx)?;
        Some((entry.vector.clone(), entry.scale))
    }

    pub fn active_generation(&self, kb_id: &str) -> Option<VectorGenerationPointer> {
        self.ensure_loaded(kb_id);
        self.cache
            .read()
            .ok()?
            .get(kb_id)
            .map(|l| l.pointer.clone())
    }

    /// Publish a full revision snapshot atomically and increment generation.
    pub fn publish_revision(
        &self,
        kb_id: &str,
        model_revision: &str,
        dimension: usize,
        entries: Vec<VectorChunkEntry>,
    ) -> Result<VectorGenerationPointer, KnowledgeErrorCode> {
        if !is_knowledge_base_id(kb_id) || !is_safe_revision(model_revision) || dimension == 0 {
            return Err(KbInvalidInput);
        }
        for entry in &entries {
            if entry.vector.len() != dimension {
                return Err(KbInvalidInput);
            }
        }
        fs::create_dir_all(&self.knowledge_root).map_err(|_| KbStoreUnavailable)?;
        let _ = ensure_workspace_directory_no_follow(
            &self.knowledge_root,
            &format!("{kb_id}/vectors/{model_revision}"),
        )
        .map_err(|_| KbStoreUnavailable)?;

        let prev_gen = self
            .active_generation(kb_id)
            .map(|p| p.generation)
            .unwrap_or(0);
        let pointer = VectorGenerationPointer {
            schema_version: 1,
            model_revision: model_revision.to_string(),
            dimension,
            generation: prev_gen.saturating_add(1),
            entry_count: entries.len(),
        };

        let entries_bytes = serde_json::to_vec(&entries).map_err(|_| KbStoreUnavailable)?;
        write_workspace_file_no_follow(
            &self.knowledge_root,
            &Self::entries_rel(kb_id, model_revision),
            &entries_bytes,
        )
        .map_err(|_| KbStoreUnavailable)?;

        let pointer_bytes = serde_json::to_vec_pretty(&pointer).map_err(|_| KbStoreUnavailable)?;
        write_workspace_file_no_follow(
            &self.knowledge_root,
            &Self::pointer_rel(kb_id),
            &pointer_bytes,
        )
        .map_err(|_| KbStoreUnavailable)?;

        let by_content_hash: HashMap<String, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.chunk_content_hash.clone(), i))
            .collect();
        if let Ok(mut guard) = self.cache.write() {
            guard.insert(
                kb_id.to_string(),
                LoadedRevision {
                    pointer: pointer.clone(),
                    entries,
                    by_content_hash,
                },
            );
        }
        Ok(pointer)
    }

    /// Remove all vector data for a library (e.g. on library deletion or model uninstall).
    pub fn clear_kb(&self, kb_id: &str) {
        if let Ok(mut guard) = self.cache.write() {
            guard.remove(kb_id);
        }
        if is_knowledge_base_id(kb_id) {
            let dir = self.knowledge_root.join(kb_id).join("vectors");
            if dir.exists() {
                let _ = fs::remove_dir_all(dir);
            }
        }
    }

    /// Return cloned entries for `(kb_id, model_revision)` if currently active.
    pub fn current_entries(&self, kb_id: &str, model_revision: &str) -> Vec<VectorChunkEntry> {
        self.ensure_loaded(kb_id);
        let Ok(guard) = self.cache.read() else {
            return Vec::new();
        };
        match guard.get(kb_id) {
            Some(loaded) if loaded.pointer.model_revision == model_revision => {
                loaded.entries.clone()
            }
            _ => Vec::new(),
        }
    }

    /// Atomically replace one source's entries within `model_revision`, publish a new
    /// generation pointer, and clean up stale sibling revision directories on success.
    pub fn upsert_source_entries(
        &self,
        kb_id: &str,
        model_revision: &str,
        dimension: usize,
        source_id: &str,
        new_entries: Vec<VectorChunkEntry>,
    ) -> Result<VectorGenerationPointer, KnowledgeErrorCode> {
        let mut merged: Vec<VectorChunkEntry> = self
            .current_entries(kb_id, model_revision)
            .into_iter()
            .filter(|e| e.source_id != source_id)
            .collect();
        merged.extend(new_entries);
        let pointer = self.publish_revision(kb_id, model_revision, dimension, merged)?;
        self.cleanup_stale_revisions(kb_id, model_revision);
        Ok(pointer)
    }

    /// Remove all entries belonging to `source_id` from the active revision, if present.
    pub fn remove_source_entries(&self, kb_id: &str, source_id: &str) {
        self.ensure_loaded(kb_id);
        let snapshot = {
            let Ok(guard) = self.cache.read() else {
                return;
            };
            guard.get(kb_id).map(|loaded| {
                let had_source = loaded.entries.iter().any(|e| e.source_id == source_id);
                (
                    had_source,
                    loaded.pointer.model_revision.clone(),
                    loaded.pointer.dimension,
                    loaded.entries.clone(),
                )
            })
        };
        if let Some((true, rev, dim, entries)) = snapshot {
            let remaining: Vec<VectorChunkEntry> = entries
                .into_iter()
                .filter(|e| e.source_id != source_id)
                .collect();
            let _ = self.publish_revision(kb_id, &rev, dim, remaining);
        }
    }

    /// Remove old revision directories under `<knowledge_root>/<kb_id>/vectors/`
    /// once `keep_revision` has been atomically published.
    pub fn cleanup_stale_revisions(&self, kb_id: &str, keep_revision: &str) {
        if !is_knowledge_base_id(kb_id) || !is_safe_revision(keep_revision) {
            return;
        }
        let vectors_dir = self.knowledge_root.join(kb_id).join("vectors");
        let Ok(read_dir) = fs::read_dir(&vectors_dir) else {
            return;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name != keep_revision {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }

    fn ensure_loaded(&self, kb_id: &str) {
        if !is_knowledge_base_id(kb_id) {
            return;
        }
        if let Ok(guard) = self.cache.read() {
            if guard.contains_key(kb_id) {
                return;
            }
        }
        let Ok((_, ptr_bytes)) = read_workspace_file_no_follow(
            &self.knowledge_root,
            &Self::pointer_rel(kb_id),
            MAX_POINTER_BYTES,
        ) else {
            return;
        };
        let Ok(pointer) = serde_json::from_slice::<VectorGenerationPointer>(&ptr_bytes) else {
            return;
        };
        if pointer.schema_version != 1 || !is_safe_revision(&pointer.model_revision) {
            return;
        }
        let Ok((_, entries_bytes)) = read_workspace_file_no_follow(
            &self.knowledge_root,
            &Self::entries_rel(kb_id, &pointer.model_revision),
            MAX_VECTOR_FILE_BYTES,
        ) else {
            return;
        };
        let Ok(entries) = serde_json::from_slice::<Vec<VectorChunkEntry>>(&entries_bytes) else {
            return;
        };
        let by_content_hash: HashMap<String, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.chunk_content_hash.clone(), i))
            .collect();
        if let Ok(mut guard) = self.cache.write() {
            guard.insert(
                kb_id.to_string(),
                LoadedRevision {
                    pointer,
                    entries,
                    by_content_hash,
                },
            );
        }
    }

    /// Top-K cosine similarity search across the requested libraries for an active model revision.
    /// Returns empty vec if no matching revision vectors exist (triggers clean BM25 fallback).
    pub fn search_top_k(
        &self,
        kb_ids: &[String],
        active_revision: &str,
        query_vec: &[i8],
        query_scale: f32,
        limit: usize,
    ) -> Vec<KnowledgeSearchHit> {
        if kb_ids.is_empty() || query_vec.is_empty() || limit == 0 {
            return Vec::new();
        }
        for kb_id in kb_ids {
            self.ensure_loaded(kb_id);
        }
        let Ok(guard) = self.cache.read() else {
            return Vec::new();
        };

        let mut scored: Vec<(f32, &VectorChunkEntry)> = Vec::new();
        for kb_id in kb_ids {
            let Some(loaded) = guard.get(kb_id) else {
                continue;
            };
            if loaded.pointer.model_revision != active_revision
                || loaded.pointer.dimension != query_vec.len()
            {
                continue;
            }
            for entry in &loaded.entries {
                let sim = cosine_similarity_int8(query_vec, query_scale, &entry.vector, entry.scale);
                if sim > 0.0 {
                    scored.push((sim, entry));
                }
            }
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));

        let mut per_source: HashMap<(&str, &str), usize> = HashMap::new();
        let mut hits = Vec::with_capacity(limit);
        for (sim, entry) in scored {
            let count = per_source
                .entry((entry.kb_id.as_str(), entry.source_id.as_str()))
                .or_insert(0);
            if *count >= SOURCE_CAP {
                continue;
            }
            *count += 1;
            let reference = format!(
                "kb://{}/{}#{}",
                entry.kb_id, entry.source_id, entry.chunk_id
            );
            hits.push(KnowledgeSearchHit {
                reference,
                kb_id: entry.kb_id.clone(),
                source_id: entry.source_id.clone(),
                chunk_id: entry.chunk_id.clone(),
                title: entry.title.clone(),
                prefix: entry.prefix.clone(),
                page: entry.page,
                snippet: entry.snippet.clone(),
                score: sim as f64,
            });
            if hits.len() >= limit {
                break;
            }
        }
        hits
    }

    pub fn knowledge_root(&self) -> &Path {
        &self.knowledge_root
    }
}

/// Fuse BM25 hits and Vector hits via Reciprocal Rank Fusion (`1 / (k + rank)`).
///
/// Degradation rule:
/// - If `vector_hits` is empty (no model installed or no vectors indexed),
///   returns `("bm25", bm25_hits[..limit])`.
/// - Otherwise returns `("hybrid", rrf_fused[..limit])`.
pub fn fuse_bm25_and_vector_rrf(
    bm25_hits: Vec<KnowledgeSearchHit>,
    vector_hits: Vec<KnowledgeSearchHit>,
    limit: usize,
) -> (&'static str, Vec<KnowledgeSearchHit>) {
    if vector_hits.is_empty() {
        return ("bm25", bm25_hits.into_iter().take(limit).collect());
    }
    if bm25_hits.is_empty() {
        return ("hybrid", vector_hits.into_iter().take(limit).collect());
    }

    let mut scores: BTreeMap<String, f64> = BTreeMap::new();
    let mut templates: HashMap<String, KnowledgeSearchHit> = HashMap::new();

    for (rank, hit) in bm25_hits.into_iter().enumerate() {
        let key = format!("{}/{}/{}", hit.kb_id, hit.source_id, hit.chunk_id);
        *scores.entry(key.clone()).or_insert(0.0) += 1.0 / (HYBRID_RRF_K + (rank as f64) + 1.0);
        templates.entry(key).or_insert(hit);
    }
    for (rank, hit) in vector_hits.into_iter().enumerate() {
        let key = format!("{}/{}/{}", hit.kb_id, hit.source_id, hit.chunk_id);
        *scores.entry(key.clone()).or_insert(0.0) += 1.0 / (HYBRID_RRF_K + (rank as f64) + 1.0);
        templates.entry(key).or_insert(hit);
    }

    let mut ranked: Vec<(f64, String)> = scores.into_iter().map(|(k, s)| (s, k)).collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let mut per_source: HashMap<(String, String), usize> = HashMap::new();
    let mut fused = Vec::with_capacity(limit);
    for (score, key) in ranked {
        let Some(mut hit) = templates.remove(&key) else {
            continue;
        };
        let count = per_source
            .entry((hit.kb_id.clone(), hit.source_id.clone()))
            .or_insert(0);
        if *count >= SOURCE_CAP {
            continue;
        }
        *count += 1;
        hit.score = score;
        fused.push(hit);
        if fused.len() >= limit {
            break;
        }
    }
    ("hybrid", fused)
}
