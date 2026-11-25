use super::*;
use crate::knowledge::linked::KnowledgeLinkedManager;
use crate::knowledge::store::KnowledgeBasePatch;
use tempfile::tempdir;

fn setup_authorized_kb(
    with_git: bool,
    git_confirmed: bool,
) -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Arc<KnowledgeStore>,
    KnowledgeLinkedManager,
    KnowledgeOrganizeManager,
    String,
    PathBuf,
) {
    let data = tempdir().unwrap();
    let user_dir = tempdir().unwrap();
    if with_git {
        fs::create_dir_all(user_dir.path().join(".git")).unwrap();
    }
    let store = Arc::new(KnowledgeStore::new(data.path().to_path_buf()));
    let kb_id = store
        .create(KnowledgeBasePatch {
            name: Some("K13 Organize KB".into()),
            ..Default::default()
        })
        .unwrap()
        .id;
    let linked = KnowledgeLinkedManager::new(Arc::clone(&store), None);
    let (folder, _) = linked.add_linked_folder(&kb_id, user_dir.path()).unwrap();
    linked
        .update_linked_folder(&kb_id, &folder.path, Some(true), Some(git_confirmed))
        .unwrap();
    let organize = KnowledgeOrganizeManager::new(Arc::clone(&store), data.path().to_path_buf());
    (data, user_dir, store, linked, organize, kb_id, folder.path)
}

#[test]
fn proposal_rejects_unauthorized_git_traversal_symlinks_reserved_names_trailing_and_overlength() {
    let data = tempdir().unwrap();
    let unauth_dir = tempdir().unwrap();
    let git_dir = tempdir().unwrap();
    let outside_dir = tempdir().unwrap();
    fs::create_dir_all(git_dir.path().join(".git")).unwrap();
    fs::write(unauth_dir.path().join("a.md"), "hello").unwrap();
    fs::write(git_dir.path().join("g.md"), "git file").unwrap();
    fs::write(outside_dir.path().join("secret.md"), "secret").unwrap();

    let store = Arc::new(KnowledgeStore::new(data.path().to_path_buf()));
    let kb_id = store
        .create(KnowledgeBasePatch {
            name: Some("Rejections".into()),
            ..Default::default()
        })
        .unwrap()
        .id;
    let linked = KnowledgeLinkedManager::new(Arc::clone(&store), None);
    let (unauth_folder, _) = linked.add_linked_folder(&kb_id, unauth_dir.path()).unwrap();
    let (git_folder, _) = linked.add_linked_folder(&kb_id, git_dir.path()).unwrap();
    // Authorize git_folder for organize, but leave git_confirmed = false!
    linked
        .update_linked_folder(&kb_id, &git_folder.path, Some(true), Some(false))
        .unwrap();

    let organize = KnowledgeOrganizeManager::new(Arc::clone(&store), data.path().to_path_buf());

    // 1. Unauthorized linked folder (organizeAllowed = false) -> KbAccessDenied
    assert_eq!(
        organize
            .propose_plan(
                &kb_id,
                ProposePlanInput {
                    items: vec![ProposeItemInput {
                        kind: OrganizeOpKind::Rename,
                        folder_root: unauth_folder.path.clone(),
                        source_rel: Some("a.md".into()),
                        target_rel: "b.md".into(),
                        reason: "rename".into(),
                    }],
                },
            )
            .unwrap_err(),
        KbAccessDenied
    );

    // 2. Unlinked / out-of-bounds folder -> KbAccessDenied
    assert_eq!(
        organize
            .propose_plan(
                &kb_id,
                ProposePlanInput {
                    items: vec![ProposeItemInput {
                        kind: OrganizeOpKind::Mkdir,
                        folder_root: outside_dir.path().to_path_buf(),
                        source_rel: None,
                        target_rel: "newdir".into(),
                        reason: "mkdir outside".into(),
                    }],
                },
            )
            .unwrap_err(),
        KbAccessDenied
    );

    // 3. Unconfirmed Git worktree -> KbAccessDenied
    assert_eq!(
        organize
            .propose_plan(
                &kb_id,
                ProposePlanInput {
                    items: vec![ProposeItemInput {
                        kind: OrganizeOpKind::Rename,
                        folder_root: git_folder.path.clone(),
                        source_rel: Some("g.md".into()),
                        target_rel: "g2.md".into(),
                        reason: "rename in git".into(),
                    }],
                },
            )
            .unwrap_err(),
        KbAccessDenied
    );

    // Enable organize on unauth_folder to test all path validation rejections
    linked
        .update_linked_folder(&kb_id, &unauth_folder.path, Some(true), None)
        .unwrap();

    // Create symlink/junction inside folder
    #[cfg(windows)]
    junction::create(outside_dir.path(), unauth_dir.path().join("linkdir")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside_dir.path(), unauth_dir.path().join("linkdir")).unwrap();

    let bad_targets = vec![
        "../escape.md".to_string(),
        "CON".to_string(),
        "aux.txt".to_string(),
        "NUL.md".to_string(),
        "trailing.".to_string(),
        "trailing ".to_string(),
        "linkdir/moved.md".to_string(),
        ".hidden/secret.md".to_string(),
        "id_rsa".to_string(),
        format!("{}.md", "a".repeat(210)),
    ];
    for bad in bad_targets {
        let res = organize.propose_plan(
            &kb_id,
            ProposePlanInput {
                items: vec![ProposeItemInput {
                    kind: OrganizeOpKind::Move,
                    folder_root: unauth_folder.path.clone(),
                    source_rel: Some("a.md".into()),
                    target_rel: bad.clone(),
                    reason: "invalid target test".into(),
                }],
            },
        );
        assert!(
            res.is_err(),
            "expected bad target '{bad}' to be rejected at propose time"
        );
    }

    // Empty reason rejected
    assert_eq!(
        organize
            .propose_plan(
                &kb_id,
                ProposePlanInput {
                    items: vec![ProposeItemInput {
                        kind: OrganizeOpKind::Mkdir,
                        folder_root: unauth_folder.path.clone(),
                        source_rel: None,
                        target_rel: "docs".into(),
                        reason: "   ".into(),
                    }],
                },
            )
            .unwrap_err(),
        KbInvalidInput
    );
}

