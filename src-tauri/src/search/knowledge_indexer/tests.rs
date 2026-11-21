use super::*;
use crate::knowledge::ingest::{ImportOutcome, KnowledgeIngestManager};
use crate::knowledge::store::{KnowledgeBasePatch, MetadataPatch, SourceStatus};
use crate::knowledge::types::KnowledgeMetadataOrigin;
use std::time::{Duration, Instant};
use tempfile::{tempdir, TempDir};

fn library(store: &KnowledgeStore, name: &str) -> String {
    store
        .create(KnowledgeBasePatch {
            name: Some(name.into()),
            ..Default::default()
        })
        .unwrap()
        .id
}

fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(25);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "knowledge fixture did not settle"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn ingest(manager: &Arc<KnowledgeIngestManager>, kb: &str, path: &Path) -> String {
    let Some(ImportOutcome::Accepted { receipt }) =
        manager.import_files(kb, vec![path.into()]).pop()
    else {
        panic!("expected one accepted source");
    };
    let job = receipt.job.unwrap();
    until(|| {
        let observed = manager.status(&job.id).unwrap();
        assert_ne!(observed.status, SourceStatus::Failed, "{observed:?}");
        observed.status == SourceStatus::Ready
    });
    receipt.source_id
}

fn write_fixture(base: &TempDir, filename: &str, text: &str) -> PathBuf {
    let path = base.path().canonicalize().unwrap().join(filename);
    fs::write(&path, text).unwrap();
    path
}

fn lookup(
    index: &KnowledgeIndex,
    store: &KnowledgeStore,
    kb: &str,
    query: &str,
) -> Vec<KnowledgeSearchHit> {
    index
        .search(
            store,
            &[kb.into()],
            &[query.into()],
            &Default::default(),
            None,
        )
        .unwrap()
        .hits
}

fn tags(store: &KnowledgeStore, kb: &str, source: &str, category: &str) {
    let values = MetadataPatch::from([
        (MetadataKey::Tags, serde_json::json!(["two words", "果园"])),
        (MetadataKey::Category, serde_json::json!(category)),
        (MetadataKey::DocType, serde_json::json!("report")),
        (
            MetadataKey::Summary,
            serde_json::json!("wildflower cultivation"),
        ),
    ]);
    store
        .set_metadata(kb, source, KnowledgeMetadataOrigin::User, values)
        .unwrap();
}

#[test]
fn multilingual_terms_and_summary_resolve_only_within_scope_and_filters() {
    let data = tempdir().unwrap();
    let input = tempdir().unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().into()));
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
    let a = library(&store, "First");
    let b = library(&store, "Second");
    let body = format!("{}跨境物流校验 blueberry 运输记录。", "history ".repeat(70));
    let source = ingest(&manager, &a, &write_fixture(&input, "notes.md", &body));
    let other = ingest(
        &manager,
        &b,
        &write_fixture(
            &input,
            "another.md",
            "跨境物流校验 blueberry blueberry blueberry。",
        ),
    );
    tags(&store, &a, &source, "supply/asia");
    tags(&store, &b, &other, "supply/europe");
    let index = KnowledgeIndex::new(data.path().into());
    assert!(matches!(
        index.search(
            &store,
            std::slice::from_ref(&a),
            &["blueberry".into()],
            &Default::default(),
            None
        ),
        Err(KnowledgeErrorCode::KbIndexing)
    ));
    index.rebuild(&store).unwrap();
    for query in ["跨境物流", "BLUEBERRY", "跨境物流 blueberry", "wildflower"] {
        let hits = lookup(&index, &store, &a, query);
        assert_eq!(hits.len(), 1, "{query}");
        assert_eq!(hits[0].kb_id, a);
        assert_eq!(hits[0].source_id, source);
        if query == "BLUEBERRY" {
            assert!(hits[0].snippet.contains("blueberry"));
        }
        assert!(matches!(
            store.resolve_ref(&hits[0].reference).unwrap().fragment,
            Some(crate::knowledge::store::ResolvedFragment::Available { .. })
        ));
    }
    let inaccessible = index
        .search(
            &store,
            &[],
            &["blueberry".into()],
            &Default::default(),
            None,
        )
        .unwrap();
    assert!(inaccessible.hits.is_empty());
    let query = ["wildflower".into()];
    let scope = [a.clone(), b.clone()];
    let filter = KnowledgeSearchFilters {
        tag: Some("two words".into()),
        category: Some("supply/asia".into()),
        doc_type: Some("report".into()),
    };
    let matching = index
        .search(&store, &scope, &query, &filter, Some(1))
        .unwrap();
    assert_eq!(matching.mode, "bm25");
    assert_eq!(matching.hits[0].source_id, source);
    for mismatch in [
        KnowledgeSearchFilters {
            tag: Some("words".into()),
            ..Default::default()
        },
        KnowledgeSearchFilters {
            category: Some("supply".into()),
            ..Default::default()
        },
        KnowledgeSearchFilters {
            doc_type: Some("note".into()),
            ..Default::default()
        },
    ] {
        assert!(index
            .search(&store, &scope, &query, &mismatch, None)
            .unwrap()
            .hits
            .is_empty());
    }
    manager.shutdown().unwrap();
}

