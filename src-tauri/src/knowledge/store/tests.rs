use std::sync::Barrier;

use serde_json::json;
use tempfile::tempdir;

use super::curation::{
    allowed_categories, valid_category, with_category, CategoryProposal, MAX_PENDING_CATEGORIES,
};
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
        // Agents may only use categories the library's rules approve.
        (
            MetadataKey::Category,
            json!(allowed_categories(LIBRARY_TEMPLATES[0].1)[0]),
        ),
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

#[test]
fn new_libraries_publish_with_the_curator_rule_templates() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Templates")).unwrap();
    let root = data.path().join("knowledge").join(&kb.id);
    for (name, template) in LIBRARY_TEMPLATES {
        assert_eq!(fs::read_to_string(root.join(name)).unwrap(), template);
    }
    assert!(LIBRARY_TEMPLATES[0].1.contains("## 分类体系"));
    assert!(LIBRARY_TEMPLATES[1].1.contains("kb://"));

    // The rules belong to the user once created: later updates and a reopened
    // Store never restore the template over an edit.
    fs::write(root.join("ORGANIZE_RULES.md"), "edited").unwrap();
    store
        .update(
            &kb.id,
            KnowledgeBasePatch {
                description: Some("after edit".into()),
                ..KnowledgeBasePatch::default()
            },
        )
        .unwrap();
    drop(store);
    let reopened = KnowledgeStore::new(data.path().into());
    assert_eq!(reopened.list().unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(root.join("ORGANIZE_RULES.md")).unwrap(),
        "edited"
    );
}