#[tokio::test]
async fn precondition_mismatch_and_target_conflict_skip_item_without_overwriting_while_others_succeed()
{
    let (_data, user_dir, _store, _linked, organize, kb_id, folder_root) =
        setup_authorized_kb(false, false);

    fs::write(user_dir.path().join("stale.md"), "version 1").unwrap();
    fs::write(user_dir.path().join("collide.md"), "collide source").unwrap();
    fs::write(user_dir.path().join("clean.md"), "clean source").unwrap();

    let plan = organize
        .propose_plan(
            &kb_id,
            ProposePlanInput {
                items: vec![
                    ProposeItemInput {
                        kind: OrganizeOpKind::Mkdir,
                        folder_root: folder_root.clone(),
                        source_rel: None,
                        target_rel: "sorted".into(),
                        reason: "create sorted folder".into(),
                    },
                    ProposeItemInput {
                        kind: OrganizeOpKind::Move,
                        folder_root: folder_root.clone(),
                        source_rel: Some("stale.md".into()),
                        target_rel: "sorted/stale.md".into(),
                        reason: "move stale".into(),
                    },
                    ProposeItemInput {
                        kind: OrganizeOpKind::Move,
                        folder_root: folder_root.clone(),
                        source_rel: Some("collide.md".into()),
                        target_rel: "sorted/collide.md".into(),
                        reason: "move collide".into(),
                    },
                    ProposeItemInput {
                        kind: OrganizeOpKind::Move,
                        folder_root: folder_root.clone(),
                        source_rel: Some("clean.md".into()),
                        target_rel: "sorted/clean.md".into(),
                        reason: "move clean".into(),
                    },
                ],
            },
        )
        .unwrap();

    let approved = organize
        .approve_plan(
            &kb_id,
            &plan.id,
            ApprovalDecision {
                reject: false,
                items: vec![],
            },
        )
        .unwrap();
    assert_eq!(approved.status, OrganizePlanStatus::Approved);

    // Mutate stale.md AND pre-create sorted/collide.md BEFORE execution!
    fs::write(user_dir.path().join("stale.md"), "version 2 modified!").unwrap();
    fs::create_dir_all(user_dir.path().join("sorted")).unwrap();
    fs::write(
        user_dir.path().join("sorted").join("collide.md"),
        "EXISTING_DO_NOT_OVERWRITE",
    )
    .unwrap();

    let executed = organize.execute_plan(&kb_id, &plan.id).await.unwrap();
    assert_eq!(executed.status, OrganizePlanStatus::PartiallyCompleted);

    // op_1 (mkdir sorted) -> conflict because sorted already exists
    assert_eq!(executed.items[0].status, OrganizeItemStatus::Conflict);
    // op_2 (stale.md) -> conflict because precondition changed; original untouched
    assert_eq!(executed.items[1].status, OrganizeItemStatus::Conflict);
    assert_eq!(
        fs::read_to_string(user_dir.path().join("stale.md")).unwrap(),
        "version 2 modified!"
    );
    // op_3 (collide.md) -> conflict; EXISTING target bytes 100% untouched!
    assert_eq!(executed.items[2].status, OrganizeItemStatus::Conflict);
    assert_eq!(
        fs::read_to_string(user_dir.path().join("sorted").join("collide.md")).unwrap(),
        "EXISTING_DO_NOT_OVERWRITE"
    );
    assert!(user_dir.path().join("collide.md").exists());
    // op_4 (clean.md) -> succeeded!
    assert_eq!(executed.items[3].status, OrganizeItemStatus::Succeeded);
    assert!(!user_dir.path().join("clean.md").exists());
    assert_eq!(
        fs::read_to_string(user_dir.path().join("sorted").join("clean.md")).unwrap(),
        "clean source"
    );
}

