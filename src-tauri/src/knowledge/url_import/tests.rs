use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use reqwest::Url;

use super::*;

#[test]
fn rejects_file_scheme_non_http_schemes_and_credentials() {
    assert_eq!(validate_external_url("file:///etc/passwd").unwrap_err(), KbAccessDenied);
    assert_eq!(validate_external_url("file:///C:/Windows/win.ini").unwrap_err(), KbAccessDenied);
    assert_eq!(validate_external_url("ftp://example.com/a.txt").unwrap_err(), KbAccessDenied);
    assert_eq!(validate_external_url("javascript:alert(1)").unwrap_err(), KbAccessDenied);
    assert_eq!(validate_external_url("data:text/html,<h1>x</h1>").unwrap_err(), KbAccessDenied);
    assert_eq!(
        validate_external_url("https://user:secret@example.com/doc").unwrap_err(),
        KbAccessDenied
    );
}

#[test]
fn rejects_loopback_private_link_local_cgnat_and_internal_hosts() {
    let blocked_urls = [
        "http://127.0.0.1/admin",
        "http://localhost:8080/api",
        "http://sub.localhost/secret",
        "http://10.0.0.1/internal",
        "http://172.16.5.4/internal",
        "http://192.168.1.1/router",
        "http://169.254.169.254/latest/meta-data/",
        "http://100.64.0.1/cgnat",
        "http://0.0.0.0/",
        "http://[::1]/",
        "http://[fc00::1]/",
        "http://[fe80::1]/",
        "http://[::ffff:127.0.0.1]/",
        "https://metadata.google.internal/computeMetadata/v1/",
        "https://printer.local/status",
    ];
    for url in blocked_urls {
        assert_eq!(
            validate_external_url(url).unwrap_err(),
            KbAccessDenied,
            "expected SSRF block for {url}"
        );
    }

    assert!(validate_external_url("https://example.com/docs/guide").is_ok());
    assert!(validate_external_url("https://93.184.216.34/page").is_ok());
}

#[test]
fn rejects_redirect_to_internal_or_file_scheme() {
    let public = Url::parse("https://example.com/start").unwrap();
    assert_eq!(
        validate_redirect_target(&public, "http://127.0.0.1:9000/secret").unwrap_err(),
        KbAccessDenied
    );
    assert_eq!(
        validate_redirect_target(&public, "http://169.254.169.254/latest/meta-data/").unwrap_err(),
        KbAccessDenied
    );
    assert_eq!(
        validate_redirect_target(&public, "file:///etc/hosts").unwrap_err(),
        KbAccessDenied
    );
    assert_eq!(
        validate_redirect_target(&public, "https://docs.example.com/final")
            .unwrap()
            .as_str(),
        "https://docs.example.com/final"
    );
}

#[test]
fn rejects_dns_resolution_containing_any_private_ip() {
    let mixed = vec![
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), 443),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 443),
    ];
    assert_eq!(validate_resolved_addrs(&mixed).unwrap_err(), KbAccessDenied);

    let v6_loopback = vec![SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 443)];
    assert_eq!(
        validate_resolved_addrs(&v6_loopback).unwrap_err(),
        KbAccessDenied
    );

    let public_only = vec![SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)),
        443,
    )];
    assert_eq!(validate_resolved_addrs(&public_only).unwrap().len(), 1);
}

#[test]
fn html_to_markdown_extracts_article_and_strips_scripts_and_unsafe_links() {
    let html = r#"
    <!doctype html>
    <html>
      <head>
        <title>Architecture Guide &amp; Notes</title>
        <style>body { color: red; }</style>
        <script>alert('xss')</script>
      </head>
      <body>
        <nav><a href="/home">Home</a></nav>
        <article>
          <h1>Core Design</h1>
          <p>Read the <a href="/spec">specification</a> and avoid <a href="javascript:evil()">bad</a>.</p>
          <ul><li>Fast</li><li>Safe</li></ul>
        </article>
        <footer>Copyright</footer>
      </body>
    </html>
    "#;
    let base = Url::parse("https://example.com/docs/intro").unwrap();
    let (title, md) = extract_html_to_markdown(html, &base);
    assert_eq!(title, "Architecture Guide & Notes");
    assert!(md.contains("# Core Design"));
    assert!(md.contains("[specification](https://example.com/spec)"));
    assert!(md.contains("- Fast"));
    assert!(md.contains("- Safe"));
    assert!(!md.contains("alert('xss')"));
    assert!(!md.contains("javascript:"));
    assert!(!md.contains("Copyright"));
}
