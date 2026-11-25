//! K21-a Online embedding model service (`KnowledgeEmbeddingService`).
//!
//! Reads `embeddingModel`, `providerApiKeys[providerId]`, and provider `baseUrl`
//! fresh from `<data_dir>/config.json` and `<data_dir>/providers/*.json` on every
//! call (never caches a stale config copy).
//!
//! Security & privacy boundaries:
//! - Only `https://` URLs or loopback `http://` URLs (`127.0.0.1`, `localhost`, `::1`)
//!   are permitted; embedded credentials and HTTP redirects are strictly rejected.
//! - Outbound requests go through `proxy_config::build_client_with_proxy_for_provider`
//!   (or `local_http::builder` for loopback endpoints so system proxies never hijack localhost).
//! - Logs and error values NEVER include API keys, chunk text, query text, or raw
//!   upstream response bodies — only sanitized error kinds and HTTP status codes.

#[cfg(test)]
mod tests;

use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::knowledge::embedding::{quantize_l2_int8, QuantizedEmbedding};
use crate::ulog_warn;

pub const MAX_ONLINE_EMBED_BATCH_ITEMS: usize = 64;
pub const MAX_ONLINE_EMBED_BATCH_CHARS: usize = 100_000;
pub const MAX_ONLINE_EMBED_ITEM_CHARS: usize = 16_000;
pub const MAX_ONLINE_EMBED_RETRIES: usize = 5;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRY_AFTER_SECS: u64 = 30;
const BASE_BACKOFF_MS: u64 = 250;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingModelConfig {
    pub enabled: bool,
    pub provider_id: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEmbeddingEndpoint {
    pub provider_id: String,
    pub model: String,
    pub base_url: Url,
    pub host: String,
    pub api_key: Option<String>,
    pub dimensions: Option<usize>,
    pub is_loopback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingErrorKind {
    Disabled,
    NotConfigured,
    InvalidBaseUrl,
    AuthFailed,
    InsufficientBalance,
    ModelNotFound,
    RateLimited,
    ServerError,
    NetworkUnreachable,
    Timeout,
    EgressBlocked,
    InvalidResponse,
    InvalidInput,
}

/// Sanitized error type: guaranteed NEVER to carry API keys, chunk text,
/// query text, or raw upstream bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingError {
    pub kind: EmbeddingErrorKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
}

impl EmbeddingError {
    pub const fn new(kind: EmbeddingErrorKind) -> Self {
        Self {
            kind,
            status_code: None,
        }
    }

    pub const fn with_status(kind: EmbeddingErrorKind, status_code: u16) -> Self {
        Self {
            kind,
            status_code: Some(status_code),
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind,
            EmbeddingErrorKind::RateLimited
                | EmbeddingErrorKind::ServerError
                | EmbeddingErrorKind::NetworkUnreachable
                | EmbeddingErrorKind::Timeout
        )
    }
}

impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status_code {
            Some(code) => write!(f, "embedding_error({:?}, status={code})", self.kind),
            None => write!(f, "embedding_error({:?})", self.kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingConnectionTestResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_revision: Option<String>,
    pub latency_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<EmbeddingErrorKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OnlineEmbedBatchOutput {
    pub model_revision: String,
    pub dimension: usize,
    pub vectors: Vec<QuantizedEmbedding>,
    pub elapsed_ms: u64,
}

#[derive(Serialize)]
struct OpenAiEmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Deserialize)]
struct OpenAiEmbedResponse {
    data: Vec<OpenAiEmbedDatum>,
}

#[derive(Deserialize)]
struct OpenAiEmbedDatum {
    index: usize,
    embedding: Vec<f32>,
}

/// Deterministic model revision string: `online:<host>:<model>:<dimension>`,
/// sanitized for safe use as a filesystem directory component.
pub fn format_online_model_revision(host: &str, model: &str, dimension: usize) -> String {
    let sanitize = |raw: &str| -> String {
        raw.trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '_'
                }
            })
            .take(48)
            .collect()
    };
    let h = sanitize(host);
    let m = sanitize(model);
    format!("online_{h}_{m}_{dimension}")
}

