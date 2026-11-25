//! K17 Document Worker `embed` job handler.
//!
//! Single-workload, stateless, zero-network batch embedding execution:
//! accepts a bounded batch of chunks, runs int8 ONNX inference through the
//! App-bundled ONNX Runtime, applies configured pooling + L2 normalization,
//! and emits symmetrically quantized int8 vectors.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use ort::session::Session;
use ort::value::Tensor;
use serde_json::Value;

use crate::protocol::{EmbedStartRequest, EmbedVectorItem};

pub const MAX_EMBED_BATCH_ITEMS: usize = 256;
pub const MAX_EMBED_ITEM_BYTES: usize = 32 * 1024;
pub const MIN_EMBED_DIM: usize = 64;
pub const MAX_EMBED_DIM: usize = 4096;

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
    let max_abs = work
        .iter()
        .fold(0.0_f32, |acc, &v| acc.max(v.abs()));
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

fn validate_regular_non_link(path: &Path) -> Result<(), &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "EMBED_MODEL_FILE_MISSING")?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("EMBED_MODEL_PATH_UNSAFE");
    }
    Ok(())
}

struct SimpleVocabTokenizer {
    vocab: HashMap<String, i64>,
    unk_id: i64,
    eos_id: Option<i64>,
    max_token_chars: usize,
}

impl SimpleVocabTokenizer {
    fn from_tokenizer_json(path: &Path) -> Result<Self, &'static str> {
        let bytes = fs::read(path).map_err(|_| "EMBED_TOKENIZER_READ_FAILED")?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("EMBED_TOKENIZER_TOO_LARGE");
        }
        let json: Value =
            serde_json::from_slice(&bytes).map_err(|_| "EMBED_TOKENIZER_INVALID_JSON")?;
        let mut vocab = HashMap::new();
        let mut max_token_chars = 1usize;

        if let Some(map) = json
            .get("model")
            .and_then(|m| m.get("vocab"))
            .and_then(Value::as_object)
        {
            for (tok, id) in map {
                if let Some(id_val) = id.as_i64() {
                    max_token_chars = max_token_chars.max(tok.chars().count().min(32));
                    vocab.insert(tok.clone(), id_val);
                }
            }
        }
        if let Some(added) = json.get("added_tokens").and_then(Value::as_array) {
            for item in added {
                if let (Some(content), Some(id_val)) = (
                    item.get("content").and_then(Value::as_str),
                    item.get("id").and_then(Value::as_i64),
                ) {
                    vocab.insert(content.to_string(), id_val);
                }
            }
        }
        let unk_id = vocab
            .get("<unk>")
            .or_else(|| vocab.get("[UNK]"))
            .or_else(|| vocab.get("<|endoftext|>"))
            .copied()
            .unwrap_or(0);
        let eos_id = vocab
            .get("<|endoftext|>")
            .or_else(|| vocab.get("</s>"))
            .or_else(|| vocab.get("[SEP]"))
            .copied();

        Ok(Self {
            vocab,
            unk_id,
            eos_id,
            max_token_chars,
        })
    }

    fn encode(&self, text: &str, max_tokens: usize) -> Vec<i64> {
        let cap = max_tokens.max(1);
        let chars: Vec<char> = text.chars().collect();
        let mut ids = Vec::with_capacity(chars.len().min(cap));
        let mut i = 0usize;
        while i < chars.len() && ids.len() + usize::from(self.eos_id.is_some()) < cap {
            let mut matched = None;
            let max_end = (i + self.max_token_chars).min(chars.len());
            for end in (i + 1..=max_end).rev() {
                let slice: String = chars[i..end].iter().collect();
                if let Some(&id) = self.vocab.get(&slice) {
                    matched = Some((id, end));
                    break;
                }
            }
            if let Some((id, next_i)) = matched {
                ids.push(id);
                i = next_i;
            } else {
                ids.push(self.unk_id);
                i += 1;
            }
        }
        if let Some(eos) = self.eos_id {
            if ids.len() < cap {
                ids.push(eos);
            }
        }
        if ids.is_empty() {
            ids.push(self.unk_id);
        }
        ids
    }
}

