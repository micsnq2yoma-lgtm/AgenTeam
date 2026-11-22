//! The knowledge search projection is disposable. Store remains the source
//! of truth for content, lifecycle and metadata.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::directory::{Directory, MmapDirectory, INDEX_WRITER_LOCK};
use tantivy::query::{BooleanQuery, BoostQuery, Occur, Query, QueryParser, TermSetQuery};
use tantivy::schema::{Field, OwnedValue};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, TantivyError, Term};
use tokio::sync::broadcast;

use crate::knowledge::chunker::KnowledgeChunk;
use crate::knowledge::store::{
    KnowledgeChange, KnowledgeSource, KnowledgeStore, MetadataKey, SourceKind,
};
use crate::knowledge::types::{
    format_knowledge_ref, is_knowledge_base_id, is_knowledge_source_id, KnowledgeErrorCode,
    KnowledgeRef,
};
use crate::utils::file_lock::{with_file_lock_blocking, FileLockOptions};
use crate::workspace_files::path_safety::{
    ensure_workspace_directory_no_follow, read_workspace_file_no_follow_classified,
    write_workspace_file_no_follow, RegularFileError,
};

use super::schema::{knowledge_schema, KnowledgeFields, KNOWLEDGE_SCHEMA_VERSION};
use super::tokenizer;

#[cfg(test)]
mod tests;

const RETRIEVAL_LIMIT: usize = 50;
const SOURCE_LIMIT: usize = 3;
const DEFAULT_LIMIT: usize = 8;
const RRF_K: f64 = 60.0;
const CHUNK: &str = "chunk";
const SUMMARY: &str = "source_summary";
const VERSION_FILE: &str = "search_index/knowledge/.schema_version";

type SourceKey = (String, String);
type SearchResult<T> = Result<T, KnowledgeErrorCode>;

#[derive(Debug)]
enum QueryFailure {
    Invalid(KnowledgeErrorCode),
    Tantivy(TantivyError),
}

