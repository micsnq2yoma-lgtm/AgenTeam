use super::*;
use crate::knowledge::types::KnowledgeMetadataOrigin;
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn library(store: &KnowledgeStore) -> String {
    store
        .create(KnowledgeBasePatch {
            name: Some("Import".into()),
            ..Default::default()
        })
        .unwrap()
        .id
}

#[test]
fn knowledge_ingest_restarts_with_original_and_fences_previous_attempt() {
    let data = tempdir().unwrap();
    let inputs = tempdir().unwrap();
    let original = fs::canonicalize(inputs.path()).unwrap().join("source.md");
    fs::write(&original, b"original\r\n").unwrap();
    let open = || KnowledgeStore::new(data.path().into());
    let store = open();
    let kb = library(&store);
    let receipt = store.enqueue_file(&kb, &original).unwrap();
    let first = receipt.job.unwrap();
    let running = store.begin_ingest(&first.id).unwrap();
    store
        .attach_conversion(&first.id, "20261121_0123456789ab")
        .unwrap();
    assert_eq!(fs::read(&running.original).unwrap(), b"original\r\n");
    fs::write(running.stage.path().join("content.md"), b"partial").unwrap();
    let abandoned = running.stage.keep();
    fs::remove_file(&original).unwrap();
    let reopened = open();
    assert_eq!(
        reopened.interrupt_ingests().unwrap(),
        ["20261121_0123456789ab"]
    );
    assert!(!abandoned.exists());
    assert_eq!(
        reopened.ingest_job(&first.id).unwrap().status,
        SourceStatus::Interrupted
    );
    assert!(!running
        .original
        .parent()
        .unwrap()
        .join("content.md")
        .exists());
    let retry = reopened.retry_ingest(&kb, &receipt.source_id).unwrap();
    assert_ne!(first.id, retry.id);
    assert_eq!(
        reopened.ingest_job(&first.id).unwrap().status,
        SourceStatus::Interrupted
    );
    assert_eq!(reopened.cancel_ingest(&first.id), Err(KbJobConflict));
    let next = reopened.begin_ingest(&retry.id).unwrap();
    assert_eq!(fs::read(&next.original).unwrap(), b"original\r\n");
    fs::write(next.stage.path().join("content.md"), b"normalized\n").unwrap();
    fs::create_dir(next.stage.path().join("assets")).unwrap();
    fs::write(next.stage.path().join("assets/one.png"), b"asset").unwrap();
    reopened
        .set_metadata(
            &kb,
            &receipt.source_id,
            KnowledgeMetadataOrigin::User,
            BTreeMap::from([(MetadataKey::Category, json!("human"))]),
        )
        .unwrap();
    assert_eq!(
        reopened
            .publish_ingest(&retry.id, next.stage.path())
            .unwrap()
            .status,
        SourceStatus::Chunking
    );
    let committed = open();
    assert!(committed.interrupt_ingests().unwrap().is_empty());
    let source = committed.get_source(&kb, &receipt.source_id).unwrap();
    assert_eq!(source.metadata[&MetadataKey::Category].value, "human");
    assert_eq!(source.facts.status, SourceStatus::Chunking);
    assert_eq!(
        source.facts.content_hash,
        Some(format!("{:x}", Sha256::digest(b"normalized\n")))
    );
    let published = next.original.parent().unwrap();
    assert_eq!(
        fs::read(published.join("content.md")).unwrap(),
        b"normalized\n"
    );
    assert_eq!(
        fs::read(published.join("assets/one.png")).unwrap(),
        b"asset"
    );
    assert_eq!(fs::read(next.original).unwrap(), b"original\r\n");
    assert!(serde_json::to_value(&source)
        .unwrap()
        .get("ingest")
        .is_none());
}

#[test]
fn knowledge_ingest_parallel_duplicate_is_unique_while_other_library_is_independent() {
    let data = tempdir().unwrap();
    let inputs = tempdir().unwrap();
    let original = fs::canonicalize(inputs.path()).unwrap().join("source.txt");
    fs::write(&original, b"same content").unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = library(&store);
    let barrier = std::sync::Barrier::new(4);
    let results = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..4 {
            workers.push(scope.spawn(|| {
                barrier.wait();
                KnowledgeStore::new(data.path().into())
                    .enqueue_file(&kb, &original)
                    .unwrap()
            }));
        }
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|r| !r.duplicate).count(), 1);
    assert!(results.iter().all(|r| r.source_id == results[0].source_id));
    assert_eq!(store.list_sources(&kb).unwrap().len(), 1);
    let different = library(&store);
    assert!(!store.enqueue_file(&different, &original).unwrap().duplicate);
    store.remove_source(&kb, &results[0].source_id).unwrap();
    assert!(!store.enqueue_file(&kb, &original).unwrap().duplicate);
    assert_eq!(fs::read(&original).unwrap(), b"same content");
}

