use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::Value;
use tauri::{async_runtime::spawn_blocking, State};

use super::store::{
    KnowledgeBase, KnowledgeBasePatch, KnowledgeSource, KnowledgeStore, MetadataKey,
    MetadataOutcome, MetadataPatch, ResolvedKnowledgeRef, StoreResult,
};
use super::types::{KnowledgeErrorCode, KnowledgeMetadataOrigin};

async fn run_store<T: Send + 'static>(
    state: State<'_, Arc<KnowledgeStore>>,
    operation: impl FnOnce(&KnowledgeStore) -> StoreResult<T> + Send + 'static,
) -> StoreResult<T> {
    let store = Arc::clone(state.inner());
    spawn_blocking(move || operation(&store))
        .await
        .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?
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
    run_store(state, move |store| store.create(settings)).await
}

#[tauri::command]
pub async fn cmd_kb_update(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
    settings: Value,
) -> StoreResult<KnowledgeBase> {
    let settings: KnowledgeBasePatch = decode(settings)?;
    run_store(state, move |store| store.update(&kb_id, settings)).await
}

#[tauri::command]
pub async fn cmd_kb_delete(
    state: State<'_, Arc<KnowledgeStore>>,
    kb_id: String,
) -> StoreResult<()> {
    run_store(state, move |store| store.delete(&kb_id)).await
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