impl From<TantivyError> for QueryFailure {
    fn from(error: TantivyError) -> Self {
        Self::Tantivy(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeSearchStatus {
    Pending,
    Indexed,
    Failed,
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeSearchFilters {
    pub tag: Option<String>,
    pub category: Option<String>,
    pub doc_type: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchHit {
    pub reference: String,
    pub kb_id: String,
    pub source_id: String,
    pub chunk_id: String,
    pub title: String,
    pub prefix: String,
    pub page: Option<u32>,
    pub snippet: String,
    pub score: f64,
}

#[derive(Serialize)]
pub struct KnowledgeSearchResult {
    pub mode: &'static str,
    pub hits: Vec<KnowledgeSearchHit>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Pending,
    Indexed,
    Failed,
}

pub(super) struct KnowledgeIndex {
    data_dir: PathBuf,
    // Increments, startup and recoveries share one writer lifecycle. Searches
    // only hold a state read lock and use Tantivy's last committed reader.
    mutation: Mutex<()>,
    state: RwLock<Option<IndexState>>,
    phase: RwLock<Phase>,
}

struct IndexState {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: KnowledgeFields,
    statuses: RwLock<BTreeMap<SourceKey, KnowledgeSearchStatus>>,
}

fn unavailable<T>(_: T) -> KnowledgeErrorCode {
    KnowledgeErrorCode::KbStoreUnavailable
}

fn corrupt(error: &TantivyError) -> bool {
    matches!(
        error,
        TantivyError::DataCorruption(_)
            | TantivyError::OpenReadError(
                tantivy::directory::error::OpenReadError::FileDoesNotExist(_)
            )
    )
}

fn exact(field: Field, values: impl IntoIterator<Item = impl AsRef<str>>) -> Box<dyn Query> {
    Box::new(TermSetQuery::new(
        values
            .into_iter()
            .map(|value| Term::from_field_text(field, value.as_ref())),
    ))
}

fn source_key(kb: &str, source: &str) -> String {
    // Both inputs are fixed-width validated IDs with no slash.
    format!("{kb}/{source}")
}

fn text(doc: &TantivyDocument, field: Field) -> &str {
    match doc.get_first(field) {
        Some(OwnedValue::Str(value)) => value,
        _ => "",
    }
}

fn add_text(doc: &mut TantivyDocument, field: Field, value: &str) {
    if !value.is_empty() {
        doc.add_text(field, value);
    }
}

type Projection = Option<(KnowledgeSource, Vec<KnowledgeChunk>)>;

fn project(store: &KnowledgeStore, kb: &str, source: &str) -> SearchResult<Projection> {
    for _ in 0..3 {
        match store.search_document(kb, source) {
            Err(KnowledgeErrorCode::KbJobConflict) => continue,
            Err(KnowledgeErrorCode::KbNotFound) => return Ok(None),
            result => return result,
        }
    }
    Err(KnowledgeErrorCode::KbJobConflict)
}

impl KnowledgeIndex {
    pub(super) fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            mutation: Mutex::new(()),
            state: RwLock::new(None),
            phase: RwLock::new(Phase::Pending),
        }
    }

    pub(super) fn rebuild(&self, store: &KnowledgeStore) -> SearchResult<()> {
        let _guard = self.mutation.lock().map_err(unavailable)?;
        self.rebuild_locked(store, false)
    }

    pub(super) async fn observe(
        self: Arc<Self>,
        store: Arc<KnowledgeStore>,
        mut changes: broadcast::Receiver<KnowledgeChange>,
    ) {
        // The caller has subscribed before the baseline captures its first
        // source. Queued notifications reconcile all writes during rebuilding.
        let mut change = None;
        loop {
            let index = Arc::clone(&self);
            let store = Arc::clone(&store);
            let applied = tokio::task::spawn_blocking(move || match change {
                Some(change) => index
                    .update(&store, &change)
                    .or_else(|_| index.rebuild(&store)),
                None => index.rebuild(&store),
            })
            .await;
            let failed = !matches!(applied, Ok(Ok(())));
            if failed {
                crate::ulog_warn!("[search] Knowledge index update failed");
            }
            change = match changes.recv().await {
                Ok(event) if !failed => Some(event),
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => None,
                Err(broadcast::error::RecvError::Closed) => break,
            };
        }
    }

    fn rebuild_locked(&self, store: &KnowledgeStore, force_reset: bool) -> SearchResult<()> {
        *self.phase.write().map_err(unavailable)? = Phase::Pending;
        let result = self.build_state(store, force_reset);
        match result {
            Ok(state) => {
                *self.state.write().map_err(unavailable)? = Some(state);
                *self.phase.write().map_err(unavailable)? = Phase::Indexed;
                Ok(())
            }
            Err(error) => {
                *self.phase.write().map_err(unavailable)? = Phase::Failed;
                Err(error)
            }
        }
    }

    fn open_index(&self, force_reset: bool) -> SearchResult<IndexState> {
        let (parent, _) =
            ensure_workspace_directory_no_follow(&self.data_dir, "search_index/knowledge")
                .map_err(unavailable)?;
        with_file_lock_blocking(
            &parent.join(".open.lock"),
            FileLockOptions::default(),
            || Ok(self.open_index_locked(force_reset)),
        )
        .map_err(unavailable)?
    }

    fn open_index_locked(&self, force_reset: bool) -> SearchResult<IndexState> {
        let version = KNOWLEDGE_SCHEMA_VERSION.to_string();
        let seen = match read_workspace_file_no_follow_classified(&self.data_dir, VERSION_FILE, 32)
        {
            Ok((_, bytes)) => Some(bytes),
            Err(RegularFileError::Io(_, error)) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(unavailable(error)),
        };
        let ensure = || {
            ensure_workspace_directory_no_follow(&self.data_dir, "search_index/knowledge/bm25")
                .map(|(path, _)| path)
                .map_err(unavailable)
        };
        let mut path = ensure()?;
        if force_reset
            || (seen.as_deref() != Some(version.as_bytes()) && path.join("meta.json").exists())
        {
            Self::reset(&path)?;
            path = ensure()?;
        }
        let state = match IndexState::open(&path) {
            Ok(state) => state,
            Err(error) if corrupt(&error) => {
                Self::reset(&path)?;
                IndexState::open(&ensure()?).map_err(unavailable)?
            }
            Err(error) => return Err(unavailable(error)),
        };
        write_workspace_file_no_follow(&self.data_dir, VERSION_FILE, version.as_bytes())
            .map_err(unavailable)?;
        Ok(state)
    }

    fn reset(path: &Path) -> SearchResult<()> {
        // The path is the validated private BM25 directory. Drop all mmap
        // handles before calling reset, especially on Windows. A different
        // app instance already holding Tantivy's writer must finish first.
        let directory = MmapDirectory::open(path).map_err(unavailable)?;
        let writer_guard = directory
            .acquire_lock(&INDEX_WRITER_LOCK)
            .map_err(unavailable)?;
        drop(writer_guard);
        drop(directory);
        match fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(unavailable(error)),
        }
    }

    fn build_state(&self, store: &KnowledgeStore, force_reset: bool) -> SearchResult<IndexState> {
        drop(self.state.write().map_err(unavailable)?.take());
        let state = self.open_index(force_reset)?;
        {
            let mut writer = state.writer.lock().map_err(unavailable)?;
            writer.delete_all_documents().map_err(unavailable)?;
            let mut statuses = state.statuses.write().map_err(unavailable)?;
            for kb in store.list()? {
                let sources = match store.list_sources(&kb.id) {
                    Ok(sources) => sources,
                    Err(KnowledgeErrorCode::KbNotFound) => continue,
                    Err(error) => return Err(error),
                };
                for source in sources {
                    let status = match project(store, &kb.id, &source.id) {
                        Ok(Some((source, chunks))) => {
                            state
                                .add_source(&writer, &kb.id, &source, chunks)
                                .map_err(unavailable)?;
                            KnowledgeSearchStatus::Indexed
                        }
                        Ok(None) | Err(KnowledgeErrorCode::KbJobConflict) => continue,
                        Err(_) => KnowledgeSearchStatus::Failed,
                    };
                    statuses.insert((kb.id.clone(), source.id), status);
                }
            }
            state.commit(&mut writer).map_err(unavailable)?;
        }
        Ok(state)
    }

    pub(super) fn update(
        &self,
        store: &KnowledgeStore,
        change: &KnowledgeChange,
    ) -> SearchResult<()> {
        let _guard = self.mutation.lock().map_err(unavailable)?;
        let state = self.state.read().map_err(unavailable)?;
        let Some(state) = state.as_ref() else {
            // The observer will rebuild on the next change after failed startup.
            return Err(KnowledgeErrorCode::KbIndexing);
        };
        let mut writer = state.writer.lock().map_err(unavailable)?;
        let mut statuses = state.statuses.write().map_err(unavailable)?;
        let outcome = match change {
            KnowledgeChange::KbRemoved { kb_id } => {
                writer.delete_term(Term::from_field_text(state.fields.kb_id, kb_id));
                state.commit(&mut writer).map_err(unavailable).map(|()| {
                    statuses.retain(|(kb, _), _| kb != kb_id);
                })
            }
            KnowledgeChange::SourceUpserted {
                kb_id, source_id, ..
            }
            | KnowledgeChange::SourceRemoved { kb_id, source_id }
            | KnowledgeChange::MetadataChanged { kb_id, source_id } => {
                Self::update_source(store, state, &mut writer, &mut statuses, kb_id, source_id)
            }
        };
        if outcome.is_err() {
            let _ = writer.rollback();
        }
        outcome
    }

    fn update_source(
        store: &KnowledgeStore,
        state: &IndexState,
        writer: &mut IndexWriter,
        statuses: &mut BTreeMap<SourceKey, KnowledgeSearchStatus>,
        kb: &str,
        source_id: &str,
    ) -> SearchResult<()> {
        let projection = project(store, kb, source_id);
        if projection.as_ref().err() == Some(&KnowledgeErrorCode::KbJobConflict) {
            return Ok(());
        }
        writer.delete_term(Term::from_field_text(
            state.fields.source_key,
            &source_key(kb, source_id),
        ));
        let status = match projection {
            Ok(Some((source, chunks))) => {
                state
                    .add_source(writer, kb, &source, chunks)
                    .map_err(unavailable)?;
                Some(KnowledgeSearchStatus::Indexed)
            }
            Ok(None) => None,
            Err(_) => Some(KnowledgeSearchStatus::Failed),
        };
        state.commit(writer).map_err(unavailable)?;
        let key = (kb.to_owned(), source_id.to_owned());
        if let Some(status) = status {
            statuses.insert(key, status);
        } else {
            statuses.remove(&key);
        }
        Ok(())
    }

    pub(super) fn status(&self, kb: &str, source: &str) -> SearchResult<KnowledgeSearchStatus> {
        if !is_knowledge_base_id(kb) || !is_knowledge_source_id(source) {
            return Err(KnowledgeErrorCode::KbInvalidInput);
        }
        match *self.phase.read().map_err(unavailable)? {
            Phase::Failed => return Ok(KnowledgeSearchStatus::Failed),
            Phase::Pending => return Ok(KnowledgeSearchStatus::Pending),
            Phase::Indexed => {}
        }
        let state = self.state.read().map_err(unavailable)?;
        let Some(state) = state.as_ref() else {
            return Ok(KnowledgeSearchStatus::Pending);
        };
        let statuses = state.statuses.read().map_err(unavailable)?;
        Ok(statuses
            .get(&(kb.into(), source.into()))
            .copied()
            .unwrap_or(KnowledgeSearchStatus::Pending))
    }

    pub(super) fn search(
        &self,
        store: &KnowledgeStore,
        kb_ids: &[String],
        queries: &[String],
        filters: &KnowledgeSearchFilters,
        limit: Option<usize>,
    ) -> SearchResult<KnowledgeSearchResult> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=20).contains(&limit) || kb_ids.iter().any(|kb| !is_knowledge_base_id(kb)) {
            return Err(KnowledgeErrorCode::KbInvalidInput);
        }
        let run = || self.read_state(|state| state.search(kb_ids, queries, filters, limit));
        match run() {
            Ok(result) => Ok(result),
            Err(QueryFailure::Tantivy(error)) if corrupt(&error) => {
                let _guard = self.mutation.lock().map_err(unavailable)?;
                match run() {
                    Err(QueryFailure::Tantivy(error)) if corrupt(&error) => {
                        self.rebuild_locked(store, true)?;
                        run().map_err(QueryFailure::code)
                    }
                    result => result.map_err(QueryFailure::code),
                }
            }
            Err(error) => Err(error.code()),
        }
    }

