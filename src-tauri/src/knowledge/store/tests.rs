use std::sync::Barrier;

use serde_json::json;
use tempfile::tempdir;

use super::*;

fn settings(name: &str) -> KnowledgeBasePatch {
    KnowledgeBasePatch {
        name: Some(name.into()),
        ..KnowledgeBasePatch::default()
    }
}

fn note() -> SourceFacts {
    SourceFacts {
        kind: SourceKind::Note,
        current_path: None,
        content_hash: Some("a".repeat(64)),
        file_fingerprint: None,
        status: SourceStatus::Ready,
    }
}

fn field(key: MetadataKey, value: Value) -> MetadataPatch {
    BTreeMap::from([(key, value)])
}

#[test]
fn knowledge_crud_survives_reopen_and_preserves_linked_originals() {
    let data = tempdir().unwrap();
    let originals = tempdir().unwrap();
    let outside = originals.path().join("original.md");
    fs::write(&outside, b"external original").unwrap();

    let store = KnowledgeStore::new(data.path().into());
    assert!(store.list().unwrap().is_empty());
    let kb = store.create(settings("Reference")).unwrap();
    assert_eq!(kb.curation_interval_minutes, 4320);
    assert_eq!(kb.agent_write_access, AgentWriteAccess::CuratorOnly);
    assert_eq!(kb.curation_mode, CurationMode::Manual);
    assert!(kb.wiki_enabled);
    assert!(kb.pending_categories.is_empty());

    let updated = store
        .update(
            &kb.id,
            KnowledgeBasePatch {
                description: Some("updated".into()),
                pending_categories: Some(vec!["review".into()]),
                ..KnowledgeBasePatch::default()
            },
        )
        .unwrap();
    let linked_facts = SourceFacts {
        content_hash: Some(format!(
            "{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(b"managed copy")
        )),
        current_path: Some(outside.clone()),
        kind: SourceKind::LinkedFile,
        ..note()
    };
    let first = store.create_source(&kb.id, linked_facts.clone()).unwrap();
    let second = store.create_source(&kb.id, linked_facts).unwrap();
    let root = data.path().join("knowledge").join(&kb.id);
    let first_dir = root.join("sources").join(&first.id);
    fs::write(first_dir.join("content.md"), b"managed copy").unwrap();
    fs::create_dir(root.join("wiki")).unwrap();
    fs::write(root.join("wiki/index.md"), b"wiki").unwrap();
    drop(store);

    let reopened = KnowledgeStore::new(data.path().into());
    assert_eq!(reopened.list().unwrap(), vec![updated]);
    assert_eq!(reopened.list_sources(&kb.id).unwrap().len(), 2);
    let resolved = reopened
        .resolve_ref(&format!(
            "kb://{}/{}?page=2#abcdef0123456789",
            kb.id, first.id
        ))
        .unwrap();
    assert_eq!(resolved.reference.page, Some(2));
    assert_eq!(resolved.source, first);
    reopened.remove_source(&kb.id, &first.id).unwrap();
    assert_eq!(reopened.list_sources(&kb.id).unwrap(), vec![second]);
    assert!(!first_dir.exists());
    assert_eq!(fs::read(&outside).unwrap(), b"external original");
    reopened.delete(&kb.id).unwrap();
    assert!(!root.exists());
    assert_eq!(fs::read(&outside).unwrap(), b"external original");
    assert!(KnowledgeStore::new(data.path().into())
        .list()
        .unwrap()
        .is_empty());
}

#[test]
fn knowledge_concurrent_read_modify_write_preserves_eight_fields() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Concurrent")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    let start = Barrier::new(8);
    let fields = [
        (MetadataKey::Summary, json!("summary")),
        (MetadataKey::Tags, json!(["tag"])),
        (MetadataKey::Category, json!("category")),
        (MetadataKey::DocType, json!("note")),
        (MetadataKey::Date, json!("2026-11-21")),
        (MetadataKey::Entities, json!(["project"])),
        (MetadataKey::Language, json!("zh")),
        (MetadataKey::DuplicateOf, json!(null)),
    ];

    std::thread::scope(|scope| {
        for (key, value) in &fields {
            let start = &start;
            let kb_id = &kb.id;
            let source_id = &source.id;
            let data = data.path().to_path_buf();
            scope.spawn(move || {
                let independent_store = KnowledgeStore::new(data);
                start.wait();
                let updated = independent_store
                    .set_metadata(
                        kb_id,
                        source_id,
                        KnowledgeMetadataOrigin::Agent,
                        field(*key, value.clone()),
                    )
                    .unwrap();
                assert_eq!(updated.get(key), Some(&MetadataOutcome::Updated));
            });
        }
    });

    let persisted = store.get_source(&kb.id, &source.id).unwrap();
    assert_eq!(persisted.metadata.len(), 8);
    for (key, expected) in fields {
        assert_eq!(persisted.metadata[&key].value, expected);
    }
}

