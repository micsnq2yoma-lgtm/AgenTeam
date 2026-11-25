use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpListener;

use super::*;
use crate::knowledge::embedding::cosine_similarity_int8;

async fn spawn_mock_embedding_server(router: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    format!("http://{addr}/v1")
}

fn write_test_config(data_dir: &Path, provider_id: &str, base_url: &str, api_key: &str, model: &str, enabled: bool) {
    let config = json!({
        "embeddingModel": {
            "enabled": enabled,
            "providerId": provider_id,
            "model": model
        },
        "providerApiKeys": {
            provider_id: api_key
        }
    });
    fs::write(data_dir.join("config.json"), serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let providers_dir = data_dir.join("providers");
    fs::create_dir_all(&providers_dir).unwrap();
    let provider_doc = json!({
        "id": provider_id,
        "name": "Test OpenAI Compat",
        "config": {
            "baseUrl": base_url
        },
        "models": []
    });
    fs::write(
        providers_dir.join(format!("{provider_id}.json")),
        serde_json::to_vec_pretty(&provider_doc).unwrap(),
    )
    .unwrap();
}

#[test]
fn base_url_validation_allows_https_and_loopback_http_and_rejects_unsafe_urls() {
    assert!(validate_embedding_base_url("https://api.openai.com/v1").is_ok());
    assert!(validate_embedding_base_url("http://127.0.0.1:11434/v1").is_ok());
    assert!(validate_embedding_base_url("http://localhost:8000/v1").is_ok());
    assert!(validate_embedding_base_url("http://[::1]:8000/v1").is_ok());

    // Non-loopback http, file://, embedded credentials all rejected
    assert_eq!(
        validate_embedding_base_url("http://api.openai.com/v1").unwrap_err().kind,
        EmbeddingErrorKind::InvalidBaseUrl
    );
    assert_eq!(
        validate_embedding_base_url("http://192.168.1.10:8000/v1").unwrap_err().kind,
        EmbeddingErrorKind::InvalidBaseUrl
    );
    assert_eq!(
        validate_embedding_base_url("file:///etc/passwd").unwrap_err().kind,
        EmbeddingErrorKind::InvalidBaseUrl
    );
    assert_eq!(
        validate_embedding_base_url("https://user:secret@api.openai.com/v1").unwrap_err().kind,
        EmbeddingErrorKind::InvalidBaseUrl
    );
}

#[tokio::test]
async fn batch_embed_and_test_connection_work_against_loopback_server_and_quantize_int8() {
    let app = Router::new().route(
        "/v1/embeddings",
        post(|Json(body): Json<Value>| async move {
            let inputs = body["input"].as_array().unwrap();
            let data: Vec<Value> = inputs
                .iter()
                .enumerate()
                .map(|(i, text)| {
                    let s = text.as_str().unwrap_or("");
                    let v = if s.contains("alpha") {
                        vec![1.0_f32, 0.0, 0.0, 0.0]
                    } else {
                        vec![0.0_f32, 1.0, 0.0, 0.0]
                    };
                    json!({ "index": i, "embedding": v })
                })
                .collect();
            Json(json!({ "data": data }))
        }),
    );
    let base_url = spawn_mock_embedding_server(app).await;
    let dir = TempDir::new().unwrap();
    write_test_config(dir.path(), "custom-embed", &base_url, "sk-test-key", "text-embed-v1", true);

    let svc = KnowledgeEmbeddingService::with_fast_backoff(dir.path().to_path_buf());
    let probe = svc.test_connection("custom-embed", "text-embed-v1", None).await;
    assert!(probe.ok);
    assert_eq!(probe.dimension, Some(4));
    assert!(probe.model_revision.as_deref().unwrap().contains("text-embed-v1"));

    let out = svc
        .embed_batch(
            &[
                ("c1".into(), "alpha document".into()),
                ("c2".into(), "alpha again".into()),
                ("c3".into(), "beta topic".into()),
            ],
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.dimension, 4);
    assert_eq!(out.vectors.len(), 3);
    let same_sim = cosine_similarity_int8(
        &out.vectors[0].vector,
        out.vectors[0].scale,
        &out.vectors[1].vector,
        out.vectors[1].scale,
    );
    let diff_sim = cosine_similarity_int8(
        &out.vectors[0].vector,
        out.vectors[0].scale,
        &out.vectors[2].vector,
        out.vectors[2].scale,
    );
    assert!(same_sim > 0.99);
    assert!(diff_sim < 0.05);
}

#[tokio::test]
async fn retries_429_and_500_with_backoff_but_never_retries_401_and_redacts_secrets() {
    let attempts_429 = Arc::new(AtomicUsize::new(0));
    let attempts_401 = Arc::new(AtomicUsize::new(0));
    let attempts_402 = Arc::new(AtomicUsize::new(0));
    let a429 = Arc::clone(&attempts_429);
    let a401 = Arc::clone(&attempts_401);
    let a402 = Arc::clone(&attempts_402);

    let app = Router::new()
        .route(
            "/retry/embeddings",
            post(move |Json(body): Json<Value>| {
                let c = Arc::clone(&a429);
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        let mut headers = HeaderMap::new();
                        headers.insert(RETRY_AFTER, "0".parse().unwrap());
                        return (StatusCode::TOO_MANY_REQUESTS, headers, Json(json!({ "error": "rate" })))
                            .into_response();
                    }
                    if n == 1 {
                        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "boom" })))
                            .into_response();
                    }
                    let count = body["input"].as_array().map(|a| a.len()).unwrap_or(0);
                    let data: Vec<Value> = (0..count)
                        .map(|i| json!({ "index": i, "embedding": [0.6, 0.8] }))
                        .collect();
                    (StatusCode::OK, Json(json!({ "data": data }))).into_response()
                }
            }),
        )
        .route(
            "/authfail/embeddings",
            post(move || {
                let c = Arc::clone(&a401);
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({ "error": { "message": "Invalid sk-secret-token-99999 for TOP_SECRET_CHUNK_PAYLOAD" } })),
                    )
                        .into_response()
                }
            }),
        )
        .route(
            "/balance/embeddings",
            post(move || {
                let c = Arc::clone(&a402);
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    (
                        StatusCode::PAYMENT_REQUIRED,
                        Json(json!({ "error": "insufficient balance" })),
                    )
                        .into_response()
                }
            }),
        )
        .route(
            "/redirect/embeddings",
            post(|| async {
                let mut headers = HeaderMap::new();
                headers.insert("location", "http://127.0.0.1:9/evil".parse().unwrap());
                (StatusCode::FOUND, headers).into_response()
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dir = TempDir::new().unwrap();
    let svc = KnowledgeEmbeddingService::with_fast_backoff(dir.path().to_path_buf());

    // 1. 429 + 500 -> succeeds on 3rd attempt
    write_test_config(
        dir.path(),
        "p-retry",
        &format!("http://{addr}/retry"),
        "sk-retry",
        "m1",
        true,
    );
    let out = svc
        .embed_batch(&[("c1".into(), "hello".into())], None)
        .await
        .unwrap();
    assert_eq!(out.dimension, 2);
    assert_eq!(attempts_429.load(Ordering::SeqCst), 3);

    // 2. 401 -> NEVER retried, and error/display MUST redact API key & chunk payload (🔒)
    write_test_config(
        dir.path(),
        "p-auth",
        &format!("http://{addr}/authfail"),
        "sk-secret-token-99999",
        "m1",
        true,
    );
    let err = svc
        .embed_batch(&[("c1".into(), "TOP_SECRET_CHUNK_PAYLOAD".into())], None)
        .await
        .unwrap_err();
    assert_eq!(err.kind, EmbeddingErrorKind::AuthFailed);
    assert_eq!(err.status_code, Some(401));
    assert_eq!(attempts_401.load(Ordering::SeqCst), 1);

    let serialized = serde_json::to_string(&err).unwrap();
    let displayed = err.to_string();
    for leak in ["sk-secret-token-99999", "TOP_SECRET_CHUNK_PAYLOAD"] {
        assert!(!serialized.contains(leak), "serialized error leaked {leak}");
        assert!(!displayed.contains(leak), "display error leaked {leak}");
    }

    // 3. 402 Payment Required -> NEVER retried, mapped to InsufficientBalance
    write_test_config(
        dir.path(),
        "p-balance",
        &format!("http://{addr}/balance"),
        "sk-bal",
        "m1",
        true,
    );
    let bal_err = svc
        .embed_batch(&[("c1".into(), "hello".into())], None)
        .await
        .unwrap_err();
    assert_eq!(bal_err.kind, EmbeddingErrorKind::InsufficientBalance);
    assert_eq!(bal_err.status_code, Some(402));
    assert_eq!(attempts_402.load(Ordering::SeqCst), 1);

    // 4. Redirect -> strictly rejected
    write_test_config(
        dir.path(),
        "p-redir",
        &format!("http://{addr}/redirect"),
        "sk-r",
        "m1",
        true,
    );
    let redir_err = svc
        .embed_batch(&[("c1".into(), "hello".into())], None)
        .await
        .unwrap_err();
    assert_eq!(redir_err.kind, EmbeddingErrorKind::InvalidBaseUrl);
    assert_eq!(redir_err.status_code, Some(302));
}

#[test]
fn resolves_tokendance_preset_gateway_endpoint() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("config.json"),
        serde_json::to_string(&json!({
            "providerApiKeys": { "tokendance": "td-key-test" },
            "embeddingModel": {
                "enabled": true,
                "providerId": "tokendance",
                "model": "qwen-text-embedding-v4",
                "dimensions": 1024
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let svc = KnowledgeEmbeddingService::new(dir.path().to_path_buf());
    let ep = svc.resolve_active_endpoint().unwrap();
    assert_eq!(ep.provider_id, "tokendance");
    assert_eq!(ep.base_url.as_str(), "https://tokendance.space/gateway/v1");
    assert_eq!(ep.host, "tokendance.space");
    assert_eq!(ep.model, "qwen-text-embedding-v4");
    assert_eq!(ep.dimensions, Some(1024));
    assert_eq!(ep.api_key.as_deref(), Some("td-key-test"));
    assert!(!ep.is_loopback);
}
