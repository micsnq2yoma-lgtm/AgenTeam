use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpListener;

use super::*;
use crate::knowledge::ingest::{ImportOutcome, KnowledgeIngestManager};
use crate::knowledge::online_embedding::{EmbeddingErrorKind, KnowledgeEmbeddingService};
use crate::knowledge::store::{KnowledgeBasePatch, KnowledgeStore, SourceStatus};
use crate::knowledge::vector_index::KnowledgeVectorIndex;

fn write_embedding_config(
    data_dir: &Path,
    provider_id: &str,
    base_url: &str,
    api_key: &str,
    model: &str,
    dimensions: Option<usize>,
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
                "dimensions": dimensions
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

fn create_kb_with_markdown(store: &Arc<KnowledgeStore>, filename: &str, body: &str) -> (String, String) {
    let kb = store
        .create(KnowledgeBasePatch {
            name: Some("Embed Stage KB".into()),
            ..Default::default()
        })
        .unwrap();
    let inputs = TempDir::new().unwrap();
    let file_path = inputs.path().join(filename);
    fs::write(&file_path, body).unwrap();
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(store), None));
    manager.recover().unwrap();
    let outcomes = manager.import_files(&kb.id, vec![file_path]);
    let [ImportOutcome::Accepted { receipt }] = outcomes.as_slice() else {
        panic!("expected accepted import");
    };
    let job_id = receipt.job.as_ref().unwrap().id.clone();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let job = manager.status(&job_id).unwrap();
        if job.status == SourceStatus::Ready {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "timed out waiting for ready");
        std::thread::sleep(Duration::from_millis(10));
    }
    (kb.id, receipt.source_id.clone())
}

#[test]
fn split_embedding_batches_respects_item_and_char_caps() {
    let specs: Vec<ChunkEmbedSpec> = (0..130)
        .map(|i| ChunkEmbedSpec {
            chunk_id: format!("c{i}"),
            chunk_content_hash: format!("h{i}"),
            title: "t".into(),
            prefix: String::new(),
            page: None,
            snippet: String::new(),
            embed_text: "x".repeat(2_000),
            char_count: 2_000,
        })
        .collect();
    let indices: Vec<usize> = (0..specs.len()).collect();
    let batches = split_embedding_batches(&specs, &indices);
    // 100,000 / 2,000 = 50 items per batch -> 130 items => 3 batches (50, 50, 30)
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0].len(), 50);
    assert_eq!(batches[1].len(), 50);
    assert_eq!(batches[2].len(), 30);
}

