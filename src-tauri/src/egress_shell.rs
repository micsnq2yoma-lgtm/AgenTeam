use crate::egress_restriction::{self, EgressRestrictionStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{ipc::Invoke, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::watch;

static RESTART_FOR_RESTRICTION: AtomicBool = AtomicBool::new(false);

pub fn restarting() -> bool {
    RESTART_FOR_RESTRICTION.load(Ordering::Acquire)
}

fn needs_restart(
    recovery_only: bool,
    initial: &EgressRestrictionStatus,
    current: &EgressRestrictionStatus,
) -> bool {
    if recovery_only {
        return current.is_allowed();
    }
    // A policy switch retires every previously admitted SDK/background process,
    // even when both configurations have a matching host IP. lock_epoch also
    // catches a lock and subsequent match coalesced by the watch channel.
    current.must_lock()
        || current.lock_epoch != initial.lock_epoch
        || current.policy_epoch != initial.policy_epoch
}

pub fn guard_invoke(
    handler: impl Fn(Invoke<tauri::Wry>) -> bool,
) -> impl Fn(Invoke<tauri::Wry>) -> bool {
    move |invoke| {
        let recovery = matches!(
            invoke.message.command(),
            "cmd_get_egress_restriction"
                | "cmd_set_egress_restriction"
                | "cmd_recheck_egress_restriction"
        );
        if !recovery && (restarting() || !egress_restriction::is_allowed()) {
            invoke.resolver.reject("egress_restriction_locked");
            return true;
        }
        handler(invoke)
    }
}

async fn retire_business_work(app: &tauri::AppHandle) {
    let scheduler = crate::task_scheduler::get_task_scheduler();
    scheduler.capture_egress_interruptions().await;
    let sidecars = app
        .state::<crate::sidecar::ManagedSidecarManager>()
        .inner()
        .clone();
    let retire_sidecars = tauri::async_runtime::spawn_blocking(move || {
        crate::sidecar::begin_app_exit_shutdown()?;
        crate::sidecar::stop_all_sidecars(&sidecars, "egress-restriction")
    });
    // Captured queue identities survive the transport stop and storage retries.
    // An unavailable store must never keep a previously admitted model stream alive.
    let recording = app
        .state::<crate::recording::ManagedRecordingManager>()
        .inner()
        .clone();
    loop {
        let (goals, tasks, capture) = tokio::join!(
            crate::session_goal::get_session_goal_manager().pause_active_for_egress_shutdown(),
            scheduler.shutdown_for_egress_restriction(),
            recording.stop_for_app_exit(),
        );
        if goals.is_ok() && tasks.is_ok() && capture.is_ok() {
            break;
        }
        crate::ulog_error!(
            "[egress] Could not persist interrupted work: goals={:?}, tasks={:?}, recording={:?}",
            goals.err(),
            tasks.err(),
            capture.err()
        );
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    // The ordinary exit path also drains all process owners before relaunch.
    let retired = retire_sidecars
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result);
    if let Err(error) = retired {
        crate::ulog_error!("[egress] Could not retire sidecars early: {}", error);
    }
}

pub fn monitor(
    app: tauri::AppHandle,
    mut access: watch::Receiver<EgressRestrictionStatus>,
    recovery_only: bool,
    initial: EgressRestrictionStatus,
) {
    egress_restriction::start_monitor(app.clone());
    tauri::async_runtime::spawn(async move {
        loop {
            let status = access.borrow_and_update().clone();
            if needs_restart(recovery_only, &initial, &status) {
                RESTART_FOR_RESTRICTION.store(true, Ordering::Release);
                if !recovery_only {
                    retire_business_work(&app).await;
                }
                app.request_restart();
                return;
            }
            if access.changed().await.is_err() {
                return;
            }
        }
    });
}

pub fn run_recovery(
    access: watch::Receiver<EgressRestrictionStatus>,
    initial: EgressRestrictionStatus,
) {
    // A separate builder only exposes recovery commands. No sidecars, automation,
    // update checks, external CLI server, embedded browser or third-party plugins.
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            egress_restriction::cmd_get_egress_restriction,
            egress_restriction::cmd_set_egress_restriction,
            egress_restriction::cmd_recheck_egress_restriction,
        ])
        .setup(move |app| {
            // Normal shutdown has already drained its children. Also reap orphans
            // after a crash before admitting another full application instance.
            let previous = crate::app_dirs::acquire_lock().had_prior_instance();
            crate::sidecar::cleanup_stale_sidecars_preamble();
            crate::sidecar::cleanup_stale_sidecars(previous);
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .scroll_bar_style(crate::webview_policy::scroll_bar_style())
                .title("AgenTeam")
                .inner_size(800.0, 600.0)
                .initialization_script("window.__AGENTEAM_RECOVERY_ONLY__ = true;")
                .on_navigation(|url| {
                    matches!(crate::classify_navigation(url), crate::NavDecision::Allow)
                })
                .build()?;
            monitor(app.handle().clone(), access, true, initial);
            Ok(())
        })
        .build(crate::app_context())
        .expect("building the network restriction recovery window");
    app.run(|_, event| {
        if let tauri::RunEvent::Exit = event {
            crate::app_dirs::release_lock();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egress_restriction::CheckState;

    #[test]
    fn restart_retires_stale_admissions_and_reopens_only_after_recovery() {
        let matched = EgressRestrictionStatus {
            enabled: true,
            expected_ip: "203.0.113.1".into(),
            observed_ip: Some("203.0.113.1".into()),
            state: CheckState::Matched,
            lock_epoch: 0,
            policy_epoch: 1,
        };
        let locked = EgressRestrictionStatus {
            state: CheckState::Mismatch,
            lock_epoch: 1,
            ..matched.clone()
        };
        assert!(!needs_restart(false, &matched, &matched));
        assert!(needs_restart(false, &matched, &locked));
        assert!(!needs_restart(true, &locked, &locked));
        assert!(needs_restart(true, &locked, &matched));
        assert!(needs_restart(
            false,
            &matched,
            &EgressRestrictionStatus {
                lock_epoch: 1,
                ..matched.clone()
            }
        ));
        assert!(needs_restart(
            false,
            &matched,
            &EgressRestrictionStatus {
                policy_epoch: 2,
                ..matched.clone()
            }
        ));
    }
}