#[test]
fn knowledge_ingest_rejects_unsupported_special_and_oversized_sources_before_copy() {
    let data = tempdir().unwrap();
    let inputs = tempdir().unwrap();
    let root = fs::canonicalize(inputs.path()).unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = library(&store);
    let unsupported = root.join("data.unknown");
    fs::write(&unsupported, b"unsupported").unwrap();
    assert_eq!(
        store.enqueue_file(&kb, &unsupported).unwrap_err(),
        KnowledgeErrorCode::KbSourceUnsupported
    );
    let directory = root.join("directory.md");
    fs::create_dir(&directory).unwrap();
    assert_eq!(
        store.enqueue_file(&kb, &directory).unwrap_err(),
        KbInvalidInput
    );
    let large = root.join("large.md");
    fs::File::create(&large)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(
        store.enqueue_file(&kb, &large).unwrap_err(),
        KbLimitExceeded
    );
    #[cfg(unix)]
    {
        let link = root.join("linked.md");
        std::os::unix::fs::symlink(&unsupported, &link).unwrap();
        assert_eq!(store.enqueue_file(&kb, &link).unwrap_err(), KbInvalidInput);
        use std::os::unix::net::UnixListener;
        let socket = root.join("socket.md");
        let _listener = UnixListener::bind(&socket).unwrap();
        assert_eq!(
            store.enqueue_file(&kb, &socket).unwrap_err(),
            KbInvalidInput
        );
    }
    #[cfg(windows)]
    {
        let alias = root.join("alias");
        junction::create(&directory, &alias).unwrap();
        fs::write(directory.join("nested.md"), b"outside").unwrap();
        assert_eq!(
            store
                .enqueue_file(&kb, &alias.join("nested.md"))
                .unwrap_err(),
            KbInvalidInput
        );
    }
    assert!(store.list_sources(&kb).unwrap().is_empty());
}

#[test]
fn knowledge_ingest_cancel_and_bad_assets_do_not_publish_partial_content() {
    let data = tempdir().unwrap();
    let inputs = tempdir().unwrap();
    let original = fs::canonicalize(inputs.path()).unwrap().join("input.md");
    fs::write(&original, b"body").unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = library(&store);
    let id = store.enqueue_file(&kb, &original).unwrap().job.unwrap().id;
    let input = store.begin_ingest(&id).unwrap();
    fs::write(input.stage.path().join("content.md"), b"converted").unwrap();
    fs::create_dir_all(input.stage.path().join("assets/unexpected-directory")).unwrap();
    assert_eq!(
        store.publish_ingest(&id, input.stage.path()),
        Err(KbInvalidInput)
    );
    assert_eq!(
        store.cancel_ingest(&id).unwrap().status,
        SourceStatus::Cancelled
    );
    assert_eq!(
        store.publish_ingest(&id, input.stage.path()),
        Err(KbJobConflict)
    );
    assert!(!input.original.parent().unwrap().join("content.md").exists());
    assert!(!input.original.parent().unwrap().join("assets").exists());
    assert_eq!(fs::read(input.original).unwrap(), b"body");
}

#[test]
fn knowledge_ingest_full_queue_deduplicates_and_reclaims_a_terminal_slot() {
    let data = tempdir().unwrap();
    let inputs = tempdir().unwrap();
    let path = fs::canonicalize(inputs.path()).unwrap().join("input.md");
    let store = KnowledgeStore::new(data.path().into());
    let kb = library(&store);
    let mut jobs = Vec::new();
    for i in 0..64 {
        fs::write(&path, format!("file {i}")).unwrap();
        jobs.push(store.enqueue_file(&kb, &path).unwrap().job.unwrap().id);
    }
    assert!(store.enqueue_file(&kb, &path).unwrap().duplicate);
    fs::write(&path, b"overflow").unwrap();
    assert_eq!(store.enqueue_file(&kb, &path).unwrap_err(), KbLimitExceeded);
    assert_eq!(store.list_sources(&kb).unwrap().len(), 64);
    store
        .fail_ingest(&jobs[0], KbInvalidInput, "retrySource")
        .unwrap();
    let failed = store.ingest_job(&jobs[0]).unwrap();
    assert_eq!(failed.error, Some(KbInvalidInput));
    assert_eq!(failed.suggestion.as_deref(), Some("retrySource"));
    assert!(!store.enqueue_file(&kb, &path).unwrap().duplicate);
}

#[test]
fn knowledge_ingest_enforces_library_source_and_capacity_limits() {
    let data = tempdir().unwrap();
    let parent = data.path();
    assert_eq!(check_space(parent, u64::MAX), Err(KbInsufficientSpace));
    // The limit counts identities, independent of payload size or status.
    for n in 0..10_000u64 {
        fs::File::create(parent.join(create_knowledge_source_id(n.to_be_bytes()))).unwrap();
    }
    assert_eq!(check_source_limit(parent), Err(KbLimitExceeded));
    fs::remove_file(parent.join(create_knowledge_source_id([0; 8]))).unwrap();
    assert!(check_source_limit(parent).is_ok());
}

#[test]
fn knowledge_ingest_recovers_a_directory_swap_interrupted_before_publication() {
    let data = tempdir().unwrap();
    let inputs = tempdir().unwrap();
    let original = fs::canonicalize(inputs.path()).unwrap().join("source.txt");
    fs::write(&original, b"original").unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = library(&store);
    let job = store.enqueue_file(&kb, &original).unwrap().job.unwrap();
    let converting = store.begin_ingest(&job.id).unwrap();
    let live = converting.original.parent().unwrap().to_path_buf();
    drop(converting);
    let parent = live.parent().unwrap();
    let previous = parent.join(format!("{PREVIOUS}{}", job.source_id));
    rename_directory_noreplace(&live, &previous).unwrap();
    let restarted = KnowledgeStore::new(data.path().into());
    restarted.interrupt_ingests().unwrap();
    assert_eq!(
        restarted.ingest_job(&job.id).unwrap().status,
        SourceStatus::Interrupted
    );
    assert!(!previous.exists());
    assert_eq!(fs::read(live.join("original.txt")).unwrap(), b"original");
    assert!(!live.join("content.md").exists());
}
