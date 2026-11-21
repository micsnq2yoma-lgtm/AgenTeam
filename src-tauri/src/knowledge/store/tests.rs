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

#[cfg(unix)]
#[test]
fn knowledge_refuses_external_directory_redirects() {
    use std::os::unix::fs::symlink;

    let data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Redirect")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    let root = data.path().join("knowledge").join(&kb.id);
    fs::write(outside.path().join("original.txt"), b"outside").unwrap();
    symlink(outside.path(), root.join("assets-alias")).unwrap();
    let source_dir = root.join("sources").join(&source.id);
    fs::rename(&source_dir, root.join("retained")).unwrap();
    symlink(outside.path(), &source_dir).unwrap();

    assert!(store.remove_source(&kb.id, &source.id).is_err());
    assert_eq!(
        store.remove_source(&kb.id, "../outside"),
        Err(KbInvalidInput)
    );
    store.delete(&kb.id).unwrap();
    assert_eq!(
        fs::read(outside.path().join("original.txt")).unwrap(),
        b"outside"
    );
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
