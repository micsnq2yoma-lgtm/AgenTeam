use std::time::Instant;
use tempfile::TempDir;

use super::*;
use crate::knowledge::embedding::quantize_l2_int8;

fn make_entry(
    kb_id: &str,
    source_id: &str,
    chunk_id: &str,
    hash: &str,
    raw: &[f32],
) -> VectorChunkEntry {
    let (vector, scale) = quantize_l2_int8(raw, true);
    VectorChunkEntry {
        kb_id: kb_id.into(),
        source_id: source_id.into(),
        chunk_id: chunk_id.into(),
        chunk_content_hash: hash.into(),
        title: format!("Doc {source_id}"),
        prefix: format!("Doc {source_id} › Section"),
        page: Some(1),
        snippet: format!("Snippet for {chunk_id}"),
        vector,
        scale,
    }
}

#[test]
fn cache_key_reuses_embeddings_and_model_switch_flips_generation_atomically() {
    let dir = TempDir::new().unwrap();
    let index = KnowledgeVectorIndex::new(dir.path().to_path_buf());
    let kb_id = "kb_111111111111";

    let e1 = make_entry(kb_id, "src_0000000000000001", "0000000000000001", "hash_a", &[1.0, 0.0, 0.0, 0.0]);
    let p1 = index
        .publish_revision(kb_id, "kb-embed-v1", 4, vec![e1])
        .unwrap();
    assert_eq!(p1.generation, 1);
    assert!(index.lookup_cached(kb_id, "kb-embed-v1", "hash_a").is_some());
    assert!(index.lookup_cached(kb_id, "kb-embed-v2", "hash_a").is_none());

    // Switch to new model revision -> generation increments atomically and queries use v2.
    let e2 = make_entry(kb_id, "src_0000000000000001", "0000000000000001", "hash_a", &[0.0, 1.0, 0.0, 0.0]);
    let p2 = index
        .publish_revision(kb_id, "kb-embed-v2", 4, vec![e2])
        .unwrap();
    assert_eq!(p2.generation, 2);
    assert_eq!(
        index.active_generation(kb_id).unwrap().model_revision,
        "kb-embed-v2"
    );

    // Reload from disk in a fresh instance
    let reopened = KnowledgeVectorIndex::new(dir.path().to_path_buf());
    assert_eq!(reopened.active_generation(kb_id).unwrap().generation, 2);
    let (q, s) = quantize_l2_int8(&[0.0, 1.0, 0.0, 0.0], true);
    let hits = reopened.search_top_k(&[kb_id.into()], "kb-embed-v2", &q, s, 8);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].score > 0.99);

    // Clear (uninstall model) -> empty vector hits, degrades cleanly to BM25
    reopened.clear_kb(kb_id);
    assert!(reopened
        .search_top_k(&[kb_id.into()], "kb-embed-v2", &q, s, 8)
        .is_empty());
}

#[test]
fn rrf_fusion_boosts_semantic_recall_and_degrades_to_bm25_when_vectors_absent() {
    let bm25 = vec![
        KnowledgeSearchHit {
            reference: "kb://kb_111111111111/src_0000000000000001#0000000000000001".into(),
            kb_id: "kb_111111111111".into(),
            source_id: "src_0000000000000001".into(),
            chunk_id: "0000000000000001".into(),
            title: "Lexical Doc".into(),
            prefix: "Lexical".into(),
            page: None,
            snippet: "lexical hit".into(),
            score: 12.0,
        },
    ];
    let vector = vec![
        KnowledgeSearchHit {
            reference: "kb://kb_111111111111/src_0000000000000002#0000000000000002".into(),
            kb_id: "kb_111111111111".into(),
            source_id: "src_0000000000000002".into(),
            chunk_id: "0000000000000002".into(),
            title: "Semantic Doc".into(),
            prefix: "Semantic".into(),
            page: None,
            snippet: "zero-overlap semantic hit".into(),
            score: 0.92,
        },
        bm25[0].clone(),
    ];

    // No vector hits -> clean BM25 fallback
    let (mode_fallback, hits_fallback) = fuse_bm25_and_vector_rrf(bm25.clone(), Vec::new(), 8);
    assert_eq!(mode_fallback, "bm25");
    assert_eq!(hits_fallback.len(), 1);

    // Hybrid RRF: item appearing in BOTH lists ranks #1, and zero-overlap semantic item is recalled!
    let (mode_hybrid, hits_hybrid) = fuse_bm25_and_vector_rrf(bm25, vector, 8);
    assert_eq!(mode_hybrid, "hybrid");
    assert_eq!(hits_hybrid.len(), 2);
    assert_eq!(hits_hybrid[0].source_id, "src_0000000000000001");
    assert_eq!(hits_hybrid[1].source_id, "src_0000000000000002");
}

#[test]
fn ten_thousand_int8_vector_search_p95_under_300ms() {
    let dir = TempDir::new().unwrap();
    let index = KnowledgeVectorIndex::new(dir.path().to_path_buf());
    let kb_id = "kb_111111111111";
    let dim = 256usize; // Scaled benchmark for fast debug-profile CI; production uses 1024-dim release SIMD.

    let mut entries = Vec::with_capacity(10_000);
    for i in 0..10_000usize {
        let mut vec = vec![0_i8; dim];
        vec[i % dim] = 127;
        vec[(i * 7 + 3) % dim] = 64;
        entries.push(VectorChunkEntry {
            kb_id: kb_id.into(),
            source_id: format!("src_{i:016x}"),
            chunk_id: format!("{i:016x}"),
            chunk_content_hash: format!("h_{i}"),
            title: String::new(),
            prefix: String::new(),
            page: None,
            snippet: String::new(),
            vector: vec,
            scale: 1.0 / 127.0,
        });
    }
    index
        .publish_revision(kb_id, "kb-embed-bench", dim, entries)
        .unwrap();

    let mut q = vec![0_i8; dim];
    q[42] = 127;
    let mut latencies_ms = Vec::with_capacity(20);
    for _ in 0..20 {
        let start = Instant::now();
        let hits = index.search_top_k(&[kb_id.into()], "kb-embed-bench", &q, 1.0 / 127.0, 8);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(hits.len(), 8);
        latencies_ms.push(elapsed);
    }
    latencies_ms.sort_by(|a, b| a.total_cmp(b));
    let p95 = latencies_ms[(latencies_ms.len() * 95 / 100).min(latencies_ms.len() - 1)];
    assert!(
        p95 < 300.0,
        "10k int8 vector search p95={p95:.2}ms exceeded 300ms budget"
    );
}