/// Strict `baseUrl` validation (K21 §5.1 🔒):
/// - Must be `https://`, OR `http://` exclusively on a loopback host (`127.0.0.1`, `localhost`, `::1`).
/// - Must NOT contain embedded username or password credentials.
pub fn validate_embedding_base_url(raw: &str) -> Result<(Url, bool), EmbeddingError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
        return Err(EmbeddingError::new(EmbeddingErrorKind::InvalidBaseUrl));
    }
    let url = Url::parse(trimmed)
        .map_err(|_| EmbeddingError::new(EmbeddingErrorKind::InvalidBaseUrl))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(EmbeddingError::new(EmbeddingErrorKind::InvalidBaseUrl));
    }
    let host = url
        .host_str()
        .ok_or_else(|| EmbeddingError::new(EmbeddingErrorKind::InvalidBaseUrl))?;
    let is_loopback = is_loopback_host(host);
    match url.scheme() {
        "https" => Ok((url, is_loopback)),
        "http" if is_loopback => Ok((url, true)),
        _ => Err(EmbeddingError::new(EmbeddingErrorKind::InvalidBaseUrl)),
    }
}

fn is_loopback_host(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return true;
    }
    let unbracketed = lower.trim_start_matches('[').trim_end_matches(']');
    unbracketed
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// OpenAI-compatible preset base URLs for known built-in providers.
fn preset_openai_embedding_base_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "tokendance" => Some("https://tokendance.space/gateway/v1"),
        "openai" | "openai-api" => Some("https://api.openai.com/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "siliconflow" => Some("https://api.siliconflow.cn/v1"),
        "google-gemini" => Some("https://generativelanguage.googleapis.com/v1beta/openai"),
        "volcengine-api" => Some("https://ark.cn-beijing.volces.com/api/v3"),
        "aliyun-bailian" | "aliyun-bailian-coding" => {
            Some("https://dashscope.aliyuncs.com/compatible-mode/v1")
        }
        "zhipu" => Some("https://open.bigmodel.cn/api/paas/v4"),
        "moonshot" => Some("https://api.moonshot.cn/v1"),
        "deepseek" => Some("https://api.deepseek.com/v1"),
        _ => None,
    }
}

fn build_embeddings_endpoint_url(base_url: &Url) -> Result<Url, EmbeddingError> {
    let mut s = base_url.as_str().trim_end_matches('/').to_string();
    if !s.ends_with("/embeddings") {
        s.push_str("/embeddings");
    }
    Url::parse(&s).map_err(|_| EmbeddingError::new(EmbeddingErrorKind::InvalidBaseUrl))
}

fn parse_retry_after_seconds(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let secs = raw.parse::<u64>().ok()?;
    Some(Duration::from_secs(secs.min(MAX_RETRY_AFTER_SECS)))
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}

pub struct KnowledgeEmbeddingService {
    data_dir: PathBuf,
    retry_base_backoff_ms: u64,
}