fn case_hit(label: &str, source: &str) -> KnowledgeSearchHit {
    KnowledgeSearchHit {
        reference: label.into(),
        kb_id: "test".into(),
        source_id: source.into(),
        chunk_id: label.into(),
        title: String::new(),
        prefix: String::new(),
        snippet: String::new(),
        page: None,
        score: 0.0,
    }
}

#[test]
fn reciprocal_rank_matches_manual_scores_and_keeps_three_hits_per_source() {
    let ranking = |entries: &[&str]| {
        entries
            .iter()
            .map(|&id| case_hit(id, if id == "E" { "different" } else { "same" }))
            .collect()
    };
    let results = fuse(
        vec![
            ranking(&["A", "B", "C", "D", "E"]),
            ranking(&["C", "B", "D"]),
        ],
        8,
    );
    assert_eq!(
        results
            .iter()
            .map(|hit| hit.reference.as_str())
            .collect::<Vec<_>>(),
        ["C", "B", "D", "E"]
    );
    let manual = [
        (1.0 / 63.0 + 1.0 / 61.0),
        (2.0 / 62.0),
        (1.0 / 64.0 + 1.0 / 63.0),
        1.0 / 65.0,
    ];
    assert!(results
        .iter()
        .zip(manual)
        .all(|(hit, score)| (hit.score - score).abs() < 1e-12));
}

#[test]
fn scoped_candidates_are_filtered_before_the_top_fifty_even_with_colliding_ids() {
    use crate::i18n::SupportedLocale;
    use crate::knowledge::chunker::chunk_markdown;
    use crate::knowledge::store::{SourceFacts, SourceKind};

    let data = tempdir().unwrap();
    let state = IndexState::open(data.path()).unwrap();
    let a = "kb_0123456789ab";
    let b = "kb_abcdef012345";
    let source = KnowledgeSource {
        id: "src_0123456789abcdef".into(),
        display_name: Some("Invoice".into()),
        facts: SourceFacts {
            kind: SourceKind::Note,
            current_path: None,
            content_hash: None,
            file_fingerprint: None,
            status: SourceStatus::Ready,
        },
        metadata: BTreeMap::new(),
    };
    let a_body = "lychee shipping schedule";
    let b_body = format!("{a_body}\n\n{}", "lychee lychee lychee\n\n".repeat(60));
    {
        let mut writer = state.writer.lock().unwrap();
        for (kb, body) in [(a, a_body), (b, &b_body)] {
            let chunks = chunk_markdown(
                kb,
                &source.id,
                source.display_title(),
                body,
                SupportedLocale::EnUs,
            )
            .unwrap();
            state.add_source(&writer, kb, &source, chunks).unwrap();
        }
        state.commit(&mut writer).unwrap();
    }
    let query = ["lychee".to_owned()];
    let filters = KnowledgeSearchFilters::default();
    let response = state.search(&[a.into()], &query, &filters, 8).unwrap();
    assert_eq!(response.hits.len(), 1);
    assert!(response.hits[0]
        .reference
        .starts_with(&format!("kb://{a}/")));
    let response = state.search(&[b.into()], &query, &filters, 8).unwrap();
    assert_eq!(response.hits.len(), SOURCE_LIMIT);
    assert!(response.hits.iter().all(|hit| hit.kb_id == b));
}

