use super::*;
use crate::knowledge::store::linked::{
    detect_git_work_tree, path_key_with_case_folding, strip_macos_firmlink_prefix,
};
use crate::knowledge::store::{
    KnowledgeBasePatch, MetadataKey, MetadataPatch, ResolvedFragment, SourceKind, SourceStatus,
};
use crate::knowledge::types::KnowledgeMetadataOrigin;
use std::time::Instant;
use tempfile::tempdir;

fn make_kb(store: &KnowledgeStore, name: &str) -> String {
    store
        .create(KnowledgeBasePatch {
            name: Some(name.into()),
            ..Default::default()
        })
        .unwrap()
        .id
}

#[test]
fn external_rename_and_move_preserve_source_id_citations_and_skip_rechunking() {
    let data = tempdir().unwrap();
    let user_dir = tempdir().unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().to_path_buf()));
    let kb_id = make_kb(&store, "Linked Move KB");

    let original_file = user_dir.path().join("notes.md");
    let body = "# 项目设计\n\n## 核心机制\n\n稳定的引用锚点 linked-anchor-9921 不应因外部重命名而失效。\n";
    fs::write(&original_file, body).unwrap();

    let manager = KnowledgeLinkedManager::new(Arc::clone(&store), None);
    let (folder, initial) = manager.add_linked_folder(&kb_id, user_dir.path()).unwrap();
    assert!(!folder.organize_allowed);
    assert!(!folder.git_confirmed);
    assert_eq!(initial.added, 1);
    assert_eq!(initial.rechunked, 1);

    let sources = store.list_sources(&kb_id).unwrap();
    assert_eq!(sources.len(), 1);
    let src = &sources[0];
    assert_eq!(src.facts.kind, SourceKind::LinkedFile);
    assert_eq!(src.facts.status, SourceStatus::Ready);

    // Linked files MUST NOT duplicate `original.<ext>` inside the managed Store directory.
    let managed_src_dir = data
        .path()
        .join("knowledge")
        .join(&kb_id)
        .join("sources")
        .join(&src.id);
    assert!(!managed_src_dir.join("original.md").exists());
    assert!(managed_src_dir.join("content.md").exists());

    // Lock a user metadata field and capture stable chunk ref before moving.
    let mut patch = MetadataPatch::new();
    patch.insert(MetadataKey::Summary, serde_json::json!("人工锁定摘要"));
    store
        .set_metadata(&kb_id, &src.id, KnowledgeMetadataOrigin::User, patch)
        .unwrap();

    let chunks = store.list_chunks(&kb_id, &src.id).unwrap();
    assert!(!chunks.is_empty());
    let target_chunk = chunks
        .iter()
        .find(|c| c.text.contains("linked-anchor-9921"))
        .expect("target chunk");
    let kb_ref = format!("kb://{}/{}#{}", kb_id, src.id, target_chunk.chunk_id);
    let orig_hash = src.facts.content_hash.clone().unwrap();

    // 1. Move + rename into a subdirectory (preserving mtime on most filesystems).
    let archive_dir = user_dir.path().join("archive");
    fs::create_dir_all(&archive_dir).unwrap();
    let moved_path = archive_dir.join("renamed-notes.md");
    fs::rename(&original_file, &moved_path).unwrap();

    let after_rename = manager.scan_linked_folder(&kb_id, &folder.path).unwrap();
    assert_eq!(after_rename.moved, 1);
    assert_eq!(after_rename.rechunked, 0);
    assert_eq!(after_rename.added, 0);
    assert_eq!(after_rename.removed, 0);

    let resolved = store.resolve_ref(&kb_ref).unwrap();
    assert_eq!(resolved.source.id, src.id);
    assert_eq!(resolved.source.facts.content_hash.as_deref(), Some(orig_hash.as_str()));
    assert_eq!(
        resolved.source.metadata.get(&MetadataKey::Summary).map(|f| &f.value),
        Some(&serde_json::json!("人工锁定摘要"))
    );
    match resolved.fragment {
        Some(ResolvedFragment::Available { chunk }) => {
            assert_eq!(chunk.chunk_id, target_chunk.chunk_id);
        }
        other => panic!("expected available chunk after rename, got {other:?}"),
    }
    let preview = resolved.preview.expect("expected preview after rename");
    assert!(!preview.is_content_fallback);
    assert!(preview.path.replace('\\', "/").ends_with("archive/renamed-notes.md"));

    // Verify content_revision stayed at 1 (no re-chunk / no embedding recomputation).
    let snaps = store.list_linked_sources_for_folder(&kb_id, &folder.path).unwrap();
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].content_revision, 1);

    // 2. Copy+delete move with a fresh mtime -> still recognized as move by content hash!
    std::thread::sleep(Duration::from_millis(25));
    let deep_dir = user_dir.path().join("deep");
    fs::create_dir_all(&deep_dir).unwrap();
    let copied_move = deep_dir.join("copied-move.md");
    fs::write(&copied_move, body).unwrap();
    fs::remove_file(&moved_path).unwrap();

    let after_copy_move = manager.scan_linked_folder(&kb_id, &folder.path).unwrap();
    assert_eq!(after_copy_move.moved, 1);
    assert_eq!(after_copy_move.rechunked, 0);
    assert_eq!(after_copy_move.added, 0);
    assert_eq!(after_copy_move.removed, 0);

    let resolved2 = store.resolve_ref(&kb_ref).unwrap();
    assert_eq!(resolved2.source.id, src.id);
    let snaps2 = store.list_linked_sources_for_folder(&kb_id, &folder.path).unwrap();
    assert_eq!(snaps2[0].content_revision, 1);
}

