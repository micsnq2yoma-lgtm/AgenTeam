use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpListener;

use super::{search_hybrid, KnowledgeIndex, KnowledgeSearchFilters};
use crate::knowledge::ingest::embedding_stage::EmbeddingStage;
use crate::knowledge::ingest::{ImportOutcome, KnowledgeIngestManager};
use crate::knowledge::online_embedding::KnowledgeEmbeddingService;
use crate::knowledge::store::{KnowledgeBasePatch, KnowledgeStore, SourceStatus};
use crate::knowledge::vector_index::KnowledgeVectorIndex;

fn write_embed_config(
    data_dir: &Path,
    provider_id: &str,
    base_url: &str,
    api_key: &str,
    model: &str,
    enabled: bool,
) {
    let providers_dir = data_dir.join("providers");
    fs::create_dir_all(&providers_dir).unwrap();
    fs::write(
        providers_dir.join(format!("{provider_id}.json")),
        serde_json::to_string(&json!({
            "id": provider_id,
            "config": { "baseUrl": base_url }
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        data_dir.join("config.json"),
        serde_json::to_string(&json!({
            "providerApiKeys": { provider_id: api_key },
            "embeddingModel": {
                "enabled": enabled,
                "providerId": provider_id,
                "model": model,
                "dimensions": 4
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn end_to_end_ingest_embed_hybrid_synonym_recall_lru_cache_and_bm25_degradation() {
    let query_hits = Arc::new(AtomicUsize::new(0));
    let counter_ref = Arc::clone(&query_hits);

    let app = Router::new()
        .route(
            "/v1/embeddings",
            post(move |Json(body): Json<Value>| {
                let c = Arc::clone(&counter_ref);
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    let inputs = body["input"].as_array().cloned().unwrap_or_default();
                    let data: Vec<Value> = inputs
                        .iter()
                        .enumerate()
                        .map(|(i, val)| {
                            let s = val.as_str().unwrap_or("");
                            // Deterministic synonym vector: "automobile" and "sedan" share axis 0
                            // even with zero lexical overlap.
                            let vec = if s.contains("automobile") || s.contains("sedan") {
                                vec![1.0f32, 0.0, 0.0, 0.0]
                            } else {
                                vec![0.0f32, 1.0, 0.0, 0.0]
                            };
                            json!({ "index": i, "embedding": vec })
                        })
                        .collect();
                    (StatusCode::OK, Json(json!({ "data": data }))).into_response()
                }
            }),
        )
        .route(
            "/broken/embeddings",
            post(|| async {
                (StatusCode::UNAUTHORIZED, Json(json!({ "error": "invalid" }))).into_response()
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dir = TempDir::new().unwrap();
    let store = Arc::new(KnowledgeStore::new(dir.path().to_path_buf()));
    let kb = store
        .create(KnowledgeBasePatch {
            name: Some("Hybrid E2E".into()),
            ..Default::default()
        })
        .unwrap();

    // 1. Ingest a real markdown file containing "automobile" (never mentions "sedan").
    let inputs = TempDir::new().unwrap();
    let doc_path = inputs.path().join("vehicles.md");
    fs::write(
        &doc_path,
        "# Fleet Guide\n\nElectric automobile maintenance schedule and battery care.",
    )
    .unwrap();
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
    manager.recover().unwrap();
    let outcomes = manager.import_files(&kb.id, vec![doc_path]);
    let [ImportOutcome::Accepted { receipt }] = outcomes.as_slice() else {
        panic!("expected accepted import");
    };
    let job_id = receipt.job.as_ref().unwrap().id.clone();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if manager.status(&job_id).unwrap().status == SourceStatus::Ready {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }

    // 2. Embed the source via online embedding stage -> writes vectors to KnowledgeVectorIndex.
    write_embed_config(
        dir.path(),
        "local",
        &format!("http://{addr}/v1"),
        "sk-ok",
        "embed-test",
        true,
    );
    let vec_index = Arc::new(KnowledgeVectorIndex::new(dir.path().to_path_buf()));
    let embed_svc = Arc::new(KnowledgeEmbeddingService::with_fast_backoff(
        dir.path().to_path_buf(),
    ));
    let stage = EmbeddingStage::new(
        Arc::clone(&store),
        Arc::clone(&vec_index),
        Arc::clone(&embed_svc),
    );
    stage
        .embed_source_now(&kb.id, &receipt.source_id)
        .await
        .unwrap();

    // 3. Build BM25 index and run hybrid search for zero-overlap synonym "sedan".
    let bm25_index = Arc::new(KnowledgeIndex::new(dir.path().to_path_buf()));
    bm25_index.rebuild(&store).unwrap();

    let calls_before_query = query_hits.load(Ordering::SeqCst);
    let res = search_hybrid(
        Arc::clone(&bm25_index),
        Arc::clone(&store),
        Some(Arc::clone(&vec_index)),
        Some(Arc::clone(&embed_svc)),
        vec![kb.id.clone()],
        vec!["sedan".into()],
        KnowledgeSearchFilters::default(),
        Some(8),
    )
    .await
    .unwrap();

    assert_eq!(res.mode, "hybrid");
    assert!(res.degraded_reason.is_none());
    assert!(
        res.bm25_hits.as_ref().unwrap().is_empty(),
        "BM25 should have 0 hits for zero-overlap synonym"
    );
    assert_eq!(res.vector_hits.as_ref().unwrap().len(), 1);
    assert_eq!(res.hits.len(), 1);
    assert_eq!(res.hits[0].source_id, receipt.source_id);
    assert_eq!(query_hits.load(Ordering::SeqCst), calls_before_query + 1);

    // 4. Repeat identical query -> hits LRU cache with 0 additional HTTP requests.
    let res_cached = search_hybrid(
        Arc::clone(&bm25_index),
        Arc::clone(&store),
        Some(Arc::clone(&vec_index)),
        Some(Arc::clone(&embed_svc)),
        vec![kb.id.clone()],
        vec!["sedan".into()],
        KnowledgeSearchFilters::default(),
        Some(8),
    )
    .await
    .unwrap();
    assert_eq!(res_cached.mode, "hybrid");
    assert_eq!(
        query_hits.load(Ordering::SeqCst),
        calls_before_query + 1,
        "identical query must hit LRU cache"
    );

    // 5. Break credentials (401) -> degrades cleanly to BM25 with degraded_reason, never errors.
    write_embed_config(
        dir.path(),
        "broken",
        &format!("http://{addr}/broken"),
        "sk-bad",
        "embed-test",
        true,
    );
    let res_degraded = search_hybrid(
        Arc::clone(&bm25_index),
        Arc::clone(&store),
        Some(Arc::clone(&vec_index)),
        Some(Arc::clone(&embed_svc)),
        vec![kb.id.clone()],
        vec!["automobile".into()],
        KnowledgeSearchFilters::default(),
        Some(8),
    )
    .await
    .unwrap();
    assert_eq!(res_degraded.mode, "bm25");
    assert_eq!(res_degraded.degraded_reason.as_deref(), Some("auth_failed"));
    assert_eq!(res_degraded.hits.len(), 1);

    // 6. Disable embedding -> degrades cleanly to BM25 with degraded_reason = "disabled".
    write_embed_config(
        dir.path(),
        "local",
        &format!("http://{addr}/v1"),
        "sk-ok",
        "embed-test",
        false,
    );
    let res_disabled = search_hybrid(
        Arc::clone(&bm25_index),
        Arc::clone(&store),
        Some(Arc::clone(&vec_index)),
        Some(Arc::clone(&embed_svc)),
        vec![kb.id.clone()],
        vec!["automobile".into()],
        KnowledgeSearchFilters::default(),
        Some(8),
    )
    .await
    .unwrap();
    assert_eq!(res_disabled.mode, "bm25");
    assert_eq!(res_disabled.degraded_reason.as_deref(), Some("disabled"));
    assert_eq!(res_disabled.hits.len(), 1);
}
