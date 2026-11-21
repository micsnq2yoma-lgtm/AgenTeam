use super::*;
use crate::knowledge::store::{KnowledgeBasePatch, SourceStatus};
use std::{fs, thread, time::Instant};
use tempfile::{tempdir, TempDir};

mod acceptance;

fn library() -> (TempDir, Arc<KnowledgeIngestManager>, String) {
    let data = tempdir().unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().into()));
    let kb = store
        .create(KnowledgeBasePatch {
            name: Some("Offline import".into()),
            ..Default::default()
        })
        .unwrap();
    let manager = Arc::new(KnowledgeIngestManager::new(store, None));
    manager.recover().unwrap();
    (data, manager, kb.id)
}

fn wait_for_content(manager: &KnowledgeIngestManager, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let job = manager.status(id).unwrap();
        match job.status {
            SourceStatus::Chunking => return,
            SourceStatus::Queued | SourceStatus::Converting => {}
            state => panic!("unexpected import status {state:?}"),
        }
        assert!(Instant::now() < deadline, "timed out waiting for {id}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn mixed_import_returns_each_outcome_and_publishes_content_before_chunking() {
    let (data, manager, kb) = library();
    let files = tempdir().unwrap();
    let md = files.path().join("example.md");
    let txt = files.path().join("example.txt");
    let unsupported = files.path().join("example.invalid");
    fs::write(&md, "\u{feff}# Agenda\n").unwrap();
    fs::write(&txt, "会议记录\n").unwrap();
    fs::write(&unsupported, "other").unwrap();
    let outcomes = manager.import_files(&kb, vec![md.clone(), unsupported, txt]);
    let [ImportOutcome::Accepted { receipt: first }, ImportOutcome::Rejected { error }, ImportOutcome::Accepted { receipt: second }] =
        outcomes.as_slice()
    else {
        panic!("expected results in input order")
    };
    assert_eq!(error.code, KnowledgeErrorCode::KbSourceUnsupported);
    assert!(!error.suggestion.is_empty());
    for (receipt, expected) in [(first, "# Agenda\n"), (second, "会议记录\n")] {
        assert!(!receipt.duplicate);
        wait_for_content(&manager, &receipt.job.as_ref().unwrap().id);
        let src = manager.store.get_source(&kb, &receipt.source_id).unwrap();
        assert_eq!(src.facts.status, SourceStatus::Chunking);
        let content = data
            .path()
            .join("knowledge")
            .join(&kb)
            .join("sources")
            .join(&receipt.source_id)
            .join("content.md");
        assert_eq!(fs::read_to_string(content).unwrap(), expected);
    }
    let duplicate = manager.admit(&kb, &md).unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.source_id, first.source_id);
    manager.shutdown().unwrap();
}

#[test]
fn restart_interrupts_the_old_attempt_and_retry_uses_the_managed_original() {
    let (_data, first, kb) = library();
    let input = tempdir().unwrap();
    let path = input.path().join("resume.md");
    fs::write(&path, "retained for retry").unwrap();
    let receipt = first.store.enqueue_file(&kb, &path).unwrap();
    let old = receipt.job.unwrap();
    drop(first.store.begin_ingest(&old.id).unwrap());
    fs::remove_file(&path).unwrap();
    let reopened = Arc::new(KnowledgeIngestManager::new(Arc::clone(&first.store), None));
    reopened.recover().unwrap();
    assert_eq!(
        reopened.status(&old.id).unwrap().status,
        SourceStatus::Interrupted
    );
    let new = reopened.retry(&kb, &receipt.source_id).unwrap();
    assert_ne!(new.id, old.id);
    wait_for_content(&reopened, &new.id);
    assert_eq!(
        reopened
            .store
            .get_source(&kb, &receipt.source_id)
            .unwrap()
            .facts
            .status,
        SourceStatus::Chunking
    );
    reopened.shutdown().unwrap();
}

#[test]
fn cancelled_conversion_cannot_publish_its_staged_content() {
    let (data, manager, kb) = library();
    let input = tempdir().unwrap();
    let path = input.path().join("stop.txt");
    fs::write(&path, "original").unwrap();
    let receipt = manager.store.enqueue_file(&kb, &path).unwrap();
    let id = receipt.job.unwrap().id;
    let converting = manager.store.begin_ingest(&id).unwrap();
    fs::write(converting.stage.path().join("content.md"), "unpublished").unwrap();
    assert_eq!(manager.cancel(&id).unwrap().status, SourceStatus::Cancelled);
    assert!(manager
        .store
        .publish_ingest(&id, converting.stage.path())
        .is_err());
    let source = data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("sources")
        .join(&receipt.source_id);
    assert!(!source.join("content.md").exists());
    assert_eq!(
        fs::read_to_string(source.join("original.txt")).unwrap(),
        "original"
    );
    manager.shutdown().unwrap();
}

#[test]
fn shutdown_interrupts_pending_and_converting_jobs_and_closes_admission() {
    let (_data, manager, kb) = library();
    let input = tempdir().unwrap();
    let first = input.path().join("active.md");
    let second = input.path().join("waiting.md");
    fs::write(&first, "in progress").unwrap();
    fs::write(&second, "waiting").unwrap();
    let active = manager
        .store
        .enqueue_file(&kb, &first)
        .unwrap()
        .job
        .unwrap();
    let queued = manager
        .store
        .enqueue_file(&kb, &second)
        .unwrap()
        .job
        .unwrap();
    let _conversion = manager.store.begin_ingest(&active.id).unwrap();
    manager.shutdown().unwrap();
    for id in [&active.id, &queued.id] {
        assert_eq!(
            manager.store.ingest_job(id).unwrap().status,
            SourceStatus::Interrupted
        );
    }
    assert!(manager.admit(&kb, &second).is_err());
}
