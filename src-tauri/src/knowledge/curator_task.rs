//! Converges KB-owned settings into the existing TaskStore and scheduler.
use std::sync::Arc;

use chrono::{Duration, Utc};
use tauri::async_runtime::spawn_blocking;

use super::store::{CurationMode, KnowledgeBase, KnowledgeBasePatch, KnowledgeStore, StoreResult};
use super::types::KnowledgeErrorCode::{KbAccessDenied, KbInvalidInput, KbStoreUnavailable};
use crate::task::{
    self, NotificationConfig, Task, TaskCreateDirectInput, TaskEndConditions, TaskExecutionMode,
    TaskExecutor, TaskListFilter, TaskRunMode, TaskStatus, TaskStore, TaskUpdateInput,
    TaskUpdateStatusInput, TransitionActor, TransitionSource, MANAGED_KIND_KNOWLEDGE_CURATOR,
};
use crate::task_application::TaskApplication;
use crate::task_scheduler::{acquire_task_control, get_task_scheduler};

struct CuratorWorkspace {
    project_id: String,
    path: String,
}

fn curator_workspace(agent_id: &str) -> StoreResult<CuratorWorkspace> {
    let agent = crate::im::read_agent_configs_from_disk()
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or(KbInvalidInput)?;
    let project_id = crate::im::project_id_for_agent(agent_id).ok_or(KbInvalidInput)?;
    if agent.resolved_workspace_path.is_empty() || crate::im::is_agent_workspace_archived(&agent) {
        return Err(KbInvalidInput);
    }
    Ok(CuratorWorkspace {
        project_id,
        path: agent.resolved_workspace_path,
    })
}

fn matches_workspace(task: &Task, workspace: &CuratorWorkspace) -> bool {
    task.workspace_id == workspace.project_id
        && crate::workspace_path::normalize_workspace_path_identity(&task.workspace_path)
            == crate::workspace_path::normalize_workspace_path_identity(&workspace.path)
}

fn prompt(kb_id: &str) -> String {
    format!("Use the agenteam-knowledge-curator skill to curate knowledge library {kb_id}. Follow the library's curation context. Leave original files unchanged.")
}

fn create_input(
    library: &KnowledgeBase,
    workspace: &CuratorWorkspace,
    locale: crate::i18n::SupportedLocale,
) -> TaskCreateDirectInput {
    TaskCreateDirectInput {
        name: crate::i18n::t("task.knowledgeCurator", locale).into(),
        executor: TaskExecutor::Agent,
        description: None,
        workspace_id: workspace.project_id.clone(),
        workspace_path: workspace.path.clone(),
        task_md_content: prompt(&library.id),
        execution_mode: TaskExecutionMode::Recurring,
        run_mode: Some(TaskRunMode::NewSession),
        end_conditions: Some(TaskEndConditions::default()),
        interval_minutes: Some(library.curation_interval_minutes),
        cron_expression: None,
        cron_timezone: None,
        start_at: Some(first_start(library)),
        recurring_window: None,
        trigger: None,
        dispatch_at: None,
        model: None,
        provider_id: None,
        permission_mode: None,
        preselected_session_id: None,
        runtime: None,
        runtime_config: None,
        mcp_enabled_servers: None,
        managed_kind: Some(MANAGED_KIND_KNOWLEDGE_CURATOR.into()),
        source_record_id: None,
        tags: vec!["system".into()],
        notification: Some(NotificationConfig {
            desktop: false,
            im: Some(task::notification::NotificationTarget::Off),
            source: Some("explicit".into()),
            ..Default::default()
        }),
    }
}

fn first_start(library: &KnowledgeBase) -> String {
    (Utc::now() + Duration::minutes(i64::from(library.curation_interval_minutes))).to_rfc3339()
}

async fn existing_curators(tasks: &TaskStore) -> Vec<Task> {
    tasks
        .list(TaskListFilter {
            include_managed: Some(true),
            ..Default::default()
        })
        .await
        .into_iter()
        .filter(|task| {
            !task.deleted && task.managed_kind.as_deref() == Some(MANAGED_KIND_KNOWLEDGE_CURATOR)
        })
        .collect()
}

