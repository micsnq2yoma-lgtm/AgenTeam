use super::*;
use crate::knowledge::store::{KnowledgeBasePatch, SourceStatus};
use std::{fs, thread, time::Instant};
use tempfile::{tempdir, TempDir};

mod acceptance;
mod chunking;
mod versions;

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

fn wait_for_content(manager: &Arc<KnowledgeIngestManager>, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let job = manager.status(id).unwrap();
        match job.status {
            SourceStatus::Ready => return,
            SourceStatus::Queued | SourceStatus::Converting | SourceStatus::Chunking => {}
            state => panic!("unexpected import status {state:?}"),
        }
        assert!(Instant::now() < deadline, "timed out waiting for {id}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(windows)]
fn hold_writer(path: &Path, exclusive: bool) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    options.write(true);
    if exclusive {
        options.share_mode(0);
    }
    options.open(path).unwrap()
}

#[cfg(windows)]
#[test]
fn source_in_use_rejects_exclusive_and_shared_writers_then_allows_reimport() {
    let (_data, manager, kb) = library();
    let inputs = tempdir().unwrap();
    let path = inputs.path().join("busy.md");
    fs::write(&path, "ready after close").unwrap();
    for exclusive in [true, false] {
        let _writer = hold_writer(&path, exclusive);
        let outcomes = manager.import_files(&kb, vec![path.clone()]);
        let [ImportOutcome::Rejected { error }] = outcomes.as_slice() else {
            panic!("expected the open writer to reject admission");
        };
        assert_eq!(error.code, KnowledgeErrorCode::KbSourceInUse);
        assert_eq!(error.suggestion, "closeFileAndRetry");
        assert!(manager.store.list_sources(&kb).unwrap().is_empty());
    }
    let receipt = manager.admit(&kb, &path).unwrap();
    assert!(!receipt.duplicate);
    wait_for_content(&manager, &receipt.job.unwrap().id);
    manager.shutdown().unwrap();
}

#[cfg(windows)]
#[test]
fn source_in_use_on_managed_text_persists_failure_and_allows_retry_after_close() {
    let (data, manager, kb) = library();
    let inputs = tempdir().unwrap();
    let path = inputs.path().join("busy.txt");
    fs::write(&path, "managed original").unwrap();
    let receipt = manager.store.enqueue_file(&kb, &path).unwrap();
    let source = &receipt.source_id;
    let dir = data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("sources")
        .join(source);
    let original = dir.join("original.txt");
    let mut job = receipt.job.unwrap();
    for exclusive in [true, false] {
        // Exclusive sharing fails the bounded read. A writer sharing reads
        // reaches publication, where the original must be copied again.
        let writer = hold_writer(&original, exclusive);
        manager.run_one(&job.id, &AtomicBool::new(false));
        let failed = manager.status(&job.id).unwrap();
        assert_eq!(failed.status, SourceStatus::Failed);
        assert_eq!(failed.error, Some(KnowledgeErrorCode::KbSourceInUse));
        assert_eq!(failed.suggestion.as_deref(), Some("closeFileAndRetry"));
        assert!(!dir.join("content.md").exists());
        drop(writer);
        if exclusive {
            job = manager.store.retry_ingest(&kb, source).unwrap();
        }
    }
    let retry = manager.retry(&kb, source).unwrap();
    wait_for_content(&manager, &retry.id);
    assert_eq!(
        fs::read_to_string(dir.join("content.md")).unwrap(),
        "managed original"
    );
    manager.shutdown().unwrap();
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
        assert_eq!(src.facts.status, SourceStatus::Ready);
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
        SourceStatus::Ready
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

#[test]
fn shutdown_timeout_interrupts_jobs_before_the_worker_relinquishes_its_stage() {
    let (_data, manager, kb) = library();
    let files = tempdir().unwrap();
    let enqueue = |name: &str| {
        let path = files.path().join(name);
        fs::write(&path, name).unwrap();
        manager.store.enqueue_file(&kb, &path).unwrap().job.unwrap()
    };
    let active = enqueue("active.md");
    let pending = enqueue("queued.md");
    let input = manager.store.begin_ingest(&active.id).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut queue = manager.queue().unwrap();
        queue.active = Some((active.id.clone(), Arc::clone(&cancelled)));
        queue.pending.push_back(pending.id.clone());
    }

    assert_eq!(
        manager.shutdown_with_timeout(Duration::ZERO),
        Err(KnowledgeErrorCode::KbStoreUnavailable)
    );
    assert!(cancelled.load(Ordering::Acquire));
    for job in [&active, &pending] {
        assert_eq!(
            manager.store.ingest_job(&job.id).unwrap().status,
            SourceStatus::Interrupted
        );
    }
    assert!(input.stage.path().is_dir());
    assert_eq!(
        manager.store.publish_ingest(&active.id, input.stage.path()),
        Err(KnowledgeErrorCode::KbJobConflict)
    );
    let abandoned = input.stage.keep();
    manager.queue().unwrap().active = None;
    manager.shutdown().unwrap();
    assert!(!abandoned.exists());
}