pub fn run_embed_job(
    request: &EmbedStartRequest,
    cancelled: &AtomicBool,
) -> Result<(Vec<EmbedVectorItem>, u64), (&'static str, String)> {
    let started = Instant::now();
    if request.items.is_empty() || request.items.len() > MAX_EMBED_BATCH_ITEMS {
        return Err(("EMBED_INVALID_BATCH", "invalid embed batch size".into()));
    }
    if !(MIN_EMBED_DIM..=MAX_EMBED_DIM).contains(&request.dimension) || request.max_tokens == 0 {
        return Err(("EMBED_INVALID_CONFIG", "invalid embed dimension or maxTokens".into()));
    }
    for item in &request.items {
        if item.id.trim().is_empty() || item.text.len() > MAX_EMBED_ITEM_BYTES {
            return Err(("EMBED_INVALID_ITEM", "invalid embed item id or size".into()));
        }
    }

    let model_path = Path::new(&request.model_path);
    let tokenizer_path = Path::new(&request.tokenizer_path);
    let ort_path = Path::new(&request.onnx_runtime_path);
    validate_regular_non_link(model_path)
        .map_err(|c| (c, "embedding model file unavailable".into()))?;
    validate_regular_non_link(tokenizer_path)
        .map_err(|c| (c, "embedding tokenizer file unavailable".into()))?;
    validate_regular_non_link(ort_path)
        .map_err(|c| (c, "ONNX runtime library unavailable".into()))?;

    let tokenizer = SimpleVocabTokenizer::from_tokenizer_json(tokenizer_path)
        .map_err(|c| (c, "failed to load embedding tokenizer".into()))?;

    let _ = ort::init_from(ort_path)
        .map_err(|e| ("EMBED_RUNTIME_INIT_FAILED", format!("ort init failed: {e}")))?
        .commit();

    let mut session = Session::builder()
        .map_err(|e| ("EMBED_SESSION_BUILD_FAILED", format!("session builder: {e}")))?
        .commit_from_file(model_path)
        .map_err(|e| ("EMBED_MODEL_LOAD_FAILED", format!("model load failed: {e}")))?;

    let mut results = Vec::with_capacity(request.items.len());
    for item in &request.items {
        if cancelled.load(Ordering::Acquire) {
            return Err(("EMBED_CANCELLED", "embedding job cancelled".into()));
        }
        let token_ids = tokenizer.encode(&item.text, request.max_tokens);
        let seq_len = token_ids.len();
        let mask: Vec<i64> = vec![1_i64; seq_len];

        let input_ids = Tensor::from_array(([1usize, seq_len], token_ids))
            .map_err(|e| ("EMBED_TENSOR_FAILED", format!("input_ids tensor: {e}")))?;
        let attention_mask = Tensor::from_array(([1usize, seq_len], mask))
            .map_err(|e| ("EMBED_TENSOR_FAILED", format!("attention_mask tensor: {e}")))?;

        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids,
                "attention_mask" => attention_mask,
            ])
            .map_err(|e| ("EMBED_INFERENCE_FAILED", format!("onnx inference failed: {e}")))?;

        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| ("EMBED_OUTPUT_INVALID", format!("extract f32 tensor: {e}")))?;

        let pooled = pool_embedding_output(shape, data, request.dimension, &request.pooling)
            .map_err(|c| (c, "unexpected embedding output shape".into()))?;
        let (vector, scale) = quantize_l2_int8(&pooled, request.normalize);
        results.push(EmbedVectorItem {
            id: item.id.clone(),
            vector,
            scale,
        });
    }

    Ok((results, started.elapsed().as_millis() as u64))
}

fn pool_embedding_output(
    shape: &[i64],
    data: &[f32],
    expected_dim: usize,
    pooling: &str,
) -> Result<Vec<f32>, &'static str> {
    match shape {
        // [1, dim] already pooled
        [1, dim] if *dim as usize == expected_dim && data.len() >= expected_dim => {
            Ok(data[..expected_dim].to_vec())
        }
        // [1, seq_len, dim] token hidden states
        [1, seq_len, dim] if *dim as usize == expected_dim && *seq_len > 0 => {
            let seq = *seq_len as usize;
            let d = expected_dim;
            if data.len() < seq * d {
                return Err("EMBED_OUTPUT_SHAPE_MISMATCH");
            }
            if pooling == "mean" {
                let mut acc = vec![0.0_f64; d];
                for t in 0..seq {
                    let row = &data[t * d..(t + 1) * d];
                    for (j, &v) in row.iter().enumerate() {
                        acc[j] += v as f64;
                    }
                }
                let denom = seq as f64;
                Ok(acc.into_iter().map(|v| (v / denom) as f32).collect())
            } else {
                // Default: `last_token` pooling (Qwen3-Embedding)
                let start = (seq - 1) * d;
                Ok(data[start..start + d].to_vec())
            }
        }
        _ => Err("EMBED_OUTPUT_SHAPE_MISMATCH"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_l2_int8_normalizes_and_preserves_cosine_direction() {
        let v1 = vec![3.0_f32, 4.0, 0.0, 0.0];
        let (q1, s1) = quantize_l2_int8(&v1, true);
        assert_eq!(q1.len(), 4);
        assert!(s1 > 0.0);
        // 3/5 = 0.6, 4/5 = 0.8 -> ratio 0.75
        let r0 = q1[0] as f32 * s1;
        let r1 = q1[1] as f32 * s1;
        assert!((r0 - 0.6).abs() < 0.01);
        assert!((r1 - 0.8).abs() < 0.01);
        assert_eq!(q1[2], 0);
        assert_eq!(q1[3], 0);
    }

    #[test]
    fn pooling_supports_last_token_and_mean_modes() {
        let data = vec![1.0_f32, 2.0, 3.0, 4.0]; // [1, 2, 2]
        let last = pool_embedding_output(&[1, 2, 2], &data, 2, "last_token").unwrap();
        assert_eq!(last, vec![3.0, 4.0]);
        let mean = pool_embedding_output(&[1, 2, 2], &data, 2, "mean").unwrap();
        assert_eq!(mean, vec![2.0, 3.0]);
    }
}