#[test]
fn library_template_writes_refuse_a_redirected_library_directory() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    store.create(settings("Seed")).unwrap();
    let outside = tempdir().unwrap();
    let files = StoreFiles {
        app: data.path().into(),
        root: data.path().join("knowledge"),
    };
    let staged = "kb_00000000abcd";
    redirect_directory(outside.path(), &files.root.join(staged));
    assert!(files
        .write_text(&format!("{staged}/ORGANIZE_RULES.md"), "x")
        .is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

fn agent_category(
    store: &KnowledgeStore,
    kb: &str,
    source: &str,
    value: Value,
) -> BTreeMap<MetadataKey, MetadataOutcome> {
    store
        .set_agent_metadata(kb, source, field(MetadataKey::Category, value), |_| {
            Ok(((), true))
        })
        .unwrap()
}

#[test]
fn category_parser_reads_only_the_category_section_outside_code() {
    let rules = "# Rules\n- `ignored`：before the section\n\n## 分类体系\n\n- `客户/A公司`：合同\n- `参考资料`\n- `bad//path`：empty segment\n- `../up`：traversal\n- 没有反引号：not a category\n```\n- `fenced`：inside code\n```\n- `参考资料`：duplicate\n\n## 标签\n- `tag-only`：other section\n";
    assert_eq!(allowed_categories(rules), ["客户/A公司", "参考资料"]);
    assert!(allowed_categories("").is_empty());
    let fenced = "## 分类体系\n\x60\x60\x60\x60md\n\x60\x60\x60\n- \x60inside\x60：example\n\x60\x60\x60\x60\n    - \x60indented\x60：example\n- \x60colon\x60: wrong delimiter\n- \x60outside\x60：real\n# New chapter\n- \x60other\x60：elsewhere";
    assert_eq!(allowed_categories(fenced), ["outside"]);
    for bad in [
        "", "/a", "a/", "a//b", " a", "a/ b", "a/../b", "a/./b", "a`b", "a\nb",
    ] {
        assert!(!valid_category(bad), "{bad:?}");
    }
    assert!(valid_category("客户/A公司/合同"));
}

#[test]
fn approving_a_category_edits_only_the_category_section() {
    let template = LIBRARY_TEMPLATES[0].1;
    let updated = with_category(template, "客户/A公司");
    assert!(allowed_categories(&updated).contains(&"客户/A公司".to_string()));
    assert_eq!(
        allowed_categories(&updated).len(),
        allowed_categories(template).len() + 1
    );
    // Everything outside the one inserted bullet is byte-for-byte unchanged.
    let removed: Vec<&str> = updated
        .lines()
        .filter(|line| !template.lines().any(|old| old == *line))
        .collect();
    assert_eq!(removed, ["- `客户/A公司`：用户确认新增"]);
    assert_eq!(
        with_category(&updated, "客户/A公司")
            .matches("客户/A公司")
            .count(),
        2
    );
    // A file without the section gets one appended.
    let bare = with_category("# My rules", "新类");
    assert_eq!(allowed_categories(&bare), ["新类"]);
}

#[test]
fn an_agent_may_only_use_approved_categories_until_the_user_approves_more() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Rules")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();

    let approved = allowed_categories(LIBRARY_TEMPLATES[0].1)[0].clone();
    assert_eq!(
        agent_category(&store, &kb.id, &source.id, json!(approved))[&MetadataKey::Category],
        MetadataOutcome::Updated
    );
    for rejected in [json!("全新分类"), json!(7), json!(null)] {
        assert_eq!(
            agent_category(&store, &kb.id, &source.id, rejected)[&MetadataKey::Category],
            MetadataOutcome::Rejected {
                code: KnowledgeErrorCode::KbCategoryNotAllowed
            }
        );
    }

    // Proposing never approves; the user does.
    assert_eq!(
        store
            .propose_category(&kb.id, "全新分类", "Fixture reason", |_| Ok(()))
            .unwrap(),
        CategoryProposal::Proposed
    );
    assert_eq!(
        store
            .propose_category(&kb.id, "全新分类", "Fixture reason", |_| Ok(()))
            .unwrap(),
        CategoryProposal::AlreadyPending
    );
    assert_eq!(
        store
            .propose_category(&kb.id, &approved, "Fixture reason", |_| Ok(()))
            .unwrap(),
        CategoryProposal::Allowed
    );
    assert_eq!(store.list().unwrap()[0].pending_categories, ["全新分类"]);
    assert_eq!(
        agent_category(&store, &kb.id, &source.id, json!("全新分类"))[&MetadataKey::Category],
        MetadataOutcome::Rejected {
            code: KnowledgeErrorCode::KbCategoryNotAllowed
        }
    );

    store.confirm_category(&kb.id, "全新分类").unwrap();
    assert!(store.list().unwrap()[0].pending_categories.is_empty());
    assert_eq!(
        agent_category(&store, &kb.id, &source.id, json!("全新分类"))[&MetadataKey::Category],
        MetadataOutcome::Updated
    );
    // The user's own writes are not limited to the list.
    let user = store
        .set_metadata(
            &kb.id,
            &source.id,
            KnowledgeMetadataOrigin::User,
            field(MetadataKey::Category, json!("私人分类")),
        )
        .unwrap();
    assert_eq!(user[&MetadataKey::Category], MetadataOutcome::Updated);
}

#[test]
fn category_proposals_validate_authorize_and_can_be_dismissed() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Proposals")).unwrap();
    assert_eq!(
        store.propose_category(&kb.id, "../x", "Fixture reason", |_| Ok(())),
        Err(KnowledgeErrorCode::KbInvalidInput)
    );
    assert_eq!(
        store.propose_category(&kb.id, "ok", "Fixture reason", |_| Err::<(), _>(
            KnowledgeErrorCode::KbAccessDenied
        )),
        Err(KnowledgeErrorCode::KbAccessDenied)
    );
    assert!(store.list().unwrap()[0].pending_categories.is_empty());
    assert_eq!(
        store.confirm_category(&kb.id, "never-proposed"),
        Err(KnowledgeErrorCode::KbNotFound)
    );
    store
        .propose_category(&kb.id, "ok", "Fixture reason", |_| Ok(()))
        .unwrap();
    store.dismiss_category(&kb.id, "ok").unwrap();
    assert_eq!(
        store.dismiss_category(&kb.id, "ok"),
        Err(KnowledgeErrorCode::KbNotFound)
    );
    for index in 0..MAX_PENDING_CATEGORIES {
        store
            .propose_category(&kb.id, &format!("c{index}"), "Fixture reason", |_| Ok(()))
            .unwrap();
    }
    assert_eq!(
        store.propose_category(&kb.id, "one-more", "Fixture reason", |_| Ok(())),
        Err(KnowledgeErrorCode::KbLimitExceeded)
    );
}

