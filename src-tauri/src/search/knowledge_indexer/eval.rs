//! K11 offline end-to-end integration test and BM25 retrieval evaluation harness.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tantivy::tokenizer::TokenStream;
use tempfile::{tempdir, TempDir};

use super::{KnowledgeIndex, KnowledgeSearchFilters, KnowledgeSearchStatus};
use crate::knowledge::ingest::{ImportOutcome, KnowledgeIngestManager};
use crate::knowledge::store::{
    KnowledgeBasePatch, KnowledgeStore, ResolvedFragment, SourceStatus,
};
use crate::knowledge::types::{parse_knowledge_ref, KnowledgeErrorCode};
use crate::search::tokenizer;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QrelsFile {
    queries: Vec<QrelQuery>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QrelQuery {
    id: String,
    query: String,
    language: String,
    category: String,
    relevant: Vec<QrelTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct QrelTarget {
    file: String,
    heading_path: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HitSummary {
    rank: usize,
    file: String,
    heading_path: Vec<String>,
    reference: String,
    score: f64,
    relevant: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryEvalResult {
    id: String,
    query: String,
    language: String,
    category: String,
    zero_token_overlap: bool,
    overlapping_tokens: Vec<String>,
    relevant: Vec<QrelTarget>,
    first_relevant_rank: Option<usize>,
    recall_at_8: f64,
    mrr_at_10: f64,
    ndcg_at_10: f64,
    hits: Vec<HitSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubsetMetrics {
    count: usize,
    recall_at_8: f64,
    mrr_at_10: f64,
    ndcg_at_10: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EvalReport {
    mode: &'static str,
    corpus_docs: usize,
    total_chunks: usize,
    total_queries: usize,
    zero_overlap_queries: usize,
    zero_overlap_ratio: f64,
    verbatim_substring_queries: Vec<String>,
    missing_qrel_targets: Vec<String>,
    recall_at_8: f64,
    mrr_at_10: f64,
    ndcg_at_10: f64,
    by_category: BTreeMap<String, SubsetMetrics>,
    by_language: BTreeMap<String, SubsetMetrics>,
    queries: Vec<QueryEvalResult>,
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(Instant::now() < deadline, "knowledge ingest did not settle");
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn import_and_wait_ready(
    manager: &Arc<KnowledgeIngestManager>,
    kb_id: &str,
    path: &Path,
) -> String {
    let Some(ImportOutcome::Accepted { receipt }) =
        manager.import_files(kb_id, vec![path.to_path_buf()]).pop()
    else {
        panic!("expected accepted import for {}", path.display());
    };
    let job = receipt.job.expect("expected ingest job");
    wait_until(|| {
        let observed = manager.status(&job.id).expect("job status");
        assert_ne!(observed.status, SourceStatus::Failed, "{observed:?}");
        observed.status == SourceStatus::Ready
    });
    receipt.source_id
}

fn tokenize_content_terms(text: &str) -> BTreeSet<String> {
    let mut analyzer = tokenizer::build_chinese_tokenizer();
    let mut stream = analyzer.token_stream(text);
    let mut out = BTreeSet::new();
    while stream.advance() {
        let token = stream.token().text.trim();
        if token.is_empty() {
            continue;
        }
        if token.chars().all(|c| !c.is_alphanumeric()) {
            continue;
        }
        out.insert(token.to_string());
    }
    out
}

fn repo_eval_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join("scripts")
        .join("knowledge-eval")
}

fn compute_subset(rows: &[&QueryEvalResult]) -> SubsetMetrics {
    if rows.is_empty() {
        return SubsetMetrics {
            count: 0,
            recall_at_8: 0.0,
            mrr_at_10: 0.0,
            ndcg_at_10: 0.0,
        };
    }
    let n = rows.len() as f64;
    SubsetMetrics {
        count: rows.len(),
        recall_at_8: rows.iter().map(|r| r.recall_at_8).sum::<f64>() / n,
        mrr_at_10: rows.iter().map(|r| r.mrr_at_10).sum::<f64>() / n,
        ndcg_at_10: rows.iter().map(|r| r.ndcg_at_10).sum::<f64>() / n,
    }
}

fn run_corpus_evaluation(corpus_dir: &Path, qrels_path: &Path) -> EvalReport {
    let qrels_raw = fs::read_to_string(qrels_path).expect("read qrels.json");
    let qrels: QrelsFile = serde_json::from_str(&qrels_raw).expect("parse qrels.json");

    let mut corpus_files: Vec<PathBuf> = fs::read_dir(corpus_dir)
        .expect("read corpus dir")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md"))
                .then_some(path)
        })
        .collect();
    corpus_files.sort();

    let data_dir = tempdir().expect("temp data dir");
    let store = Arc::new(KnowledgeStore::new(data_dir.path().to_path_buf()));
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
    let kb_id = store
        .create(KnowledgeBasePatch {
            name: Some("K11 Eval Corpus".to_string()),
            ..Default::default()
        })
        .expect("create eval kb")
        .id;

    let mut raw_docs: Vec<(String, String)> = Vec::with_capacity(corpus_files.len());
    let mut chunk_lookup: BTreeMap<(String, String), QrelTarget> = BTreeMap::new();
    let mut target_tokens: BTreeMap<QrelTarget, BTreeSet<String>> = BTreeMap::new();
    let mut total_chunks = 0usize;

    let mut canonical_paths = Vec::with_capacity(corpus_files.len());
    let mut filenames = Vec::with_capacity(corpus_files.len());
    for file_path in &corpus_files {
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 filename")
            .to_string();
        let raw = fs::read_to_string(file_path).expect("read corpus file");
        raw_docs.push((filename.clone(), raw));
        filenames.push(filename);
        canonical_paths.push(file_path.canonicalize().expect("canonicalize corpus file"));
    }

    let outcomes = manager.import_files(&kb_id, canonical_paths);
    assert_eq!(outcomes.len(), filenames.len());
    let mut pending_jobs = Vec::with_capacity(outcomes.len());
    for (filename, outcome) in filenames.into_iter().zip(outcomes) {
        let ImportOutcome::Accepted { receipt } = outcome else {
            panic!("expected accepted import for {filename}");
        };
        let job = receipt.job.expect("expected ingest job");
        pending_jobs.push((filename, receipt.source_id, job.id));
    }

    wait_until(|| {
        pending_jobs.iter().all(|(_, _, job_id)| {
            let observed = manager.status(job_id).expect("job status");
            assert_ne!(observed.status, SourceStatus::Failed, "{observed:?}");
            observed.status == SourceStatus::Ready
        })
    });

    for (filename, source_id, _) in pending_jobs {
        let (_, chunks) = store
            .search_document(&kb_id, &source_id)
            .expect("search_document")
            .expect("ready source chunks");
        total_chunks += chunks.len();

        for chunk in chunks {
            let target = QrelTarget {
                file: filename.clone(),
                heading_path: chunk.heading_path.clone(),
            };
            chunk_lookup.insert((source_id.clone(), chunk.chunk_id.clone()), target.clone());
            let entry = target_tokens.entry(target).or_default();
            for heading in &chunk.heading_path {
                entry.extend(tokenize_content_terms(heading));
            }
            entry.extend(tokenize_content_terms(&chunk.text));
        }
    }

    let index = KnowledgeIndex::new(data_dir.path().to_path_buf());
    index.rebuild(&store).expect("rebuild eval bm25 index");

    let mut missing_qrel_targets = Vec::new();
    let mut verbatim_substring_queries = Vec::new();
    let mut results = Vec::with_capacity(qrels.queries.len());
    let scope = [kb_id.clone()];
    let filters = KnowledgeSearchFilters::default();

    for q in &qrels.queries {
        let normalized_q = q.query.trim().to_lowercase();
        if raw_docs
            .iter()
            .any(|(_, body)| body.to_lowercase().contains(&normalized_q))
        {
            verbatim_substring_queries.push(q.id.clone());
        }

        let rel_set: BTreeSet<QrelTarget> = q.relevant.iter().cloned().collect();
        let mut combined_target_terms = BTreeSet::new();
        for target in &rel_set {
            match target_tokens.get(target) {
                Some(terms) => combined_target_terms.extend(terms.iter().cloned()),
                None => missing_qrel_targets.push(format!(
                    "{}: {} -> {:?}",
                    q.id, target.file, target.heading_path
                )),
            }
        }

        let query_terms = tokenize_content_terms(&q.query);
        let overlapping_tokens: Vec<String> = query_terms
            .intersection(&combined_target_terms)
            .cloned()
            .collect();
        let zero_token_overlap = overlapping_tokens.is_empty();

        let search_res = index
            .search(&store, &scope, std::slice::from_ref(&q.query), &filters, Some(10))
            .expect("eval search");

        let mut matched_at_8: BTreeSet<QrelTarget> = BTreeSet::new();
        let mut matched_for_dcg: BTreeSet<QrelTarget> = BTreeSet::new();
        let mut first_relevant_rank: Option<usize> = None;
        let mut dcg = 0.0_f64;
        let mut hit_summaries = Vec::with_capacity(search_res.hits.len());

        for (idx, hit) in search_res.hits.iter().enumerate() {
            let rank = idx + 1;
            let mapped = chunk_lookup
                .get(&(hit.source_id.clone(), hit.chunk_id.clone()))
                .cloned()
                .unwrap_or_else(|| QrelTarget {
                    file: hit.title.clone(),
                    heading_path: Vec::new(),
                });
            let is_rel = rel_set.contains(&mapped);
            if is_rel {
                if first_relevant_rank.is_none() {
                    first_relevant_rank = Some(rank);
                }
                if rank <= 8 {
                    matched_at_8.insert(mapped.clone());
                }
                if matched_for_dcg.insert(mapped.clone()) {
                    dcg += 1.0 / ((rank as f64) + 1.0).log2();
                }
            }
            hit_summaries.push(HitSummary {
                rank,
                file: mapped.file,
                heading_path: mapped.heading_path,
                reference: hit.reference.clone(),
                score: hit.score,
                relevant: is_rel,
            });
        }

        let rel_count = rel_set.len().max(1);
        let recall_at_8 = (matched_at_8.len() as f64) / (rel_count as f64);
        let mrr_at_10 = first_relevant_rank.map_or(0.0, |r| 1.0 / (r as f64));
        let ideal_k = rel_count.min(10);
        let idcg: f64 = (1..=ideal_k)
            .map(|rank| 1.0 / ((rank as f64) + 1.0).log2())
            .sum();
        let ndcg_at_10 = if idcg > 0.0 { dcg / idcg } else { 0.0 };

        results.push(QueryEvalResult {
            id: q.id.clone(),
            query: q.query.clone(),
            language: q.language.clone(),
            category: q.category.clone(),
            zero_token_overlap,
            overlapping_tokens,
            relevant: q.relevant.clone(),
            first_relevant_rank,
            recall_at_8,
            mrr_at_10,
            ndcg_at_10,
            hits: hit_summaries,
        });
    }

    let all_refs: Vec<&QueryEvalResult> = results.iter().collect();
    let overall = compute_subset(&all_refs);
    let zero_overlap_queries = results.iter().filter(|r| r.zero_token_overlap).count();
    let zero_overlap_ratio = if results.is_empty() {
        0.0
    } else {
        (zero_overlap_queries as f64) / (results.len() as f64)
    };

    let mut by_category = BTreeMap::new();
    let categories: BTreeSet<String> = results.iter().map(|r| r.category.clone()).collect();
    for cat in categories {
        let rows: Vec<&QueryEvalResult> = results.iter().filter(|r| r.category == cat).collect();
        by_category.insert(cat, compute_subset(&rows));
    }

    let mut by_language = BTreeMap::new();
    let languages: BTreeSet<String> = results.iter().map(|r| r.language.clone()).collect();
    for lang in languages {
        let rows: Vec<&QueryEvalResult> = results.iter().filter(|r| r.language == lang).collect();
        by_language.insert(lang, compute_subset(&rows));
    }

    EvalReport {
        mode: "bm25",
        corpus_docs: corpus_files.len(),
        total_chunks,
        total_queries: results.len(),
        zero_overlap_queries,
        zero_overlap_ratio,
        verbatim_substring_queries,
        missing_qrel_targets,
        recall_at_8: overall.recall_at_8,
        mrr_at_10: overall.mrr_at_10,
        ndcg_at_10: overall.ndcg_at_10,
        by_category,
        by_language,
        queries: results,
    }
}

fn write_temp_doc(dir: &TempDir, name: &str, body: &str) -> PathBuf {
    let path = dir.path().canonicalize().expect("canonical temp").join(name);
    fs::write(&path, body).expect("write temp doc");
    path
}

/// AC-K11-1: Offline temp-dir end-to-end pipeline:
/// create KB -> import -> wait ready -> index via broadcast -> search -> resolve ref -> remove source -> search empty.
#[test]
fn k11_e2e_create_import_ready_search_resolve_remove_and_empty_search() {
    let data_dir = tempdir().expect("data tempdir");
    let input_dir = tempdir().expect("input tempdir");
    let store = Arc::new(KnowledgeStore::new(data_dir.path().to_path_buf()));
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));

    // Subscribe before mutating so incremental broadcast updates cover every step.
    let mut changes = store.subscribe_changes();
    let index = KnowledgeIndex::new(data_dir.path().to_path_buf());
    index.rebuild(&store).expect("initial empty index");

    let kb = store
        .create(KnowledgeBasePatch {
            name: Some("K11 离线端到端测试库".to_string()),
            description: Some("Offline tempdir E2E lifecycle check".to_string()),
            ..Default::default()
        })
        .expect("create kb");

    let doc_path = write_temp_doc(
        &input_dir,
        "architecture-handbook.md",
        "# AgenTeam 架构手册\n\n## 存储锁与原子提交\n\n跨进程文件锁保护知识库清单，所有状态写入均先落盘同步再原子替换。\n\n## 引用解析与定位\n\n稳定的星辰检索标记 `nebula-quorum-7741` 用于验证端到端检索与片段解析。\n",
    );

    let source_id = import_and_wait_ready(&manager, &kb.id, &doc_path);
    let source = store
        .get_source(&kb.id, &source_id)
        .expect("get ready source");
    assert_eq!(source.facts.status, SourceStatus::Ready);

    // Drain broadcast events into the real KnowledgeIndex.
    while let Ok(change) = changes.try_recv() {
        index.update(&store, &change).expect("apply change");
    }
    assert_eq!(
        index
            .status(&kb.id, &source_id)
            .expect("source status"),
        KnowledgeSearchStatus::Indexed
    );

    // Search via real Tantivy BM25 index.
    let search_out = index
        .search(
            &store,
            std::slice::from_ref(&kb.id),
            &["星辰检索标记 nebula-quorum-7741".to_string()],
            &KnowledgeSearchFilters::default(),
            Some(8),
        )
        .expect("search indexed source");
    assert_eq!(search_out.mode, "bm25");
    assert!(!search_out.hits.is_empty(), "expected search hit");
    let top_hit = &search_out.hits[0];
    assert_eq!(top_hit.kb_id, kb.id);
    assert_eq!(top_hit.source_id, source_id);
    let parsed_ref = parse_knowledge_ref(&top_hit.reference).expect("valid kb:// ref");
    assert_eq!(parsed_ref.kb_id, kb.id);
    assert_eq!(parsed_ref.source_id, source_id);
    assert_eq!(parsed_ref.chunk_id.as_deref(), Some(top_hit.chunk_id.as_str()));

    // Resolve ref via KnowledgeStore and verify fragment heading & body.
    let resolved = store
        .resolve_ref(&top_hit.reference)
        .expect("resolve hit reference");
    assert_eq!(resolved.source.id, source_id);
    match resolved.fragment {
        Some(ResolvedFragment::Available { chunk }) => {
            assert_eq!(
                chunk.heading_path,
                vec![
                    "AgenTeam 架构手册".to_string(),
                    "引用解析与定位".to_string()
                ]
            );
            assert!(chunk.text.contains("nebula-quorum-7741"));
        }
        other => panic!("expected available resolved fragment, got {other:?}"),
    }

    // Remove source, apply broadcast removal, and confirm search + resolve return empty / not found.
    store
        .remove_source(&kb.id, &source_id)
        .expect("remove source");
    while let Ok(change) = changes.try_recv() {
        index.update(&store, &change).expect("apply remove change");
    }
    let after_remove = index
        .search(
            &store,
            std::slice::from_ref(&kb.id),
            &["星辰检索标记 nebula-quorum-7741".to_string()],
            &KnowledgeSearchFilters::default(),
            Some(8),
        )
        .expect("search after remove");
    assert!(
        after_remove.hits.is_empty(),
        "expected no hits after remove, got {:?}",
        after_remove
            .hits
            .iter()
            .map(|h| &h.reference)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        store.resolve_ref(&top_hit.reference).unwrap_err(),
        KnowledgeErrorCode::KbNotFound
    );
}

#[test]
fn natural_chinese_and_punctuation_queries_search_without_phrase_lockin_while_preserving_quotes() {
    let data_dir = tempdir().expect("data tempdir");
    let input_dir = tempdir().expect("input tempdir");
    let store = Arc::new(KnowledgeStore::new(data_dir.path().to_path_buf()));
    let manager = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
    let kb = store
        .create(KnowledgeBasePatch {
            name: Some("Query Parser Regression".to_string()),
            ..Default::default()
        })
        .expect("create kb");

    let doc = write_temp_doc(
        &input_dir,
        "ops-guide.md",
        "# 运维手册\n\n## 校验脚本与死信\n\n使用 `verify:theme-css` 在 CI 拦截硬编码色值，并将失败事件路由至 `*.dlq` 死信队列。\n\n## 排期冻结\n\n第三季度研发排期于七月中旬冻结，移动端同步顺延至第四季度。\n",
    );
    import_and_wait_ready(&manager, &kb.id, &doc);
    let index = KnowledgeIndex::new(data_dir.path().to_path_buf());
    index.rebuild(&store).expect("rebuild index");
    let scope = [kb.id.clone()];
    let filters = KnowledgeSearchFilters::default();

    // Unspaced natural Chinese question (would turn into an unmatchable 10-word PhraseQuery without pre-tokenization)
    let zh_hits = index
        .search(
            &store,
            &scope,
            &["第三季度研发排期什么时候冻结，移动端同步延到哪一季？".to_string()],
            &filters,
            Some(8),
        )
        .expect("unspaced Chinese question");
    assert!(!zh_hits.hits.is_empty());

    // Colons and wildcards must not fail with KbInvalidInput
    let syntax_hits = index
        .search(
            &store,
            &scope,
            &["CI 怎么用 verify:theme-css 和 *.dlq？".to_string()],
            &filters,
            Some(8),
        )
        .expect("colon and wildcard query");
    assert!(!syntax_hits.hits.is_empty());

    // Quoted exact phrase still works
    let quoted_hits = index
        .search(
            &store,
            &scope,
            &["\"第三季度研发排期\"".to_string()],
            &filters,
            Some(8),
        )
        .expect("quoted phrase");
    assert!(!quoted_hits.hits.is_empty());
}

/// AC-K11-2 & AC-K11-3: Runs the real BM25 corpus evaluation offline in tempdir and enforces
/// corpus integrity, independent query construction, >=30% zero-keyword-overlap queries, and
/// Recall@8 >= 0.70, MRR@10 >= 0.50.
#[test]
fn k11_eval_corpus_meets_bm25_baseline() {
    let base = repo_eval_dir();
    let report = run_corpus_evaluation(&base.join("corpus"), &base.join("qrels.json"));

    assert!(
        report.corpus_docs >= 40,
        "expected >= 40 corpus docs, got {}",
        report.corpus_docs
    );
    assert!(
        report.total_queries >= 60,
        "expected >= 60 queries, got {}",
        report.total_queries
    );
    assert!(
        report.missing_qrel_targets.is_empty(),
        "qrels reference missing (file, headingPath) targets: {:?}",
        report.missing_qrel_targets
    );
    assert!(
        report.verbatim_substring_queries.is_empty(),
        "queries must not be verbatim substrings of corpus docs: {:?}",
        report.verbatim_substring_queries
    );
    assert!(
        report.zero_overlap_ratio >= 0.30,
        "expected >= 30% zero-keyword-overlap queries, got {:.1}% ({}/{})",
        report.zero_overlap_ratio * 100.0,
        report.zero_overlap_queries,
        report.total_queries
    );

    let overlapping_semantic: Vec<_> = report
        .queries
        .iter()
        .filter(|q| q.category == "semantic_no_overlap" && !q.zero_token_overlap)
        .map(|q| format!("{}: {:?}", q.id, q.overlapping_tokens))
        .collect();
    assert!(
        overlapping_semantic.is_empty(),
        "semantic_no_overlap queries unexpectedly shared tokens with target chunk: {:?}",
        overlapping_semantic
    );

    assert!(
        report.recall_at_8 >= 0.70,
        "BM25 Recall@8 {:.4} < 0.70 (MRR@10 {:.4}, nDCG@10 {:.4})",
        report.recall_at_8,
        report.mrr_at_10,
        report.ndcg_at_10
    );
    assert!(
        report.mrr_at_10 >= 0.50,
        "BM25 MRR@10 {:.4} < 0.50 (Recall@8 {:.4}, nDCG@10 {:.4})",
        report.mrr_at_10,
        report.recall_at_8,
        report.ndcg_at_10
    );
}

/// Invoked by `scripts/knowledge-eval/run.mjs` to emit full JSON metrics and per-query results.
#[test]
#[ignore = "invoked on demand by scripts/knowledge-eval/run.mjs"]
fn k11_eval_runner_cli() {
    let default_dir = repo_eval_dir();
    let corpus_dir = std::env::var_os("KB_EVAL_CORPUS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_dir.join("corpus"));
    let qrels_path = std::env::var_os("KB_EVAL_QRELS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_dir.join("qrels.json"));
    let report = run_corpus_evaluation(&corpus_dir, &qrels_path);
    let json = serde_json::to_string_pretty(&report).expect("serialize eval report");
    if let Some(out_path) = std::env::var_os("KB_EVAL_OUTPUT_JSON") {
        fs::write(out_path, &json).expect("write eval json output");
    } else {
        println!("KB_EVAL_JSON_BEGIN\n{json}\nKB_EVAL_JSON_END");
    }
}