#[test]
fn observes_metadata_revisions_and_source_and_library_removal() {
    use crate::knowledge::store::{SourceFacts, SourceKind};
    use sha2::{Digest, Sha256};

    let data = tempdir().unwrap();
    let input = tempdir().unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().into()));
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
    let kb = library(&store, "Incremental");
    let other = library(&store, "Remove");
    let engine = Arc::new(KnowledgeIndex::new(data.path().into()));
    let changes = store.subscribe_changes();
    let observer =
        tauri::async_runtime::spawn(Arc::clone(&engine).observe(Arc::clone(&store), changes));
    let source = ingest(
        &manager,
        &kb,
        &write_fixture(&input, "stream.md", "saffron harvest"),
    );
    let hash = store.get_source(&kb, &source).unwrap().facts.content_hash;
    until(|| engine.status(&kb, &source).unwrap() == KnowledgeSearchStatus::Indexed);
    assert_eq!(lookup(&engine, &store, &kb, "saffron").len(), 1);
    tags(&store, &kb, &source, "fresh");
    until(|| !lookup(&engine, &store, &kb, "wildflower").is_empty());
    assert_eq!(
        store.get_source(&kb, &source).unwrap().facts.content_hash,
        hash
    );
    store.remove_source(&kb, &source).unwrap();
    until(|| lookup(&engine, &store, &kb, "saffron").is_empty());

    let facts = |body: &str, status| SourceFacts {
        kind: SourceKind::Note,
        current_path: None,
        content_hash: Some(format!("{:x}", Sha256::digest(body.as_bytes()))),
        file_fingerprint: None,
        status,
    };
    let earlier = "Plum check";
    let note = store
        .create_source(&kb, facts(earlier, SourceStatus::Queued))
        .unwrap();
    let note_content = data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("sources")
        .join(&note.id)
        .join("content.md");
    fs::write(&note_content, earlier).unwrap();
    store
        .update_source(&kb, &note.id, facts(earlier, SourceStatus::Ready))
        .unwrap();
    until(|| !lookup(&engine, &store, &kb, "plum").is_empty());
    let previous_hit = lookup(&engine, &store, &kb, "plum").remove(0);
    let later = "Persimmon check";
    fs::write(&note_content, later).unwrap();
    store
        .update_source(&kb, &note.id, facts(later, SourceStatus::Ready))
        .unwrap();
    until(|| !lookup(&engine, &store, &kb, "persimmon").is_empty());
    assert!(lookup(&engine, &store, &kb, "plum").is_empty());
    let updated_hit = lookup(&engine, &store, &kb, "persimmon").remove(0);
    assert_eq!(updated_hit.source_id, previous_hit.source_id);
    assert_ne!(updated_hit.reference, previous_hit.reference);
    assert!(matches!(
        store.resolve_ref(&previous_hit.reference).unwrap().fragment,
        Some(crate::knowledge::store::ResolvedFragment::Unavailable)
    ));

    let second = ingest(
        &manager,
        &other,
        &write_fixture(&input, "gone.md", "tangerine harvest"),
    );
    until(|| engine.status(&other, &second).unwrap() == KnowledgeSearchStatus::Indexed);
    store.delete(&other).unwrap();
    until(|| lookup(&engine, &store, &other, "tangerine").is_empty());
    manager.shutdown().unwrap();
    observer.abort();
}