#[test]
fn approving_a_category_restores_missing_rules_and_is_repeatable_after_a_crash() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Old library")).unwrap();
    let rules = data
        .path()
        .join("knowledge")
        .join(&kb.id)
        .join("ORGANIZE_RULES.md");
    fs::remove_file(&rules).unwrap();
    // No rules file: an agent has no approved category at all.
    let source = store.create_source(&kb.id, note()).unwrap();
    assert_eq!(
        agent_category(&store, &kb.id, &source.id, json!("参考资料"))[&MetadataKey::Category],
        MetadataOutcome::Rejected {
            code: KnowledgeErrorCode::KbCategoryNotAllowed
        }
    );
    store
        .propose_category(&kb.id, "新类", "Fixture reason", |_| Ok(()))
        .unwrap();
    // Interrupted approval: rules already list the category, kb.json still pending.
    fs::write(&rules, with_category(LIBRARY_TEMPLATES[0].1, "新类")).unwrap();
    store.confirm_category(&kb.id, "新类").unwrap();
    assert!(store.list().unwrap()[0].pending_categories.is_empty());
    assert_eq!(
        fs::read_to_string(&rules)
            .unwrap()
            .matches("`新类`")
            .count(),
        1
    );
}

#[test]
fn a_curators_write_settles_the_pending_mark_but_a_rejected_one_does_not() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Queue")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    store.mark_curation_pending(&kb.id, &source.id).unwrap();
    assert_eq!(store.pending_curation(&kb.id).unwrap().0.len(), 1);

    // A rejected category write changes nothing, so the source stays queued.
    agent_category(&store, &kb.id, &source.id, json!("全新分类"));
    assert_eq!(store.pending_curation(&kb.id).unwrap().0.len(), 1);
    let mixed = BTreeMap::from([
        (MetadataKey::Summary, json!("accepted summary")),
        (MetadataKey::Category, json!("全新分类")),
    ]);
    store
        .set_agent_metadata(&kb.id, &source.id, mixed, |_| Ok(((), true)))
        .unwrap();
    assert_eq!(store.pending_curation(&kb.id).unwrap().0.len(), 1);
    // The user's edit is not curation.
    store
        .set_metadata(
            &kb.id,
            &source.id,
            KnowledgeMetadataOrigin::User,
            field(MetadataKey::Tags, json!(["x"])),
        )
        .unwrap();
    assert_eq!(store.pending_curation(&kb.id).unwrap().0.len(), 1);

    store
        .set_agent_metadata(
            &kb.id,
            &source.id,
            field(MetadataKey::Summary, json!("s")),
            |_| Ok(((), true)),
        )
        .unwrap();
    assert!(store.pending_curation(&kb.id).unwrap().0.is_empty());
}

#[test]
fn a_curation_round_is_capped_and_the_context_reports_the_remainder() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Batch")).unwrap();
    for index in 0..CURATION_BATCH_LIMIT + 3 {
        let facts = SourceFacts {
            content_hash: Some(format!("{index:064x}")),
            ..note()
        };
        let source = store.create_source(&kb.id, facts).unwrap();
        store.mark_curation_pending(&kb.id, &source.id).unwrap();
    }
    let (batch, more) = store.pending_curation(&kb.id).unwrap();
    assert_eq!(batch.len(), CURATION_BATCH_LIMIT);
    assert!(more);
    assert!(batch
        .windows(2)
        .all(|pair| pair[0].source_id < pair[1].source_id));
    assert!(batch[0]
        .reference
        .starts_with(&format!("kb://{}/src_", kb.id)));

    // Settling the first batch brings the rest into the next round.
    for source in &batch {
        store
            .set_agent_metadata(
                &kb.id,
                &source.source_id,
                field(MetadataKey::Summary, json!("s")),
                |_| Ok(((), true)),
            )
            .unwrap();
    }
    let (rest, more) = store.pending_curation(&kb.id).unwrap();
    assert_eq!(rest.len(), 3);
    assert!(!more);
}

