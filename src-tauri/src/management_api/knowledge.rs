use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use axum::{extract::Path as RoutePath, http::HeaderMap, Json};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use tauri::{async_runtime::spawn_blocking, Manager};

use super::{get_sidecar_state, no_store_json, request_sidecar_generation};
use crate::knowledge::{
    ingest::KnowledgeIngestManager,
    read::{self, ReadRequest},
    store::{KnowledgeStore, MetadataKey, MetadataPatch, StoreResult},
    types::{
        format_knowledge_ref, is_knowledge_base_id, parse_knowledge_ref, KnowledgeErrorCode,
        KnowledgeRef,
    },
};
use crate::search::{KnowledgeSearchFilters, SearchEngine};
use crate::sidecar::SessionProcessSource;
use crate::workspace_files::path_safety::read_workspace_file_strict;
use KnowledgeErrorCode::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    sidecar_id: String,
    request: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Search {
    queries: Vec<String>,
    #[serde(default)]
    kb_ids: Vec<String>,
    #[serde(default)]
    filters: KnowledgeSearchFilters,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Get {
    reference: String,
    #[serde(default)]
    context: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Add {
    file: String,
    kb_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Status {
    job_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Metadata {
    reference: String,
    summary_file: Option<String>,
    tags: Option<Vec<String>>,
    category: Option<String>,
    doc_type: Option<String>,
    date: Option<String>,
    entities_file: Option<String>,
    duplicate_of: Option<String>,
}

struct Scope(Vec<String>);

impl Scope {
    fn require(&self, id: &str) -> StoreResult<()> {
        if !is_knowledge_base_id(id) {
            return Err(KbInvalidInput);
        }
        if self.0.iter().any(|bound| bound == id) {
            Ok(())
        } else {
            Err(KbAccessDenied)
        }
    }

    fn select(&self, requested: Vec<String>) -> StoreResult<Vec<String>> {
        for kb in &requested {
            self.require(kb)?;
        }
        if requested.is_empty() {
            Ok(self.0.clone())
        } else {
            Ok(requested)
        }
    }

    fn reference(&self, value: &str) -> StoreResult<KnowledgeRef> {
        let reference = parse_knowledge_ref(value).ok_or(KbInvalidInput)?;
        self.require(&reference.kb_id)?;
        Ok(reference)
    }

    fn import_target(&self, requested: Option<String>) -> StoreResult<String> {
        if let Some(id) = requested {
            self.require(&id)?;
            return Ok(id);
        }
        match self.0.as_slice() {
            [id] => Ok(id.clone()),
            [] => Err(KbAccessDenied),
            _ => Err(KbInvalidInput),
        }
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> StoreResult<T> {
    serde_json::from_value(value).map_err(|_| KbInvalidInput)
}

fn resolve_caller(sidecar_id: &str, generation: u64) -> StoreResult<SessionProcessSource> {
    let manager = get_sidecar_state().ok_or(KbStoreUnavailable)?;
    let caller = manager
        .lock()
        .map_err(|_| KbStoreUnavailable)?
        .resolve_session_process_source(sidecar_id, generation);
    caller.ok_or(KbAccessDenied)
}

async fn curator_is_running(kb: &str, session: &str) -> bool {
    let Some(store) = crate::task::get_task_store() else {
        return false;
    };
    let Some(task) = store.knowledge_curator_task(kb).await else {
        return false;
    };
    crate::task_scheduler::get_task_scheduler()
        .owns_running_session(&task, session)
        .await
}

async fn blocking(
    operation: impl FnOnce() -> StoreResult<Value> + Send + 'static,
) -> StoreResult<Value> {
    spawn_blocking(operation)
        .await
        .map_err(|_| KbStoreUnavailable)?
}

pub(super) async fn handle(
    RoutePath(action): RoutePath<String>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> (HeaderMap, Json<Value>) {
    match dispatch(&action, &headers, request).await {
        Ok(data) => no_store_json(data),
        Err(code) => no_store_json(error_response(
            code,
            matches!(action.as_str(), "add" | "meta-set"),
        )),
    }
}

async fn dispatch(action: &str, headers: &HeaderMap, request: Value) -> StoreResult<Value> {
    let Request {
        sidecar_id,
        request,
    } = decode(request)?;
    let generation = request_sidecar_generation(headers).map_err(|_| KbAccessDenied)?;
    let caller = resolve_caller(&sidecar_id, generation)?;
    let app = crate::logger::get_app_handle().ok_or(KbStoreUnavailable)?;
    let store = app
        .try_state::<Arc<KnowledgeStore>>()
        .ok_or(KbStoreUnavailable)?;
    let store = Arc::clone(store.inner());
    let data_dir = crate::app_dirs::myagents_data_dir().ok_or(KbStoreUnavailable)?;
    let session = caller.product_session_id.clone();
    let scope = spawn_blocking(move || {
        crate::session_metadata::knowledge_base_ids_for_session(&data_dir, &session).map(Scope)
    })
    .await
    .map_err(|_| KbStoreUnavailable)??;
    match action {
        "list" => {
            if request.as_object().is_none_or(|value| !value.is_empty()) {
                return Err(KbInvalidInput);
            }
            blocking(move || list(&store, &scope)).await
        }
        "search" => {
            let query: Search = decode(request)?;
            if query.queries.is_empty()
                || query.queries.len() > 16
                || query
                    .queries
                    .iter()
                    .any(|text| text.trim().is_empty() || text.len() > 4096)
                || query.limit.is_some_and(|limit| !(1..=20).contains(&limit))
            {
                return Err(KbInvalidInput);
            }
            let kb_ids = scope.select(query.kb_ids)?;
            if kb_ids.is_empty() {
                return Ok(json!({"ok": true, "mode": "bm25", "hits": []}));
            }
            let engine = app
                .try_state::<Arc<SearchEngine>>()
                .ok_or(KbStoreUnavailable)?;
            let result = engine
                .search_knowledge(store, kb_ids, query.queries, query.filters, query.limit)
                .await?;
            Ok(json!({"ok": true, "mode": result.mode, "hits": result.hits}))
        }
        "get" => {
            let input: Get = decode(request)?;
            let reference = scope.reference(&input.reference)?;
            blocking(move || get_chunks(&store, reference, input.context)).await
        }
        "read" => {
            let input: ReadRequest = decode(request)?;
            scope.reference(&input.reference)?;
            blocking(move || {
                let page =
                    read::read_page(&store, (&caller.product_session_id, generation), input)?;
                let mut data = serde_json::to_value(page).map_err(|_| KbStoreUnavailable)?;
                data["ok"] = json!(true);
                Ok(data)
            })
            .await
        }
        "add" => {
            let input: Add = decode(request)?;
            let kb = scope.import_target(input.kb_id)?;
            let curator = curator_is_running(&kb, &caller.product_session_id).await;
            let ingest = app
                .try_state::<Arc<KnowledgeIngestManager>>()
                .ok_or(KbStoreUnavailable)?;
            let ingest = Arc::clone(ingest.inner());
            blocking(move || {
                let receipt = ingest.import_workspace_file(
                    &kb,
                    &caller.workspace_path,
                    Path::new(&input.file),
                    curator,
                )?;
                Ok(json!({"ok": true, "receipt": receipt}))
            })
            .await
        }
        "meta-set" => {
            let input: Metadata = decode(request)?;
            let reference = scope.reference(&input.reference)?;
            if reference.chunk_id.is_some() {
                return Err(KbInvalidInput);
            }
            let curator = curator_is_running(&reference.kb_id, &caller.product_session_id).await;
            blocking(move || {
                store.check_agent_write(&reference.kb_id, curator)?;
                let fields =
                    metadata_patch(&store, input, &scope, &reference, &caller.workspace_path)?;
                let outcomes = store.set_agent_metadata(
                    &reference.kb_id,
                    &reference.source_id,
                    fields,
                    curator,
                )?;
                Ok(json!({"ok": true, "outcomes": outcomes}))
            })
            .await
        }
        "status" => {
            let input: Status = decode(request)?;
            let engine = app
                .try_state::<Arc<SearchEngine>>()
                .ok_or(KbStoreUnavailable)?;
            let engine = Arc::clone(engine.inner());
            blocking(move || {
                let job = store.ingest_job(&input.job_id)?;
                scope.require(&job.kb_id)?;
                let status = engine.knowledge_search_status(&job.kb_id, &job.source_id)?;
                Ok(json!({"ok": true, "job": job, "searchStatus": status}))
            })
            .await
        }
        _ => Err(KbInvalidInput),
    }
}

fn list(store: &KnowledgeStore, scope: &Scope) -> StoreResult<Value> {
    let mut libraries = Vec::new();
    // The current Library registry can remove bindings still present in an
    // older Session snapshot. Return only live rows the snapshot authorizes.
    for kb in store
        .list()?
        .into_iter()
        .filter(|kb| scope.require(&kb.id).is_ok())
    {
        let source_count = store.list_sources(&kb.id)?.len();
        libraries.push(json!({
            "id": kb.id, "name": kb.name, "description": kb.description, "sourceCount": source_count,
        }));
    }
    Ok(json!({"ok": true, "mode": "bm25", "libraries": libraries}))
}

fn get_chunks(
    store: &KnowledgeStore,
    reference: KnowledgeRef,
    context: usize,
) -> StoreResult<Value> {
    let chunk_id = reference.chunk_id.as_deref().ok_or(KbInvalidInput)?;
    if context > 3 {
        return Err(KbInvalidInput);
    }
    let source_ref = format_knowledge_ref(&KnowledgeRef {
        chunk_id: None,
        ..reference.clone()
    })
    .ok_or(KbInvalidInput)?;
    let projection = store.search_document(&reference.kb_id, &reference.source_id)?;
    let Some(index) = projection
        .as_ref()
        .and_then(|(_, chunks)| chunks.iter().position(|chunk| chunk.chunk_id == chunk_id))
    else {
        return Ok(
            json!({"ok": true, "fragmentStatus": "unavailable", "sourceReference": source_ref}),
        );
    };
    let (_, chunks) = projection.ok_or(KbStoreUnavailable)?;
    let mut result = Vec::new();
    for chunk in &chunks[index.saturating_sub(context)..(index + context + 1).min(chunks.len())] {
        let reference = format_knowledge_ref(&KnowledgeRef {
            chunk_id: Some(chunk.chunk_id.clone()),
            page: chunk.page,
            ..reference.clone()
        })
        .ok_or(KbStoreCorrupt)?;
        let end = chunk
            .text
            .char_indices()
            .nth(300)
            .map_or(chunk.text.len(), |(offset, _)| offset);
        result.push(json!({
            "reference": reference, "snippet": &chunk.text[..end], "page": chunk.page,
            "prefix": chunk.prefix, "ordinal": chunk.ordinal, "byteRange": chunk.byte_range,
            "text": chunk.text, "utf16Range": chunk.utf16_range, "headingPath": chunk.heading_path,
        }));
    }
    Ok(
        json!({"ok": true, "fragmentStatus": "available", "sourceReference": source_ref, "chunks": result}),
    )
}

fn metadata_patch(
    store: &KnowledgeStore,
    input: Metadata,
    scope: &Scope,
    source: &KnowledgeRef,
    workspace: &Path,
) -> StoreResult<MetadataPatch> {
    let mut fields = BTreeMap::new();
    for (key, value) in [
        (MetadataKey::Category, input.category),
        (MetadataKey::DocType, input.doc_type),
    ] {
        if let Some(value) = value {
            if value.trim().is_empty() {
                return Err(KbInvalidInput);
            }
            fields.insert(key, json!(value));
        }
    }
    if let Some(tags) = input.tags {
        if tags.iter().any(|tag| tag.trim().is_empty()) {
            return Err(KbInvalidInput);
        }
        fields.insert(MetadataKey::Tags, json!(tags));
    }
    if let Some(date) = input.date {
        let parsed =
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|_| KbInvalidInput)?;
        if date.len() != 10
            || date.starts_with("0000")
            || parsed.format("%Y-%m-%d").to_string() != date
        {
            return Err(KbInvalidInput);
        }
        fields.insert(MetadataKey::Date, json!(date));
    }
    if let Some(path) = input.summary_file {
        let bytes =
            read_workspace_file_strict(workspace, &path, 64 * 1024).map_err(|_| KbInvalidInput)?;
        let text = String::from_utf8(bytes).map_err(|_| KbInvalidInput)?;
        if text.contains('\0') {
            return Err(KbInvalidInput);
        }
        fields.insert(MetadataKey::Summary, json!(text));
    }
    if let Some(path) = input.entities_file {
        let bytes =
            read_workspace_file_strict(workspace, &path, 64 * 1024).map_err(|_| KbInvalidInput)?;
        let entities: Vec<String> = serde_json::from_slice(&bytes).map_err(|_| KbInvalidInput)?;
        fields.insert(MetadataKey::Entities, json!(entities));
    }
    if let Some(duplicate) = input.duplicate_of {
        let reference = scope.reference(&duplicate)?;
        if reference.page.is_some()
            || reference.chunk_id.is_some()
            || (reference.kb_id == source.kb_id && reference.source_id == source.source_id)
        {
            return Err(KbInvalidInput);
        }
        store.get_source(&reference.kb_id, &reference.source_id)?;
        fields.insert(MetadataKey::DuplicateOf, json!(duplicate));
    }
    if fields.is_empty() {
        return Err(KbInvalidInput);
    }
    Ok(fields)
}

fn error_response(code: KnowledgeErrorCode, write: bool) -> Value {
    let (error, suggestion) = match code {
        KbAccessDenied if write => (
            "Knowledge write access denied.",
            "Check the library's Agent write access in Knowledge settings.",
        ),
        KbAccessDenied => (
            "Knowledge access denied.",
            "Use a live Session and a knowledge library bound to it.",
        ),
        KbNotFound => (
            "The requested knowledge item is unavailable.",
            "Refresh the list of sources or jobs.",
        ),
        KbIndexing => (
            "The knowledge index is being built.",
            "Retry after indexing finishes.",
        ),
        KbInvalidInput => (
            "Invalid knowledge command input.",
            "Check the command options and Workspace paths.",
        ),
        KbSourceInUse => ("The source file is in use.", "Close the file and retry."),
        KbSourceUnsupported => (
            "The source format is unsupported.",
            "Choose a supported document.",
        ),
        _ => (
            "The knowledge operation could not finish.",
            "Check the source or job status before retrying.",
        ),
    };
    json!({
        "ok": false,
        "code": code,
        "error": error,
        "suggestion": suggestion,
        "recoveryHint": {"message": suggestion}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::store::{AgentWriteAccess, KnowledgeBasePatch};

    #[test]
    fn knowledge_api_rejects_unbound_ids_and_client_reported_authority() {
        let a = "kb_0123456789ab";
        let b = "kb_ffffffffffff";
        let scope = Scope(vec![a.into()]);
        assert_eq!(scope.select(Vec::new()).unwrap(), vec![a.to_string()]);
        assert_eq!(scope.select(vec![a.into(), b.into()]), Err(KbAccessDenied));
        assert!(matches!(
            scope.reference(&format!("kb://{b}/src_0123456789abcdef")),
            Err(KbAccessDenied)
        ));
        assert!(decode::<Request>(json!({
            "sidecarId": "internal-sidecar", "request": {}, "sessionId": "client-session"
        }))
        .is_err());
        assert!(decode::<Add>(json!({
            "file": "note.txt", "workspacePath": "forged workspace"
        }))
        .is_err());
    }

    #[test]
    fn knowledge_api_scope_reads_only_the_exact_published_session_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let kb = "kb_0123456789ab";
        let other_kb = "kb_ffffffffffff";
        let path = tmp.path().join("sessions.json");
        std::fs::write(&path, json!([
            {"id": "legacy"},
            {"id": "bound", "knowledgeBaseIds": [kb, kb]},
            {"id": "other", "knowledgeBaseIds": [other_kb]},
            {"id": "prepared", "materializationState": "prepared", "knowledgeBaseIds": [other_kb]}
        ]).to_string()).unwrap();
        let read =
            |session| crate::session_metadata::knowledge_base_ids_for_session(tmp.path(), session);
        assert_eq!(read("bound").unwrap(), vec![kb]);
        assert!(read("legacy").unwrap().is_empty());
        assert!(read("prepared").unwrap().is_empty());
        assert!(read("unknown").unwrap().is_empty());
        std::fs::write(
            &path,
            json!([
                {"id": "bound", "knowledgeBaseIds": [kb]},
                {"id": "bound", "knowledgeBaseIds": [other_kb]}
            ])
            .to_string(),
        )
        .unwrap();
        assert_eq!(read("bound"), Err(KbStoreCorrupt));
    }

    #[test]
    fn knowledge_api_import_denies_relative_absolute_and_link_escapes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let workspace = root.join("workspace");
        let outside = root.join("outside");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let inside = workspace.join("inside");
        std::fs::create_dir(&inside).unwrap();
        let inside_file = inside.join("legitimate.txt");
        std::fs::write(&inside_file, "legitimate").unwrap();
        let file = outside.join("note.txt");
        std::fs::write(&file, "unchanged").unwrap();
        let link = workspace.join("linked");
        let in_scope_link = workspace.join("inside-link");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            std::os::unix::fs::symlink(&inside, &in_scope_link).unwrap();
        }
        #[cfg(windows)]
        {
            junction::create(&outside, &link).unwrap();
            junction::create(&inside, &in_scope_link).unwrap();
        }

        let store = Arc::new(KnowledgeStore::new(root.join("app")));
        let kb = store
            .create(KnowledgeBasePatch {
                name: Some("Imported files".into()),
                agent_write_access: Some(AgentWriteAccess::BoundSessions),
                ..Default::default()
            })
            .unwrap();
        let ingest = Arc::new(KnowledgeIngestManager::new(Arc::clone(&store), None));
        for escaped in [
            Path::new("../outside/note.txt"),
            &file,
            Path::new("linked/note.txt"),
            Path::new("inside-link/legitimate.txt"),
        ] {
            assert!(ingest
                .import_workspace_file(&kb.id, &workspace, escaped, false)
                .is_err());
        }
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "unchanged");
        assert!(store.list_sources(&kb.id).unwrap().is_empty());
        assert_eq!(
            read_workspace_file_strict(&workspace, inside_file.to_str().unwrap(), 64).unwrap(),
            b"legitimate"
        );
        ingest.shutdown().unwrap();
        #[cfg(windows)]
        {
            junction::delete(&link).unwrap();
            junction::delete(&in_scope_link).unwrap();
        }
    }
}