#[test]
fn missing_segment_or_old_schema_rebuilds_only_the_knowledge_projection() {
    let data = tempdir().unwrap();
    let input = tempdir().unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().into()));
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
    let kb = library(&store, "Rebuild");
    let source = ingest(
        &manager,
        &kb,
        &write_fixture(&input, "text.txt", "hazelnut harvest"),
    );
    let root = data.path().join("search_index");
    let original = data
        .path()
        .join("knowledge")
        .join(&kb)
        .join("sources")
        .join(&source)
        .join("source.json");
    let original = (fs::read(&original).unwrap(), original);
    let file_search_workspace = tempdir().unwrap();
    write_fixture(&file_search_workspace, "workspace-note.txt", "millet");
    let file_search_workspace = file_search_workspace.path().canonicalize().unwrap();
    let existing = crate::search::SearchEngine::new(data.path().into()).unwrap();
    existing
        .file_indices
        .refresh_or_create(file_search_workspace.to_str().unwrap())
        .unwrap();
    fn index_metadata(dir: &Path, metadata: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for item in fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                index_metadata(&path, metadata);
            } else if path
                .file_name()
                .is_some_and(|name| name == "meta.json" || name == ".schema_version")
            {
                metadata.insert(path.clone(), fs::read(path).unwrap());
            }
        }
    }
    let other_roots = ["sessions", "records", "workspaces"].map(|name| root.join(name));
    let mut other_files = BTreeMap::new();
    for dir in &other_roots {
        let before = other_files.len();
        index_metadata(dir, &mut other_files);
        assert!(other_files.len() > before);
    }
    let open = || {
        let index = KnowledgeIndex::new(data.path().into());
        assert_eq!(
            index.status(&kb, &source).unwrap(),
            KnowledgeSearchStatus::Pending
        );
        index.rebuild(&store).unwrap();
        assert_eq!(
            index.status(&kb, &source).unwrap(),
            KnowledgeSearchStatus::Indexed
        );
        assert_eq!(lookup(&index, &store, &kb, "hazelnut").len(), 1);
    };
    open();
    let segment = fs::read_dir(root.join("knowledge/bm25"))
        .unwrap()
        .find_map(|entry| {
            let path = entry.unwrap().path();
            (path.extension().and_then(|ext| ext.to_str()) == Some("term")).then_some(path)
        })
        .expect("an actual Tantivy terms file");
    fs::remove_file(segment).unwrap();
    open();
    fs::write(data.path().join(VERSION_FILE), "0").unwrap();
    open();
    let marker = data.path().join(VERSION_FILE);
    let marker_backup = marker.with_extension("original");
    fs::rename(&marker, &marker_backup).unwrap();
    fs::create_dir(&marker).unwrap();
    let index_meta = root.join("knowledge/bm25/meta.json");
    let intact = fs::read(&index_meta).unwrap();
    let index = KnowledgeIndex::new(data.path().into());
    assert_eq!(
        index.rebuild(&store),
        Err(KnowledgeErrorCode::KbStoreUnavailable)
    );
    assert_eq!(fs::read(&index_meta).unwrap(), intact);
    fs::remove_dir(&marker).unwrap();
    fs::rename(&marker_backup, marker).unwrap();
    open();
    assert_eq!(fs::read(&original.1).unwrap(), original.0);
    let mut other_files_after = BTreeMap::new();
    for dir in &other_roots {
        index_metadata(dir, &mut other_files_after);
    }
    assert_eq!(other_files_after, other_files);
    manager.shutdown().unwrap();
}

