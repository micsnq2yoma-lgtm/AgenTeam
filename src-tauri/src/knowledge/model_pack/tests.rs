use std::fs;
use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;

use super::*;

fn test_verifier() -> SignatureVerifier {
    Arc::new(|bytes: &[u8], sig: &str| {
        let expected = format!("test-minisig:{}", sha256_hex_bytes(bytes));
        if sig == expected {
            Ok(())
        } else {
            Err("signature mismatch".into())
        }
    })
}

fn create_valid_bundle(dir: &Path, revision: &str) -> (Vec<u8>, String) {
    let model_bytes = b"FAKE_INT8_ONNX_QWEN3_EMBEDDING_WEIGHTS_V1";
    let tok_bytes = br#"{"version":"1.0","model":{"type":"BPE"}}"#;
    let license_bytes = b"Apache License 2.0 - Qwen3-Embedding-0.6B";

    let files = [
        ("models/embedding/model.int8.onnx", model_bytes.as_slice()),
        ("models/embedding/tokenizer.json", tok_bytes.as_slice()),
        ("legal/LICENSE.txt", license_bytes.as_slice()),
    ];

    let mut manifest_files = Vec::new();
    for (rel, content) in files {
        let full = dir.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, content).unwrap();
        manifest_files.push(SignedEmbeddingFileEntry {
            path: rel.to_string(),
            sha256: sha256_hex_bytes(content),
            size: content.len() as u64,
            url: Some(format!(
                "https://download.myagents.io/models/knowledge/sets/{revision}/{rel}"
            )),
        });
    }

    let manifest = SignedEmbeddingManifest {
        schema_version: 1,
        pack_id: "local-knowledge-embedding-qwen3-0.6b-int8".into(),
        pack_revision: revision.into(),
        dimension: 1024,
        max_tokens: 8192,
        pooling: "last_token".into(),
        normalize: true,
        files: manifest_files,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    let sig = format!("test-minisig:{}", sha256_hex_bytes(&manifest_bytes));
    fs::write(dir.join("manifest.json"), &manifest_bytes).unwrap();
    fs::write(dir.join("manifest.json.minisig"), &sig).unwrap();
    (manifest_bytes, sig)
}

#[test]
fn source_lock_parses_and_enforces_minisign_and_expected_inventory() {
    let lock = parse_source_lock().expect("embedded source lock must be valid");
    assert_eq!(lock.pack_id, "local-knowledge-embedding-qwen3-0.6b-int8");
    assert_eq!(lock.pack_revision, "kb-embed-qwen3-0.6b-int8-v1");
    assert_eq!(lock.dimension, 1024);
    assert_eq!(lock.max_tokens, 8192);
    assert_eq!(lock.download_host, "download.myagents.io");
    assert_eq!(lock.signature_policy.algorithm, "minisign");
    assert!(lock.signature_policy.detached_signature_required);
    assert!(lock
        .expected_files
        .iter()
        .any(|p| p == "models/embedding/model.int8.onnx"));
    assert!(lock
        .expected_files
        .iter()
        .any(|p| p == "models/embedding/tokenizer.json"));
}

#[test]
fn official_download_url_rejects_http_credentials_and_non_official_hosts() {
    assert!(validate_official_download_url(
        "https://download.myagents.io/models/knowledge/sets/v1/manifest.json",
        "download.myagents.io"
    )
    .is_ok());
    assert!(validate_official_download_url(
        "http://download.myagents.io/models/knowledge/sets/v1/manifest.json",
        "download.myagents.io"
    )
    .is_err());
    assert!(validate_official_download_url(
        "https://evil.example.com/models/knowledge/sets/v1/manifest.json",
        "download.myagents.io"
    )
    .is_err());
    assert!(validate_official_download_url(
        "https://user:pass@download.myagents.io/models/knowledge/sets/v1/manifest.json",
        "download.myagents.io"
    )
    .is_err());
}

#[test]
fn local_bundle_install_verifies_signature_and_hashes_publishes_active_pointer_and_survives_restart()
{
    let root = TempDir::new().unwrap();
    let bundle = TempDir::new().unwrap();
    create_valid_bundle(bundle.path(), "kb-embed-qwen3-0.6b-int8-v1");

    let manager = KnowledgeModelPackManager::initialize_with_test_verifier(
        root.path().to_path_buf(),
        test_verifier(),
    )
    .unwrap();
    assert_eq!(manager.status().status, KnowledgeModelPackStatusKind::NotInstalled);
    assert!(!manager.status().usable);

    let status = manager.install_from_local_bundle(bundle.path()).unwrap();
    assert_eq!(status.status, KnowledgeModelPackStatusKind::Ready);
    assert!(status.usable);
    assert_eq!(
        status.active_revision.as_deref(),
        Some("kb-embed-qwen3-0.6b-int8-v1")
    );
    assert!(root.path().join("active.json").exists());

    // Simulate app restart -> reload and re-verify from disk.
    let restarted = KnowledgeModelPackManager::initialize_with_test_verifier(
        root.path().to_path_buf(),
        test_verifier(),
    )
    .unwrap();
    let restarted_status = restarted.status();
    assert_eq!(restarted_status.status, KnowledgeModelPackStatusKind::Ready);
    assert!(restarted_status.usable);
    let active = restarted.active_pack().expect("active pack after restart");
    assert_eq!(active.dimension, 1024);
    assert!(active.model_path.exists());
    assert!(active.tokenizer_path.exists());

    // Clean uninstall degrades back to NotInstalled without error.
    let removed = restarted.remove().unwrap();
    assert_eq!(removed.status, KnowledgeModelPackStatusKind::NotInstalled);
    assert!(!removed.usable);
    assert!(restarted.active_pack().is_none());
    assert!(!root.path().join("active.json").exists());
}

