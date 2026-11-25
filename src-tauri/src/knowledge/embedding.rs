//! Shared vector quantization & int8 cosine similarity primitives used by
//! `KnowledgeEmbeddingService` (K21 online embedding) and `KnowledgeVectorIndex` (K18).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantizedEmbedding {
    pub id: String,
    pub vector: Vec<i8>,
    pub scale: f32,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_l2_int8_preserves_cosine_similarity_and_handles_non_finite() {
        let (a, sa) = quantize_l2_int8(&[3.0, 4.0, 0.0], true);
        let (b, sb) = quantize_l2_int8(&[6.0, 8.0, 0.0], true);
        let (c, sc) = quantize_l2_int8(&[0.0, 0.0, 5.0], true);
        assert!((cosine_similarity_int8(&a, sa, &b, sb) - 1.0).abs() < 0.02);
        assert!(cosine_similarity_int8(&a, sa, &c, sc).abs() < 0.02);

        let (nan_q, _) = quantize_l2_int8(&[f32::NAN, f32::INFINITY], true);
        assert_eq!(nan_q, vec![0_i8, 0_i8]);
    }
}
