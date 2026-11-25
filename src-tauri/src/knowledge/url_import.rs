//! K19 external URL import with strict SSRF boundaries and HTML-to-Markdown extraction.
//!
//! Security boundaries:
//! - Only `http://` and `https://` schemes are allowed (`file://`, `data:`, etc. rejected).
//! - Every hop (initial URL and up to 5 redirects) validates scheme, credentials,
//!   host, and ALL DNS-resolved IP addresses against global public IP rules
//!   (rejecting loopback, RFC1918 private, link-local, CGNAT, ULA, IPv4-mapped, etc.).
//! - Each hop pins the verified socket addresses via `ClientBuilder::resolve_to_addrs`
//!   to prevent DNS rebinding between check and connect.
//! - Response body is strictly bounded (`MAX_URL_RESPONSE_BYTES = 5 MiB`).

#[cfg(test)]
mod tests;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use reqwest::Url;

use crate::knowledge::ingest::{ImportOutcome, KnowledgeIngestManager};
use crate::knowledge::types::{is_knowledge_base_id, KnowledgeErrorCode};
use KnowledgeErrorCode::{
    KbAccessDenied, KbInvalidInput, KbLimitExceeded, KbSourceUnsupported, KbStoreUnavailable,
};

pub const MAX_URL_RESPONSE_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_URL_REDIRECTS: usize = 5;
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

/// Strict public unicast IP check aligned with `egress_restriction::public_ip`.
pub fn is_public_global_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, d] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_documentation()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 198 && (18..=19).contains(&b))
                || (a == 192 && b == 0 && c == 0 && d != 9 && d != 10))
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_global_ip(IpAddr::V4(mapped));
            }
            let [a, b, ..] = v6.segments();
            (a & 0xe000) == 0x2000
                && (a, b) != (0x2001, 0x0db8)
                && !(a == 0x3fff && (b & 0xf000) == 0)
        }
    }
}

/// Validate URL scheme, absence of embedded credentials, and non-internal host literal.
pub fn validate_external_url(raw: &str) -> Result<Url, KnowledgeErrorCode> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
        return Err(KbInvalidInput);
    }
    let url = Url::parse(trimmed).map_err(|_| KbInvalidInput)?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(KbAccessDenied),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(KbAccessDenied);
    }
    let Some(host_str) = url.host_str() else {
        return Err(KbInvalidInput);
    };
    let lower = host_str.to_ascii_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower == "metadata.google.internal"
    {
        return Err(KbAccessDenied);
    }
    let unbracketed = lower.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = unbracketed.parse::<IpAddr>() {
        if !is_public_global_ip(ip) {
            return Err(KbAccessDenied);
        }
    }
    Ok(url)
}

/// Validate all resolved addresses for a URL hop.
pub fn validate_resolved_addrs(addrs: &[SocketAddr]) -> Result<Vec<SocketAddr>, KnowledgeErrorCode> {
    if addrs.is_empty() {
        return Err(KbStoreUnavailable);
    }
    for addr in addrs {
        if !is_public_global_ip(addr.ip()) {
            return Err(KbAccessDenied);
        }
    }
    Ok(addrs.to_vec())
}