#[tokio::test]
async fn case_only_rename_succeeds_and_updates_linked_source() {
    let (_data, user_dir, store, linked, organize, kb_id, folder_root) =
        setup_authorized_kb(false, false);

    fs::write(user_dir.path().join("DesignDoc.md"), "# Case Test\n\nBody.\n").unwrap();
    linked.scan_linked_folder(&kb_id, &folder_root).unwrap();
    let orig_id = store.list_sources(&kb_id).unwrap()[0].id.clone();

    let plan = organize
        .propose_plan(
            &kb_id,
            ProposePlanInput {
                items: vec![ProposeItemInput {
                    kind: OrganizeOpKind::Rename,
                    folder_root: folder_root.clone(),
                    source_rel: Some("DesignDoc.md".into()),
                    target_rel: "designdoc.md".into(),
                    reason: "lowercase filename".into(),
                }],
            },
        )
        .unwrap();
    organize
        .approve_plan(
            &kb_id,
            &plan.id,
            ApprovalDecision {
                reject: false,
                items: vec![],
            },
        )
        .unwrap();

    let executed = organize.execute_plan(&kb_id, &plan.id).await.unwrap();
    assert_eq!(executed.status, OrganizePlanStatus::Completed);
    assert_eq!(executed.items[0].status, OrganizeItemStatus::Succeeded);

    // Verify actual on-disk directory entry name is now lowercase
    let names: Vec<String> = fs::read_dir(user_dir.path())
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    assert!(
        names.iter().any(|n| n == "designdoc.md"),
        "expected exact lowercase entry 'designdoc.md', got {names:?}"
    );
    let src = store.get_source(&kb_id, &orig_id).unwrap();
    assert_eq!(src.display_name.as_deref(), Some("designdoc.md"));
}

#[cfg(windows)]
#[tokio::test]
async fn windows_file_in_use_fails_single_item_with_explicit_reason_and_continues_others() {
    use std::os::windows::fs::OpenOptionsExt;

    let (_data, user_dir, _store, _linked, organize, kb_id, folder_root) =
        setup_authorized_kb(false, false);

    let locked_path = user_dir.path().join("locked.md");
    fs::write(&locked_path, "locked content").unwrap();
    fs::write(user_dir.path().join("free.md"), "free content").unwrap();

    let plan = organize
        .propose_plan(
            &kb_id,
            ProposePlanInput {
                items: vec![
                    ProposeItemInput {
                        kind: OrganizeOpKind::Rename,
                        folder_root: folder_root.clone(),
                        source_rel: Some("locked.md".into()),
                        target_rel: "locked-renamed.md".into(),
                        reason: "rename locked".into(),
                    },
                    ProposeItemInput {
                        kind: OrganizeOpKind::Rename,
                        folder_root: folder_root.clone(),
                        source_rel: Some("free.md".into()),
                        target_rel: "free-renamed.md".into(),
                        reason: "rename free".into(),
                    },
                ],
            },
        )
        .unwrap();
    organize
        .approve_plan(
            &kb_id,
            &plan.id,
            ApprovalDecision {
                reject: false,
                items: vec![],
            },
        )
        .unwrap();

    // Hold an exclusive Windows handle (share_mode = 0) so rename fails with ERROR_SHARING_VIOLATION.
    let _exclusive_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&locked_path)
        .unwrap();

    let executed = organize.execute_plan(&kb_id, &plan.id).await.unwrap();
    assert_eq!(executed.status, OrganizePlanStatus::PartiallyCompleted);
    assert_eq!(executed.items[0].status, OrganizeItemStatus::Failed);
    assert!(
        executed.items[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("FILE_IN_USE")),
        "expected FILE_IN_USE error, got {:?}",
        executed.items[0].error
    );
    assert_eq!(executed.items[1].status, OrganizeItemStatus::Succeeded);
    assert!(user_dir.path().join("free-renamed.md").exists());
}

