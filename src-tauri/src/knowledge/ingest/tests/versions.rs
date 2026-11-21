use super::*;

#[test]
fn interrupted_retry_preserves_earlier_bytes_after_the_selected_path_changes() {
    let (data, manager, kb) = library();
    let inputs = tempdir().unwrap();
    let selected = inputs.path().canonicalize().unwrap().join("quarterly.txt");
    let first_text = "Version A — original\n";
    let revised_text = "Version B — revision\n";
    fs::write(&selected, first_text).unwrap();
    let first = manager.store.enqueue_file(&kb, &selected).unwrap();
    let first_job = first.job.unwrap();
    let running = manager.store.begin_ingest(&first_job.id).unwrap();
    let stage = running.stage.keep();
    fs::write(stage.join("content.md"), "unfinished").unwrap();
    fs::write(&selected, revised_text).unwrap();
    drop(manager);

    let reopened = Arc::new(KnowledgeIngestManager::new(
        Arc::new(KnowledgeStore::new(data.path().into())),
        None,
    ));
    reopened.recover().unwrap();
    assert_eq!(
        reopened.status(&first_job.id).unwrap().status,
        SourceStatus::Interrupted
    );
    assert!(!stage.exists());

    let retry = reopened.retry(&kb, &first.source_id).unwrap();
    wait_for_content(&reopened, &retry.id);
    let revised = reopened.admit(&kb, &selected).unwrap();
    assert!(!revised.duplicate);
    assert_ne!(revised.source_id, first.source_id);
    wait_for_content(&reopened, &revised.job.unwrap().id);
    let duplicate = reopened.admit(&kb, &selected).unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.source_id, revised.source_id);

    let sources = data.path().join("knowledge").join(&kb).join("sources");
    for (id, text) in [
        (&first.source_id, first_text),
        (&revised.source_id, revised_text),
    ] {
        for name in ["original.txt", "content.md"] {
            assert_eq!(
                fs::read_to_string(sources.join(id).join(name)).unwrap(),
                text
            );
        }
    }
    assert_eq!(fs::read_to_string(&selected).unwrap(), revised_text);
    assert_eq!(
        reopened.status(&first_job.id).unwrap().status,
        SourceStatus::Interrupted
    );
    assert_eq!(reopened.store.list_sources(&kb).unwrap().len(), 2);
    reopened.shutdown().unwrap();
}

#[test]
fn mixed_rejections_preserve_a_published_source_and_a_corrected_input_can_be_imported() {
    use KnowledgeErrorCode::{KbInvalidInput, KbLimitExceeded, KbSourceUnsupported};

    let (data, manager, kb) = library();
    let inputs = tempdir().unwrap();
    let dir = inputs.path().canonicalize().unwrap();
    let good = dir.join("published.md");
    let content = b"# published\n";
    fs::write(&good, content).unwrap();
    let existing = manager.admit(&kb, &good).unwrap();
    wait_for_content(&manager, &existing.job.unwrap().id);
    let source = data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("sources")
        .join(&existing.source_id);
    let source_json = fs::read(source.join("source.json")).unwrap();

    let unsupported = dir.join("unknown.zzz");
    let oversized = dir.join("large.txt");
    let folder = dir.join("folder.txt");
    let rejected_size = 8 * 1024 * 1024 + 1;
    fs::write(&unsupported, b"retained").unwrap();
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("preserved.txt"), b"inside").unwrap();
    fs::File::create(&oversized)
        .unwrap()
        .set_len(rejected_size)
        .unwrap();
    let cases = [
        (&unsupported, Some(KbSourceUnsupported)),
        (&good, None),
        (&folder, Some(KbInvalidInput)),
        (&oversized, Some(KbLimitExceeded)),
    ];
    for _ in 0..2 {
        let paths = cases.iter().map(|(path, _)| path.to_path_buf()).collect();
        let outcomes = manager.import_files(&kb, paths);
        assert_eq!(outcomes.len(), cases.len());
        for (outcome, (_, expected)) in outcomes.iter().zip(&cases) {
            match (outcome, expected) {
                (ImportOutcome::Rejected { error }, Some(code)) => assert_eq!(&error.code, code),
                (ImportOutcome::Accepted { receipt }, None) => {
                    assert!(receipt.duplicate);
                    assert_eq!(receipt.source_id, existing.source_id);
                }
                _ => panic!("unexpected outcome for the mixed import"),
            }
        }
        assert_eq!(manager.store.list_sources(&kb).unwrap().len(), 1);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), source_json);
        assert_eq!(fs::read(source.join("content.md")).unwrap(), content);
        assert_eq!(fs::read(source.join("original.md")).unwrap(), content);
        assert_eq!(fs::read(&unsupported).unwrap(), b"retained");
        assert_eq!(fs::read(folder.join("preserved.txt")).unwrap(), b"inside");
        assert_eq!(fs::metadata(&oversized).unwrap().len(), rejected_size);
    }

    fs::write(&oversized, b"corrected size").unwrap();
    let corrected = manager.admit(&kb, &oversized).unwrap();
    assert!(!corrected.duplicate);
    wait_for_content(&manager, &corrected.job.unwrap().id);
    let duplicate = manager.admit(&kb, &oversized).unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.source_id, corrected.source_id);
    assert_eq!(manager.store.list_sources(&kb).unwrap().len(), 2);
    manager.shutdown().unwrap();
}