impl KnowledgeEmbeddingService {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            retry_base_backoff_ms: BASE_BACKOFF_MS,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_fast_backoff(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            retry_base_backoff_ms: 5,
        }
    }

    /// Read current `embeddingModel` config fresh from disk (`config.json`).
    pub fn current_config(&self) -> Option<EmbeddingModelConfig> {
        let root = self.read_config_root()?;
        let val = root.get("embeddingModel")?;
        serde_json::from_value::<EmbeddingModelConfig>(val.clone()).ok()
    }

    /// Resolve active endpoint from disk config; returns `Err(Disabled)` or
    /// `Err(NotConfigured)` when embedding should degrade to BM25.
    pub fn resolve_active_endpoint(&self) -> Result<ResolvedEmbeddingEndpoint, EmbeddingError> {
        let cfg = self
            .current_config()
            .ok_or_else(|| EmbeddingError::new(EmbeddingErrorKind::NotConfigured))?;
        if !cfg.enabled {
            return Err(EmbeddingError::new(EmbeddingErrorKind::Disabled));
        }
        self.resolve_endpoint_for_params(&cfg.provider_id, &cfg.model, cfg.dimensions)
    }

    pub fn resolve_endpoint_for_params(
        &self,
        provider_id: &str,
        model: &str,
        dimensions: Option<usize>,
    ) -> Result<ResolvedEmbeddingEndpoint, EmbeddingError> {
        let provider_id = provider_id.trim();
        let model = model.trim();
        if provider_id.is_empty() || model.is_empty() {
            return Err(EmbeddingError::new(EmbeddingErrorKind::NotConfigured));
        }
        let root = self.read_config_root();
        let api_key = root
            .as_ref()
            .and_then(|r| r.get("providerApiKeys"))
            .and_then(|m| m.get(provider_id))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        let raw_base = self
            .read_custom_provider_base_url(provider_id)
            .or_else(|| preset_openai_embedding_base_url(provider_id).map(str::to_owned))
            .ok_or_else(|| EmbeddingError::new(EmbeddingErrorKind::NotConfigured))?;

        let (base_url, is_loopback) = validate_embedding_base_url(&raw_base)?;
        let host = base_url
            .host_str()
            .ok_or_else(|| EmbeddingError::new(EmbeddingErrorKind::InvalidBaseUrl))?
            .to_string();

        Ok(ResolvedEmbeddingEndpoint {
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            base_url,
            host,
            api_key,
            dimensions,
            is_loopback,
        })
    }

    fn read_config_root(&self) -> Option<serde_json::Value> {
        let path = self.data_dir.join("config.json");
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(crate::utils::bom::strip_bom(&content)).ok()
    }

    fn read_custom_provider_base_url(&self, provider_id: &str) -> Option<String> {
        let providers_dir = self.data_dir.join("providers");
        let entries = fs::read_dir(&providers_dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(
                crate::utils::bom::strip_bom(&text),
            ) else {
                continue;
            };
            if json.get("id").and_then(|v| v.as_str()) == Some(provider_id) {
                if let Some(url) = json
                    .get("config")
                    .and_then(|c| c.get("baseUrl"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    return Some(url.to_string());
                }
            }
        }
        None
    }

    fn build_http_client(
        &self,
        endpoint: &ResolvedEmbeddingEndpoint,
        timeout: Duration,
    ) -> Result<reqwest::Client, EmbeddingError> {
        if endpoint.is_loopback {
            crate::local_http::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(timeout)
                .build()
                .map_err(|_| EmbeddingError::new(EmbeddingErrorKind::NetworkUnreachable))
        } else {
            // Non-loopback external model provider: route through provider-scoped proxy_config
            // (which also enforces `ensure_model_egress_admitted()`).
            #[allow(clippy::disallowed_methods)]
            let builder = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(timeout);
            crate::proxy_config::build_client_with_proxy_for_provider(
                builder,
                &endpoint.provider_id,
            )
            .map_err(|msg| {
                if msg.contains("限制模式") || msg.contains("出口 IP") {
                    EmbeddingError::new(EmbeddingErrorKind::EgressBlocked)
                } else {
                    EmbeddingError::new(EmbeddingErrorKind::NetworkUnreachable)
                }
            })
        }
    }

    /// Test connection with a short probe text and return actual vector dimension.
    pub async fn test_connection(
        &self,
        provider_id: &str,
        model: &str,
        dimensions: Option<usize>,
    ) -> EmbeddingConnectionTestResult {
        let started = Instant::now();
        let endpoint = match self.resolve_endpoint_for_params(provider_id, model, dimensions) {
            Ok(ep) => ep,
            Err(err) => {
                return EmbeddingConnectionTestResult {
                    ok: false,
                    dimension: None,
                    model_revision: None,
                    latency_ms: started.elapsed().as_millis() as u64,
                    error_kind: Some(err.kind),
                    status_code: err.status_code,
                };
            }
        };

        let items = vec![("probe".to_string(), "ping".to_string())];
        match self
            .embed_with_endpoint(&endpoint, &items, Duration::from_secs(15), 1)
            .await
        {
            Ok(out) => EmbeddingConnectionTestResult {
                ok: true,
                dimension: Some(out.dimension),
                model_revision: Some(out.model_revision),
                latency_ms: started.elapsed().as_millis() as u64,
                error_kind: None,
                status_code: None,
            },
            Err(err) => EmbeddingConnectionTestResult {
                ok: false,
                dimension: None,
                model_revision: None,
                latency_ms: started.elapsed().as_millis() as u64,
                error_kind: Some(err.kind),
                status_code: err.status_code,
            },
        }
    }

    /// Embed a batch of `(id, text)` items using the currently active online config.
    pub async fn embed_batch(
        &self,
        items: &[(String, String)],
        timeout: Option<Duration>,
    ) -> Result<OnlineEmbedBatchOutput, EmbeddingError> {
        let endpoint = self.resolve_active_endpoint()?;
        self.embed_with_endpoint(
            &endpoint,
            items,
            timeout.unwrap_or(DEFAULT_REQUEST_TIMEOUT),
            MAX_ONLINE_EMBED_RETRIES,
        )
        .await
    }

    /// Core HTTP execution with retry/backoff, redirect rejection, and strict redaction.
    pub async fn embed_with_endpoint(
        &self,
        endpoint: &ResolvedEmbeddingEndpoint,
        items: &[(String, String)],
        timeout: Duration,
        max_retries: usize,
    ) -> Result<OnlineEmbedBatchOutput, EmbeddingError> {
        if items.is_empty() || items.len() > MAX_ONLINE_EMBED_BATCH_ITEMS {
            return Err(EmbeddingError::new(EmbeddingErrorKind::InvalidInput));
        }

        let mut truncated_inputs = Vec::with_capacity(items.len());
        let mut total_chars = 0usize;
        for (_, text) in items {
            let trimmed = truncate_chars(text, MAX_ONLINE_EMBED_ITEM_CHARS);
            total_chars = total_chars.saturating_add(trimmed.chars().count());
            if total_chars > MAX_ONLINE_EMBED_BATCH_CHARS {
                return Err(EmbeddingError::new(EmbeddingErrorKind::InvalidInput));
            }
            truncated_inputs.push(trimmed);
        }

        let url = build_embeddings_endpoint_url(&endpoint.base_url)?;
        let client = self.build_http_client(endpoint, timeout)?;
        let started = Instant::now();

        let body = OpenAiEmbedRequest {
            model: &endpoint.model,
            input: &truncated_inputs,
            dimensions: endpoint.dimensions,
        };

        let mut attempt = 0usize;
        loop {
            let mut req = client
                .post(url.clone())
                .header(CONTENT_TYPE, "application/json")
                .json(&body);
            if let Some(key) = endpoint.api_key.as_deref() {
                req = req.header(AUTHORIZATION, format!("Bearer {key}"));
            }

            let response = match req.send().await {
                Ok(resp) => resp,
                Err(err) => {
                    let kind = if err.is_timeout() {
                        EmbeddingErrorKind::Timeout
                    } else {
                        EmbeddingErrorKind::NetworkUnreachable
                    };
                    let mapped = EmbeddingError::new(kind);
                    ulog_warn!(
                        "[knowledge:embedding] request failed provider={} attempt={} kind={:?}",
                        endpoint.provider_id,
                        attempt + 1,
                        mapped.kind
                    );
                    if attempt < max_retries && mapped.is_retryable() {
                        attempt += 1;
                        let delay = Duration::from_millis(
                            self.retry_base_backoff_ms.saturating_mul(1_u64 << attempt.min(6)),
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(mapped);
                }
            };

            let status = response.status();
            let status_u16 = status.as_u16();
            if status.is_redirection() {
                // Redirects are strictly forbidden per K21 §5.1.
                ulog_warn!(
                    "[knowledge:embedding] redirect rejected provider={} status={}",
                    endpoint.provider_id,
                    status_u16
                );
                return Err(EmbeddingError::with_status(
                    EmbeddingErrorKind::InvalidBaseUrl,
                    status_u16,
                ));
            }

            if status_u16 == 401 || status_u16 == 403 {
                // Auth errors MUST NOT be retried (K21 §5.4).
                ulog_warn!(
                    "[knowledge:embedding] auth rejected provider={} status={}",
                    endpoint.provider_id,
                    status_u16
                );
                return Err(EmbeddingError::with_status(
                    EmbeddingErrorKind::AuthFailed,
                    status_u16,
                ));
            }

            if status_u16 == 402 {
                // Insufficient balance (e.g. TokenDance 402 Payment Required) MUST NOT be retried.
                ulog_warn!(
                    "[knowledge:embedding] insufficient balance provider={} status={}",
                    endpoint.provider_id,
                    status_u16
                );
                return Err(EmbeddingError::with_status(
                    EmbeddingErrorKind::InsufficientBalance,
                    status_u16,
                ));
            }

            if status_u16 == 404 {
                ulog_warn!(
                    "[knowledge:embedding] model or endpoint not found provider={} status={}",
                    endpoint.provider_id,
                    status_u16
                );
                return Err(EmbeddingError::with_status(
                    EmbeddingErrorKind::ModelNotFound,
                    status_u16,
                ));
            }

            if status_u16 == 429 || status.is_server_error() {
                let kind = if status_u16 == 429 {
                    EmbeddingErrorKind::RateLimited
                } else {
                    EmbeddingErrorKind::ServerError
                };
                let retry_after = parse_retry_after_seconds(response.headers());
                ulog_warn!(
                    "[knowledge:embedding] transient error provider={} status={} attempt={}",
                    endpoint.provider_id,
                    status_u16,
                    attempt + 1
                );
                if attempt < max_retries {
                    attempt += 1;
                    let delay = retry_after.unwrap_or_else(|| {
                        Duration::from_millis(
                            self.retry_base_backoff_ms.saturating_mul(1_u64 << attempt.min(6)),
                        )
                    });
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(EmbeddingError::with_status(kind, status_u16));
            }

            if !status.is_success() {
                ulog_warn!(
                    "[knowledge:embedding] non-success response provider={} status={}",
                    endpoint.provider_id,
                    status_u16
                );
                return Err(EmbeddingError::with_status(
                    EmbeddingErrorKind::InvalidResponse,
                    status_u16,
                ));
            }

            let parsed: OpenAiEmbedResponse = response
                .json()
                .await
                .map_err(|_| EmbeddingError::new(EmbeddingErrorKind::InvalidResponse))?;

            if parsed.data.len() != items.len() {
                return Err(EmbeddingError::new(EmbeddingErrorKind::InvalidResponse));
            }

            let mut ordered: Vec<Option<Vec<f32>>> = vec![None; items.len()];
            for datum in parsed.data {
                if datum.index >= items.len() || datum.embedding.is_empty() {
                    return Err(EmbeddingError::new(EmbeddingErrorKind::InvalidResponse));
                }
                ordered[datum.index] = Some(datum.embedding);
            }

            let mut dimension = 0usize;
            let mut vectors = Vec::with_capacity(items.len());
            for (idx, maybe_vec) in ordered.into_iter().enumerate() {
                let raw = maybe_vec
                    .ok_or_else(|| EmbeddingError::new(EmbeddingErrorKind::InvalidResponse))?;
                if dimension == 0 {
                    dimension = raw.len();
                } else if raw.len() != dimension {
                    return Err(EmbeddingError::new(EmbeddingErrorKind::InvalidResponse));
                }
                let (q, scale) = quantize_l2_int8(&raw, true);
                vectors.push(QuantizedEmbedding {
                    id: items[idx].0.clone(),
                    vector: q,
                    scale,
                });
            }

            let model_revision =
                format_online_model_revision(&endpoint.host, &endpoint.model, dimension);
            return Ok(OnlineEmbedBatchOutput {
                model_revision,
                dimension,
                vectors,
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}