async fn remove_curator(tasks: &TaskStore, kb_id: &str) -> StoreResult<()> {
    if let Some(id) = tasks.knowledge_curator_task(kb_id).await {
        TaskApplication::new(tasks, None)
            .delete_internal(&id)
            .await
            .map_err(|_| KbStoreUnavailable)?;
    }
    Ok(())
}

async fn sync_task(
    tasks: &TaskStore,
    kb: &KnowledgeBase,
    workspace: Option<CuratorWorkspace>,
    locale: crate::i18n::SupportedLocale,
) -> StoreResult<(String, TaskStatus)> {
    let id = match tasks.knowledge_curator_task(&kb.id).await {
        Some(id) => id,
        None => {
            let workspace = workspace.as_ref().ok_or(KbInvalidInput)?;
            tasks
                .create_knowledge_curator(&kb.id, create_input(kb, workspace, locale))
                .await
                .map_err(|_| KbStoreUnavailable)?
                .id
        }
    };
    let control = acquire_task_control(&id).await;
    let current = tasks.get(&id).await.ok_or(KbStoreUnavailable)?;
    let drifted = workspace.as_ref().is_some_and(|workspace| {
        !matches_workspace(&current, workspace)
            || current.interval_minutes != Some(kb.curation_interval_minutes)
    });
    let active_schedule = matches!(current.status, TaskStatus::Running | TaskStatus::Verifying);
    let cancel_execution = drifted || kb.curation_mode == CurationMode::Off;
    // A stopped manual Task can still have a live one-shot. An unchanged
    // reconcile must leave it running so the scheduler can reject duplicates.
    let status = if active_schedule && (drifted || kb.curation_mode != CurationMode::Auto) {
        tasks
            .update_status_with_task_control_held(
                TaskUpdateStatusInput {
                    id: id.clone(),
                    status: TaskStatus::Stopped,
                    message: None,
                    actor: TransitionActor::System,
                    source: Some(TransitionSource::Scheduler),
                },
                &control,
            )
            .await
            .map_err(|_| KbStoreUnavailable)?
            .0
            .status
    } else {
        if cancel_execution
            && get_task_scheduler()
                .execution_projection(&id)
                .await
                .is_some()
        {
            get_task_scheduler()
                .stop_with_control_held(&id, &control)
                .await
                .map_err(|_| KbStoreUnavailable)?;
        }
        current.status
    };
    if let Some(workspace) = workspace.filter(|_| drifted) {
        tasks
            .update_with_task_control_held(
                TaskUpdateInput {
                    id: id.clone(),
                    workspace_id: Some(workspace.project_id),
                    workspace_path: Some(workspace.path),
                    interval_minutes: Some(kb.curation_interval_minutes),
                    start_at: Some(first_start(kb)),
                    ..Default::default()
                },
                &control,
            )
            .await
            .map_err(|_| KbStoreUnavailable)?;
    }
    Ok((id, status))
}

async fn reconcile(
    tasks: &TaskStore,
    kb: &KnowledgeBase,
    arm_restored: bool,
) -> StoreResult<Option<String>> {
    let Some(agent_id) = kb.curator_agent_id.clone() else {
        remove_curator(tasks, &kb.id).await?;
        return Ok(None);
    };
    let (workspace, locale) = blocking(move || {
        let workspace = curator_workspace(&agent_id);
        let locale = crate::i18n::current_locale();
        Ok((workspace, locale))
    })
    .await?;
    let workspace = match workspace {
        Ok(workspace) => Some(workspace),
        Err(KbInvalidInput)
            if kb.curation_mode == CurationMode::Off
                && tasks.knowledge_curator_task(&kb.id).await.is_some() =>
        {
            None
        }
        Err(code) => return Err(code),
    };
    let (id, status) = sync_task(tasks, kb, workspace, locale).await?;
    if kb.curation_mode == CurationMode::Auto
        && (matches!(status, TaskStatus::Stopped | TaskStatus::Todo)
            || (status == TaskStatus::Running && arm_restored))
    {
        TaskApplication::new(tasks, None)
            .start_scheduled_task(&id)
            .await
            .map_err(|_| KbStoreUnavailable)?;
    }
    Ok(Some(id))
}

