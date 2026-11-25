use std::path::PathBuf;
use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tauri::{async_runtime::spawn_blocking, AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast::error::RecvError;

use super::chunker::KnowledgeChunk;
use super::ingest::{ImportFailure, ImportOutcome, KnowledgeIngestManager};
use super::linked::{KnowledgeLinkedManager, LinkedFolderScanSummary};
use super::organize::{ApprovalDecision, KnowledgeOrganizeManager, OrganizePlan, UndoReport};
use super::store::ingest::IngestJob;
use super::store::{
    KnowledgeBase, KnowledgeBasePatch, KnowledgeBaseStats, KnowledgeSource, KnowledgeSourceDetail,
    KnowledgeStore, KnowledgeUiEvent, LinkedFolder, LintReportEntry, MetadataKey, MetadataOutcome,
    MetadataPatch, ResolvedKnowledgeRef, RuleDocKind, StoreResult, WikiPageEntry,
};
use super::types::{KnowledgeErrorCode, KnowledgeMetadataOrigin};
use crate::search::{
    KnowledgeSearchFilters, KnowledgeSearchResult, KnowledgeSearchStatus, SearchEngine,
};

pub const KNOWLEDGE_CHANGED_EVENT: &str = "knowledge:changed";

/// Forward Store UI invalidation events to WebView listeners. Carries only IDs
/// and kind so UI views can refetch from Rust Store without keeping a second copy.
pub fn spawn_ui_event_forwarder(app: AppHandle, store: Arc<KnowledgeStore>) {
    let mut rx = store.subscribe_ui_events();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let _ = app.emit(KNOWLEDGE_CHANGED_EVENT, event);
                }
                Err(RecvError::Lagged(_)) => {
                    let _ = app.emit(
                        KNOWLEDGE_CHANGED_EVENT,
                        KnowledgeUiEvent {
                            kind: "resync",
                            kb_id: String::new(),
                            source_id: None,
                        },
                    );
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceView {
    #[serde(flatten)]
    pub detail: KnowledgeSourceDetail,
    pub search_status: KnowledgeSearchStatus,
}

async fn run_store<T: Send + 'static>(
    state: State<'_, Arc<KnowledgeStore>>,
    operation: impl FnOnce(&KnowledgeStore) -> StoreResult<T> + Send + 'static,
) -> StoreResult<T> {
    let store = Arc::clone(state.inner());
    spawn_blocking(move || operation(&store))
        .await
        .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

async fn run_ingest<T: Send + 'static>(
    state: State<'_, Arc<KnowledgeIngestManager>>,
    operation: impl FnOnce(Arc<KnowledgeIngestManager>) -> StoreResult<T> + Send + 'static,
) -> Result<T, ImportFailure> {
    let manager = Arc::clone(state.inner());
    spawn_blocking(move || operation(manager))
        .await
        .map_err(|_| ImportFailure::from(KnowledgeErrorCode::KbStoreUnavailable))?
        .map_err(ImportFailure::from)
}

#[tauri::command]
pub async fn cmd_kb_import_files(
    state: State<'_, Arc<KnowledgeIngestManager>>,
    kb_id: String,
    paths: Vec<PathBuf>,
) -> Result<Vec<ImportOutcome>, ImportFailure> {
    run_ingest(
        state,
        move |manager| Ok(manager.import_files(&kb_id, paths)),
    )
    .await
}

#[tauri::command]
pub async fn cmd_kb_job_status(
    state: State<'_, Arc<KnowledgeIngestManager>>,
    job_id: String,
) -> Result<IngestJob, ImportFailure> {
    run_ingest(state, move |manager| manager.status(&job_id)).await
}

#[tauri::command]
pub async fn cmd_kb_cancel_job(
    state: State<'_, Arc<KnowledgeIngestManager>>,
    job_id: String,
) -> Result<IngestJob, ImportFailure> {
    run_ingest(state, move |manager| manager.cancel(&job_id)).await
}

#[tauri::command]
pub async fn cmd_kb_retry_source(
    state: State<'_, Arc<KnowledgeIngestManager>>,
    kb_id: String,
    source_id: String,
) -> Result<IngestJob, ImportFailure> {
    run_ingest(state, move |manager| manager.retry(&kb_id, &source_id)).await
}

// Map invalid invocation bodies to the same stable code as Store validation.
fn decode<T: DeserializeOwned>(input: Value) -> StoreResult<T> {
    serde_json::from_value(input).map_err(|_| KnowledgeErrorCode::KbInvalidInput)
}

#[tauri::command]
pub async fn cmd_kb_list(state: State<'_, Arc<KnowledgeStore>>) -> StoreResult<Vec<KnowledgeBase>> {
    run_store(state, KnowledgeStore::list).await
}

#[tauri::command]
pub async fn cmd_kb_create(
    state: State<'_, Arc<KnowledgeStore>>,
    settings: Value,
) -> StoreResult<KnowledgeBase> {
    let settings: KnowledgeBasePatch = decode(settings)?;
    super::curator_task::create(Arc::clone(state.inner()), settings).await
}

#[tauri::command]
pub async fn cmd_kb_update(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    settings: Value,
) -> StoreResult<KnowledgeBase> {
    let settings: KnowledgeBasePatch = decode(settings)?;
    super::curator_task::update(Arc::clone(state.inner()), kb_id, settings).await
}

#[tauri::command]
pub async fn cmd_kb_delete(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<()> {
    super::curator_task::delete(Arc::clone(state.inner()), kb_id).await
}

#[tauri::command]
pub async fn cmd_kb_curate_now(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<String> {
    super::curator_task::run_now(Arc::clone(state.inner()), kb_id).await
}

#[tauri::command]
pub async fn cmd_kb_list_sources(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<Vec<KnowledgeSource>> {
    run_store(state, move |store| store.list_sources(&kb_id)).await
}

#[tauri::command]
pub async fn cmd_kb_get_source(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    source_id: String,
) -> StoreResult<KnowledgeSource> {
    run_store(state, move |store| store.get_source(&kb_id, &source_id)).await
}

#[tauri::command]
pub async fn cmd_kb_remove_source(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    source_id: String,
) -> StoreResult<()> {
    run_store(state, move |store| store.remove_source(&kb_id, &source_id)).await
}

#[tauri::command]
pub async fn cmd_kb_set_metadata(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    source_id: String,
    fields: Value,
) -> StoreResult<std::collections::BTreeMap<MetadataKey, MetadataOutcome>> {
    let fields: MetadataPatch = decode(fields)?;
    run_store(state, move |store| {
        store.set_metadata(&kb_id, &source_id, KnowledgeMetadataOrigin::User, fields)
    })
    .await
}

#[tauri::command]
pub async fn cmd_kb_resolve_ref(
    state: State<'_, Arc<KnowledgeStore>>,
    reference: String,
) -> StoreResult<ResolvedKnowledgeRef> {
    run_store(state, move |store| store.resolve_ref(&reference)).await
}

#[tauri::command]
pub async fn cmd_kb_confirm_category(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    category: String,
) -> StoreResult<()> {
    run_store(state, move |store| {
        store.confirm_category(&kb_id, &category)
    })
    .await
}

#[tauri::command]
pub async fn cmd_kb_dismiss_category(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    category: String,
) -> StoreResult<()> {
    run_store(state, move |store| {
        store.dismiss_category(&kb_id, &category)
    })
    .await
}

#[tauri::command]
pub async fn cmd_kb_stats(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<KnowledgeBaseStats> {
    run_store(state, move |store| store.stats(&kb_id)).await
}

#[tauri::command]
pub async fn cmd_kb_list_source_views(
    app: AppHandle,
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<Vec<KnowledgeSourceView>> {
    let store = Arc::clone(state.inner());
    let search = app.try_state::<Arc<SearchEngine>>().map(|s| Arc::clone(s.inner()));
    spawn_blocking(move || {
        let details = store.list_source_details(&kb_id)?;
        let views = details
            .into_iter()
            .map(|detail| {
                let search_status = search
                    .as_ref()
                    .and_then(|engine| engine.knowledge_search_status(&kb_id, &detail.source.id).ok())
                    .unwrap_or(KnowledgeSearchStatus::Pending);
                KnowledgeSourceView {
                    detail,
                    search_status,
                }
            })
            .collect();
        Ok(views)
    })
    .await
    .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

#[tauri::command]
pub async fn cmd_kb_list_categories(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<Vec<String>> {
    run_store(state, move |store| store.list_categories(&kb_id)).await
}

#[tauri::command]
pub async fn cmd_kb_list_chunks(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    source_id: String,
) -> StoreResult<Vec<KnowledgeChunk>> {
    run_store(state, move |store| store.list_chunks(&kb_id, &source_id)).await
}

#[tauri::command]
pub async fn cmd_kb_unlock_metadata(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    source_id: String,
    key: Value,
) -> StoreResult<KnowledgeSource> {
    let key: MetadataKey = decode(key)?;
    run_store(state, move |store| {
        store.unlock_metadata_field(&kb_id, &source_id, key)
    })
    .await
}

#[tauri::command]
pub async fn cmd_kb_search(
    app: AppHandle,
    state: State<'_, Arc<KnowledgeStore>>,
    kb_ids: Vec<String>,
    queries: Vec<String>,
    filters: Option<Value>,
    limit: Option<usize>,
) -> StoreResult<KnowledgeSearchResult> {
    let filters: KnowledgeSearchFilters = match filters {
        Some(value) => decode(value)?,
        None => KnowledgeSearchFilters::default(),
    };
    let search = app
        .try_state::<Arc<SearchEngine>>()
        .map(|s| Arc::clone(s.inner()))
        .ok_or(KnowledgeErrorCode::KbStoreUnavailable)?;
    search
        .search_knowledge(Arc::clone(state.inner()), kb_ids, queries, filters, limit)
        .await
}

#[tauri::command]
pub async fn cmd_kb_list_wiki_pages(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<Vec<WikiPageEntry>> {
    run_store(state, move |store| store.list_wiki_pages(&kb_id)).await
}

#[tauri::command]
pub async fn cmd_kb_read_wiki_page(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    page: String,
) -> StoreResult<String> {
    run_store(state, move |store| store.read_wiki_page(&kb_id, &page)).await
}

#[tauri::command]
pub async fn cmd_kb_list_lint_reports(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<Vec<LintReportEntry>> {
    run_store(state, move |store| store.list_lint_reports(&kb_id)).await
}

#[tauri::command]
pub async fn cmd_kb_read_rule_doc(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    doc: Value,
) -> StoreResult<Option<String>> {
    let doc: RuleDocKind = decode(doc)?;
    run_store(state, move |store| store.read_rule_doc(&kb_id, doc)).await
}

#[tauri::command]
pub async fn cmd_kb_write_rule_doc(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    doc: Value,
    content: String,
) -> StoreResult<()> {
    let doc: RuleDocKind = decode(doc)?;
    run_store(state, move |store| {
        store.write_rule_doc(&kb_id, doc, &content)
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedFolderAttachResult {
    pub folder: LinkedFolder,
    pub summary: LinkedFolderScanSummary,
}

#[tauri::command]
pub async fn cmd_kb_add_linked_folder(
    state: State<'_, Arc<KnowledgeLinkedManager>>,
    kb_id: String,
    path: PathBuf,
) -> StoreResult<LinkedFolderAttachResult> {
    let manager = Arc::clone(state.inner());
    spawn_blocking(move || {
        let (folder, summary) = manager.add_linked_folder(&kb_id, &path)?;
        Ok(LinkedFolderAttachResult { folder, summary })
    })
    .await
    .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

#[tauri::command]
pub async fn cmd_kb_remove_linked_folder(
    state: State<'_, Arc<KnowledgeLinkedManager>>,
    kb_id: String,
    path: PathBuf,
) -> StoreResult<usize> {
    let manager = Arc::clone(state.inner());
    spawn_blocking(move || manager.remove_linked_folder(&kb_id, &path))
        .await
        .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

#[tauri::command]
pub async fn cmd_kb_update_linked_folder(
    state: State<'_, Arc<KnowledgeLinkedManager>>,
    kb_id: String,
    path: PathBuf,
    organize_allowed: Option<bool>,
    git_confirmed: Option<bool>,
) -> StoreResult<LinkedFolder> {
    let manager = Arc::clone(state.inner());
    spawn_blocking(move || {
        manager.update_linked_folder(&kb_id, &path, organize_allowed, git_confirmed)
    })
    .await
    .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

#[tauri::command]
pub async fn cmd_kb_scan_linked_folder(
    state: State<'_, Arc<KnowledgeLinkedManager>>,
    kb_id: String,
    path: PathBuf,
) -> StoreResult<LinkedFolderScanSummary> {
    let manager = Arc::clone(state.inner());
    spawn_blocking(move || manager.scan_linked_folder(&kb_id, &path))
        .await
        .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

#[tauri::command]
pub async fn cmd_kb_organize_list(
    state: State<'_, Arc<KnowledgeOrganizeManager>>,
    kb_id: String,
) -> StoreResult<Vec<OrganizePlan>> {
    let manager = Arc::clone(state.inner());
    spawn_blocking(move || manager.list_plans(&kb_id))
        .await
        .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

#[tauri::command]
pub async fn cmd_kb_organize_get(
    state: State<'_, Arc<KnowledgeOrganizeManager>>,
    kb_id: String,
    plan_id: String,
) -> StoreResult<OrganizePlan> {
    let manager = Arc::clone(state.inner());
    spawn_blocking(move || manager.get_plan(&kb_id, &plan_id))
        .await
        .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

/// User-only physical organize approval command (never exposed on Management API or CLI).
#[tauri::command]
pub async fn cmd_kb_organize_approve(
    state: State<'_, Arc<KnowledgeOrganizeManager>>,
    kb_id: String,
    plan_id: String,
    decision: Value,
) -> StoreResult<OrganizePlan> {
    let decision: ApprovalDecision = decode(decision)?;
    let manager = Arc::clone(state.inner());
    spawn_blocking(move || manager.approve_plan(&kb_id, &plan_id, decision))
        .await
        .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
}

/// User-only physical organize execution command (never exposed on Management API or CLI).
#[tauri::command]
pub async fn cmd_kb_organize_execute(
    state: State<'_, Arc<KnowledgeOrganizeManager>>,
    kb_id: String,
    plan_id: String,
) -> StoreResult<OrganizePlan> {
    let manager = Arc::clone(state.inner());
    manager.execute_plan(&kb_id, &plan_id).await
}

/// User-only physical organize undo command (never exposed on Management API or CLI).
#[tauri::command]
pub async fn cmd_kb_organize_undo(
    state: State<'_, Arc<KnowledgeOrganizeManager>>,
    kb_id: String,
    plan_id: String,
) -> StoreResult<UndoReport> {
    let manager = Arc::clone(state.inner());
    manager.undo_plan(&kb_id, &plan_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_malformed_ipc_payloads_return_only_a_stable_code() {
        let malformed = serde_json::json!({ "curationMode": "untrusted payload" });
        let code = decode::<KnowledgeBasePatch>(malformed).unwrap_err();
        assert_eq!(serde_json::to_value(code).unwrap(), "KB_INVALID_INPUT");
        let malformed_fields = serde_json::json!({ "untrusted field": "untrusted payload" });
        assert_eq!(
            decode::<MetadataPatch>(malformed_fields),
            Err(KnowledgeErrorCode::KbInvalidInput)
        );
    }
}
