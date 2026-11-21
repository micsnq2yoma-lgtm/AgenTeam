use super::*;
use KnowledgeErrorCode::{KbInvalidInput, KbJobConflict, KbLimitExceeded, KbSourceUnsupported};

#[test]
fn two_restarts_preserve_attempt_history_and_only_one_concurrent_retry_wins() {
    let (data, original_manager, kb) = library();
    let inputs = tempdir().unwrap();
    let file = inputs.path().canonicalize().unwrap().join("minutes.md");
    let bytes = "\u{feff}# Minutes\n議事録\n".as_bytes();
    fs::write(&file, bytes).unwrap();
    let receipt = original_manager.store.enqueue_file(&kb, &file).unwrap();
    let source = receipt.source_id;
    let mut attempt = receipt.job.unwrap();
    let open = || {
        Arc::new(KnowledgeIngestManager::new(
            Arc::new(KnowledgeStore::new(data.path().into())),
            None,
        ))
    };
    let mut manager = original_manager;
    let mut history = Vec::new();
    // The selected path can change while the app is closed. Only the managed
    // snapshot belongs to this source and its retries.
    fs::write(&file, "replacement at the user's path").unwrap();
    for cycle in 0..2 {
        let running = manager.store.begin_ingest(&attempt.id).unwrap();
        assert_eq!(running.job.status, SourceStatus::Converting);
        let abandoned = running.stage.keep();
        fs::write(abandoned.join("content.md"), "incomplete conversion").unwrap();
        let published = running.original.parent().unwrap().join("content.md");
        drop(manager);
        manager = open();
        manager.recover().unwrap();
        manager.recover().unwrap();
        assert!(!abandoned.exists());
        assert!(!published.exists());
        assert_eq!(fs::read(running.original).unwrap(), bytes);
        history.push(attempt.id.clone());
        for id in &history {
            assert_eq!(
                manager.status(id).unwrap().status,
                SourceStatus::Interrupted
            );
        }
        if cycle == 0 {
            attempt = manager.store.retry_ingest(&kb, &source).unwrap();
        }
    }

    let barrier = std::sync::Barrier::new(2);
    let attempts = thread::scope(|scope| {
        let retry = || {
            barrier.wait();
            manager.retry(&kb, &source)
        };
        let left = scope.spawn(retry);
        let right = scope.spawn(retry);
        [left.join().unwrap(), right.join().unwrap()]
    });
    assert_eq!(
        attempts
            .iter()
            .filter(|r| **r == Err(KbJobConflict))
            .count(),
        1
    );
    let completed = attempts.into_iter().find_map(Result::ok).unwrap();
    assert!(!history.contains(&completed.id));
    assert_eq!(completed.source_id, source);
    wait_for_content(&manager, &completed.id);
    manager.shutdown().unwrap();
    let verified = open();
    assert_eq!(
        verified.status(&completed.id).unwrap().status,
        SourceStatus::Chunking
    );
    for id in history {
        assert_eq!(verified.cancel(&id), Err(KbJobConflict));
        assert_eq!(
            verified.status(&id).unwrap().status,
            SourceStatus::Interrupted
        );
    }
    let sources = verified.store.list_sources(&kb).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].id, source);
    assert_eq!(sources[0].facts.status, SourceStatus::Chunking);
    let dir = data
        .path()
        .join("knowledge")
        .join(kb)
        .join("sources")
        .join(source);
    assert_eq!(fs::read(dir.join("original.md")).unwrap(), bytes);
    assert_eq!(
        fs::read_to_string(dir.join("content.md")).unwrap(),
        "# Minutes\n議事録\n"
    );
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "replacement at the user's path"
    );
    verified.shutdown().unwrap();
}

#[test]
fn rejected_batch_creates_no_source_and_preserves_every_input() {
    let (_data, manager, kb) = library();
    let inputs = tempdir().unwrap();
    let root = inputs.path().canonicalize().unwrap();
    let folder = root.join("folder.txt");
    fs::create_dir(&folder).unwrap();
    let note = folder.join("keeper.md");
    fs::write(&note, "original contents").unwrap();
    let unsupported = root.join("file.unsupported");
    fs::write(&unsupported, "unsupported contents").unwrap();
    let mut cases = vec![
        (PathBuf::from("relative.md"), KbInvalidInput),
        (root.join("missing.txt"), KbInvalidInput),
        (folder.clone(), KbInvalidInput),
        (unsupported.clone(), KbSourceUnsupported),
    ];
    let oversized = [
        ("too-large.Markdown", 8 * 1024 * 1024 + 1),
        (
            "too-large.PDF",
            crate::document_processing::MAX_SOURCE_BYTES + 1,
        ),
    ];
    for (name, size) in oversized {
        let path = root.join(name);
        fs::File::create(&path).unwrap().set_len(size).unwrap();
        cases.push((path, KbLimitExceeded));
    }
    #[cfg(unix)]
    let _socket = {
        use std::os::unix::{fs::symlink, net::UnixListener};
        let alias = root.join("alias");
        symlink(&folder, &alias).unwrap();
        let leaf = root.join("leaf.txt");
        symlink(&note, &leaf).unwrap();
        let dangling = root.join("dangling.md");
        symlink(root.join("absent.md"), &dangling).unwrap();
        let socket = root.join("special.txt");
        let listener = UnixListener::bind(&socket).unwrap();
        // The kernel character device has no supported extension. The socket
        // exercises the regular-file check with an otherwise accepted suffix.
        for path in [alias.join("keeper.md"), leaf, dangling, socket] {
            cases.push((path, KbInvalidInput));
        }
        cases.push((PathBuf::from("/dev/null"), KbSourceUnsupported));
        listener
    };
    #[cfg(windows)]
    {
        let alias = root.join("alias");
        junction::create(&folder, &alias).unwrap();
        cases.push((alias.join("keeper.md"), KbInvalidInput));
    }
    for _ in 0..2 {
        let paths = cases.iter().map(|(path, _)| path.clone()).collect();
        let results = manager.import_files(&kb, paths);
        assert_eq!(results.len(), cases.len());
        for (outcome, (_, expected)) in results.iter().zip(&cases) {
            let ImportOutcome::Rejected { error } = outcome else {
                panic!("invalid fixture was admitted")
            };
            assert_eq!(&error.code, expected);
            assert!(!error.suggestion.is_empty());
        }
        assert!(manager.store.list_sources(&kb).unwrap().is_empty());
    }
    for (name, size) in oversized {
        assert_eq!(fs::metadata(root.join(name)).unwrap().len(), size);
    }
    assert_eq!(fs::read_to_string(note).unwrap(), "original contents");
    assert_eq!(
        fs::read_to_string(unsupported).unwrap(),
        "unsupported contents"
    );
    manager.shutdown().unwrap();
}