#[tokio::test]
async fn crash_injection_at_all_four_journal_points_recovers_cleanly_without_loss_or_duplication() {
    let points = [
        CrashPoint::IntentRecorded,
        CrashPoint::FsMoved,
        CrashPoint::StoreRelinked,
        CrashPoint::DoneRecorded,
    ];

    for point in points {
        let (_data, user_dir, store, linked, organize, kb_id, folder_root) =
            setup_authorized_kb(false, false);
        fs::write(user_dir.path().join("doc.md"), "# Crash Recovery\n\nSafe bytes.\n").unwrap();
        linked.scan_linked_folder(&kb_id, &folder_root).unwrap();
        let src_id = store.list_sources(&kb_id).unwrap()[0].id.clone();

        let plan = organize
            .propose_plan(
                &kb_id,
                ProposePlanInput {
                    items: vec![ProposeItemInput {
                        kind: OrganizeOpKind::Rename,
                        folder_root: folder_root.clone(),
                        source_rel: Some("doc.md".into()),
                        target_rel: "doc-moved.md".into(),
                        reason: "crash test".into(),
                    }],
                },
            )
            .unwrap();
        organize
            .approve_plan(
                &kb_id,
                &plan.id,
                ApprovalDecision {
                    reject: false,
                    items: vec![],
                },
            )
            .unwrap();

        let err = organize
            .execute_plan_with_crash(&kb_id, &plan.id, Some(("op_1".into(), point)))
            .await
            .unwrap_err();
        assert_eq!(err, KbStoreUnavailable);

        // Simulate restart -> recover_interrupted_plans()
        let recovered_ids = organize.recover_interrupted_plans().unwrap();
        assert_eq!(recovered_ids, vec![plan.id.clone()], "point={point:?}");

        let recovered = organize.get_plan(&kb_id, &plan.id).unwrap();
        assert_eq!(recovered.status, OrganizePlanStatus::Interrupted, "point={point:?}");

        let old_exists = user_dir.path().join("doc.md").exists();
        let new_exists = user_dir.path().join("doc-moved.md").exists();
        // Exactly one copy exists: NEVER lost and NEVER duplicated!
        assert_ne!(old_exists, new_exists, "point={point:?}");

        match point {
            CrashPoint::IntentRecorded => {
                assert!(old_exists);
                assert_eq!(recovered.items[0].status, OrganizeItemStatus::Pending);
            }
            CrashPoint::FsMoved | CrashPoint::StoreRelinked | CrashPoint::DoneRecorded => {
                assert!(new_exists);
                assert_eq!(recovered.items[0].status, OrganizeItemStatus::Succeeded);
                // Recovery must also ensure source.json reflects the new path!
                let src = store.get_source(&kb_id, &src_id).unwrap();
                assert_eq!(src.display_name.as_deref(), Some("doc-moved.md"));
            }
        }
    }
}