async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> StoreResult<T> + Send + 'static,
) -> StoreResult<T> {
    spawn_blocking(task).await.map_err(|_| KbStoreUnavailable)?
}

pub(super) async fn create(
    store: Arc<KnowledgeStore>,
    patch: KnowledgeBasePatch,
) -> StoreResult<KnowledgeBase> {
    let _control = store.curator_control.lock().await;
    let tasks = task::get_task_store().ok_or(KbStoreUnavailable)?;
    let storage = Arc::clone(&store);
    let kb = blocking(move || {
        if let Some(agent) = patch.curator_agent_id.as_ref().and_then(Option::as_deref) {
            curator_workspace(agent)?;
        }
        storage.create(patch)
    })
    .await?;
    reconcile(tasks, &kb, false).await?;
    Ok(kb)
}

pub(super) async fn update(
    store: Arc<KnowledgeStore>,
    kb_id: String,
    patch: KnowledgeBasePatch,
) -> StoreResult<KnowledgeBase> {
    let _control = store.curator_control.lock().await;
    let tasks = task::get_task_store().ok_or(KbStoreUnavailable)?;
    let storage = Arc::clone(&store);
    let kb = blocking(move || {
        let mut desired = storage.get(&kb_id)?;
        desired.apply_patch(patch.clone())?;
        if let Some(agent) = desired.curator_agent_id.as_deref() {
            if desired.curation_mode != CurationMode::Off || patch.curator_agent_id.is_some() {
                curator_workspace(agent)?;
            }
        }
        storage.update(&kb_id, patch)
    })
    .await?;
    reconcile(tasks, &kb, false).await?;
    Ok(kb)
}

pub(super) async fn delete(store: Arc<KnowledgeStore>, kb_id: String) -> StoreResult<()> {
    let _control = store.curator_control.lock().await;
    let tasks = task::get_task_store().ok_or(KbStoreUnavailable)?;
    let storage = Arc::clone(&store);
    let library = blocking(move || storage.get(&kb_id)).await?;
    remove_curator(tasks, &library.id).await?;
    let storage = Arc::clone(&store);
    let deleting = library.id.clone();
    if let Err(code) = blocking(move || storage.delete(&deleting)).await {
        if let Err(recovery) = reconcile(tasks, &library, false).await {
            crate::ulog_warn!(
                "[knowledge] curator recovery after failed library delete code={recovery:?}"
            );
        }
        return Err(code);
    }
    Ok(())
}

pub(super) async fn run_now(store: Arc<KnowledgeStore>, kb_id: String) -> StoreResult<String> {
    let _control = store.curator_control.lock().await;
    let tasks = task::get_task_store().ok_or(KbStoreUnavailable)?;
    let storage = Arc::clone(&store);
    let kb = blocking(move || storage.get(&kb_id)).await?;
    if kb.curation_mode == CurationMode::Off || kb.curator_agent_id.is_none() {
        return Err(KbAccessDenied);
    }
    let task_id = reconcile(tasks, &kb, false)
        .await?
        .ok_or(KbStoreUnavailable)?;
    get_task_scheduler()
        .trigger_now(&task_id)
        .await
        .map_err(|_| KbStoreUnavailable)
}

pub(crate) async fn recover(store: &Arc<KnowledgeStore>, tasks: &TaskStore) {
    let _control = store.curator_control.lock().await;
    let storage = Arc::clone(store);
    let libraries = match blocking(move || storage.list()).await {
        Ok(value) => value,
        Err(code) => {
            crate::ulog_warn!("[knowledge] curator recovery failed code={code:?}");
            return;
        }
    };
    for task in existing_curators(tasks).await {
        if task
            .curator_knowledge_base_id
            .as_ref()
            .is_some_and(|id| libraries.iter().any(|kb| &kb.id == id))
        {
            continue;
        }
        if TaskApplication::new(tasks, None)
            .delete_internal(&task.id)
            .await
            .is_err()
        {
            crate::ulog_warn!("[knowledge] orphan curator removal failed");
        }
    }
    for library in &libraries {
        if let Err(code) = reconcile(tasks, library, true).await {
            crate::ulog_warn!(
                "[knowledge] curator recovery failed kb={} code={code:?}",
                library.id
            );
        }
    }
}

