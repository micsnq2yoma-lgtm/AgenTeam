// Acceptance probes (not part of the delivery).
use super::*;
use tempfile::tempdir;

fn settings(name: &str) -> KnowledgeBasePatch {
    KnowledgeBasePatch { name: Some(name.into()), ..KnowledgeBasePatch::default() }
}
fn note() -> SourceFacts {
    SourceFacts { kind: SourceKind::Note, current_path: None, content_hash: None, file_fingerprint: None, status: SourceStatus::Ready }
}
#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    let ok = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"]).arg(link).arg(target)
        .output().unwrap().status.success();
    assert!(ok, "mklink /J failed");
}

#[cfg(windows)]
#[test]
fn probe_windows_junctions_never_reach_outside_files() {
    let data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("original.txt"), b"outside").unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Junction")).unwrap();
    let source = store.create_source(&kb.id, note()).unwrap();
    let root = data.path().join("knowledge").join(&kb.id);

    // A junction inside the library survives deletion of the library.
    junction(&root.join("assets-alias"), outside.path());
    // The source directory itself replaced by a junction.
    let source_dir = root.join("sources").join(&source.id);
    fs::rename(&source_dir, root.join("retained")).unwrap();
    junction(&source_dir, outside.path());

    assert!(store.get_source(&kb.id, &source.id).is_err(), "junctioned source must not be read");
    assert!(store.remove_source(&kb.id, &source.id).is_err(), "junctioned source must not be removed");
    let _ = store.delete(&kb.id);
    assert_eq!(fs::read(outside.path().join("original.txt")).unwrap(), b"outside");
}

#[cfg(windows)]
#[test]
fn probe_windows_library_dir_junction_is_refused() {
    let data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("kb.json"), b"{}").unwrap();
    fs::write(outside.path().join("original.txt"), b"outside").unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Whole")).unwrap();
    let live = data.path().join("knowledge").join(&kb.id);
    fs::rename(&live, data.path().join("moved")).unwrap();
    junction(&live, outside.path());
    assert!(store.delete(&kb.id).is_err());
    assert_eq!(fs::read(outside.path().join("original.txt")).unwrap(), b"outside");
}

#[test]
fn probe_orphan_library_from_crash_is_invisible_and_harmless() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    let kb = store.create(settings("Real")).unwrap();
    // Simulate a crash after kb.json was written but before kbs.json published it.
    let orphan = create_knowledge_base_id([7; 6]);
    let dir = data.path().join("knowledge").join(&orphan);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kb.json"), b"{\"partial\":").unwrap();
    let reopened = KnowledgeStore::new(data.path().into());
    assert_eq!(reopened.list().unwrap(), vec![kb]);
    assert_eq!(reopened.get_source(&orphan, &create_knowledge_source_id([1; 8])), Err(KbNotFound));
    assert!(reopened.create(settings("After crash")).is_ok());
}

#[test]
fn probe_traversal_ids_are_rejected_before_io() {
    let data = tempdir().unwrap();
    let store = KnowledgeStore::new(data.path().into());
    for bad in ["..", "../kb_000000000000", "kb_000000000000/..", "KB_000000000000", "kb_00000000000g"] {
        assert_eq!(store.list_sources(bad), Err(KbInvalidInput), "{bad}");
        assert_eq!(store.delete(bad), Err(KbInvalidInput), "{bad}");
    }
    assert_eq!(store.resolve_ref("kb://kb_000000000000/src_0000000000000000"), Err(KbNotFound));
}