    fn read_state<T>(
        &self,
        read: impl FnOnce(&IndexState) -> Result<T, QueryFailure>,
    ) -> Result<T, QueryFailure> {
        let to_error = |phase| match phase {
            Phase::Pending => Err(QueryFailure::Invalid(KnowledgeErrorCode::KbIndexing)),
            Phase::Failed => Err(QueryFailure::Invalid(
                KnowledgeErrorCode::KbStoreUnavailable,
            )),
            Phase::Indexed => Ok(()),
        };
        let lock_error = |_| QueryFailure::Invalid(KnowledgeErrorCode::KbStoreUnavailable);
        to_error(*self.phase.read().map_err(lock_error)?)?;
        let state = self
            .state
            .read()
            .map_err(|_| QueryFailure::Invalid(KnowledgeErrorCode::KbStoreUnavailable))?;
        to_error(*self.phase.read().map_err(lock_error)?)?;
        read(
            state
                .as_ref()
                .ok_or(QueryFailure::Invalid(KnowledgeErrorCode::KbIndexing))?,
        )
    }
}

impl QueryFailure {
    fn code(self) -> KnowledgeErrorCode {
        match self {
            Self::Invalid(code) => code,
            Self::Tantivy(error) => unavailable(error),
        }
    }
}

fn fuse(rankings: Vec<Vec<KnowledgeSearchHit>>, limit: usize) -> Vec<KnowledgeSearchHit> {
    let mut hits = BTreeMap::<String, KnowledgeSearchHit>::new();
    for ranking in rankings {
        let mut seen = BTreeSet::new();
        for (rank, mut hit) in ranking.into_iter().enumerate() {
            if !seen.insert(hit.reference.clone()) {
                continue;
            }
            let score = 1.0 / (RRF_K + rank as f64 + 1.0);
            let entry = hits.entry(hit.reference.clone()).or_insert_with(|| {
                hit.score = 0.0;
                hit
            });
            entry.score += score;
        }
    }
    let mut hits: Vec<_> = hits.into_values().collect();
    hits.sort_unstable_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.reference.cmp(&b.reference))
    });
    let mut counts = BTreeMap::new();
    hits.into_iter()
        .filter(|hit| {
            let count = counts
                .entry((hit.kb_id.clone(), hit.source_id.clone()))
                .or_insert(0);
            *count += 1;
            *count <= SOURCE_LIMIT
        })
        .take(limit)
        .collect()
}

