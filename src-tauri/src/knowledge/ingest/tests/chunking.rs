use super::*;
use crate::i18n::SupportedLocale;
use crate::knowledge::chunker::chunk_markdown;
use crate::knowledge::types::{format_knowledge_ref, KnowledgeRef};

fn publish(store: &KnowledgeStore, id: &str, markdown: &str) {
    let attempt = store.begin_ingest(id).unwrap();
    fs::write(attempt.stage.path().join("content.md"), markdown).unwrap();
    assert_eq!(
        store
            .publish_ingest(id, attempt.stage.path())
            .unwrap()
            .status,
        SourceStatus::Chunking
    );
}

#[test]
fn startup_finishes_legacy_chunking_and_never_marks_corrupt_content_ready() {
    let (data, previous, kb) = library();
    let inputs = tempdir().unwrap();
    let mut jobs = Vec::new();
    for name in ["valid", "changed"] {
        let path = inputs.path().join(format!("{name}.md"));
        fs::write(&path, name).unwrap();
        let receipt = previous.store.enqueue_file(&kb, &path).unwrap();
        let job = receipt.job.unwrap();
        publish(&previous.store, &job.id, name);
        jobs.push(job);
    }
    let store = Arc::clone(&previous.store);
    previous.shutdown().unwrap();
    drop(previous);
    // A K2 shutdown could have cleared the active index after publishing Markdown.
    fs::write(data.path().join("knowledge/active-imports.json"), b"[]").unwrap();
    let locator = data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("jobs")
        .join(format!("{}.json", jobs[0].id));
    fs::remove_file(&locator).unwrap();
    let broken = &jobs[1];
    let dir = data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("sources")
        .join(&broken.source_id);
    fs::write(dir.join("content.md"), "different bytes").unwrap();
    let resumed = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
    resumed.recover().unwrap();
    assert!(locator.is_file());
    wait_for_content(&resumed, &jobs[0].id);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let job = resumed.status(&broken.id).unwrap();
        if job.status == SourceStatus::Failed {
            break;
        }
        assert_eq!(job.status, SourceStatus::Chunking);
        assert!(Instant::now() < deadline, "failed chunking was not settled");
        thread::sleep(Duration::from_millis(10));
    }
    let retry = resumed.retry(&kb, &broken.source_id).unwrap();
    wait_for_content(&resumed, &retry.id);
    assert_eq!(
        fs::read_to_string(dir.join("content.md")).unwrap(),
        "changed"
    );
    resumed.shutdown().unwrap();
}

#[test]
fn legacy_recovery_leaves_a_conflicting_job_locator_untouched() {
    let (data, manager, kb) = library();
    let inputs = tempdir().unwrap();
    let path = inputs.path().join("resume.md");
    fs::write(&path, "published").unwrap();
    let receipt = manager.store.enqueue_file(&kb, &path).unwrap();
    let job = receipt.job.unwrap();
    publish(&manager.store, &job.id, "published");
    manager.shutdown().unwrap();
    drop(manager);
    let root = data.path().join("knowledge");
    fs::write(root.join("active-imports.json"), b"[]").unwrap();
    let locator = root.join(&kb).join("jobs").join(format!("{}.json", job.id));
    let conflict = br#"{"sourceId":"src_0000000000000000","archived":null}"#;
    fs::write(&locator, conflict).unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().into()));
    let reopened = Arc::new(KnowledgeIngestManager::new(store, None));
    assert_eq!(reopened.recover(), Err(KnowledgeErrorCode::KbStoreCorrupt));
    assert_eq!(fs::read(locator).unwrap(), conflict);
}

fn reference(kb_id: &str, source_id: &str, chunk_id: &str) -> String {
    format_knowledge_ref(&KnowledgeRef {
        kb_id: kb_id.into(),
        source_id: source_id.into(),
        chunk_id: Some(chunk_id.into()),
        page: None,
    })
    .unwrap()
}

