//! Local task-comment notification projection and durable read receipts.
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::app_route::AppRoute;
use crate::{ulog_debug, ulog_warn};

const STATE_FILE: &str = "notification-state.json";
const UPDATED_EVENT: &str = "notification-center:updated";
const PAGE_SIZE: usize = 20;
const LOCAL_NOTIFICATION_WINDOW_DAYS: i64 = 90;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSortPoint {
    pub created_at: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotificationTarget {
    AppRoute { route: AppRoute },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationActor {
    #[serde(rename = "type")]
    pub actor_type: String,
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum NotificationItem {
    #[serde(rename = "task_agent_comment")]
    TaskAgentComment {
        id: String,
        created_at: String,
        is_read: bool,
        task_id: String,
        task_name: String,
        comment_id: String,
        agent: NotificationActor,
        excerpt: String,
        target: NotificationTarget,
    },
}

impl NotificationItem {
    fn id(&self) -> &str {
        match self {
            Self::TaskAgentComment { id, .. } => id,
        }
    }

    fn created_at(&self) -> &str {
        match self {
            Self::TaskAgentComment { created_at, .. } => created_at,
        }
    }

    fn is_read(&self) -> bool {
        match self {
            Self::TaskAgentComment { is_read, .. } => *is_read,
        }
    }

    fn target(&self) -> NotificationTarget {
        match self {
            Self::TaskAgentComment { target, .. } => target.clone(),
        }
    }

    fn point(&self) -> NotificationSortPoint {
        NotificationSortPoint {
            created_at: self.created_at().to_string(),
            id: self.id().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLoadState {
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSnapshot {
    pub load_state: NotificationLoadState,
    pub items: Vec<NotificationItem>,
    pub has_unread: bool,
    pub has_more: bool,
    pub is_loading_more: bool,
    pub feed_cutoff: Option<NotificationSortPoint>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalTaskReceipts {
    #[serde(default)]
    ids: Vec<String>,
    /// Sort points let the local owner expire receipts by the same 90-day
    /// visibility window without dropping an older-but-still-visible read fact
    /// merely because the bounded source index temporarily omitted it.
    #[serde(default)]
    points: BTreeMap<String, NotificationSortPoint>,
    #[serde(default)]
    cutoff: Option<NotificationSortPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedNotificationState {
    version: u8,
    #[serde(default)]
    local_tasks: LocalTaskReceipts,
    // Keep retired fields intact when updating a receipt in an existing data directory.
    #[serde(flatten)]
    legacy_fields: BTreeMap<String, serde_json::Value>,
}

impl Default for PersistedNotificationState {
    fn default() -> Self {
        Self {
            version: 2,
            local_tasks: LocalTaskReceipts::default(),
            legacy_fields: BTreeMap::new(),
        }
    }
}

pub struct NotificationCenter {
    task_store: crate::task::ManagedTaskStore,
    path: PathBuf,
    persisted: Mutex<PersistedNotificationState>,
    visible_limit: AtomicUsize,
    receipt_failed: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationActivation {
    notification_id: String,
    target: NotificationTarget,
}

pub type ManagedNotificationCenter = Arc<NotificationCenter>;

impl NotificationCenter {
    fn local_items(
        &self,
        persisted: &PersistedNotificationState,
    ) -> (bool, bool, Vec<NotificationItem>) {
        let source = self.task_store.agent_comment_notification_source();
        let cutoff_ms = Utc::now().timestamp_millis()
            - chrono::Duration::days(LOCAL_NOTIFICATION_WINDOW_DAYS).num_milliseconds();
        let items = source
            .items
            .into_iter()
            .filter(|locator| locator.created_at >= cutoff_ms)
            .filter_map(|locator| {
                let created_at =
                    chrono::DateTime::<Utc>::from_timestamp_millis(locator.created_at)?
                        .to_rfc3339();
                let point = NotificationSortPoint {
                    created_at: created_at.clone(),
                    id: locator.notification_id.clone(),
                };
                let is_read = persisted
                    .local_tasks
                    .ids
                    .iter()
                    .any(|id| id == &locator.notification_id)
                    || persisted
                        .local_tasks
                        .cutoff
                        .as_ref()
                        .is_some_and(|cutoff| is_at_or_before(&point, cutoff));
                let route = AppRoute::task_comment(&locator.task_id, &locator.comment_id)?;
                Some(NotificationItem::TaskAgentComment {
                    id: locator.notification_id,
                    created_at,
                    is_read,
                    task_id: locator.task_id,
                    task_name: locator.task_name,
                    comment_id: locator.comment_id,
                    agent: NotificationActor {
                        actor_type: "agent".to_string(),
                        id: locator.session_id,
                        display_name: locator.agent_label.unwrap_or_else(|| "Agent".to_string()),
                    },
                    excerpt: locator.excerpt,
                    target: NotificationTarget::AppRoute { route },
                })
            })
            .collect();
        (source.ready, source.partial_error, items)
    }

    fn snapshot(&self) -> NotificationSnapshot {
        let persisted = self.persisted.lock().unwrap_or_else(|e| e.into_inner());
        let (ready, partial, mut items) = self.local_items(&persisted);
        items.sort_by(|left, right| {
            right
                .created_at()
                .cmp(left.created_at())
                .then_with(|| right.id().cmp(left.id()))
        });
        let visible_limit = self.visible_limit.load(Ordering::Relaxed);
        let has_more = items.len() > visible_limit;
        let has_unread = items.iter().any(|item| !item.is_read());
        let feed_cutoff = items.first().map(NotificationItem::point);
        items.truncate(visible_limit);
        NotificationSnapshot {
            load_state: if !ready {
                NotificationLoadState::Loading
            } else if partial && items.is_empty() {
                NotificationLoadState::Error
            } else {
                NotificationLoadState::Ready
            },
            items,
            has_more,
            has_unread,
            feed_cutoff,
            is_loading_more: false,
            error_code: if self.receipt_failed.load(Ordering::Relaxed) {
                Some("task_receipt_persist_failed".into())
            } else {
                partial.then(|| "task_source_partial".into())
            },
        }
    }
}

fn notification_state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STATE_FILE)
}

fn state_path() -> PathBuf {
    crate::app_dirs::myagents_data_dir()
        .map(|path| notification_state_path(&path))
        .unwrap_or_else(|| PathBuf::from(STATE_FILE))
}

fn load_persisted(path: &Path) -> PersistedNotificationState {
    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<PersistedNotificationState>(&content) {
            Ok(state) if state.version == 2 => state,
            Ok(_) => {
                ulog_warn!("[NotificationCenter] ignored unsupported local state version");
                PersistedNotificationState::default()
            }
            Err(error) => {
                ulog_warn!(
                    "[NotificationCenter] ignored invalid local state: {}",
                    error
                );
                PersistedNotificationState::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            PersistedNotificationState::default()
        }
        Err(error) => {
            ulog_warn!("[NotificationCenter] failed to read local state: {}", error);
            PersistedNotificationState::default()
        }
    }
}

fn write_private_json_unlocked<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|e| format!("Failed to serialize JSON: {}", e))?;
    write_private_bytes_unlocked(path, &bytes)
}

fn write_private_bytes_unlocked(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent dir: {}", e))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("Failed to write temp file: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to chmod temp file: {}", e))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("Failed to commit file: {}", e))?;
    Ok(())
}

fn mutate_persisted<T>(
    center: &NotificationCenter,
    mutate: impl FnOnce(&mut PersistedNotificationState) -> T,
) -> Result<T, String> {
    let mut persisted = center
        .persisted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = persisted.clone();
    let result = mutate(&mut persisted);
    if let Err(error) = write_private_json_unlocked(&center.path, &*persisted) {
        *persisted = before;
        return Err(error);
    }
    Ok(result)
}

fn emit_snapshot<R: tauri::Runtime>(app: &AppHandle<R>, center: &NotificationCenter) {
    let snapshot = center.snapshot();
    if let Err(error) = app.emit(UPDATED_EVENT, snapshot) {
        ulog_debug!(
            "[NotificationCenter] renderer update not delivered: {}",
            error
        );
    }
}

fn is_at_or_before(point: &NotificationSortPoint, cutoff: &NotificationSortPoint) -> bool {
    point.created_at < cutoff.created_at
        || (point.created_at == cutoff.created_at && point.id <= cutoff.id)
}

fn is_after(point: &NotificationSortPoint, cutoff: &NotificationSortPoint) -> bool {
    point.created_at > cutoff.created_at
        || (point.created_at == cutoff.created_at && point.id > cutoff.id)
}

fn later_cutoff(
    current: Option<NotificationSortPoint>,
    candidate: NotificationSortPoint,
) -> NotificationSortPoint {
    match current {
        Some(current) if !is_after(&candidate, &current) => current,
        _ => candidate,
    }
}

fn record_local_task_read(
    persisted: &mut PersistedNotificationState,
    notification_id: &str,
    point: NotificationSortPoint,
) {
    let already_recorded = persisted.local_tasks.points.contains_key(notification_id)
        || persisted
            .local_tasks
            .ids
            .iter()
            .any(|id| id == notification_id);
    if !already_recorded {
        persisted.local_tasks.ids.push(notification_id.to_string());
    }
    persisted
        .local_tasks
        .points
        .insert(notification_id.to_string(), point);

    // Expiry is a temporal bound, not a count bound. Amortize the scan so a
    // large burst of individually-read notifications remains O(n log n).
    if persisted.local_tasks.points.len() % 256 != 0 {
        return;
    }
    let expiry = Utc::now() - chrono::Duration::days(LOCAL_NOTIFICATION_WINDOW_DAYS);
    let expired = persisted
        .local_tasks
        .points
        .iter()
        .filter_map(|(id, point)| {
            chrono::DateTime::parse_from_rfc3339(&point.created_at)
                .ok()
                .filter(|created_at| created_at.with_timezone(&Utc) < expiry)
                .map(|_| id.clone())
        })
        .collect::<HashSet<_>>();
    if !expired.is_empty() {
        persisted.local_tasks.ids.retain(|id| !expired.contains(id));
        persisted
            .local_tasks
            .points
            .retain(|id, _| !expired.contains(id));
    }
}

fn local_task_notification_point(
    center: &NotificationCenter,
    notification_id: &str,
) -> Option<NotificationSortPoint> {
    center
        .task_store
        .agent_comment_notification_source()
        .items
        .into_iter()
        .find(|item| item.notification_id == notification_id)
        .and_then(|item| {
            chrono::DateTime::<Utc>::from_timestamp_millis(item.created_at).map(|created_at| {
                NotificationSortPoint {
                    created_at: created_at.to_rfc3339(),
                    id: item.notification_id,
                }
            })
        })
}

fn main_window_unfocused<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    app.get_window("main")
        .map(|window| {
            !window.is_visible().unwrap_or(false) || !window.is_focused().unwrap_or(false)
        })
        .unwrap_or(true)
}

pub fn create_state(task_store: crate::task::ManagedTaskStore) -> ManagedNotificationCenter {
    let path = state_path();
    Arc::new(NotificationCenter {
        persisted: Mutex::new(load_persisted(&path)),
        path,
        visible_limit: AtomicUsize::new(PAGE_SIZE),
        receipt_failed: AtomicBool::new(false),
        task_store,
    })
}

pub fn agent_comment_appended<R: tauri::Runtime>(
    app: &AppHandle<R>,
    center: &ManagedNotificationCenter,
    notification_id: &str,
) {
    emit_snapshot(app, center);
    if !main_window_unfocused(app) {
        return;
    }
    if let Some(item) = center
        .snapshot()
        .items
        .into_iter()
        .find(|item| item.id() == notification_id)
    {
        let NotificationItem::TaskAgentComment {
            agent, task_name, excerpt, ..
        } = &item;
        let title = crate::i18n::t("notification.taskCommentTitle", crate::i18n::current_locale());
        crate::notification::show_task_comment(
            app,
            title,
            &format!("{} · {} · {}", agent.display_name, task_name, excerpt),
            item.id().to_string(),
            item.target(),
        );
    }
}

fn mark_read(center: &NotificationCenter, id: &str) -> Result<(), String> {
    let point = local_task_notification_point(center, id)
        .ok_or_else(|| "Task notification is no longer available".to_string())?;
    let result = mutate_persisted(center, |persisted| {
        record_local_task_read(persisted, id, point);
    });
    center.receipt_failed.store(result.is_err(), Ordering::Relaxed);
    result
}

#[tauri::command]
pub fn cmd_notification_get_snapshot(
    state: tauri::State<'_, ManagedNotificationCenter>,
) -> NotificationSnapshot {
    state.snapshot()
}

#[tauri::command]
pub async fn cmd_notification_refresh(
    state: tauri::State<'_, ManagedNotificationCenter>,
) -> Result<NotificationSnapshot, String> {
    state.task_store.retry_comment_notification_index_if_partial().await;
    state.task_store.wait_for_comment_notification_index().await;
    Ok(state.snapshot())
}

#[tauri::command]
pub fn cmd_notification_load_more(
    state: tauri::State<'_, ManagedNotificationCenter>,
) -> NotificationSnapshot {
    let _ = state
        .visible_limit
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |limit| {
            Some(limit.saturating_add(PAGE_SIZE))
        });
    state.snapshot()
}

#[tauri::command]
pub fn cmd_notification_mark_read(
    app: AppHandle,
    state: tauri::State<'_, ManagedNotificationCenter>,
    notification_id: String,
) -> Result<NotificationActivation, String> {
    let item = state
        .snapshot()
        .items
        .into_iter()
        .find(|item| item.id() == notification_id)
        .ok_or_else(|| "Task notification is no longer available".to_string())?;
    let result = mark_read(&state, &notification_id);
    emit_snapshot(&app, &state);
    result.map(|_| NotificationActivation {
        notification_id,
        target: item.target(),
    })
}

#[tauri::command]
pub fn cmd_notification_mark_all_read(
    app: AppHandle,
    state: tauri::State<'_, ManagedNotificationCenter>,
) -> Result<NotificationSnapshot, String> {
    let snapshot = state.snapshot();
    let Some(feed_cutoff) = snapshot.feed_cutoff else {
        return Ok(snapshot);
    };
    let result = mutate_persisted(&state, |persisted| {
        persisted.local_tasks.cutoff = Some(later_cutoff(
            persisted.local_tasks.cutoff.take(),
            feed_cutoff,
        ));
    });
    state.receipt_failed.store(result.is_err(), Ordering::Relaxed);
    emit_snapshot(&app, &state);
    result.map(|_| state.snapshot())
}

pub fn activate_from_toast<R: tauri::Runtime>(
    app: AppHandle<R>,
    id: String,
    target: NotificationTarget,
) {
    let center = app.state::<ManagedNotificationCenter>().inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = mark_read(&center, &id) {
            ulog_warn!("[NotificationCenter] failed to persist Task toast receipt: {}", error);
        }
        emit_snapshot(&app, &center);
        let NotificationTarget::AppRoute { route } = target;
        let queue = app.state::<crate::app_route::ManagedAppRouteQueue>();
        crate::app_route::enqueue(&app, &queue, route);
    });
}