#[test]
fn rebuilding_reports_pending_without_changing_the_ready_source() {
    let data = tempdir().unwrap();
    let input = tempdir().unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().into()));
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
    let kb = library(&store, "Status");
    let path = write_fixture(&input, "figs.md", "fig orchard");
    let source = ingest(&manager, &kb, &path);
    let before = store.get_source(&kb, &source).unwrap();
    let index = KnowledgeIndex::new(data.path().into());
    index.rebuild(&store).unwrap();
    assert_eq!(
        index.status(&kb, &source).unwrap(),
        KnowledgeSearchStatus::Indexed
    );
    let lock = data.path().join("search_index/knowledge/.open.lock");
    std::thread::scope(|threads| {
        let (locked, wait_for_lock) = std::sync::mpsc::channel();
        let (release, wait_for_release) = std::sync::mpsc::channel();
        let holding = threads.spawn(move || {
            with_file_lock_blocking(&lock, FileLockOptions::default(), || {
                locked.send(()).unwrap();
                let _ = wait_for_release.recv();
                Ok(())
            })
            .unwrap();
        });
        wait_for_lock.recv_timeout(Duration::from_secs(2)).unwrap();
        let rebuilding = threads.spawn(|| index.rebuild(&store));
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut pending = false;
        while Instant::now() < deadline {
            if index.status(&kb, &source).unwrap() == KnowledgeSearchStatus::Pending {
                pending = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let unavailable = index.search(
            &store,
            std::slice::from_ref(&kb),
            &["fig".into()],
            &Default::default(),
            None,
        );
        let during = store.get_source(&kb, &source).unwrap();
        release.send(()).unwrap();
        holding.join().unwrap();
        rebuilding.join().unwrap().unwrap();
        assert!(pending);
        assert!(matches!(unavailable, Err(KnowledgeErrorCode::KbIndexing)));
        assert_eq!(during, before);
    });
    assert_eq!(lookup(&index, &store, &kb, "fig").len(), 1);
    assert_eq!(store.get_source(&kb, &source).unwrap(), before);
    manager.shutdown().unwrap();
}

#[test]
#[ignore = "requires the official Document Worker, PDFium and OCR resource bundle"]
fn resource_backed_ingestion_indexes_five_formats_end_to_end() {
    use crate::document_processing::DocumentProcessingManager;
    use crate::local_inference::{LocalComputeCoordinator, LocalInferenceRuntimeRegistry};

    let root = std::env::var_os("MYAGENTS_KB_TEST_RESOURCE_ROOT")
        .expect("point MYAGENTS_KB_TEST_RESOURCE_ROOT at prepared document resources");
    let resources = PathBuf::from(root).canonicalize().unwrap();
    let data = tempdir().unwrap();
    let input = tempdir().unwrap();
    let docs = DocumentProcessingManager::initialize(
        data.path().into(),
        resources.clone(),
        LocalInferenceRuntimeRegistry::initialize(&resources),
        LocalComputeCoordinator::new(),
    )
    .unwrap();
    let store = Arc::new(KnowledgeStore::new(data.path().into()));
    let manager = Arc::new(KnowledgeIngestManager::new(
        Arc::clone(&store),
        Some(Arc::clone(&docs)),
    ));
    let kb = library(&store, "Five formats");
    let engine = Arc::new(KnowledgeIndex::new(data.path().into()));
    let observer = tauri::async_runtime::spawn(
        Arc::clone(&engine).observe(Arc::clone(&store), store.subscribe_changes()),
    );
    let fixtures: &[(&str, &[u8], &str)] = &[
        (
            "garden.md",
            b"# Mandarin\n\nClementine journal",
            "Clementine journal",
        ),
        ("notes.txt", b"Apricot picking", "Apricot picking"),
        (
            "sample.docx",
            include_bytes!("../../knowledge/ingest/conversion/fixtures/sample.docx"),
            "The test orchard has three trees",
        ),
        (
            "text.pdf",
            include_bytes!("../../knowledge/ingest/conversion/fixtures/text.pdf"),
            "Native orchard",
        ),
        (
            "attachment.pdf",
            include_bytes!("../../knowledge/ingest/conversion/fixtures/scan.pdf"),
            "SCAN ORCHARD",
        ),
    ];
    for &(name, bytes, expected) in fixtures {
        let path = input.path().canonicalize().unwrap().join(name);
        fs::write(&path, bytes).unwrap();
        let source_id = ingest(&manager, &kb, &path);
        until(|| engine.status(&kb, &source_id).unwrap() == KnowledgeSearchStatus::Indexed);
        let hits = lookup(&engine, &store, &kb, &format!("\"{expected}\""));
        assert!(
            hits.iter().any(|hit| {
                hit.source_id == source_id
                    && matches!(
                        store.resolve_ref(&hit.reference).unwrap().fragment,
                        Some(crate::knowledge::store::ResolvedFragment::Available { chunk })
                            if chunk.text.contains(expected)
                    )
            }),
            "no live reference with the converted body for {name}"
        );
        if name.ends_with(".pdf") {
            let source = data
                .path()
                .join("knowledge")
                .join(&kb)
                .join("sources")
                .join(&source_id)
                .join("source.json");
            let record: serde_json::Value =
                serde_json::from_slice(&fs::read(source).unwrap()).unwrap();
            let worker_id = record["ingest"]["documentJobId"].as_str().unwrap();
            assert_eq!(
                docs.get(worker_id).unwrap().metrics.unwrap().pages_ocr,
                Some(u32::from(name == "attachment.pdf"))
            );
        }
    }
    manager.shutdown().unwrap();
    docs.shutdown().unwrap();
    observer.abort();
}

#[cfg(not(debug_assertions))]
#[test]
#[ignore = "release-only BM25 performance budget"]
fn knowledge_bm25_10k_chunks_keeps_p95_below_100ms() {
    use crate::knowledge::store::{SourceFacts, SourceKind};

    let data = tempdir().unwrap();
    let state = IndexState::open(data.path()).unwrap();
    let kb = "kb_0123456789ab";
    let queries = ["blueberry shipment", "柑橘采收", "orchard 计划"];
    {
        let mut writer = state.writer.lock().unwrap();
        for source_id in 0..250 {
            let source = KnowledgeSource {
                id: format!("src_{source_id:016x}"),
                display_name: Some(format!("Harvest schedule {source_id}")),
                facts: SourceFacts {
                    kind: SourceKind::Note,
                    current_path: None,
                    content_hash: None,
                    file_fingerprint: None,
                    status: SourceStatus::Ready,
                },
                metadata: BTreeMap::new(),
            };
            let chunks = (0..40)
                .map(|ordinal| KnowledgeChunk {
                    ordinal,
                    chunk_id: format!("{ordinal:016x}"),
                    heading_path: Vec::new(),
                    page: None,
                    text: format!(
                        "{}。 Ledger for picking and transport in lot {source_id} / {ordinal}",
                        queries[(source_id + ordinal) % queries.len()]
                    ),
                    prefix: source.display_title().into(),
                    byte_range: [0, 0],
                    utf16_range: [0, 0],
                    table: None,
                })
                .collect();
            state.add_source(&writer, kb, &source, chunks).unwrap();
        }
        state.commit(&mut writer).unwrap();
    }
    let scope = [kb.to_owned()];
    let filters = KnowledgeSearchFilters::default();
    let mut samples = Vec::new();
    for i in 0..140 {
        let query = [queries[i % queries.len()].to_owned()];
        let start = Instant::now();
        let response = state.search(&scope, &query, &filters, 8).unwrap();
        let elapsed = start.elapsed();
        assert!(!response.hits.is_empty());
        if i >= 20 {
            samples.push(elapsed);
        }
    }
    samples.sort_unstable();
    let p95 = samples[(samples.len() * 95).div_ceil(100) - 1];
    println!("10000 chunks, {} samples, p95={p95:?}", samples.len());
    assert!(p95 < Duration::from_millis(100));
}