#[test]
fn ipc_opens_the_first_table_header_chunk_and_reports_an_outdated_fragment() {
    let (_data, manager, kb) = library();
    let inputs = tempdir().unwrap();
    let path = inputs.path().join("table.md");
    let markdown = format!(
        "| {} | second |\n| --- | --- |\n| {} | preserved |\n",
        "梨".repeat(1200),
        "树".repeat(1800)
    );
    fs::write(&path, &markdown).unwrap();
    let receipt = manager.admit(&kb, &path).unwrap();
    let job = receipt.job.unwrap();
    wait_for_content(&manager, &job.id);
    let chunks = chunk_markdown(
        &kb,
        &receipt.source_id,
        "table.md",
        &markdown,
        SupportedLocale::ZhCn,
    )
    .unwrap();
    let first = &chunks[0].chunk_id;
    let valid = reference(&kb, &receipt.source_id, first);
    assert!(chunks
        .iter()
        .skip(1)
        .any(|chunk| chunk.text.contains(&valid)));

    let app = tauri::test::mock_builder()
        .manage(Arc::clone(&manager.store))
        .invoke_handler(tauri::generate_handler![
            crate::knowledge::commands::cmd_kb_resolve_ref
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let resolve = |value: &str| {
        tauri::test::get_ipc_response(
            &Arc::new(window.as_ref().clone()),
            tauri::webview::InvokeRequest {
                cmd: "cmd_kb_resolve_ref".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({"reference": value})),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_owned(),
            },
        )
        .unwrap()
        .deserialize::<serde_json::Value>()
        .unwrap()
    };
    let resolved = resolve(&valid);
    assert_eq!(resolved["fragment"]["status"], "available");
    assert_eq!(resolved["fragment"]["chunk"]["chunkId"], first.as_str());
    assert_eq!(resolved["fragment"]["chunk"]["prefix"], "table.md");

    // Publish two actual managed note editions, then resolve both the old and
    // the unchanged citation against the same persistent source identity.
    use crate::knowledge::store::{SourceFacts, SourceKind};
    use sha2::{Digest, Sha256};
    let facts = |body: &str| SourceFacts {
        kind: SourceKind::Note,
        current_path: None,
        file_fingerprint: None,
        status: SourceStatus::Ready,
        content_hash: Some(format!("{:x}", Sha256::digest(body.as_bytes()))),
    };
    let earlier = "Original wording.\n\nUnchanged paragraph.";
    let later = "Revised wording.\n\nUnchanged paragraph.";
    let note = manager.store.create_source(&kb, facts(earlier)).unwrap();
    let path = _data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("sources")
        .join(&note.id)
        .join("content.md");
    fs::write(&path, earlier).unwrap();
    let earlier_chunks =
        chunk_markdown(&kb, &note.id, &note.id, earlier, SupportedLocale::EnUs).unwrap();
    let expired = reference(&kb, &note.id, &earlier_chunks[0].chunk_id);
    let retained = reference(&kb, &note.id, &earlier_chunks[1].chunk_id);
    assert_eq!(resolve(&expired)["fragment"]["status"], "available");
    fs::write(&path, later).unwrap();
    manager
        .store
        .update_source(&kb, &note.id, facts(later))
        .unwrap();
    let stale = resolve(&expired);
    assert_eq!(stale["source"]["id"], note.id);
    assert_eq!(stale["fragment"]["status"], "unavailable");
    assert_eq!(resolve(&retained)["fragment"]["status"], "available");
    manager.shutdown().unwrap();
}

#[test]
fn filenames_survive_reopening_and_legacy_sources_fall_back_to_their_id() {
    use crate::knowledge::store::ResolvedFragment;
    let (data, manager, kb) = library();
    let directories = [tempdir().unwrap(), tempdir().unwrap()];
    let mut references = Vec::new();
    for (dir, body) in directories.iter().zip(["First edition", "Second edition"]) {
        let path = dir.path().join("report.md");
        fs::write(&path, body).unwrap();
        let receipt = manager.admit(&kb, &path).unwrap();
        wait_for_content(&manager, &receipt.job.unwrap().id);
        let chunks = chunk_markdown(
            &kb,
            &receipt.source_id,
            "report.md",
            body,
            SupportedLocale::EnUs,
        )
        .unwrap();
        let value = reference(&kb, &receipt.source_id, &chunks[0].chunk_id);
        let record = data
            .path()
            .join("knowledge")
            .join(&kb)
            .join("sources")
            .join(&receipt.source_id)
            .join("source.json");
        let json: serde_json::Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        assert_eq!(json["displayName"], "report.md");
        assert!(json.get("currentPath").is_none());
        references.push((receipt.source_id, value, record));
    }
    assert_ne!(references[0].0, references[1].0);
    manager.shutdown().unwrap();
    drop(manager);
    let reopened = KnowledgeStore::new(data.path().into());
    let prefix = |value: &str| match reopened.resolve_ref(value).unwrap().fragment {
        Some(ResolvedFragment::Available { chunk }) => chunk.prefix,
        _ => panic!("expected an available fragment"),
    };
    for (_, value, _) in &references {
        assert_eq!(prefix(value), "report.md");
    }
    // The title is presentation only. A pre-K3 record has the same citation.
    let (source_id, value, path) = &references[0];
    let mut legacy: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("displayName");
    fs::write(path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(&prefix(value), source_id);
}

#[test]
fn an_unsplittable_grapheme_preserves_the_source_and_allows_the_next_job_to_finish() {
    let (data, manager, kb) = library();
    let inputs = tempdir().unwrap();
    let broken = inputs.path().join("cluster.md");
    let good = inputs.path().join("next.md");
    let prefix = "前一行\n\n梨 ";
    let input = format!("{prefix}X{}", "\u{301}".repeat(1100));
    fs::write(&broken, &input).unwrap();
    fs::write(&good, "第二份资料").unwrap();
    let outcomes = manager.import_files(&kb, vec![broken, good]);
    let mut jobs = outcomes.into_iter().map(|outcome| match outcome {
        ImportOutcome::Accepted { receipt } => receipt.job.unwrap(),
        ImportOutcome::Rejected { error } => panic!("unexpected admission failure: {error:?}"),
    });
    let first = jobs.next().unwrap();
    let second = jobs.next().unwrap();
    wait_for_content(&manager, &second.id);
    let failed = manager.status(&first.id).unwrap();
    assert_eq!(failed.status, SourceStatus::Failed);
    assert_eq!(failed.error, Some(KnowledgeErrorCode::KbChunkUnsplittable));
    assert_eq!(
        failed.suggestion.as_deref(),
        Some("simplifyGraphemeAndRetry")
    );
    let expected = serde_json::json!({"row":3,"column":3,"byteOffset":prefix.len()});
    assert_eq!(serde_json::to_value(&failed).unwrap()["position"], expected);
    let dir = data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("sources")
        .join(&first.source_id);
    for name in ["original.md", "content.md"] {
        assert_eq!(fs::read(dir.join(name)).unwrap(), input.as_bytes());
    }
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("source.json")).unwrap()).unwrap();
    assert_eq!(disk["ingest"]["position"], expected);
    manager.shutdown().unwrap();
    let reopened = KnowledgeStore::new(data.path().into());
    assert_eq!(reopened.ingest_job(&first.id).unwrap(), failed);
    assert_eq!(
        reopened.ingest_job(&second.id).unwrap().status,
        SourceStatus::Ready
    );
}