#[test]
fn install_rejects_invalid_signature_tampered_file_hash_and_path_traversal() {
    let root = TempDir::new().unwrap();
    let manager = KnowledgeModelPackManager::initialize_with_test_verifier(
        root.path().to_path_buf(),
        test_verifier(),
    )
    .unwrap();

    // 1. Invalid signature -> KbAccessDenied
    let bad_sig_dir = TempDir::new().unwrap();
    create_valid_bundle(bad_sig_dir.path(), "kb-embed-qwen3-0.6b-int8-v1");
    fs::write(bad_sig_dir.path().join("manifest.json.minisig"), "forged-sig").unwrap();
    assert_eq!(
        manager.install_from_local_bundle(bad_sig_dir.path()).unwrap_err(),
        KbAccessDenied
    );
    assert!(!root.path().join("active.json").exists());

    // 2. Tampered model bytes (same length, different hash) -> KbInvalidInput
    let tampered_dir = TempDir::new().unwrap();
    create_valid_bundle(tampered_dir.path(), "kb-embed-qwen3-0.6b-int8-v1");
    fs::write(
        tampered_dir.path().join("models/embedding/model.int8.onnx"),
        b"XFAKE_INT8_ONNX_QWEN3_EMBEDDING_WEIGHTS_V",
    )
    .unwrap();
    assert_eq!(
        manager.install_from_local_bundle(tampered_dir.path()).unwrap_err(),
        KbInvalidInput
    );
    assert!(!root.path().join("active.json").exists());

    // 3. Path traversal in manifest -> KbInvalidInput
    let traversal_dir = TempDir::new().unwrap();
    let bad_manifest = SignedEmbeddingManifest {
        schema_version: 1,
        pack_id: "local-knowledge-embedding-qwen3-0.6b-int8".into(),
        pack_revision: "kb-embed-qwen3-0.6b-int8-v1".into(),
        dimension: 1024,
        max_tokens: 8192,
        pooling: "last_token".into(),
        normalize: true,
        files: vec![SignedEmbeddingFileEntry {
            path: "../escape.onnx".into(),
            sha256: "a".repeat(64),
            size: 10,
            url: None,
        }],
    };
    let manifest_bytes = serde_json::to_vec(&bad_manifest).unwrap();
    let sig = format!("test-minisig:{}", sha256_hex_bytes(&manifest_bytes));
    fs::write(traversal_dir.path().join("manifest.json"), &manifest_bytes).unwrap();
    fs::write(traversal_dir.path().join("manifest.json.minisig"), &sig).unwrap();
    assert_eq!(
        manager.install_from_local_bundle(traversal_dir.path()).unwrap_err(),
        KbInvalidInput
    );
    assert!(!root.path().join("active.json").exists());
}

#[test]
fn startup_detects_tampered_installed_model_and_degrades_safely() {
    let root = TempDir::new().unwrap();
    let bundle = TempDir::new().unwrap();
    create_valid_bundle(bundle.path(), "kb-embed-qwen3-0.6b-int8-v1");

    let manager = KnowledgeModelPackManager::initialize_with_test_verifier(
        root.path().to_path_buf(),
        test_verifier(),
    )
    .unwrap();
    manager.install_from_local_bundle(bundle.path()).unwrap();

    // Tamper with installed ONNX model on disk after activation.
    let installed_model = root
        .path()
        .join("packs/kb-embed-qwen3-0.6b-int8-v1/models/embedding/model.int8.onnx");
    fs::write(&installed_model, b"CORRUPTED_ONNX_BYTES").unwrap();

    // Restart must detect tampering and mark usable=false (BM25 fallback) instead of panicking.
    let restarted = KnowledgeModelPackManager::initialize_with_test_verifier(
        root.path().to_path_buf(),
        test_verifier(),
    )
    .unwrap();
    let status = restarted.status();
    assert_eq!(status.status, KnowledgeModelPackStatusKind::Error);
    assert!(!status.usable);
    assert!(restarted.active_pack().is_none());
    assert!(status.last_error_code.is_some());
}