#[test]
fn live_debounced_watcher_tracks_external_rename_and_new_file() {
    let data = tempdir().unwrap();
    let user_dir = tempdir().unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().to_path_buf()));
    let kb_id = make_kb(&store, "Watcher KB");

    let initial_file = user_dir.path().join("alpha.md");
    fs::write(&initial_file, "# Alpha\n\nWatcher baseline document.\n").unwrap();

    let manager = Arc::new(KnowledgeLinkedManager::new_with_debounce(
        Arc::clone(&store),
        None,
        Duration::from_millis(120),
    ));
    manager.start().unwrap();

    let (folder, summary) = manager.add_linked_folder(&kb_id, user_dir.path()).unwrap();
    assert_eq!(summary.added, 1);
    let original_id = store.list_sources(&kb_id).unwrap()[0].id.clone();

    // Perform an external rename + add a new file on disk while watcher is live.
    let renamed_file = user_dir.path().join("alpha-renamed.md");
    fs::rename(&initial_file, &renamed_file).unwrap();
    fs::write(user_dir.path().join("beta.md"), "# Beta\n\nNewly created file.\n").unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snaps = store
            .list_linked_sources_for_folder(&kb_id, &folder.path)
            .unwrap();
        let renamed_ok = snaps.iter().any(|s| {
            s.source_id == original_id
                && s.current_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    == Some("alpha-renamed.md")
                && s.content_revision == 1
        });
        let added_ok = snaps.iter().any(|s| {
            s.current_path.file_name().and_then(|n| n.to_str()) == Some("beta.md")
        });
        if renamed_ok && added_ok {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "watcher did not reconcile rename + addition in time; snapshots={snaps:?}"
        );
        std::thread::sleep(Duration::from_millis(80));
    }

    manager.shutdown();
}