#[test]
fn a_cancelled_chunking_attempt_cannot_settle_a_later_retry() {
    let (_data, manager, kb) = library();
    let input = tempdir().unwrap();
    let file = input.path().join("cancel.md");
    fs::write(&file, "published").unwrap();
    let receipt = manager.store.enqueue_file(&kb, &file).unwrap();
    let old = receipt.job.unwrap().id;
    publish(&manager.store, &old, "published");
    assert_eq!(
        manager.cancel(&old).unwrap().status,
        SourceStatus::Cancelled
    );
    let next = manager.store.retry_ingest(&kb, &receipt.source_id).unwrap();
    assert_eq!(
        manager.store.finish_chunking(&old, &AtomicBool::new(false)),
        Err(KnowledgeErrorCode::KbJobConflict)
    );
    assert_eq!(
        manager.store.ingest_job(&next.id).unwrap().status,
        SourceStatus::Queued
    );
    publish(&manager.store, &next.id, "published");
    let mut changes = manager.store.subscribe_changes();
    manager
        .store
        .finish_chunking(&next.id, &AtomicBool::new(false))
        .unwrap();
    let source = manager.store.get_source(&kb, &receipt.source_id).unwrap();
    assert_eq!(source.facts.status, SourceStatus::Ready);
    assert_eq!(
        changes.try_recv().unwrap(),
        crate::knowledge::store::KnowledgeChange::SourceUpserted {
            kb_id: kb,
            source_id: receipt.source_id,
            content_hash: source.facts.content_hash
        }
    );
    manager.shutdown().unwrap();
}