/// Resolve and validate a redirect target against the current URL.
pub fn validate_redirect_target(
    current: &Url,
    location_header: &str,
) -> Result<Url, KnowledgeErrorCode> {
    let next = current.join(location_header.trim()).map_err(|_| KbInvalidInput)?;
    validate_external_url(next.as_str())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedWebDocument {
    pub final_url: String,
    pub title: String,
    pub markdown: String,
}

/// Fetch a public HTTP(S) URL with per-hop SSRF validation, DNS pinning, redirect cap,
/// and strict response byte cap.
pub async fn fetch_url_as_markdown(raw_url: &str) -> Result<FetchedWebDocument, KnowledgeErrorCode> {
    let mut current = validate_external_url(raw_url)?;

    for hop in 0..=MAX_URL_REDIRECTS {
        let host = current.host_str().ok_or(KbInvalidInput)?.to_string();
        let port = current.port_or_known_default().ok_or(KbInvalidInput)?;
        let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|_| KbStoreUnavailable)?
            .collect();
        let pinned_addrs = validate_resolved_addrs(&resolved)?;

        // External public web host fetch with DNS pinning and redirects disabled per hop.
        #[allow(clippy::disallowed_methods)]
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(FETCH_TIMEOUT)
            .user_agent("AgenTeam-KnowledgeImporter/1.0")
            .resolve_to_addrs(&host, &pinned_addrs)
            .build()
            .map_err(|_| KbStoreUnavailable)?;

        let resp = client
            .get(current.clone())
            .send()
            .await
            .map_err(|_| KbStoreUnavailable)?;

        if resp.status().is_redirection() {
            if hop == MAX_URL_REDIRECTS {
                return Err(KbLimitExceeded);
            }
            let loc = resp
                .headers()
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(KbInvalidInput)?;
            current = validate_redirect_target(&current, loc)?;
            continue;
        }

        if !resp.status().is_success() {
            return Err(KbStoreUnavailable);
        }

        if let Some(len) = resp
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
        {
            if len > MAX_URL_RESPONSE_BYTES {
                return Err(KbLimitExceeded);
            }
        }

        let content_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_ascii_lowercase();
        let is_html = content_type.contains("text/html")
            || content_type.contains("application/xhtml+xml");
        let is_text = is_html
            || content_type.contains("text/plain")
            || content_type.contains("text/markdown");
        if !is_text {
            return Err(KbSourceUnsupported);
        }

        let mut body = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| KbStoreUnavailable)?;
            if (body.len() as u64).saturating_add(chunk.len() as u64) > MAX_URL_RESPONSE_BYTES {
                return Err(KbLimitExceeded);
            }
            body.extend_from_slice(&chunk);
        }

        let text = String::from_utf8_lossy(&body);
        let final_url = current.to_string();
        let (title, md_body) = if is_html {
            extract_html_to_markdown(&text, &current)
        } else {
            let fallback_title = current.host_str().unwrap_or("web-page").to_string();
            (fallback_title, text.into_owned())
        };
        let markdown = format!("# {title}\n\nSource: {final_url}\n\n{}", md_body.trim());
        return Ok(FetchedWebDocument {
            final_url,
            title,
            markdown,
        });
    }

    Err(KbLimitExceeded)
}

/// Fetch a public URL safely and import its extracted Markdown into the knowledge base
/// through the standard K2 ingest pipeline.
pub async fn import_url_into_kb(
    ingest: Arc<KnowledgeIngestManager>,
    kb_id: String,
    raw_url: String,
) -> Result<ImportOutcome, KnowledgeErrorCode> {
    if !is_knowledge_base_id(&kb_id) {
        return Err(KbInvalidInput);
    }
    let doc = fetch_url_as_markdown(&raw_url).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let temp_dir = tempfile::tempdir().map_err(|_| KbStoreUnavailable)?;
        let safe_stem: String = doc
            .title
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                    c
                } else {
                    '_'
                }
            })
            .take(64)
            .collect();
        let stem = safe_stem.trim();
        let filename = if stem.is_empty() {
            "web-page.md".to_string()
        } else {
            format!("{stem}.md")
        };
        let file_path = temp_dir.path().join(filename);
        std::fs::write(&file_path, doc.markdown.as_bytes()).map_err(|_| KbStoreUnavailable)?;
        let mut outcomes = ingest.import_files(&kb_id, vec![file_path]);
        outcomes.pop().ok_or(KbStoreUnavailable)
    })
    .await
    .map_err(|_| KbStoreUnavailable)?
}