#[tokio::test]
async fn embed_resume_model_switch_cancel_and_auth_fail_end_to_end() {
    let embed_calls = Arc::new(AtomicUsize::new(0));
    let calls_ref = Arc::clone(&embed_calls);

    let app = Router::new()
        .route(
            "/v1/embeddings",
            post(move |Json(body): Json<Value>| {
                let counter = Arc::clone(&calls_ref);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    let dim = body["dimensions"].as_u64().unwrap_or(4) as usize;
                    let count = body["input"].as_array().map(|a| a.len()).unwrap_or(0);
                    let data: Vec<Value> = (0..count)
                        .map(|i| {
                            let mut v = vec![0.0f32; dim];
                            v[i % dim] = 1.0;
                            json!({ "index": i, "embedding": v })
                        })
                        .collect();
                    (StatusCode::OK, Json(json!({ "data": data }))).into_response()
                }
            }),
        )
        .route(
            "/authfail/embeddings",
            post(|| async {
                (StatusCode::UNAUTHORIZED, Json(json!({ "error": "bad key" }))).into_response()
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dir = TempDir::new().unwrap();
    let store = Arc::new(KnowledgeStore::new(dir.path().to_path_buf()));
    let (kb_id, src1_id) = create_kb_with_markdown(
        &store,
        "guide.md",
        "# Alpha\n\nRust hybrid search combines BM25 with semantic vector recall.",
    );

    // 1. Disabled config -> Skipped with 0 HTTP requests
    write_embedding_config(
        dir.path(),
        "local",
        &format!("http://{addr}/v1"),
        "sk-1",
        "embed-v1",
        Some(4),
        false,
    );
    let vec_index = Arc::new(KnowledgeVectorIndex::new(dir.path().to_path_buf()));
    let svc = Arc::new(KnowledgeEmbeddingService::with_fast_backoff(
        dir.path().to_path_buf(),
    ));
    let stage = EmbeddingStage::new(Arc::clone(&store), Arc::clone(&vec_index), Arc::clone(&svc));

    let skipped = stage.embed_source_now(&kb_id, &src1_id).await.unwrap();
    assert_eq!(skipped.status, SourceEmbeddingStatusKind::Skipped);
    assert_eq!(embed_calls.load(Ordering::SeqCst), 0);

    // 2. Enable online embedding -> embeds src1 and publishes generation 1
    write_embedding_config(
        dir.path(),
        "local",
        &format!("http://{addr}/v1"),
        "sk-1",
        "embed-v1",
        Some(4),
        true,
    );
    let ready = stage.embed_source_now(&kb_id, &src1_id).await.unwrap();
    assert_eq!(ready.status, SourceEmbeddingStatusKind::Ready);
    assert_eq!(ready.embedded_chunks, ready.total_chunks);
    assert!(ready.embedded_chunks >= 1);
    assert_eq!(embed_calls.load(Ordering::SeqCst), 1);

    let ptr1 = vec_index.active_generation(&kb_id).unwrap();
    assert_eq!(ptr1.generation, 1);
    assert_eq!(ptr1.dimension, 4);
    let rev1 = ptr1.model_revision.clone();

    // 3. Simulate app restart: fresh VectorIndex + EmbeddingStage -> 0 HTTP requests for cached src1
    let vec_index2 = Arc::new(KnowledgeVectorIndex::new(dir.path().to_path_buf()));
    let stage2 = EmbeddingStage::new(
        Arc::clone(&store),
        Arc::clone(&vec_index2),
        Arc::clone(&svc),
    );
    let summary_resume = stage2.embed_library_now(&kb_id).await.unwrap();
    assert_eq!(summary_resume.ready_sources, 1);
    assert_eq!(
        embed_calls.load(Ordering::SeqCst),
        1,
        "restart resume MUST reuse cached vectors with 0 HTTP calls"
    );

    // 4. Switch model dimension (4 -> 8): atomic generation bump + old revision dir deleted
    write_embedding_config(
        dir.path(),
        "local",
        &format!("http://{addr}/v1"),
        "sk-1",
        "embed-v2",
        Some(8),
        true,
    );
    let _ = stage2.embed_library_now(&kb_id).await.unwrap();
    let ptr2 = vec_index2.active_generation(&kb_id).unwrap();
    assert!(ptr2.generation > ptr1.generation);
    assert_eq!(ptr2.dimension, 8);
    assert_ne!(ptr2.model_revision, rev1);
    let old_rev_dir = dir
        .path()
        .join("knowledge")
        .join(&kb_id)
        .join("vectors")
        .join(&rev1);
    assert!(
        !old_rev_dir.exists(),
        "old revision directory must be cleaned up after atomic switch"
    );

    // 5. Cancel source -> removes source entries from active vector index
    stage2.cancel_source(&kb_id, &src1_id);
    assert!(vec_index2.current_entries(&kb_id, &ptr2.model_revision).is_empty());

    // 6. Auth failure (401) -> immediately fails with AuthFailed without retrying
    write_embedding_config(
        dir.path(),
        "local-bad",
        &format!("http://{addr}/authfail"),
        "sk-bad",
        "embed-v2",
        Some(8),
        true,
    );
    let auth_err = stage2.embed_source_now(&kb_id, &src1_id).await.unwrap_err();
    assert_eq!(auth_err.kind, EmbeddingErrorKind::AuthFailed);
    let failed_status = stage2.source_progress(&kb_id, &src1_id);
    assert_eq!(failed_status.status, SourceEmbeddingStatusKind::Failed);
    assert_eq!(failed_status.error_kind, Some(EmbeddingErrorKind::AuthFailed));
}