#[test]
fn the_curation_context_carries_rules_schema_index_and_approved_categories() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Context")).unwrap();
    let context = store.curation_context(&kb.id, |_| Ok(Some(()))).unwrap();
    assert_eq!(context.rules.as_deref(), Some(LIBRARY_TEMPLATES[0].1));
    assert_eq!(context.wiki_schema.as_deref(), Some(LIBRARY_TEMPLATES[1].1));
    assert_eq!(context.wiki_index, None);
    assert_eq!(
        context.categories,
        allowed_categories(LIBRARY_TEMPLATES[0].1)
    );
    assert!(context.pending.is_empty() && !context.more_pending);

    let root = data.path().join("knowledge").join(&kb.id);
    fs::create_dir(root.join("wiki")).unwrap();
    fs::write(root.join("wiki/index.md"), "# index").unwrap();
    assert_eq!(
        store
            .curation_context(&kb.id, |_| Ok(Some(())))
            .unwrap()
            .wiki_index
            .as_deref(),
        Some("# index")
    );
    assert_eq!(
        store.curation_context("kb_000000000000", |_| Ok(Some(()))),
        Err(KnowledgeErrorCode::KbNotFound)
    );

    let source = store.create_source(&kb.id, note()).unwrap();
    store.mark_curation_pending(&kb.id, &source.id).unwrap();
    let ordinary = store
        .curation_context(&kb.id, |_| Ok::<Option<()>, _>(None))
        .unwrap();
    assert!(ordinary.pending.is_empty() && !ordinary.more_pending);
    assert!(ordinary.rules.is_some());
    let curator = store.curation_context(&kb.id, |_| Ok(Some(()))).unwrap();
    assert_eq!(curator.pending[0].source_id, source.id);
    assert_eq!(curator.pending[0].status, SourceStatus::Ready);
}

#[cfg(any(unix, windows))]
#[test]
fn knowledge_wiki_refuses_traversal_and_redirected_ancestors() {
    let data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Wiki")).unwrap();
    for path in [
        "../escape.md",
        "/absolute.md",
        "C:/file.md",
        "topics//gap.md",
        "topics/a.txt",
        "topics/./p.md",
        "topics/CON.md",
    ] {
        assert_eq!(
            store.write_wiki_page(&kb.id, path, "# test", |_| Ok(())),
            Err(KbInvalidInput)
        );
    }
    let wiki = data.path().join("knowledge").join(&kb.id).join("wiki");
    fs::create_dir(&wiki).unwrap();
    let sentinel = outside.path().join("escape.md");
    fs::write(&sentinel, "untouched original").unwrap();
    redirect_directory(outside.path(), &wiki.join("topics"));
    assert_eq!(
        store.write_wiki_page(&kb.id, "topics/escape.md", "# test", |_| Ok(())),
        Err(KbInvalidInput)
    );
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "untouched original");
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    assert!(store.list_sources(&kb.id).unwrap().is_empty());
}

#[test]
fn knowledge_wiki_enforces_page_size_and_named_page_capacity() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Wiki limits")).unwrap();
    let too_large = "a".repeat(MAX_WIKI_PAGE_BYTES as usize + 1);
    assert_eq!(
        store.write_wiki_page(&kb.id, "large.md", &too_large, |_| Ok(())),
        Err(KbLimitExceeded)
    );

    let root = data.path().join("knowledge").join(&kb.id).join("wiki");
    fs::create_dir_all(root.join("topics")).unwrap();
    for index in 0..2000 {
        fs::write(root.join(format!("topics/{index}.md")), "# Prior entry").unwrap();
    }
    assert_eq!(
        store.write_wiki_page(&kb.id, "new.md", "# New entry", |_| Ok(())),
        Err(KbLimitExceeded)
    );
    store
        .write_wiki_page(&kb.id, "topics/1.md", "# Updated entry", |_| Ok(()))
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.join("topics/1.md")).unwrap(),
        "# Updated entry"
    );
    assert!(!root.join("new.md").exists());
}