/// Lightweight, dependency-free HTML-to-Markdown main content extractor.
pub fn extract_html_to_markdown(html: &str, base_url: &Url) -> (String, String) {
    let title = extract_tag_inner(html, "title")
        .map(|t| decode_html_entities(t.trim()))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| base_url.host_str().unwrap_or("Web Page").to_string());

    let mut cleaned = strip_html_comments(html);
    for noise in ["script", "style", "noscript", "nav", "footer", "aside", "svg", "iframe"] {
        cleaned = strip_tag_blocks(&cleaned, noise);
    }

    // Prefer <main> or <article> when present.
    let scope = extract_tag_inner(&cleaned, "article")
        .or_else(|| extract_tag_inner(&cleaned, "main"))
        .or_else(|| extract_tag_inner(&cleaned, "body"))
        .unwrap_or(cleaned);

    let mut out = String::with_capacity(scope.len() / 2);
    let mut in_pre = false;
    let mut link_href: Option<String> = None;
    let mut link_text = String::new();
    let bytes = scope.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(rel_end) = scope[i..].find('>') {
                let tag_raw = &scope[i + 1..i + rel_end];
                let tag_trim = tag_raw.trim();
                let closing = tag_trim.starts_with('/');
                let name_start = usize::from(closing);
                let tag_name: String = tag_trim[name_start..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();

                if !closing {
                    match tag_name.as_str() {
                        "h1" => out.push_str("\n\n# "),
                        "h2" => out.push_str("\n\n## "),
                        "h3" => out.push_str("\n\n### "),
                        "h4" => out.push_str("\n\n#### "),
                        "h5" => out.push_str("\n\n##### "),
                        "h6" => out.push_str("\n\n###### "),
                        "p" | "div" | "section" | "tr" => out.push_str("\n\n"),
                        "br" => out.push('\n'),
                        "li" => out.push_str("\n- "),
                        "blockquote" => out.push_str("\n\n> "),
                        "pre" => {
                            in_pre = true;
                            out.push_str("\n\n```\n");
                        }
                        "code" if !in_pre => out.push('`'),
                        "a" => {
                            if let Some(href) = extract_href(tag_trim) {
                                if let Ok(joined) = base_url.join(&href) {
                                    if matches!(joined.scheme(), "http" | "https") {
                                        link_href = Some(joined.to_string());
                                        link_text.clear();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    match tag_name.as_str() {
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "blockquote" => {
                            out.push_str("\n\n");
                        }
                        "pre" => {
                            in_pre = false;
                            out.push_str("\n```\n\n");
                        }
                        "code" if !in_pre => out.push('`'),
                        "a" => {
                            if let Some(href) = link_href.take() {
                                let label = link_text.trim();
                                if !label.is_empty() {
                                    out.push('[');
                                    out.push_str(label);
                                    out.push_str("](");
                                    out.push_str(&href);
                                    out.push(')');
                                }
                                link_text.clear();
                            }
                        }
                        _ => {}
                    }
                }
                i += rel_end + 1;
                continue;
            }
        }

        let next_lt = scope[i..].find('<').map(|o| i + o).unwrap_or(scope.len());
        let chunk = decode_html_entities(&scope[i..next_lt]);
        if link_href.is_some() {
            link_text.push_str(&chunk);
        } else if in_pre {
            out.push_str(&chunk);
        } else {
            let collapsed = collapse_inline_ws(&chunk);
            if !collapsed.is_empty() {
                out.push_str(&collapsed);
            }
        }
        i = next_lt;
    }

    (title, normalize_blank_lines(&out))
}

fn extract_tag_inner(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}>");
    let start_idx = lower.find(&open_pat)?;
    let after_open = lower[start_idx..].find('>')? + start_idx + 1;
    let end_idx = lower[after_open..].find(&close_pat)? + after_open;
    Some(html[after_open..end_idx].to_string())
}

fn strip_html_comments(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start + 4..].find("-->") {
            rest = &rest[start + 4 + end + 3..];
        } else {
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn strip_tag_blocks(html: &str, tag: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}>");
    let mut cursor = 0usize;
    while let Some(rel) = lower[cursor..].find(&open_pat) {
        let start = cursor + rel;
        let next_ch = lower.as_bytes().get(start + open_pat.len()).copied();
        if !matches!(next_ch, Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/')) {
            out.push_str(&html[cursor..start + open_pat.len()]);
            cursor = start + open_pat.len();
            continue;
        }
        out.push_str(&html[cursor..start]);
        if let Some(close_rel) = lower[start..].find(&close_pat) {
            cursor = start + close_rel + close_pat.len();
        } else {
            return out;
        }
    }
    out.push_str(&html[cursor..]);
    out
}

fn extract_href(tag_content: &str) -> Option<String> {
    let lower = tag_content.to_ascii_lowercase();
    let pos = lower.find("href=")?;
    let after = tag_content[pos + 5..].trim_start();
    if let Some(quote) = after.chars().next().filter(|&c| c == '"' || c == '\'') {
        let rest = &after[1..];
        let end = rest.find(quote)?;
        Some(decode_html_entities(&rest[..end]))
    } else {
        let end = after
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(after.len());
        Some(decode_html_entities(&after[..end]))
    }
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn collapse_inline_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out
}

fn normalize_blank_lines(text: &str) -> String {
    let mut lines = Vec::new();
    let mut blank_run = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 && !lines.is_empty() {
                lines.push(String::new());
            }
        } else {
            blank_run = 0;
            lines.push(trimmed.trim_start().to_string());
        }
    }
    lines.join("\n").trim().to_string()
}