#[cfg(windows)]
#[test]
fn knowledge_store_waits_for_windows_lock_directory_deletion() {
    use crate::utils::file_lock::FileLockError;
    use crate::workspace_files::path_safety::delete_windows_file_by_handle;
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_FLAG_BACKUP_SEMANTICS};

    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    assert!(store.list().unwrap().is_empty());
    let path = data.path().join("knowledge").join(".store.lock");
    fs::create_dir(&path).unwrap();
    let retiring = fs::OpenOptions::new()
        .access_mode(DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(&path)
        .unwrap();
    delete_windows_file_by_handle(&retiring).unwrap();

    // Hold the old directory in STATUS_DELETE_PENDING for the entire attempt.
    let options = FileLockOptions {
        timeout: Duration::from_millis(15),
        ..FileLockOptions::default()
    };
    assert!(matches!(
        with_file_lock_blocking::<_, ()>(&path, options, || panic!("entered before release")),
        Err(FileLockError::Busy { .. })
    ));

    std::thread::scope(|scope| {
        scope.spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            drop(retiring);
        });
        let kb = store.create(settings("After handoff")).unwrap();
        assert_eq!(store.list().unwrap()[0].id, kb.id);
    });
}

#[test]
fn knowledge_failed_atomic_replacement_keeps_previous_bytes() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Committed")).unwrap();
    let path = data.path().join("knowledge").join(&kb.id);
    let target = path.join("kb.json");
    let before = fs::read(&target).unwrap();

    // Reproduce interrupted writes at the shared no-follow writer's staging
    // names. Exhausting them also exercises a real pre-publication I/O failure.
    let staging_paths: Vec<_> = (0..100)
        .map(|nonce| {
            path.join(format!(
                ".kb.json.myagents-{}-{nonce}.tmp",
                std::process::id()
            ))
        })
        .collect();
    for staging in &staging_paths {
        fs::write(staging, b"{ \"name\": \"partially written").unwrap();
    }

    let reopened = KnowledgeStore::new(data.path().into());
    assert_eq!(reopened.list().unwrap(), vec![kb.clone()]);
    assert_eq!(
        reopened.update(&kb.id, settings("Uncommitted")),
        Err(KbStoreUnavailable)
    );
    assert_eq!(fs::read(&target).unwrap(), before);

    fs::remove_file(&staging_paths[0]).unwrap();
    assert_eq!(
        reopened.update(&kb.id, settings("Retry")).unwrap().name,
        "Retry"
    );
}

#[test]
fn knowledge_user_fields_lock_individually_and_survive_source_updates() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Metadata")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    store
        .set_metadata(
            &kb.id,
            &source.id,
            KnowledgeMetadataOrigin::User,
            field(MetadataKey::Category, json!("human")),
        )
        .unwrap();

    let result = store
        .set_metadata(
            &kb.id,
            &source.id,
            KnowledgeMetadataOrigin::Agent,
            BTreeMap::from([
                (MetadataKey::Category, json!("generated")),
                (MetadataKey::Tags, json!(["accepted"])),
            ]),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({
            "category": { "status": "rejected", "code": "KB_FIELD_LOCKED" },
            "tags": { "status": "updated" },
        })
    );
    let persisted = store
        .update_source(
            &kb.id,
            &source.id,
            SourceFacts {
                content_hash: Some("b".repeat(64)),
                ..note()
            },
        )
        .unwrap();
    assert_eq!(
        persisted.metadata[&MetadataKey::Category].value,
        json!("human")
    );
    assert_eq!(
        persisted.metadata[&MetadataKey::Category].origin,
        KnowledgeMetadataOrigin::User
    );
    assert_eq!(
        persisted.metadata[&MetadataKey::Tags].value,
        json!(["accepted"])
    );
    assert_eq!(
        persisted.metadata[&MetadataKey::Tags].origin,
        KnowledgeMetadataOrigin::Agent
    );
    assert!(chrono::DateTime::parse_from_rfc3339(
        &persisted.metadata[&MetadataKey::Tags].updated_at
    )
    .is_ok());
}

