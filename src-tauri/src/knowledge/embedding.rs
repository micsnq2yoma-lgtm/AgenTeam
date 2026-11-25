//! K17 host-side embedding execution client (`KnowledgeEmbeddingClient`).
//!
//! Coordinates `KnowledgeModelPackManager` (active signed model pack) and
//! `LocalComputeCoordinator` (shared heavy-compute lease with OCR/ASR),
//! dispatching stateless single-workload `embed` batches to `myagents-document-worker`
//! and returning symmetrically quantized int8 vectors.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::knowledge::model_pack::{ActivatedEmbeddingPack, KnowledgeModelPackManager};
use crate::knowledge::types::KnowledgeErrorCode;
use crate::local_inference::{
    ComputeWorkloadIdentity, ComputeWorkloadKind, LocalComputeCoordinator,
};
use KnowledgeErrorCode::{KbInvalidInput, KbLimitExceeded, KbStoreUnavailable};

pub const EMBED_PROTOCOL_VERSION: u32 = 3;
pub const MAX_EMBED_BATCH_ITEMS: usize = 256;
pub const MAX_EMBED_ITEM_BYTES: usize = 32 * 1024;
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedInputItem {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantizedEmbedding {
    pub id: String,
    pub vector: Vec<i8>,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedBatchResult {
    pub model_revision: String,
    pub dimension: usize,
    pub vectors: Vec<QuantizedEmbedding>,
    pub elapsed_ms: u64,
}

/// Symmetric int8 scalar quantization after optional L2 normalization.
pub fn quantize_l2_int8(raw: &[f32], normalize: bool) -> (Vec<i8>, f32) {
    if raw.is_empty() {
        return (Vec::new(), 1.0);
    }
    let mut work: Vec<f32> = raw
        .iter()
        .map(|&v| if v.is_finite() { v } else { 0.0 })
        .collect();
    if normalize {
        let norm_sq: f64 = work.iter().map(|&v| (v as f64) * (v as f64)).sum();
        let norm = norm_sq.sqrt();
        if norm > 1e-12 {
            for v in &mut work {
                *v = ((*v as f64) / norm) as f32;
            }
        }
    }
    let max_abs = work.iter().fold(0.0_f32, |acc, &v| acc.max(v.abs()));
    if max_abs <= 1e-12 {
        return (vec![0_i8; work.len()], 1.0);
    }
    let scale = max_abs / 127.0;
    let inv_scale = 127.0 / max_abs;
    let quantized = work
        .into_iter()
        .map(|v| (v * inv_scale).round().clamp(-127.0, 127.0) as i8)
        .collect();
    (quantized, scale)
}

/// Cosine similarity between two symmetrically quantized int8 L2-normalized vectors.
pub fn cosine_similarity_int8(a: &[i8], scale_a: f32, b: &[i8], scale_b: f32) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0_i64;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += (x as i64) * (y as i64);
    }
    (dot as f64 * (scale_a as f64) * (scale_b as f64)) as f32
}

/// Execution backend abstraction so tests can exercise lease scheduling,
/// model revision switching, and int8 quantization completely offline.
pub trait EmbedWorkerBackend: Send + Sync {
    fn execute_embed(
        &self,
        pack: &ActivatedEmbeddingPack,
        job_id: &str,
        generation: u64,
        items: &[EmbedInputItem],
    ) -> Result<(Vec<QuantizedEmbedding>, u64), KnowledgeErrorCode>;
}

/// Subprocess backend invoking `myagents-document-worker` with `start_embed`.
pub struct DocumentWorkerEmbedBackend {
    worker_path: PathBuf,
    onnx_runtime_path: PathBuf,
}