#[test]
fn symlinks_junctions_hidden_dirs_and_credential_files_are_never_followed_or_indexed() {
    let data = tempdir().unwrap();
    let user_dir = tempdir().unwrap();
    let outside_dir = tempdir().unwrap();

    // External secret outside the linked folder boundary
    let outside_secret = outside_dir.path().join("external-secret.md");
    fs::write(&outside_secret, "# Secret\n\nTOP_SECRET_OUTSIDE_TOKEN\n").unwrap();

    // Legitimate file inside the linked folder
    let docs_dir = user_dir.path().join("docs");
    fs::create_dir_all(&docs_dir).unwrap();
    fs::write(docs_dir.join("public.md"), "# Public\n\nSafe documentation.\n").unwrap();

    // Hidden directory, skipped build/vendor directories, and credential-like files
    fs::create_dir_all(user_dir.path().join(".hidden")).unwrap();
    fs::write(user_dir.path().join(".hidden").join("secret.md"), "HIDDEN_SECRET").unwrap();
    fs::create_dir_all(user_dir.path().join("node_modules").join("pkg")).unwrap();
    fs::write(
        user_dir.path().join("node_modules").join("pkg").join("README.md"),
        "NODE_MODULES_CONTENT",
    )
    .unwrap();
    fs::write(user_dir.path().join(".env"), "API_KEY=12345").unwrap();
    fs::write(user_dir.path().join("id_rsa"), "PRIVATE KEY").unwrap();
    fs::write(user_dir.path().join("server.key"), "TLS PRIVATE KEY").unwrap();
    fs::write(user_dir.path().join("credentials.json"), "{\"secret\":true}").unwrap();

    // Create external directory redirect (NTFS junction on Windows, symlink dir + file on Unix)
    #[cfg(windows)]
    {
        junction::create(outside_dir.path(), user_dir.path().join("escape-junction")).unwrap();
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside_dir.path(), user_dir.path().join("escape-dir-link"))
            .unwrap();
        std::os::unix::fs::symlink(&outside_secret, user_dir.path().join("escape-file-link.md"))
            .unwrap();
    }

    let store = Arc::new(KnowledgeStore::new(data.path().to_path_buf()));
    let kb_id = make_kb(&store, "Security Boundary KB");
    let manager = KnowledgeLinkedManager::new(Arc::clone(&store), None);
    let (_, summary) = manager.add_linked_folder(&kb_id, user_dir.path()).unwrap();

    assert_eq!(summary.added, 1, "only docs/public.md should be indexed");
    assert!(summary.skipped >= 4);

    let sources = store.list_sources(&kb_id).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].display_name.as_deref(), Some("public.md"));
    assert!(outside_secret.exists());
}

#[test]
fn removing_linked_folder_and_deleting_kb_never_delete_user_originals() {
    let data = tempdir().unwrap();
    let user_dir = tempdir().unwrap();
    let file_a = user_dir.path().join("contract.md");
    let sub = user_dir.path().join("specs");
    fs::create_dir_all(&sub).unwrap();
    let file_b = sub.join("design.txt");
    fs::write(&file_a, "# Contract\n\nOriginal contract text.\n").unwrap();
    fs::write(&file_b, "Original design notes.").unwrap();

    let store = Arc::new(KnowledgeStore::new(data.path().to_path_buf()));
    let kb_id = make_kb(&store, "Safe Unlink KB");
    let manager = KnowledgeLinkedManager::new(Arc::clone(&store), None);

    let (folder, summary) = manager.add_linked_folder(&kb_id, user_dir.path()).unwrap();
    assert_eq!(summary.added, 2);
    assert_eq!(store.list_sources(&kb_id).unwrap().len(), 2);

    // 1. Remove linked folder -> removes Store projections ONLY; user originals untouched!
    let removed = manager.remove_linked_folder(&kb_id, &folder.path).unwrap();
    assert_eq!(removed, 2);
    assert!(store.list_sources(&kb_id).unwrap().is_empty());
    assert!(store.get(&kb_id).unwrap().linked_folders.is_empty());
    assert_eq!(
        fs::read_to_string(&file_a).unwrap(),
        "# Contract\n\nOriginal contract text.\n"
    );
    assert_eq!(fs::read_to_string(&file_b).unwrap(), "Original design notes.");

    // 2. Re-link and delete entire KB -> user originals STILL 100% untouched!
    manager.add_linked_folder(&kb_id, user_dir.path()).unwrap();
    assert_eq!(store.list_sources(&kb_id).unwrap().len(), 2);
    store.delete(&kb_id).unwrap();
    assert_eq!(
        fs::read_to_string(&file_a).unwrap(),
        "# Contract\n\nOriginal contract text.\n"
    );
    assert_eq!(fs::read_to_string(&file_b).unwrap(), "Original design notes.");
}