#[test]
fn knowledge_auto_mode_requires_a_curator_on_create_and_update() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let auto_without_curator = || KnowledgeBasePatch {
        curation_mode: Some(CurationMode::Auto),
        ..settings("Auto")
    };
    assert_eq!(store.create(auto_without_curator()), Err(KbInvalidInput));
    let kb = store.create(settings("Manual")).unwrap();
    assert_eq!(
        store.update(&kb.id, auto_without_curator()),
        Err(KbInvalidInput)
    );
    assert_eq!(store.list().unwrap(), vec![kb.clone()]);
    let auto = store
        .update(
            &kb.id,
            serde_json::from_value(json!({
                "curatorAgentId": "curator",
                "curationMode": "auto",
            }))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(auto.curation_mode, CurationMode::Auto);
    assert_eq!(
        store.update(
            &kb.id,
            serde_json::from_value(json!({
                "curatorAgentId": null,
            }))
            .unwrap()
        ),
        Err(KbInvalidInput)
    );
    let off = store
        .update(
            &kb.id,
            serde_json::from_value(json!({
                "curatorAgentId": null,
                "curationMode": "off",
            }))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(off.curator_agent_id, None);
    assert_eq!(off.curation_mode, CurationMode::Off);
}

#[test]
fn knowledge_broadcasts_only_committed_changes() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let reader = KnowledgeStore::new(data.path().into());
    let mut pending = store.subscribe_changes();
    let kb = store.create(settings("Events")).unwrap();
    let invalid = SourceFacts {
        content_hash: Some("invalid".into()),
        ..note()
    };
    assert_eq!(store.create_source(&kb.id, invalid), Err(KbInvalidInput));
    assert!(matches!(
        pending.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    let source = store.create_source(&kb.id, note()).unwrap();
    assert_eq!(
        pending.try_recv().unwrap(),
        KnowledgeChange::SourceUpserted {
            kb_id: kb.id.clone(),
            source_id: source.id.clone(),
            content_hash: Some("a".repeat(64)),
        }
    );
    assert_eq!(reader.get_source(&kb.id, &source.id).unwrap(), source);

    store
        .set_metadata(
            &kb.id,
            &source.id,
            KnowledgeMetadataOrigin::User,
            field(MetadataKey::Category, json!("saved")),
        )
        .unwrap();
    assert_eq!(
        pending.try_recv().unwrap(),
        KnowledgeChange::MetadataChanged {
            kb_id: kb.id.clone(),
            source_id: source.id.clone(),
        }
    );
    assert_eq!(
        reader.get_source(&kb.id, &source.id).unwrap().metadata[&MetadataKey::Category].value,
        json!("saved")
    );
    store
        .set_metadata(
            &kb.id,
            &source.id,
            KnowledgeMetadataOrigin::Agent,
            field(MetadataKey::Category, json!("ignored")),
        )
        .unwrap();
    assert!(matches!(
        pending.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    store.remove_source(&kb.id, &source.id).unwrap();
    assert_eq!(
        pending.try_recv().unwrap(),
        KnowledgeChange::SourceRemoved {
            kb_id: kb.id.clone(),
            source_id: source.id.clone(),
        }
    );
    assert_eq!(reader.get_source(&kb.id, &source.id), Err(KbNotFound));
    store.delete(&kb.id).unwrap();
    assert_eq!(
        pending.try_recv().unwrap(),
        KnowledgeChange::KbRemoved { kb_id: kb.id }
    );
    assert!(reader.list().unwrap().is_empty());
}

#[test]
fn knowledge_corrupt_json_fails_closed_without_rewriting_it() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Library")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    let root = data.path().join("knowledge");
    let source_file = root.join(source_path(&kb.id, &source.id));
    fs::write(&source_file, b"{").unwrap();
    assert_eq!(store.list_sources(&kb.id), Err(KbStoreCorrupt));
    assert_eq!(store.get_source(&kb.id, &source.id), Err(KbStoreCorrupt));
    assert_eq!(fs::read(&source_file).unwrap(), b"{");

    let index = root.join(REGISTRY);
    fs::write(&index, b"[").unwrap();
    let reopened = KnowledgeStore::new(data.path().into());
    assert_eq!(reopened.list(), Err(KbStoreCorrupt));
    assert_eq!(
        reopened.create(settings("Unpublished")),
        Err(KbStoreCorrupt)
    );
    assert_eq!(fs::read(index).unwrap(), b"[");
}

#[test]
fn knowledge_recovers_both_sides_of_the_library_deletion_commit() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Recover")).unwrap();
    let root = data.path().join("knowledge");
    let live = root.join(&kb.id);
    let removed = root.join(format!("{REMOVED}{}", kb.id));
    rename_directory_noreplace(&live, &removed).unwrap();
    assert_eq!(
        KnowledgeStore::new(data.path().into()).list().unwrap(),
        vec![kb.clone()]
    );
    assert!(live.exists());

    rename_directory_noreplace(&live, &removed).unwrap();
    write_workspace_file_no_follow(data.path(), "knowledge/kbs.json", b"[]").unwrap();
    assert!(KnowledgeStore::new(data.path().into())
        .list()
        .unwrap()
        .is_empty());
    assert!(!removed.exists());
}

#[cfg(any(unix, windows))]
fn redirect_directory(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    junction::create(target, link).unwrap();
}

#[cfg(any(unix, windows))]
#[test]
fn knowledge_refuses_a_redirected_source_directory() {
    let data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Redirected source")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    let library = data.path().join("knowledge").join(&kb.id);
    let original = library.join("sources").join(&source.id);
    let target = outside.path().join("source");
    fs::rename(&original, &target).unwrap();
    let original_json = fs::read(target.join("source.json")).unwrap();
    fs::write(target.join("original.txt"), b"external source").unwrap();
    redirect_directory(&target, &original);

    assert!(store.get_source(&kb.id, &source.id).is_err());
    assert!(store.list_sources(&kb.id).is_err());
    assert!(store.remove_source(&kb.id, &source.id).is_err());
    redirect_directory(&target, &library.join("assets-alias"));
    store.delete(&kb.id).unwrap();
    assert_eq!(fs::read(target.join("source.json")).unwrap(), original_json);
    assert_eq!(
        fs::read(target.join("original.txt")).unwrap(),
        b"external source"
    );
}

#[cfg(any(unix, windows))]
#[test]
fn knowledge_refuses_a_redirected_library_directory() {
    let data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Redirected library")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    let original = data.path().join("knowledge").join(&kb.id);
    let target = outside.path().join("library");
    fs::rename(&original, &target).unwrap();
    let original_json = fs::read(target.join("kb.json")).unwrap();
    fs::write(target.join("original.txt"), b"external library").unwrap();
    redirect_directory(&target, &original);

    assert!(store.list().is_err());
    assert!(store.get_source(&kb.id, &source.id).is_err());
    assert!(store.delete(&kb.id).is_err());
    assert_eq!(fs::read(target.join("kb.json")).unwrap(), original_json);
    assert_eq!(
        fs::read(target.join("original.txt")).unwrap(),
        b"external library"
    );
}

#[cfg(any(unix, windows))]
#[test]
fn knowledge_library_deletion_preserves_a_nested_redirect_target() {
    let data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Nested redirect")).unwrap();
    let entry = store.create_source(&kb.id, note()).unwrap();
    let root = data.path().join("knowledge");
    let managed = root.join(&kb.id).join("sources").join(entry.id);
    let original = outside.path().join("original.txt");
    fs::write(&original, b"external original").unwrap();
    redirect_directory(outside.path(), &managed.join("assets"));

    store.delete(&kb.id).unwrap();
    assert!(store.list().unwrap().is_empty());
    assert!(!root.join(&kb.id).exists());
    assert!(!root.join(format!("{REMOVED}{}", kb.id)).exists());
    assert_eq!(fs::read(original).unwrap(), b"external original");
}

#[test]
fn knowledge_unregistered_library_remains_invisible_after_reopen() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let registered = store.create(settings("Registered")).unwrap();
    let mut orphan_id = create_knowledge_base_id([0; 6]);
    if registered.id == orphan_id {
        orphan_id = create_knowledge_base_id([1; 6]);
    }
    let orphan = KnowledgeBase::from_patch(orphan_id.clone(), settings("Unpublished")).unwrap();
    let path = data.path().join("knowledge").join(&orphan_id);
    fs::create_dir(&path).unwrap();
    let bytes = serde_json::to_vec(&orphan).unwrap();
    fs::write(path.join("kb.json"), &bytes).unwrap();
    drop(store);

    let reopened = KnowledgeStore::new(data.path().into());
    assert_eq!(reopened.list().unwrap(), vec![registered]);
    assert_eq!(reopened.list_sources(&orphan_id), Err(KbNotFound));
    assert_eq!(reopened.delete(&orphan_id), Err(KbNotFound));
    assert_eq!(fs::read(path.join("kb.json")).unwrap(), bytes);
    fs::write(path.join("kb.json"), br#"{"partial":"#).unwrap();
    let next = reopened.create(settings("After crash")).unwrap();
    assert_ne!(next.id, orphan_id);
    assert!(reopened.list().unwrap().contains(&next));
    assert_eq!(
        reopened.get_source(&orphan_id, &create_knowledge_source_id([1; 8])),
        Err(KbNotFound)
    );
}

#[test]
fn knowledge_invalid_ids_are_rejected_before_opening_the_store() {
    let parent = tempdir().unwrap();
    let data = parent.path().join("unopened");
    let store = KnowledgeStore::new(data.clone());
    let kb = create_knowledge_base_id([0; 6]);
    let source = create_knowledge_source_id([0; 8]);
    for invalid in [
        "..",
        "../kb_000000000000",
        "kb_000000000000/..",
        "KB_000000000000",
        "kb_00000000000g",
    ] {
        assert_eq!(store.list_sources(invalid), Err(KbInvalidInput));
        assert_eq!(store.delete(invalid), Err(KbInvalidInput));
        assert_eq!(
            store.update(invalid, settings("Invalid")),
            Err(KbInvalidInput)
        );
        assert_eq!(store.create_source(invalid, note()), Err(KbInvalidInput));
        for (kb_id, source_id) in [(invalid, source.as_str()), (kb.as_str(), invalid)] {
            assert_eq!(store.get_source(kb_id, source_id), Err(KbInvalidInput));
            assert_eq!(
                store.update_source(kb_id, source_id, note()),
                Err(KbInvalidInput)
            );
            assert_eq!(store.remove_source(kb_id, source_id), Err(KbInvalidInput));
            assert_eq!(
                store.set_metadata(
                    kb_id,
                    source_id,
                    KnowledgeMetadataOrigin::User,
                    MetadataPatch::new(),
                ),
                Err(KbInvalidInput)
            );
        }
        assert!(!data.exists());
    }
    assert_eq!(
        store.resolve_ref("kb://kb_000000000000/src_0000000000000000"),
        Err(KbNotFound)
    );
}

#[test]
fn knowledge_rejects_path_traversal_in_library_and_source_ids() {
    let data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Valid")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    let original = outside.path().join("keep.txt");
    fs::write(&original, b"untouched").unwrap();

    let outside_id = outside.path().to_str().unwrap();
    for invalid in [
        "../keep",
        r"..\keep",
        "kb_../keep",
        "src_../keep",
        outside_id,
    ] {
        assert_eq!(store.list_sources(invalid), Err(KbInvalidInput));
        assert_eq!(
            store.update(invalid, settings("Rejected")),
            Err(KbInvalidInput)
        );
        assert_eq!(store.delete(invalid), Err(KbInvalidInput));
        assert_eq!(store.get_source(&kb.id, invalid), Err(KbInvalidInput));
        assert_eq!(store.remove_source(&kb.id, invalid), Err(KbInvalidInput));
    }

    assert_eq!(store.list().unwrap(), vec![kb.clone()]);
    assert_eq!(store.list_sources(&kb.id).unwrap(), vec![source]);
    assert_eq!(fs::read(original).unwrap(), b"untouched");
}

#[test]
fn knowledge_missing_registry_with_live_data_fails_closed() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Registered")).unwrap();
    let root = data.path().join("knowledge");
    let registry = root.join(REGISTRY);
    fs::remove_file(&registry).unwrap();

    assert_eq!(
        KnowledgeStore::new(data.path().into()).list(),
        Err(KbStoreCorrupt)
    );
    assert!(root.join(&kb.id).join("kb.json").is_file());
    assert!(!registry.exists());
}

#[test]
fn knowledge_unpublished_sources_are_invisible() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Library")).unwrap();
    let id = create_knowledge_source_id([0; 8]);
    let folder = data.path().join("knowledge").join(&kb.id).join("sources");
    let staged = folder.join(format!("{STAGING}{id}"));
    fs::create_dir_all(&staged).unwrap();
    fs::write(staged.join("source.json"), b"{").unwrap();

    assert_eq!(store.get_source(&kb.id, &id), Err(KbNotFound));
    assert!(store.list_sources(&kb.id).unwrap().is_empty());
    assert!(!staged.exists());

    fs::create_dir(folder.join(&id)).unwrap();
    assert_eq!(store.get_source(&kb.id, &id), Err(KbStoreCorrupt));
}

mod acceptance;