pub(crate) fn validate_execution(store: &KnowledgeStore, task: &Task) -> StoreResult<String> {
    let kb_id = task
        .curator_knowledge_base_id
        .as_deref()
        .ok_or(KbAccessDenied)?;
    let kb = store.get(kb_id)?;
    let agent_id = kb.curator_agent_id.as_deref().ok_or(KbAccessDenied)?;
    if kb.curation_mode == CurationMode::Off
        || !matches_workspace(task, &curator_workspace(agent_id)?)
    {
        return Err(KbAccessDenied);
    }
    Ok(kb.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::SupportedLocale;
    use crate::task_scheduler::next_execution_at;
    use tempfile::tempdir;

    #[tokio::test]
    async fn knowledge_curator_reuses_its_task_when_modes_change_and_removes_it() {
        task::ensure_test_docs_root();
        let data = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let store = KnowledgeStore::new(data.path().join("app"));
        let tasks = Arc::new(TaskStore::new(data.path().join("tasks")));
        task::set_task_store(Arc::clone(&tasks));
        let mut kb = store
            .create(KnowledgeBasePatch {
                name: Some("Fixture library".into()),
                curator_agent_id: Some(Some("fixture-agent".into())),
                curation_mode: Some(CurationMode::Manual),
                curation_interval_minutes: Some(60),
                ..Default::default()
            })
            .unwrap();
        let workspace = || {
            Some(CuratorWorkspace {
                project_id: "fixture-project".into(),
                path: workspace.path().to_string_lossy().into_owned(),
            })
        };
        let locale = SupportedLocale::EnUs;
        let (id, status) = sync_task(&tasks, &kb, workspace(), locale).await.unwrap();
        assert_eq!(status, TaskStatus::Stopped);
        assert_eq!(
            next_execution_at(&tasks.get(&id).await.unwrap()).unwrap(),
            None
        );
        assert_eq!(
            sync_task(&tasks, &kb, workspace(), locale).await.unwrap().0,
            id
        );

        kb.curation_mode = CurationMode::Auto;
        TaskApplication::new(&tasks, None)
            .start_scheduled_task(&id)
            .await
            .unwrap();
        let automatic = tasks.get(&id).await.unwrap();
        assert_eq!(automatic.status, TaskStatus::Running);
        assert!(next_execution_at(&automatic).unwrap().is_some());
        assert_eq!(
            sync_task(&tasks, &kb, workspace(), locale).await.unwrap(),
            (id.clone(), TaskStatus::Running)
        );

        for mode in [CurationMode::Manual, CurationMode::Off] {
            kb.curation_mode = mode;
            assert_eq!(
                sync_task(&tasks, &kb, workspace(), locale).await.unwrap(),
                (id.clone(), TaskStatus::Stopped)
            );
            assert_eq!(
                next_execution_at(&tasks.get(&id).await.unwrap()).unwrap(),
                None
            );
        }
        remove_curator(&tasks, &kb.id).await.unwrap();
        store.delete(&kb.id).unwrap();
        assert!(tasks.knowledge_curator_task(&kb.id).await.is_none());
        assert!(tasks.get(&id).await.unwrap().deleted);
        remove_curator(&tasks, &kb.id).await.unwrap();
    }
    #[test]
    fn the_curation_prompt_names_a_skill_the_app_ships() {
        let text = prompt("kb_000000000000");
        let named = text
            .split_whitespace()
            .find(|word| word.starts_with("agenteam-"))
            .expect("the prompt names a system skill");
        let skill = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../bundled-skills")
            .join(named)
            .join("SKILL.md");
        assert!(skill.is_file(), "{named} is not a bundled skill");
    }
}
