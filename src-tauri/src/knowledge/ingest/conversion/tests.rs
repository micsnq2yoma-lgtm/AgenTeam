use super::*;
use crate::document_processing::DocumentProcessingManager;
use crate::knowledge::ingest::{ImportOutcome, KnowledgeIngestManager};
use crate::knowledge::store::{KnowledgeBasePatch, KnowledgeStore, SourceStatus};
use crate::local_inference::{LocalComputeCoordinator, LocalInferenceRuntimeRegistry};
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn decodes_utf8_and_bom_marked_unicode_without_losing_text() {
    let expected = "# 清单 🗂\r\n";
    let mut variants = vec![
        expected.as_bytes().to_vec(),
        format!("\u{feff}{expected}").into_bytes(),
    ];
    for big_endian in [false, true] {
        let mut bytes = if big_endian {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        for unit in expected.encode_utf16() {
            bytes.extend(if big_endian {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
        variants.push(bytes);
    }
    for bytes in variants {
        assert_eq!(decode_text(bytes).unwrap(), expected);
    }
}

#[test]
fn corrupt_or_ambiguous_binary_input_does_not_become_lossy_markdown() {
    for bytes in [
        vec![0xff],
        vec![0xfe, 0xff, 0],
        vec![0xff, 0xfe, 0, 0xd8],
        vec![b'A', 0, b'B', 0],
        vec![0xff, 0xfe, 0, 0, b'A', 0, 0, 0],
    ] {
        assert_eq!(decode_text(bytes), Err(INVALID_TEXT));
    }
}

#[test]
fn bounds_managed_text_and_keeps_failed_or_cancelled_content_private() {
    let temp = tempdir().unwrap();
    let source = temp
        .path()
        .canonicalize()
        .unwrap()
        .join("original.Markdown");
    let stage = temp.path().join("stage");
    fs::create_dir(&stage).unwrap();
    let stage = stage.canonicalize().unwrap();
    let file = fs::File::create(&source).unwrap();
    file.set_len(TEXT_LIMIT + 1).unwrap();
    drop(file);
    let cancelled = AtomicBool::new(false);
    assert_eq!(source_limit(&source), Ok(TEXT_LIMIT));
    assert!(matches!(
        convert(&source, &stage, None, &cancelled, |_| unreachable!()),
        Err(Failed(KbLimitExceeded, _))
    ));
    fs::write(&source, b"no partial document").unwrap();
    cancelled.store(true, Ordering::Release);
    assert_eq!(
        convert(&source, &stage, None, &cancelled, |_| unreachable!()),
        Err(ConversionError::Cancelled)
    );
    assert!(!stage.join("content.md").exists());
}

#[test]
fn source_admission_and_errors_share_the_anydoc_format_and_privacy_contracts() {
    for ext in ["docx", "PDF", "odt", "jpeg"] {
        assert_eq!(
            source_limit(Path::new(&format!("original.{ext}"))),
            Ok(MAX_SOURCE_BYTES)
        );
    }
    assert_eq!(source_limit(Path::new("original.exe")), Err(UNSUPPORTED));
    assert_eq!(
        document_error("DOCUMENT_PASSWORD_REQUIRED"),
        Failed(KbInvalidInput, "provideUnencryptedCopy")
    );
    assert_eq!(
        document_error("private file text or path"),
        Failed(KbStoreUnavailable, "checkDocumentConverter")
    );
}

#[test]
fn source_in_use_keeps_the_same_recovery_at_admission_conversion_and_publication() {
    let failure = crate::knowledge::ingest::ImportFailure::from(KbSourceInUse);
    assert_eq!(
        serde_json::to_value(failure).unwrap(),
        serde_json::json!({"code": "KB_SOURCE_IN_USE", "suggestion": "closeFileAndRetry"})
    );
    let expected = Failed(KbSourceInUse, "closeFileAndRetry");
    assert_eq!(document_error("DOCUMENT_SOURCE_IN_USE"), expected);
    assert_eq!(ConversionError::store(KbSourceInUse), expected);
}

// Run with MYAGENTS_KB_TEST_RESOURCE_ROOT pointing at the prepared app resource
// directory containing document-processing/v1 and the shared inference runtime.
// cargo test --manifest-path src-tauri/Cargo.toml --lib resource_backed_document_ingest -- --ignored
#[test]
#[ignore = "requires the prepared native document Worker, PDFium and OCR resource bundle"]
fn resource_backed_document_ingest_reaches_ready_with_markdown_and_assets() {
    let resources = std::env::var_os("MYAGENTS_KB_TEST_RESOURCE_ROOT")
        .expect("set MYAGENTS_KB_TEST_RESOURCE_ROOT to prepared native app resources");
    let resources = Path::new(&resources).canonicalize().unwrap();
    let data = tempdir().unwrap();
    let inputs = tempdir().unwrap();
    let input_root = inputs.path().canonicalize().unwrap();
    let docs = DocumentProcessingManager::initialize(
        data.path().into(),
        resources.clone(),
        LocalInferenceRuntimeRegistry::initialize(&resources),
        LocalComputeCoordinator::new(),
    )
    .unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().into()));
    let kb = store
        .create(KnowledgeBasePatch {
            name: Some("Native conversion test".into()),
            ..Default::default()
        })
        .unwrap();
    let manager = Arc::new(KnowledgeIngestManager::new(
        Arc::clone(&store),
        Some(Arc::clone(&docs)),
    ));
    manager.recover().unwrap();

    let fixtures: &[(&str, &[u8], &str)] = &[
        (
            "sample.docx",
            include_bytes!("fixtures/sample.docx"),
            "Office orchard",
        ),
        (
            "text.pdf",
            include_bytes!("fixtures/text.pdf"),
            "Native orchard",
        ),
        (
            "scan.pdf",
            include_bytes!("fixtures/scan.pdf"),
            "SCAN ORCHARD",
        ),
    ];
    let paths = fixtures
        .iter()
        .map(|(name, bytes, _)| {
            let path = input_root.join(name);
            fs::write(&path, bytes).unwrap();
            path
        })
        .collect();
    let outcomes = manager.import_files(&kb.id, paths);
    assert_eq!(outcomes.len(), fixtures.len());
    for ((name, _, expected), outcome) in fixtures.iter().zip(outcomes) {
        let receipt = match outcome {
            ImportOutcome::Accepted { receipt } => receipt,
            ImportOutcome::Rejected { error } => panic!("fixture rejected: {error:?}"),
        };
        let id = receipt.job.unwrap().id;
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            let job = manager.status(&id).unwrap();
            match job.status {
                SourceStatus::Ready => break,
                SourceStatus::Queued | SourceStatus::Converting | SourceStatus::Chunking => {}
                _ => panic!("fixture {name} stopped before ready: {job:?}"),
            }
            assert!(Instant::now() < deadline, "fixture {name} timed out");
            std::thread::sleep(Duration::from_millis(100));
        }
        let source = store.get_source(&kb.id, &receipt.source_id).unwrap();
        assert_eq!(source.facts.status, SourceStatus::Ready);
        let dir = data
            .path()
            .join("knowledge")
            .join(&kb.id)
            .join("sources")
            .join(&receipt.source_id);
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("source.json")).unwrap()).unwrap();
        let worker_id = record["ingest"]["documentJobId"].as_str().unwrap();
        let metrics = docs.get(worker_id).unwrap().metrics.unwrap();
        if name.ends_with(".pdf") {
            assert_eq!(metrics.pages_ocr, Some(u32::from(*name == "scan.pdf")));
        }
        let content = fs::read_to_string(dir.join("content.md")).unwrap();
        assert!(
            content.contains(expected),
            "wrong content in fixture {name}"
        );
        if *name == "sample.docx" {
            let asset = "assets/asset-0001.png";
            assert!(content.contains(asset));
            let mut docx = zip::ZipArchive::new(std::io::Cursor::new(
                include_bytes!("fixtures/sample.docx").as_slice(),
            ))
            .unwrap();
            let mut expected_png = Vec::new();
            std::io::Read::read_to_end(
                &mut docx.by_name("word/media/image1.png").unwrap(),
                &mut expected_png,
            )
            .unwrap();
            assert_eq!(fs::read(dir.join(asset)).unwrap(), expected_png);
        }
    }
    manager.shutdown().unwrap();
    docs.shutdown().unwrap();
}