#[test]
fn interrupted_wiki_projection_recovers_the_committed_source() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Wiki")).unwrap();
    let page = "index.md";
    let text = "# Source\n\nCommitted version";
    let source = store
        .write_wiki_page(&kb.id, page, text, |_| Ok(()))
        .unwrap();
    let lib = data.path().join("knowledge").join(&kb.id);
    fs::write(lib.join("wiki/index.md"), "# Prior version").unwrap();
    let marker = json!({
        "page": page,
        "source_id": source.id,
        "content_hash": source.facts.content_hash,
    });
    fs::write(lib.join(".wiki-publication.json"), marker.to_string()).unwrap();

    let restarted = KnowledgeStore::new(data.path().into());
    restarted.reclaim_ingest_staging().unwrap();
    assert_eq!(fs::read_to_string(lib.join("wiki/index.md")).unwrap(), text);
    assert_eq!(
        fs::read_to_string(lib.join(".wiki-publication.json")).unwrap(),
        "null"
    );
    assert_eq!(restarted.list_sources(&kb.id).unwrap().len(), 1);
}

#[test]
fn knowledge_wiki_saves_only_versioned_reports_and_appends_the_log() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Wiki")).unwrap();
    let mut report = json!({"version": 1, "generatedAt": "2026-11-22T08:00:00Z", "items": [
        {"kind": "orphan", "page": "topics/Example.md", "message": "No inbound link", "refs": []},
    ]});
    assert_eq!(
        store
            .save_wiki_lint_report(&kb.id, &serde_json::to_vec(&report).unwrap(), |_| Ok(()))
            .unwrap(),
        1
    );
    report["items"][0]["page"] = json!("../escape.md");
    assert_eq!(
        store.save_wiki_lint_report(&kb.id, &serde_json::to_vec(&report).unwrap(), |_| Ok(())),
        Err(KbInvalidInput)
    );
    store
        .append_wiki_log(&kb.id, "First change", |_| Ok(()))
        .unwrap();
    store
        .append_wiki_log(&kb.id, "Second change", |_| Ok(()))
        .unwrap();
    let wiki = data.path().join("knowledge").join(&kb.id).join("wiki");
    let log = fs::read_to_string(wiki.join("log.md")).unwrap();
    assert!(log.find("First change").unwrap() < log.find("Second change").unwrap());
    assert_eq!(fs::read_dir(wiki.join("lint")).unwrap().count(), 1);
    report["items"][0]["page"] = json!("index.md");
    report["items"][0]["refs"] = json!(["kb://kb_000000000000/src_0123456789abcdef"]);
    assert_eq!(
        store.save_wiki_lint_report(&kb.id, &serde_json::to_vec(&report).unwrap(), |_| Ok(())),
        Err(KbInvalidInput)
    );
    assert_eq!(
        store.write_wiki_page(&kb.id, "log.md", "overwrite", |_| Ok(())),
        Err(KbInvalidInput)
    );
}

#[test]
fn a_bound_non_curator_writes_metadata_without_settling_the_batch() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Wiki")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    store.mark_curation_pending(&kb.id, &source.id).unwrap();
    store
        .set_agent_metadata(
            &kb.id,
            &source.id,
            field(MetadataKey::Summary, json!("known")),
            |_| Ok(((), false)),
        )
        .unwrap();
    assert_eq!(store.pending_curation(&kb.id).unwrap().0.len(), 1);
}