#[test]
fn git_worktree_detection_and_default_organize_flags_persist() {
    let data = tempdir().unwrap();
    let plain_dir = tempdir().unwrap();
    let git_repo = tempdir().unwrap();
    fs::create_dir_all(git_repo.path().join(".git")).unwrap();
    let nested_in_git = git_repo.path().join("docs").join("guides");
    fs::create_dir_all(&nested_in_git).unwrap();

    let worktree_dir = tempdir().unwrap();
    fs::write(
        worktree_dir.path().join(".git"),
        "gitdir: /some/repo/.git/worktrees/wt1\n",
    )
    .unwrap();

    assert!(!detect_git_work_tree(plain_dir.path()));
    assert!(detect_git_work_tree(&nested_in_git));
    assert!(detect_git_work_tree(worktree_dir.path()));

    let store = Arc::new(KnowledgeStore::new(data.path().to_path_buf()));
    let kb_id = make_kb(&store, "Git Flags KB");
    let manager = KnowledgeLinkedManager::new(Arc::clone(&store), None);

    let (plain_linked, _) = manager.add_linked_folder(&kb_id, plain_dir.path()).unwrap();
    assert!(!plain_linked.organize_allowed);
    assert!(!plain_linked.git_confirmed);
    assert!(!plain_linked.is_git_work_tree);

    let (git_linked, _) = manager.add_linked_folder(&kb_id, &nested_in_git).unwrap();
    assert!(!git_linked.organize_allowed);
    assert!(!git_linked.git_confirmed);
    assert!(git_linked.is_git_work_tree);

    let updated = manager
        .update_linked_folder(&kb_id, &git_linked.path, Some(true), Some(true))
        .unwrap();
    assert!(updated.organize_allowed);
    assert!(updated.git_confirmed);
    assert!(updated.is_git_work_tree);

    // Reopen Store from disk to verify persistence in kb.json
    let reopened = KnowledgeStore::new(data.path().to_path_buf());
    let kb = reopened.get(&kb_id).unwrap();
    assert_eq!(kb.linked_folders.len(), 2);
    let persisted_git = kb
        .linked_folders
        .iter()
        .find(|f| f.path == git_linked.path)
        .expect("persisted git linked folder");
    assert!(persisted_git.organize_allowed);
    assert!(persisted_git.git_confirmed);
    assert!(persisted_git.is_git_work_tree);
}

#[test]
fn windows_case_insensitivity_and_macos_firmlink_path_normalization() {
    // macOS APFS firmlink prefix stripping
    let firmlink = Path::new("/System/Volumes/Data/Users/alice/Projects/Docs/guide.md");
    assert_eq!(
        strip_macos_firmlink_prefix(firmlink),
        PathBuf::from("/Users/alice/Projects/Docs/guide.md")
    );
    assert_eq!(
        path_key_with_case_folding(firmlink, true),
        path_key_with_case_folding(Path::new("/users/alice/projects/docs/GUIDE.MD"), true)
    );

    // Windows drive/backslash/case normalization
    assert_eq!(
        path_key_with_case_folding(Path::new(r"\\?\C:\Users\Alice\Docs\Report.MD"), true),
        path_key_with_case_folding(Path::new("c:/users/alice/docs/report.md"), true)
    );

    // Case-sensitive mode (Linux) preserves case distinction
    assert_ne!(
        path_key_with_case_folding(Path::new("/home/alice/Report.md"), false),
        path_key_with_case_folding(Path::new("/home/alice/report.md"), false)
    );

    // Live case-only rename on Windows / macOS preserves source_id with 0 re-chunking
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let data = tempdir().unwrap();
        let user_dir = tempdir().unwrap();
        let upper = user_dir.path().join("DesignNote.md");
        fs::write(&upper, "# Case Rename\n\nPreserved across case change.\n").unwrap();

        let store = Arc::new(KnowledgeStore::new(data.path().to_path_buf()));
        let kb_id = make_kb(&store, "Case Rename KB");
        let manager = KnowledgeLinkedManager::new(Arc::clone(&store), None);
        let (folder, _) = manager.add_linked_folder(&kb_id, user_dir.path()).unwrap();
        let orig_id = store.list_sources(&kb_id).unwrap()[0].id.clone();

        // Two-step rename ensures directory entry case changes on Windows NTFS / macOS APFS.
        let tmp = user_dir.path().join("DesignNote.tmp.md");
        let lower = user_dir.path().join("designnote.md");
        fs::rename(&upper, &tmp).unwrap();
        fs::rename(&tmp, &lower).unwrap();

        let summary = manager.scan_linked_folder(&kb_id, &folder.path).unwrap();
        assert_eq!(summary.rechunked, 0);
        assert_eq!(summary.added, 0);
        assert_eq!(summary.removed, 0);
        assert_eq!(summary.moved, 1);

        let snaps = store.list_linked_sources_for_folder(&kb_id, &folder.path).unwrap();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].source_id, orig_id);
        assert_eq!(snaps[0].content_revision, 1);
        assert_eq!(
            snaps[0].current_path.file_name().and_then(|n| n.to_str()),
            Some("designnote.md")
        );
    }
}
