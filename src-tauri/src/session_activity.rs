use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActivityReport {
    Running,
    WaitingForUser,
    Complete,
    Stopped,
    Error,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionActivity {
    Running,
    WaitingForUser,
    Error,
}

impl ActivityReport {
    fn projection(self) -> Option<SessionActivity> {
        match self {
            Self::Running => Some(SessionActivity::Running),
            Self::WaitingForUser => Some(SessionActivity::WaitingForUser),
            Self::Error => Some(SessionActivity::Error),
            Self::Complete | Self::Stopped | Self::Idle => None,
        }
    }
}

#[derive(Clone, Copy)]
struct ActivityEntry {
    generation: u64,
    activity: SessionActivity,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityChanged<'a> {
    session_id: &'a str,
    activity: Option<SessionActivity>,
}

/// Process-lifetime projection. The SidecarManager admits reports and
/// forwards rekeys and retirements under its generation lock.
#[derive(Default)]
pub(crate) struct SessionActivityRegistry {
    entries: Mutex<HashMap<String, ActivityEntry>>,
    app: OnceLock<AppHandle>,
}

impl SessionActivityRegistry {
    pub(crate) fn set_app_handle(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    fn emit(&self, session_id: &str, activity: Option<SessionActivity>) {
        if let Some(app) = self.app.get() {
            if let Err(error) = app.emit("session:activity-changed", ActivityChanged { session_id, activity }) {
                crate::ulog_warn!("[session-activity] Failed to emit state change: {}", error);
            }
        }
    }

    pub(crate) fn record(&self, session_id: &str, generation: u64, report: ActivityReport) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let previous = entries.get(session_id).copied();
        if previous.is_some_and(|entry| {
            entry.generation > generation
                || (entry.activity == SessionActivity::Error && report == ActivityReport::Idle)
        }) {
            return;
        }
        let next = report.projection();
        match next {
            Some(activity) => {
                entries.insert(session_id.to_owned(), ActivityEntry { generation, activity });
            }
            None => {
                entries.remove(session_id);
            }
        }
        // Emit in mutation order while holding the same lock as snapshot.
        if previous.map(|entry| entry.activity) != next {
            self.emit(session_id, next);
        }
    }

    pub(crate) fn retire(&self, session_id: &str, generation: u64) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.get(session_id).is_some_and(|entry| {
            entry.generation == generation && entry.activity != SessionActivity::Error
        }) {
            entries.remove(session_id);
            self.emit(session_id, None);
        }
    }

    pub(crate) fn rekey(&self, from: &str, to: &str, generation: u64) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(entry) = entries.get(from).copied() else {
            return;
        };
        if entry.generation != generation {
            return;
        }
        if entry.activity == SessionActivity::Error && !from.starts_with("pending-") {
            return;
        }
        entries.remove(from);
        self.emit(from, None);
        let previous = entries.insert(to.to_owned(), entry);
        if previous.map(|entry| entry.activity) != Some(entry.activity) {
            self.emit(to, Some(entry.activity));
        }
    }

    fn acknowledge(&self, session_id: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.get(session_id).is_some_and(|entry| entry.activity == SessionActivity::Error) {
            entries.remove(session_id);
            self.emit(session_id, None);
        }
    }

    pub(crate) fn snapshot(&self) -> HashMap<String, SessionActivity> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(id, entry)| (id.clone(), entry.activity))
            .collect()
    }
}

#[tauri::command]
pub(crate) fn cmd_session_activity_snapshot(
    registry: State<'_, Arc<SessionActivityRegistry>>,
) -> HashMap<String, SessionActivity> {
    registry.snapshot()
}

#[tauri::command]
pub(crate) fn cmd_acknowledge_session_error(
    registry: State<'_, Arc<SessionActivityRegistry>>,
    session_id: String,
) {
    registry.acknowledge(&session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_run_wait_resume_and_terminal_states() {
        let registry = SessionActivityRegistry::default();
        let id = "session";
        for (report, expected) in [
            (ActivityReport::Running, Some(SessionActivity::Running)),
            (ActivityReport::WaitingForUser, Some(SessionActivity::WaitingForUser)),
            (ActivityReport::Running, Some(SessionActivity::Running)),
            (ActivityReport::Complete, None),
            (ActivityReport::Running, Some(SessionActivity::Running)),
            (ActivityReport::Error, Some(SessionActivity::Error)),
            (ActivityReport::Idle, Some(SessionActivity::Error)),
            (ActivityReport::Running, Some(SessionActivity::Running)),
            (ActivityReport::Stopped, None),
        ] {
            registry.record(id, 1, report);
            assert_eq!(registry.snapshot().get(id).copied(), expected);
        }
    }

    #[test]
    fn retirement_clears_live_state_without_clearing_error() {
        let registry = SessionActivityRegistry::default();
        registry.record("session", 2, ActivityReport::WaitingForUser);
        registry.record("session", 1, ActivityReport::Error);
        registry.retire("session", 1);
        assert_eq!(registry.snapshot().get("session"), Some(&SessionActivity::WaitingForUser));
        registry.retire("session", 2);
        assert!(registry.snapshot().is_empty());

        registry.record("session", 3, ActivityReport::Error);
        registry.retire("session", 3);
        assert_eq!(registry.snapshot().get("session"), Some(&SessionActivity::Error));
        registry.acknowledge("session");
        assert!(registry.snapshot().is_empty());
    }

    #[test]
    fn pending_rekey_keeps_waiting_status_on_materialized_session() {
        let registry = SessionActivityRegistry::default();
        registry.record("pending-session", 1, ActivityReport::WaitingForUser);
        registry.rekey("pending-session", "session", 1);
        assert_eq!(registry.snapshot(), HashMap::from([("session".to_owned(), SessionActivity::WaitingForUser)]));
    }
}
