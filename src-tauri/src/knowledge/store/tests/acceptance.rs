use super::*;

#[test]
fn knowledge_acceptance_disjoint_settings_commute_across_store_instances() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Before")).unwrap();
    let patches = [
        ("name", json!("After")),
        ("description", json!("Independent description")),
        ("curatorAgentId", json!("agent-1")),
        ("curationMode", json!("off")),
        ("curationIntervalMinutes", json!(1440)),
        ("agentWriteAccess", json!("bound-sessions")),
        ("wikiEnabled", json!(false)),
        ("pendingCategories", json!(["review"])),
    ];
    let barrier = Barrier::new(patches.len());
    std::thread::scope(|scope| {
        for (key, value) in &patches {
            let store = KnowledgeStore::new(data.path().into());
            let barrier = &barrier;
            let id = &kb.id;
            scope.spawn(move || {
                let patch = serde_json::from_value(json!({ (*key): value })).unwrap();
                barrier.wait();
                store.update(id, patch).unwrap();
            });
        }
    });

    let reopened = KnowledgeStore::new(data.path().into());
    let updated = serde_json::to_value(&reopened.list().unwrap()[0]).unwrap();
    assert_eq!(updated["id"], kb.id);
    for (key, expected) in patches {
        assert_eq!(updated[key], expected, "missing update for {key}");
    }
}

#[test]
fn knowledge_acceptance_checks_every_lock_combination_after_reopening() {
    let data = tempdir().unwrap();
    let open = || KnowledgeStore::new(data.path().into());
    let kb = open().create(settings("Permissions")).unwrap();
    let fields = [
        (
            MetadataKey::Category,
            json!("user category"),
            json!("agent category"),
        ),
        (MetadataKey::Tags, json!(["user tag"]), json!(["agent tag"])),
    ];

    for locks in 0u8..4 {
        let source = open().create_source(&kb.id, note()).unwrap();
        let mut user_patch = MetadataPatch::new();
        let mut agent_patch = MetadataPatch::new();
        for (index, (key, user_value, agent_value)) in fields.iter().enumerate() {
            if locks & (1 << index) != 0 {
                user_patch.insert(*key, user_value.clone());
            }
            agent_patch.insert(*key, agent_value.clone());
        }
        open()
            .set_metadata(
                &kb.id,
                &source.id,
                KnowledgeMetadataOrigin::User,
                user_patch,
            )
            .unwrap();
        let before = open().get_source(&kb.id, &source.id).unwrap();
        let result = open()
            .set_metadata(
                &kb.id,
                &source.id,
                KnowledgeMetadataOrigin::Agent,
                agent_patch,
            )
            .unwrap();
        let after = open().get_source(&kb.id, &source.id).unwrap();
        assert_eq!(result.len(), fields.len());
        for (index, (key, user_value, agent_value)) in fields.iter().enumerate() {
            let saved = &after.metadata[key];
            if locks & (1 << index) != 0 {
                assert_eq!(
                    result[key],
                    MetadataOutcome::Rejected {
                        code: KbFieldLocked
                    }
                );
                assert_eq!(saved, &before.metadata[key]);
                assert_eq!(saved.value, *user_value);
                assert_eq!(saved.origin, KnowledgeMetadataOrigin::User);
            } else {
                assert_eq!(result[key], MetadataOutcome::Updated);
                assert_eq!(saved.value, *agent_value);
                assert_eq!(saved.origin, KnowledgeMetadataOrigin::Agent);
            }
        }
    }
}