#[test]
fn resolve_ref_returns_preview_target_excerpt_and_falls_back_when_original_missing() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Citations")).unwrap();

    // 1. Markdown/wiki source with multi-line sections and long text (>300 chars)
    let long_para = "长段落测试内容，用来验证悬停摘要会在大约三百个字符处优雅截断。".repeat(16);
    let md = format!("# 标题\n\n第一段简介。\n\n## 目标章节\n\n{long_para}\n");
    let wiki_source = store
        .write_wiki_page(&kb.id, "topics/Design.md", &md, |_| Ok(()))
        .unwrap();
    let (_, chunks) = store
        .search_document(&kb.id, &wiki_source.id)
        .unwrap()
        .unwrap();
    let target_chunk = chunks
        .iter()
        .find(|c| c.heading_path.iter().any(|h| h == "目标章节"))
        .unwrap();

    let resolved = store
        .resolve_ref(&format!(
            "kb://{}/{}#{}",
            kb.id, wiki_source.id, target_chunk.chunk_id
        ))
        .unwrap();
    let excerpt = resolved.excerpt.expect("expected excerpt");
    assert!(excerpt.chars().count() <= 301);
    assert!(excerpt.ends_with('…'));
    let preview = resolved.preview.expect("expected preview");
    assert!(!preview.is_content_fallback);
    assert!(preview.path.ends_with("content.md"));
    assert_eq!(preview.utf16_range, Some(target_chunk.utf16_range));
    assert_eq!(preview.line_number, Some(7));

    // 2. Managed PDF source: prefers original.pdf when present, falls back to content.md if deleted
    let pdf_md = "## Page 1\n\nCover text.\n\n## Page 2\n\nSecond page clause.\n";
    let pdf_hash = format!(
        "{:x}",
        <sha2::Sha256 as sha2::Digest>::digest(pdf_md.as_bytes())
    );
    let pdf_source = store
        .create_source(
            &kb.id,
            SourceFacts {
                kind: SourceKind::File,
                current_path: None,
                file_fingerprint: Some(SourceFingerprint {
                    size: 16,
                    mtime_ms: 1,
                }),
                content_hash: Some(pdf_hash),
                status: SourceStatus::Ready,
            },
        )
        .unwrap();
    let src_dir = data
        .path()
        .join("knowledge")
        .join(&kb.id)
        .join("sources")
        .join(&pdf_source.id);
    fs::write(src_dir.join("content.md"), pdf_md).unwrap();
    fs::write(src_dir.join("original.pdf"), b"%PDF-1.4\n%%EOF\n").unwrap();
    let source_json_path = src_dir.join("source.json");
    let mut record_val: Value =
        serde_json::from_str(&fs::read_to_string(&source_json_path).unwrap()).unwrap();
    record_val["displayName"] = json!("contract.pdf");
    record_val["ingest"] = json!({
        "id": "job_20261124_0123456789ab",
        "extension": "pdf",
        "originalSha256": "0".repeat(64),
        "documentJobId": null,
        "error": null,
        "suggestion": null
    });
    fs::write(
        &source_json_path,
        serde_json::to_vec_pretty(&record_val).unwrap(),
    )
    .unwrap();

    let resolved_pdf = store
        .resolve_ref(&format!("kb://{}/{}?page=2", kb.id, pdf_source.id))
        .unwrap();
    let pdf_preview = resolved_pdf.preview.expect("expected pdf preview");
    assert!(!pdf_preview.is_content_fallback);
    assert!(pdf_preview.path.ends_with("original.pdf"));
    assert_eq!(pdf_preview.name, "contract.pdf");
    assert_eq!(pdf_preview.page, Some(2));
    assert_eq!(pdf_preview.utf16_range, None);
    assert!(resolved_pdf
        .excerpt
        .as_deref()
        .unwrap_or_default()
        .contains("Second page clause"));

    // Remove original.pdf -> must fall back cleanly to content.md
    fs::remove_file(src_dir.join("original.pdf")).unwrap();
    let resolved_fallback = store
        .resolve_ref(&format!("kb://{}/{}?page=2", kb.id, pdf_source.id))
        .unwrap();
    let fb_preview = resolved_fallback.preview.expect("expected fallback preview");
    assert!(fb_preview.is_content_fallback);
    assert!(fb_preview.path.ends_with("content.md"));
    assert_eq!(fb_preview.page, Some(2));
    assert!(fb_preview.utf16_range.is_some());
    assert_eq!(fb_preview.line_number, Some(7));
}
