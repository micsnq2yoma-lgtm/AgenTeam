use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};

const APP_ROUTE_SCHEME: &str = "agenteam";
const APP_ROUTE_AUTHORITY: &str = "open";
const MAX_ID_BYTES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum AppRoute {
    #[serde(rename = "task.comment")]
    TaskComment {
        version: u8,
        #[serde(rename = "params")]
        params: TaskCommentRouteParams,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskCommentRouteParams {
    pub task_id: String,
    pub comment_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingAppRoute {
    pub generation: u64,
    pub route: AppRoute,
}

#[derive(Debug, Default)]
pub struct AppRouteQueue {
    generation: u64,
    pending: Option<PendingAppRoute>,
}

pub type ManagedAppRouteQueue = Arc<Mutex<AppRouteQueue>>;

pub fn create_queue() -> ManagedAppRouteQueue {
    Arc::new(Mutex::new(AppRouteQueue::default()))
}

/// Latest-wins queue between the native deep-link lifecycle and Renderer.
///
/// Tauri can deliver a link before the WebView has mounted its listener. The
/// event is therefore only a wake signal; Renderer atomically takes the value
/// through `cmd_take_pending_app_route`. Keeping the typed value in Rust also
/// means untrusted URL parsing never leaks into React components.
pub fn enqueue<R: Runtime>(app: &AppHandle<R>, state: &ManagedAppRouteQueue, route: AppRoute) {
    let generation = {
        let mut queue = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        queue.generation = queue.generation.saturating_add(1);
        let generation = queue.generation;
        queue.pending = Some(PendingAppRoute { generation, route });
        generation
    };
    if let Err(error) = app.emit("app-route:available", generation) {
        crate::ulog_warn!("[AppRoute] failed to wake renderer: {}", error);
    }
}

pub fn enqueue_deep_link<R: Runtime>(
    app: &AppHandle<R>,
    state: &ManagedAppRouteQueue,
    raw: &str,
) -> bool {
    let Some(route) = parse_deep_link(raw) else {
        return false;
    };
    enqueue(app, state, route);
    true
}

pub fn enqueue_from_args<R: Runtime>(
    app: &AppHandle<R>,
    state: &ManagedAppRouteQueue,
    args: &[String],
) -> bool {
    let mut accepted = false;
    for arg in args {
        accepted |= enqueue_deep_link(app, state, arg);
    }
    accepted
}

#[tauri::command]
pub fn cmd_take_pending_app_route(
    state: tauri::State<'_, ManagedAppRouteQueue>,
) -> Option<PendingAppRoute> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .pending
        .take()
}

impl AppRoute {
    pub fn task_comment(task_id: impl Into<String>, comment_id: impl Into<String>) -> Option<Self> {
        let task_id = task_id.into();
        let comment_id = comment_id.into();
        if !is_route_id(&task_id) || !is_route_id(&comment_id) {
            return None;
        }
        Some(Self::TaskComment {
            version: 1,
            params: TaskCommentRouteParams {
                task_id,
                comment_id,
            },
        })
    }

    pub fn to_deep_link(&self) -> String {
        match self {
            Self::TaskComment { params, .. } => format!(
                "agenteam://open/v1/tasks/{}/comments/{}",
                params.task_id, params.comment_id
            ),
        }
    }
}

fn is_route_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn decode_segment(value: &str) -> Option<String> {
    let decoded = percent_decode_str(value).decode_utf8().ok()?.into_owned();
    is_route_id(&decoded).then_some(decoded)
}

pub fn parse_deep_link(raw: &str) -> Option<AppRoute> {
    let value = raw.trim();
    if value.is_empty() || value.contains('?') || value.contains('#') || value.contains('\\') {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != APP_ROUTE_SCHEME
        || url.host_str() != Some(APP_ROUTE_AUTHORITY)
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    // Check the source path before accepting URL-normalized dot segments.
    let authority_and_path = value.split_once("://")?.1;
    let source_path = &authority_and_path[authority_and_path.find('/')?..];
    if source_path != url.path() {
        return None;
    }
    let path = url.path().strip_prefix('/')?;
    if path.starts_with('/') {
        return None;
    }
    let segments = path.split('/').collect::<Vec<_>>();
    let ["v1", "tasks", task, "comments", comment] = segments.as_slice() else {
        return None;
    };
    AppRoute::task_comment(decode_segment(task)?, decode_segment(comment)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_shared_route_corpus() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../src/shared/appRoute.fixtures.json"))
                .expect("route fixtures");
        for item in fixtures["accepted"].as_array().expect("accepted routes") {
            let expected: AppRoute = serde_json::from_value(item["route"].clone()).expect("route");
            assert_eq!(
                parse_deep_link(item["url"].as_str().expect("url")),
                Some(expected.clone())
            );
            assert_eq!(parse_deep_link(&expected.to_deep_link()), Some(expected));
        }
        for url in fixtures["rejected"].as_array().expect("rejected routes") {
            assert_eq!(parse_deep_link(url.as_str().expect("url")), None);
        }
    }


    #[test]
    fn rejects_resource_and_ambiguous_urls() {
        for value in [
            "agenteam://attachment/session/file.png",
            "agenteam://tool-attachment/session/turn/file.png",
            "myagents-resource://attachment/session/file.png",
            "agenteam://evil/v1/spaces/a/issues/b",
            "agenteam://open/v2/spaces/a/issues/b",
            "agenteam://open/v1/spaces/a/issues/b/extra",
            "agenteam://open/v1/tasks/a/comments/b/extra",
            "agenteam://open/v1/tasks/a/issues/b",
            "agenteam://open//v1/spaces/a/issues/b",
            "agenteam://open/v1/spaces/a/issues/b?prompt=run",
            "agenteam://open/v1/spaces/a/issues/b#fragment",
            "agenteam://open/v1/spaces/a%2Fb/issues/c",
            "agenteam://open/v1/spaces/a/issues/%ZZ",
            "agenteam://user@open/v1/spaces/a/issues/b",
            "agenteam://open:42/v1/spaces/a/issues/b",
        ] {
            assert_eq!(parse_deep_link(value), None, "{value}");
        }
    }


    #[test]
    fn queue_is_latest_generation_wins() {
        let state = create_queue();
        {
            let mut queue = state.lock().expect("queue lock");
            queue.generation = 40;
            queue.pending = Some(PendingAppRoute {
                generation: 41,
                route: AppRoute::task_comment("task-a", "comment-a").expect("route"),
            });
            queue.pending = Some(PendingAppRoute {
                generation: 42,
                route: AppRoute::task_comment("task-b", "comment-b").expect("route"),
            });
        }
        let pending = state
            .lock()
            .expect("queue lock")
            .pending
            .clone()
            .expect("pending");
        assert_eq!(pending.generation, 42);
        assert_eq!(
            pending.route.to_deep_link(),
            "agenteam://open/v1/tasks/task-b/comments/comment-b"
        );
    }
}