impl IndexState {
    fn open(path: &Path) -> Result<Self, TantivyError> {
        let (schema, fields) = knowledge_schema();
        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(path)?
        } else {
            Index::create_in_dir(path, schema)?
        };
        index.tokenizers().register(
            tokenizer::TOKENIZER_NAME,
            tokenizer::build_chinese_tokenizer(),
        );
        let writer = index.writer_with_num_threads(1, 50_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
            statuses: RwLock::new(BTreeMap::new()),
        })
    }

    fn base_document(&self, kb: &str, source: &KnowledgeSource, kind: &str) -> TantivyDocument {
        let f = &self.fields;
        let mut doc = TantivyDocument::default();
        doc.add_text(f.kb_id, kb);
        doc.add_text(f.source_id, &source.id);
        doc.add_text(f.source_key, source_key(kb, &source.id));
        doc.add_text(f.doc_kind, kind);
        let source_kind = match source.facts.kind {
            SourceKind::File => "file",
            SourceKind::Note => "note",
            SourceKind::Wiki => "wiki",
            SourceKind::LinkedFile => "linked-file",
        };
        doc.add_text(f.source_kind, source_kind);
        doc.add_text(f.title, source.display_title());
        if let Some(tags) = source
            .metadata
            .get(&MetadataKey::Tags)
            .and_then(|f| f.value.as_array())
        {
            for tag in tags.iter().filter_map(|tag| tag.as_str()) {
                add_text(&mut doc, f.tags, tag);
                add_text(&mut doc, f.tag_filter, tag);
            }
        }
        for (key, field) in [
            (MetadataKey::Category, f.category),
            (MetadataKey::DocType, f.doc_type),
        ] {
            if let Some(value) = source.metadata.get(&key).and_then(|f| f.value.as_str()) {
                add_text(&mut doc, field, value);
            }
        }
        doc
    }

    fn add_source(
        &self,
        writer: &IndexWriter,
        kb: &str,
        source: &KnowledgeSource,
        chunks: Vec<KnowledgeChunk>,
    ) -> Result<(), TantivyError> {
        let mut summary = self.base_document(kb, source, SUMMARY);
        if let Some(body) = source
            .metadata
            .get(&MetadataKey::Summary)
            .and_then(|field| field.value.as_str())
        {
            add_text(&mut summary, self.fields.text, body);
        }
        writer.add_document(summary)?;
        let base = self.base_document(kb, source, CHUNK);
        for chunk in chunks {
            let mut doc = base.clone();
            doc.add_text(self.fields.chunk_id, &chunk.chunk_id);
            add_text(&mut doc, self.fields.text, &chunk.text);
            add_text(&mut doc, self.fields.prefix, &chunk.prefix);
            if let Some(page) = chunk.page {
                doc.add_u64(self.fields.page, u64::from(page));
            }
            writer.add_document(doc)?;
        }
        Ok(())
    }

    fn commit(&self, writer: &mut IndexWriter) -> Result<(), TantivyError> {
        writer.commit()?;
        self.reader.reload()
    }

    fn query(
        &self,
        kb_ids: &[String],
        input: &str,
        filters: &KnowledgeSearchFilters,
        kind: &str,
        related: &[String],
    ) -> SearchResult<Box<dyn Query>> {
        let f = &self.fields;
        let mut parser =
            QueryParser::for_index(&self.index, vec![f.title, f.text, f.prefix, f.tags]);
        parser.set_field_boost(f.title, 2.0);
        let query = parser
            .parse_query(input)
            .map_err(|_| KnowledgeErrorCode::KbInvalidInput)?;
        let query = if related.is_empty() {
            query
        } else {
            Box::new(BooleanQuery::new(vec![
                (Occur::Should, query),
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(exact(f.source_key, related), 0.01)),
                ),
            ]))
        };
        let mut clauses = vec![
            (Occur::Must, query),
            (Occur::Must, exact(f.kb_id, kb_ids)),
            (Occur::Must, exact(f.doc_kind, [kind])),
        ];
        for (field, val) in [
            (f.tag_filter, &filters.tag),
            (f.category, &filters.category),
            (f.doc_type, &filters.doc_type),
        ] {
            if let Some(val) = val {
                clauses.push((Occur::Must, exact(field, [val])));
            }
        }
        Ok(Box::new(BooleanQuery::new(clauses)))
    }

    fn search(
        &self,
        kb_ids: &[String],
        queries: &[String],
        filters: &KnowledgeSearchFilters,
        limit: usize,
    ) -> Result<KnowledgeSearchResult, QueryFailure> {
        let mut response = KnowledgeSearchResult {
            mode: "bm25",
            hits: Vec::new(),
        };
        if kb_ids.is_empty() {
            return Ok(response);
        }
        let searcher = self.reader.searcher();
        let mut rankings = Vec::with_capacity(queries.len());
        for input in queries
            .iter()
            .map(|query| query.trim())
            .filter(|query| !query.is_empty())
        {
            let needle = input.trim_matches('"').to_lowercase();
            let summary_query = self
                .query(kb_ids, input, filters, SUMMARY, &[])
                .map_err(QueryFailure::Invalid)?;
            let mut related = Vec::new();
            for (_, address) in
                searcher.search(&summary_query, &TopDocs::with_limit(RETRIEVAL_LIMIT))?
            {
                let doc = searcher.doc::<TantivyDocument>(address)?;
                related.push(source_key(
                    text(&doc, self.fields.kb_id),
                    text(&doc, self.fields.source_id),
                ));
            }
            let chunk_query = self
                .query(kb_ids, input, filters, CHUNK, &related)
                .map_err(QueryFailure::Invalid)?;
            let mut candidates = Vec::new();
            for (_, address) in
                searcher.search(&chunk_query, &TopDocs::with_limit(RETRIEVAL_LIMIT))?
            {
                let doc = searcher.doc::<TantivyDocument>(address)?;
                candidates.push(
                    self.hit(&doc, &needle)
                        .ok_or(QueryFailure::Invalid(KnowledgeErrorCode::KbStoreCorrupt))?,
                );
            }
            rankings.push(candidates);
        }
        response.hits = fuse(rankings, limit);
        Ok(response)
    }

    fn hit(&self, doc: &TantivyDocument, needle: &str) -> Option<KnowledgeSearchHit> {
        let f = &self.fields;
        let kb_id = text(doc, f.kb_id).to_string();
        let source_id = text(doc, f.source_id).to_string();
        let chunk_id = text(doc, f.chunk_id).to_string();
        let page = match doc.get_first(f.page) {
            Some(OwnedValue::U64(page)) => u32::try_from(*page).ok(),
            _ => None,
        };
        let reference = format_knowledge_ref(&KnowledgeRef {
            kb_id: kb_id.clone(),
            source_id: source_id.clone(),
            chunk_id: Some(chunk_id.clone()),
            page,
        })?;
        Some(KnowledgeSearchHit {
            reference,
            kb_id,
            source_id,
            chunk_id,
            title: text(doc, f.title).into(),
            prefix: text(doc, f.prefix).into(),
            page,
            snippet: super::make_snippet(text(doc, f.text), needle, 300),
            score: 0.0,
        })
    }
}