impl DocumentWorkerEmbedBackend {
    pub fn new(worker_path: PathBuf, onnx_runtime_path: PathBuf) -> Self {
        Self {
            worker_path,
            onnx_runtime_path,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HostEmbedRequest<'a> {
    StartEmbed(HostEmbedStart<'a>),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostEmbedStart<'a> {
    protocol_version: u32,
    job_id: &'a str,
    worker_generation: u64,
    model_path: String,
    tokenizer_path: String,
    onnx_runtime_path: String,
    dimension: usize,
    max_tokens: usize,
    pooling: &'a str,
    normalize: bool,
    items: &'a [EmbedInputItem],
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
enum HostEmbedResponse {
    Ready {
        #[allow(dead_code)]
        protocol_version: u32,
    },
    EmbedCompleted {
        result: HostEmbedCompletedResult,
    },
    Failed {
        #[allow(dead_code)]
        error: serde_json::Value,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostEmbedCompletedResult {
    #[allow(dead_code)]
    dimension: usize,
    vectors: Vec<QuantizedEmbedding>,
    elapsed_ms: u64,
}

fn write_length_prefixed_frame(writer: &mut impl Write, payload: &[u8]) -> Result<(), KnowledgeErrorCode> {
    let len = u32::try_from(payload.len()).map_err(|_| KbLimitExceeded)?;
    writer
        .write_all(&len.to_be_bytes())
        .and_then(|_| writer.write_all(payload))
        .and_then(|_| writer.flush())
        .map_err(|_| KbStoreUnavailable)
}

fn read_length_prefixed_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>, KnowledgeErrorCode> {
    let mut header = [0_u8; 4];
    match reader.read(&mut header[..1]) {
        Ok(0) => return Ok(None),
        Ok(1) => {}
        _ => return Err(KbStoreUnavailable),
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|_| KbStoreUnavailable)?;
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_FRAME_BYTES {
        return Err(KbStoreUnavailable);
    }
    let mut buf = vec![0_u8; len];
    reader.read_exact(&mut buf).map_err(|_| KbStoreUnavailable)?;
    Ok(Some(buf))
}

impl EmbedWorkerBackend for DocumentWorkerEmbedBackend {
    fn execute_embed(
        &self,
        pack: &ActivatedEmbeddingPack,
        job_id: &str,
        generation: u64,
        items: &[EmbedInputItem],
    ) -> Result<(Vec<QuantizedEmbedding>, u64), KnowledgeErrorCode> {
        let mut child = crate::process_cmd::new(&self.worker_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| KbStoreUnavailable)?;

        let payload = serde_json::to_vec(&HostEmbedRequest::StartEmbed(HostEmbedStart {
            protocol_version: EMBED_PROTOCOL_VERSION,
            job_id,
            worker_generation: generation,
            model_path: pack.model_path.to_string_lossy().into_owned(),
            tokenizer_path: pack.tokenizer_path.to_string_lossy().into_owned(),
            onnx_runtime_path: self.onnx_runtime_path.to_string_lossy().into_owned(),
            dimension: pack.dimension,
            max_tokens: pack.max_tokens,
            pooling: &pack.pooling,
            normalize: pack.normalize,
            items,
        }))
        .map_err(|_| KbInvalidInput)?;

        if let Some(mut stdin) = child.stdin.take() {
            write_length_prefixed_frame(&mut stdin, &payload)?;
        }

        let mut stdout = child.stdout.take().ok_or(KbStoreUnavailable)?;
        let mut completed = None;
        while let Some(frame) = read_length_prefixed_frame(&mut stdout)? {
            let parsed: HostEmbedResponse =
                serde_json::from_slice(&frame).map_err(|_| KbStoreUnavailable)?;
            match parsed {
                HostEmbedResponse::Ready { .. } | HostEmbedResponse::Other => continue,
                HostEmbedResponse::EmbedCompleted { result } => {
                    completed = Some((result.vectors, result.elapsed_ms));
                    break;
                }
                HostEmbedResponse::Failed { .. } => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(KbStoreUnavailable);
                }
            }
        }
        let _ = child.wait();
        completed.ok_or(KbStoreUnavailable)
    }
}

pub struct KnowledgeEmbeddingClient {
    model_pack: Arc<KnowledgeModelPackManager>,
    coordinator: Arc<LocalComputeCoordinator>,
    backend: Arc<dyn EmbedWorkerBackend>,
    next_generation: AtomicU64,
}

impl KnowledgeEmbeddingClient {
    pub fn new(
        model_pack: Arc<KnowledgeModelPackManager>,
        coordinator: Arc<LocalComputeCoordinator>,
        backend: Arc<dyn EmbedWorkerBackend>,
    ) -> Self {
        Self {
            model_pack,
            coordinator,
            backend,
            next_generation: AtomicU64::new(1),
        }
    }

    pub fn active_revision(&self) -> Option<String> {
        self.model_pack.active_pack().map(|p| p.revision)
    }

    /// Execute one bounded embedding batch under a `LocalComputeCoordinator` lease.
    pub async fn embed_batch(
        &self,
        items: Vec<EmbedInputItem>,
    ) -> Result<EmbedBatchResult, KnowledgeErrorCode> {
        if items.is_empty() || items.len() > MAX_EMBED_BATCH_ITEMS {
            return Err(KbInvalidInput);
        }
        for item in &items {
            if item.id.trim().is_empty() || item.text.len() > MAX_EMBED_ITEM_BYTES {
                return Err(KbInvalidInput);
            }
        }
        let pack = self.model_pack.active_pack().ok_or(KbStoreUnavailable)?;
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let job_id = format!("embed_{generation}");

        // Acquire shared heavy-compute lease (same coordinator as OCR/ASR).
        let _lease = self
            .coordinator
            .acquire(ComputeWorkloadIdentity {
                kind: ComputeWorkloadKind::KnowledgeEmbedding,
                id: job_id.clone(),
                generation,
            })
            .await;

        let backend = Arc::clone(&self.backend);
        let pack_clone = pack.clone();
        let (vectors, elapsed_ms) = tauri::async_runtime::spawn_blocking(move || {
            backend.execute_embed(&pack_clone, &job_id, generation, &items)
        })
        .await
        .map_err(|_| KbStoreUnavailable)??;

        Ok(EmbedBatchResult {
            model_revision: pack.revision,
            dimension: pack.dimension,
            vectors,
            elapsed_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::AtomicUsize;
    use tempfile::TempDir;

    use crate::knowledge::model_pack::{
        SignatureVerifier, SignedEmbeddingFileEntry, SignedEmbeddingManifest,
    };
    use sha2::{Digest, Sha256};

    struct MockEmbedBackend {
        calls: AtomicUsize,
    }

    impl EmbedWorkerBackend for MockEmbedBackend {
        fn execute_embed(
            &self,
            pack: &ActivatedEmbeddingPack,
            _job_id: &str,
            _generation: u64,
            items: &[EmbedInputItem],
        ) -> Result<(Vec<QuantizedEmbedding>, u64), KnowledgeErrorCode> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let mut raw = vec![0.0_f32; pack.dimension];
                for (idx, b) in item.text.bytes().enumerate() {
                    raw[idx % pack.dimension] += (b as f32) / 255.0;
                }
                let (vector, scale) = quantize_l2_int8(&raw, pack.normalize);
                out.push(QuantizedEmbedding {
                    id: item.id.clone(),
                    vector,
                    scale,
                });
            }
            Ok((out, 1))
        }
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        format!("{:x}", h.finalize())
    }

    fn install_test_pack(root: &Path) -> Arc<KnowledgeModelPackManager> {
        let verifier: SignatureVerifier = Arc::new(|bytes, sig| {
            if sig == format!("sig:{}", sha256_hex(bytes)) {
                Ok(())
            } else {
                Err("mismatch".into())
            }
        });
        let bundle = TempDir::new().unwrap();
        let files = [
            ("models/embedding/model.int8.onnx", b"MOCK_ONNX".as_slice()),
            ("models/embedding/tokenizer.json", b"{}".as_slice()),
            ("legal/LICENSE.txt", b"Apache-2.0".as_slice()),
        ];
        let mut entries = Vec::new();
        for (rel, content) in files {
            let full = bundle.path().join(rel);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(&full, content).unwrap();
            entries.push(SignedEmbeddingFileEntry {
                path: rel.into(),
                sha256: sha256_hex(content),
                size: content.len() as u64,
                url: None,
            });
        }
        let manifest = SignedEmbeddingManifest {
            schema_version: 1,
            pack_id: "local-knowledge-embedding-qwen3-0.6b-int8".into(),
            pack_revision: "kb-embed-qwen3-0.6b-int8-v1".into(),
            dimension: 1024,
            max_tokens: 8192,
            pooling: "last_token".into(),
            normalize: true,
            files: entries,
        };
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let sig = format!("sig:{}", sha256_hex(&manifest_bytes));
        fs::write(bundle.path().join("manifest.json"), &manifest_bytes).unwrap();
        fs::write(bundle.path().join("manifest.json.minisig"), &sig).unwrap();

        let mgr = KnowledgeModelPackManager::initialize_with_test_verifier(
            root.to_path_buf(),
            verifier,
        )
        .unwrap();
        mgr.install_from_local_bundle(bundle.path()).unwrap();
        mgr
    }

    #[tokio::test]
    async fn embed_batch_acquires_compute_lease_and_respects_uninstall_degradation() {
        let root = TempDir::new().unwrap();
        let model_pack = install_test_pack(root.path());
        let coordinator = LocalComputeCoordinator::new();
        let backend = Arc::new(MockEmbedBackend {
            calls: AtomicUsize::new(0),
        });
        let client = KnowledgeEmbeddingClient::new(
            Arc::clone(&model_pack),
            Arc::clone(&coordinator),
            Arc::clone(&backend) as Arc<dyn EmbedWorkerBackend>,
        );

        // Hold an OCR lease first; embed must wait until OCR releases it.
        let ocr_lease = coordinator
            .acquire(ComputeWorkloadIdentity {
                kind: ComputeWorkloadKind::DocumentOcr,
                id: "ocr-1".into(),
                generation: 1,
            })
            .await;

        let embed_task = tauri::async_runtime::spawn({
            let client_items = vec![
                EmbedInputItem {
                    id: "c1".into(),
                    text: "hybrid retrieval architecture".into(),
                },
                EmbedInputItem {
                    id: "c2".into(),
                    text: "hybrid retrieval architecture".into(),
                },
            ];
            let pack = Arc::clone(&model_pack);
            let coord = Arc::clone(&coordinator);
            let be = Arc::clone(&backend) as Arc<dyn EmbedWorkerBackend>;
            async move {
                let c = KnowledgeEmbeddingClient::new(pack, coord, be);
                c.embed_batch(client_items).await
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        // Backend must NOT have run while OCR holds the shared compute lease.
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);

        drop(ocr_lease);
        let result = embed_task.await.unwrap().unwrap();
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.dimension, 1024);
        assert_eq!(result.vectors.len(), 2);
        let sim = cosine_similarity_int8(
            &result.vectors[0].vector,
            result.vectors[0].scale,
            &result.vectors[1].vector,
            result.vectors[1].scale,
        );
        assert!(sim > 0.99, "identical inputs must have cosine ~1.0, got {sim}");

        // Uninstall model pack -> embed_batch cleanly fails with KbStoreUnavailable (BM25 fallback).
        model_pack.remove().unwrap();
        let err = client
            .embed_batch(vec![EmbedInputItem {
                id: "c3".into(),
                text: "after uninstall".into(),
            }])
            .await
            .unwrap_err();
        assert_eq!(err, KbStoreUnavailable);
    }
}