#[tokio::test]
async fn undo_restores_directory_tree_byte_for_byte_and_skips_post_execution_modified_files() {
    let (_data, user_dir, store, linked, organize, kb_id, folder_root) =
        setup_authorized_kb(false, false);

    let bytes_a = b"# Alpha\n\nExact original bytes A.\n".to_vec();
    let bytes_b = b"# Beta\n\nOriginal bytes B before post-edit.\n".to_vec();
    fs::write(user_dir.path().join("alpha.md"), &bytes_a).unwrap();
    fs::write(user_dir.path().join("beta.md"), &bytes_b).unwrap();
    linked.scan_linked_folder(&kb_id, &folder_root).unwrap();

    let plan = organize
        .propose_plan(
            &kb_id,
            ProposePlanInput {
                items: vec![
                    ProposeItemInput {
                        kind: OrganizeOpKind::Mkdir,
                        folder_root: folder_root.clone(),
                        source_rel: None,
                        target_rel: "bucket_a".into(),
                        reason: "create bucket A".into(),
                    },
                    ProposeItemInput {
                        kind: OrganizeOpKind::Mkdir,
                        folder_root: folder_root.clone(),
                        source_rel: None,
                        target_rel: "bucket_b".into(),
                        reason: "create bucket B".into(),
                    },
                    ProposeItemInput {
                        kind: OrganizeOpKind::Move,
                        folder_root: folder_root.clone(),
                        source_rel: Some("alpha.md".into()),
                        target_rel: "bucket_a/alpha.md".into(),
                        reason: "move alpha".into(),
                    },
                    ProposeItemInput {
                        kind: OrganizeOpKind::Move,
                        folder_root: folder_root.clone(),
                        source_rel: Some("beta.md".into()),
                        target_rel: "bucket_b/beta.md".into(),
                        reason: "move beta".into(),
                    },
                ],
            },
        )
        .unwrap();
    organize
        .approve_plan(
            &kb_id,
            &plan.id,
            ApprovalDecision {
                reject: false,
                items: vec![],
            },
        )
        .unwrap();
    let executed = organize.execute_plan(&kb_id, &plan.id).await.unwrap();
    assert_eq!(executed.status, OrganizePlanStatus::Completed);

    // User modifies bucket_b/beta.md AFTER execution!
    let modified_b = b"# Beta\n\nUser edited after organize!\n";
    fs::write(user_dir.path().join("bucket_b").join("beta.md"), modified_b).unwrap();

    let undo = organize.undo_plan(&kb_id, &plan.id).await.unwrap();

    // alpha.md restored byte-for-byte & empty bucket_a removed!
    assert!(undo.undone.contains(&"op_3".to_string()));
    assert!(undo.undone.contains(&"op_1".to_string()));
    assert_eq!(fs::read(user_dir.path().join("alpha.md")).unwrap(), bytes_a);
    assert!(!user_dir.path().join("bucket_a").exists());

    // beta.md was modified -> NOT undone, explicitly listed in skipped_modified, bytes preserved!
    assert!(undo.skipped_modified.contains(&"op_4".to_string()));
    assert!(!user_dir.path().join("beta.md").exists());
    assert_eq!(
        fs::read(user_dir.path().join("bucket_b").join("beta.md")).unwrap(),
        modified_b
    );
    // bucket_b still holds modified beta.md -> not deleted!
    assert!(undo.skipped_modified.contains(&"op_2".to_string()));
    assert!(user_dir.path().join("bucket_b").is_dir());

    // Store path for alpha.md is also restored back to root!
    let sources = store.list_sources(&kb_id).unwrap();
    let alpha_src = sources
        .iter()
        .find(|s| s.display_name.as_deref() == Some("alpha.md"))
        .expect("alpha source");
    assert_eq!(
        alpha_src.facts.current_path.as_deref().and_then(Path::parent),
        Some(folder_root.as_path())
    );
}

#[test]
fn pending_plan_expires_after_seven_days_and_becomes_stale() {
    let (data, user_dir, _store, _linked, organize, kb_id, folder_root) =
        setup_authorized_kb(false, false);
    fs::write(user_dir.path().join("old.md"), "old").unwrap();

    let plan = organize
        .propose_plan(
            &kb_id,
            ProposePlanInput {
                items: vec![ProposeItemInput {
                    kind: OrganizeOpKind::Rename,
                    folder_root,
                    source_rel: Some("old.md".into()),
                    target_rel: "older.md".into(),
                    reason: "expiry test".into(),
                }],
            },
        )
        .unwrap();

    // Backdate expires_at by 8 days on disk to simulate 7-day expiration.
    let plan_path = data
        .path()
        .join("knowledge")
        .join(&kb_id)
        .join("organize")
        .join("plans")
        .join(&plan.id)
        .join("plan.json");
    let mut stored: OrganizePlan =
        serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
    stored.expires_at = (Utc::now() - ChronoDuration::days(1)).to_rfc3339();
    fs::write(&plan_path, serde_json::to_vec_pretty(&stored).unwrap()).unwrap();

    let loaded = organize.get_plan(&kb_id, &plan.id).unwrap();
    assert_eq!(loaded.status, OrganizePlanStatus::Stale);
    assert!(organize
        .approve_plan(
            &kb_id,
            &plan.id,
            ApprovalDecision {
                reject: false,
                items: vec![],
            },
        )
        .is_err());
}

/// Static security redline check (AC-K13): Management API route table MUST NOT register
/// approve, execute, or undo endpoints for physical organize.
#[test]
fn management_api_does_not_expose_organize_approve_execute_or_undo_routes() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let api_files = [
        manifest_dir.join("src").join("management_api.rs"),
        manifest_dir
            .join("src")
            .join("management_api")
            .join("knowledge.rs"),
    ];
    let forbidden = [
        "organize/approve",
        "organize/execute",
        "organize/undo",
        "cmd_kb_organize_approve",
        "cmd_kb_organize_execute",
        "cmd_kb_organize_undo",
    ];
    for file in api_files {
        let content = fs::read_to_string(&file).expect("read management_api source");
        for needle in forbidden {
            assert!(
                !content.contains(needle),
                "Management API file {} must never register forbidden physical organize route '{}'",
                file.display(),
                needle
            );
        }
    }
}
