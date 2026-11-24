//! Unified process cleanup using `sysinfo` (native API).
//!
//! Replaces the legacy PowerShell/WMI-based Windows cleanup (which spawned
//! ~6 PowerShell processes per invocation, each cold-starting .NET + WMI
//! for 1–3 s) and the Unix `pgrep` path. The legacy Windows path blocked
//! Tauri `setup()` on the main thread for 5–15 s on first launch, which
//! directly caused the "frontend freeze" user reports.
//!
//! Startup recovery requires a birth-time owner stamp scoped to the app data
//! directory. Updater recovery separately checks for processes that hold the
//! installation open. Live Sidecar / Plugin Bridge shutdown retains the exact
//! birth-time [`crate::process_cmd::ChildTree`] authority.
//!
//! Performance: on a clean first launch (zero matches), the single
//! `sysinfo` enumeration completes in ~10–50 ms vs ~5–15 s for the old
//! PowerShell chain. On restarts with live children, ~50–200 ms total.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};

const STARTUP_OWNER_ENV: &str = "MYAGENTS_STARTUP_PROCESS_OWNER";

#[derive(Clone, Copy, PartialEq, Eq)]
struct AppGeneration {
    pid: Pid,
    started_at: u64,
}

struct StartupProcessScope {
    data_dir: PathBuf,
    key: String,
    current: AppGeneration,
}

impl StartupProcessScope {
    fn new(data_dir: &Path, current: AppGeneration) -> Self {
        let key = format!(
            "{:x}",
            Sha256::digest(data_dir.as_os_str().as_encoded_bytes())
        );
        Self {
            data_dir: data_dir.to_path_buf(),
            key,
            current,
        }
    }

    fn stamp(&self) -> String {
        format!(
            "{}:{}:{}",
            self.key, self.current.pid, self.current.started_at
        )
    }

    fn generation(&self, stamp: &str) -> Option<AppGeneration> {
        let (key, generation) = stamp.split_once(':')?;
        if key != self.key {
            return None;
        }
        let (pid, started_at) = generation.split_once(':')?;
        let pid: u32 = pid.parse().ok()?;
        let started_at: u64 = started_at.parse().ok()?;
        (pid > 0 && started_at > 0).then_some(AppGeneration {
            pid: Pid::from_u32(pid),
            started_at,
        })
    }
}

fn startup_process_scope() -> Option<&'static StartupProcessScope> {
    static SCOPE: OnceLock<Option<StartupProcessScope>> = OnceLock::new();
    SCOPE
        .get_or_init(|| {
            let data_dir = crate::app_dirs::myagents_data_dir()?.canonicalize().ok()?;
            let pid = Pid::from_u32(std::process::id());
            let mut system = System::new();
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                ProcessRefreshKind::nothing(),
            );
            let started_at = system.process(pid)?.start_time();
            (started_at > 0)
                .then(|| StartupProcessScope::new(&data_dir, AppGeneration { pid, started_at }))
        })
        .as_ref()
}

/// Mark the roots owned by crash recovery. Descendants that inherit the stamp
/// can still be identified after their parent has exited and they are reparented.
pub(crate) fn mark_startup_owned_process(command: &mut Command) {
    if let Some(scope) = startup_process_scope() {
        command.env(STARTUP_OWNER_ENV, scope.stamp());
    } else {
        command.env_remove(STARTUP_OWNER_ENV);
    }
}

fn process_env<'a>(process: &'a sysinfo::Process, name: &str) -> Option<&'a str> {
    process.environ().iter().find_map(|entry| {
        let (key, value) = entry.to_str()?.split_once('=')?;
        (key == name).then_some(value)
    })
}

fn startup_stamp(process: &sysinfo::Process) -> Option<&str> {
    process_env(process, STARTUP_OWNER_ENV)
}

fn is_legacy_sidecar_in_profile(process: &sysinfo::Process, scope: &StartupProcessScope) -> bool {
    // Both historical Sidecar launch paths inject this ID. Use the home only
    // to protect the ref inventory from a possible writer during migration.
    if process_env(process, "MYAGENTS_SIDECAR_ID").is_none() {
        return false;
    }
    let home_key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    process_env(process, home_key)
        .and_then(|home| Path::new(home).canonicalize().ok())
        .is_some_and(|home| Some(home.as_path()) == scope.data_dir.parent())
}

fn is_app_process(process: &sysinfo::Process) -> bool {
    matches!(
        process
            .name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .as_str(),
        "agenteam" | "agenteam.exe" | "myagents" | "myagents.exe"
    )
}

pub(crate) fn kill_stale_startup_processes() -> Result<CleanupReport, &'static str> {
    let scope =
        startup_process_scope().ok_or("cannot identify this app's data directory or process")?;
    kill_stale_processes_in_scope(scope)
}

fn kill_stale_processes_in_scope(
    scope: &StartupProcessScope,
) -> Result<CleanupReport, &'static str> {
    let started = Instant::now();
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_environ(sysinfo::UpdateKind::Always),
    );

    let mut roots = HashSet::new();
    for (&pid, process) in system.processes() {
        if pid == scope.current.pid
            || process.status() == ProcessStatus::Zombie
            || is_app_process(process)
        {
            continue;
        }
        let Some(stamp) = startup_stamp(process) else {
            if is_legacy_sidecar_in_profile(process, scope) {
                return Err("a prior Sidecar in this data directory has no owner stamp");
            }
            continue;
        };
        let Some(owner) = scope.generation(stamp) else {
            continue;
        };
        if owner == scope.current || process.start_time() < owner.started_at {
            continue;
        }
        // A PID reused by a newer process does not keep the old generation
        // alive. When its creation time is unavailable, leave it alone.
        if system.process(owner.pid).is_some_and(|process| {
            process.status() != ProcessStatus::Zombie
                && (process.start_time() == 0 || process.start_time() == owner.started_at)
        }) {
            return Err("another app generation still owns processes in this data directory");
        }
        roots.insert(pid);
    }

    let targets = descendant_targets(&system, &roots, |process, parent| {
        process.pid() != scope.current.pid
            && !is_app_process(process)
            && process.start_time() >= parent.start_time()
            && (startup_stamp(process).is_none() || roots.contains(&process.pid()))
    });
    Ok(terminate_targets(
        system,
        roots.len(),
        targets,
        started,
        terminate_startup_process,
    ))
}

#[cfg(windows)]
fn terminate_startup_process(process: &sysinfo::Process) -> bool {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_TERMINATE,
    };

    // Use the same kernel process object for the birth-time check and the
    // termination so PID reuse cannot redirect the signal after that check.
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            0,
            process.pid().as_u32(),
        )
    };
    if raw.is_null() {
        return false;
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
    let raw = handle.as_raw_handle();
    let [mut creation, mut exit, mut kernel, mut user] = [FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    }; 4];
    if unsafe { GetProcessTimes(raw, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return false;
    }
    let filetime = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    const WINDOWS_EPOCH_OFFSET_SECONDS: u64 = 11_644_473_600;
    let start_time = (filetime / 10_000_000).checked_sub(WINDOWS_EPOCH_OFFSET_SECONDS);
    start_time == Some(process.start_time()) && unsafe { TerminateProcess(raw, 1) != 0 }
}

#[cfg(not(windows))]
fn terminate_startup_process(process: &sysinfo::Process) -> bool {
    process.kill()
}

/// A substring pattern tested against a process's full command line (the
/// `argv` array joined by space).
///
/// Patterns MUST use forward slashes for path components even when targeting
/// Windows paths — the matcher normalizes `\` → `/` before comparison so a
/// single pattern handles both separator conventions.
#[derive(Debug, Clone, Copy)]
pub struct ProcessPattern {
    pub name: &'static str,
    pub pattern: &'static str,
}

impl ProcessPattern {
    pub const fn new(name: &'static str, pattern: &'static str) -> Self {
        Self { name, pattern }
    }
}

#[derive(Debug, Default, Clone)]
pub struct CleanupReport {
    /// Processes selected directly by the recovery policy.
    pub matched_roots: usize,
    /// Additional descendants (via PPID) killed alongside matched roots.
    pub descendants: usize,
    /// Processes actually terminated successfully.
    pub killed: usize,
    /// Processes still alive after the termination deadline.
    pub residual: usize,
    /// PIDs still alive after the termination deadline. Includes descendant
    /// processes that may not match MyAgents command-line/root patterns
    /// themselves, so update shutdown can keep verifying them explicitly.
    pub residual_pids: Vec<u32>,
    /// Total wall-clock time spent in this call.
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub struct ProcessMatch {
    pub pid: u32,
    pub name: String,
    pub reason: String,
    pub exe: Option<String>,
    pub cmd: String,
}

impl CleanupReport {
    pub fn total_targets(&self) -> usize {
        self.matched_roots + self.descendants
    }
}

/// Normalize a command-line string or pattern for substring matching:
/// backslashes → forward slashes (so one pattern covers both separator
/// conventions), and lowercase (so we match Windows' case-insensitive
/// filesystem behavior, reproducing the prior PowerShell `-like` semantics).
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\\' {
            out.push('/');
        } else {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
        }
    }
    out
}

fn normalize_path_boundary(s: &str) -> String {
    let stripped = s
        .strip_prefix(r"\\?\")
        .or_else(|| s.strip_prefix(r"//?/"))
        .unwrap_or(s);
    normalize(stripped).trim_end_matches('/').to_string()
}

fn normalized_roots(roots: &[PathBuf]) -> Vec<String> {
    roots
        .iter()
        .filter_map(|root| {
            let normalized = normalize_path_boundary(&root.to_string_lossy());
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn is_under_root(candidate: &str, root: &str) -> bool {
    candidate == root
        || candidate
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn contains_root_reference(haystack: &str, root: &str) -> bool {
    let mut offset = 0usize;
    while let Some(found) = haystack[offset..].find(root) {
        let start = offset + found;
        let end = start + root.len();
        let before_ok = start == 0
            || haystack[..start].chars().next_back().is_some_and(|c| {
                c.is_whitespace() || c == '"' || c == '\'' || c == '/' || c == '='
            });
        let after_ok = end == haystack.len()
            || haystack[end..]
                .chars()
                .next()
                .is_some_and(|c| c == '/' || c.is_whitespace() || c == '"' || c == '\'');
        if before_ok && after_ok {
            return true;
        }
        offset = end;
    }
    false
}

fn process_match_reason(
    cmd_norm: &str,
    exe_norm: Option<&str>,
    patterns: &[String],
    roots: &[String],
) -> Option<String> {
    for pattern in patterns {
        if cmd_norm.contains(pattern.as_str()) {
            return Some(format!("cmd contains '{}'", pattern));
        }
    }

    for root in roots {
        if let Some(exe) = exe_norm {
            if is_under_root(exe, root) {
                return Some(format!("exe under '{}'", root));
            }
        }
        if contains_root_reference(cmd_norm, root) {
            return Some(format!("cmd references '{}'", root));
        }
    }

    None
}

/// Enumerate, terminate, and confirm death of all processes whose command
/// line matches any of `patterns`, plus their descendants by PPID.
///
/// Updater recovery uses these patterns to locate installation file holders.
/// Startup recovery uses `kill_stale_startup_processes` instead.
///
/// Wait budget for confirmed termination: **3 s**. Any process still alive
/// after that is counted in `residual` and logged by callers.
pub fn kill_stale_processes(patterns: &[ProcessPattern]) -> CleanupReport {
    kill_stale_processes_with_roots(patterns, &[])
}

/// Enumerate, terminate, and confirm death of all processes matching command
/// line patterns or running from/with argv references to one of `protected_roots`.
///
/// The roots path is used by the Windows updater shutdown path: anything still
/// executing from the current MyAgents install/resource directory can hold
/// files that NSIS needs to overwrite, even if its argv no longer includes a
/// legacy marker.
pub fn kill_stale_processes_with_roots(
    patterns: &[ProcessPattern],
    protected_roots: &[PathBuf],
) -> CleanupReport {
    let started = Instant::now();
    let mut system = System::new();
    // Refresh with CMD info so Process::cmd() is populated.
    // `remove_dead_processes=true` — IMPORTANT: when false, sysinfo keeps
    // stale entries in the map even after the process has exited, and our
    // later liveness polling loop would always see "alive" and wait the
    // full deadline. Verified against sysinfo 0.33 source (common/system.rs).
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_exe(sysinfo::UpdateKind::Always),
    );

    let self_pid = Pid::from_u32(std::process::id());

    // Pre-normalize patterns once — saves per-process string allocation.
    let norm_patterns: Vec<String> = patterns.iter().map(|p| normalize(p.pattern)).collect();
    let norm_roots = normalized_roots(protected_roots);

    // Find root matches by command-line pattern.
    let mut roots: HashSet<Pid> = HashSet::new();
    for (pid, proc) in system.processes() {
        if *pid == self_pid {
            continue;
        }
        let cmd_raw: String = proc
            .cmd()
            .iter()
            .map(|os| os.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let cmd_norm = normalize(&cmd_raw);
        let exe_norm = proc
            .exe()
            .map(|p| normalize_path_boundary(&p.to_string_lossy()));
        if cmd_raw.is_empty() && exe_norm.is_none() {
            continue;
        }
        if process_match_reason(&cmd_norm, exe_norm.as_deref(), &norm_patterns, &norm_roots)
            .is_some()
        {
            roots.insert(*pid);
        }
    }
    let targets = descendant_targets(&system, &roots, |process, _| process.pid() != self_pid);
    terminate_targets(
        system,
        roots.len(),
        targets,
        started,
        sysinfo::Process::kill,
    )
}

fn descendant_targets(
    system: &System,
    roots: &HashSet<Pid>,
    follow_child: impl Fn(&sysinfo::Process, &sysinfo::Process) -> bool,
) -> HashSet<Pid> {
    let mut children_of: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (&pid, process) in system.processes() {
        if let Some(parent) = process.parent().and_then(|pid| system.process(pid)) {
            if follow_child(process, parent) {
                children_of.entry(parent.pid()).or_default().push(pid);
            }
        }
    }

    // BFS: collect all descendants of matched roots.
    let mut to_kill: HashSet<Pid> = roots.clone();
    let mut queue: Vec<Pid> = roots.iter().copied().collect();
    while let Some(p) = queue.pop() {
        if let Some(kids) = children_of.get(&p) {
            for kid in kids {
                if to_kill.insert(*kid) {
                    queue.push(*kid);
                }
            }
        }
    }
    to_kill
}

fn terminate_targets(
    mut system: System,
    matched_roots: usize,
    to_kill: HashSet<Pid>,
    started: Instant,
    terminate: impl Fn(&sysinfo::Process) -> bool,
) -> CleanupReport {
    let descendants = to_kill.len().saturating_sub(matched_roots);
    if to_kill.is_empty() {
        return CleanupReport {
            elapsed: started.elapsed(),
            ..Default::default()
        };
    }

    let mut killed = 0;
    let mut target_start_times = HashMap::new();
    for pid in to_kill {
        if let Some(proc) = system.process(pid) {
            target_start_times.insert(pid, proc.start_time());
            if terminate(proc) {
                killed += 1;
            }
        }
    }

    // Confirm death. TerminateProcess is synchronous in theory but the
    // kernel handle-closure finalizer can lag a few ms. 3 s deadline
    // covers worst-case; in practice this loop exits in <50 ms.
    //
    // CRITICAL: `remove_dead_processes=true` in the refresh call — sysinfo
    // otherwise keeps dead PIDs in its internal HashMap and every poll
    // would see the process as still present, forcing us to wait the full
    // deadline on every real cleanup run. With `true`, dead PIDs are
    // purged from the map and `system.process(pid)` correctly returns
    // None once the kernel releases the handle.
    let deadline = started + Duration::from_secs(3);
    let kill_slice: Vec<Pid> = target_start_times.keys().copied().collect();
    let residual_pids: Vec<u32>;
    loop {
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&kill_slice),
            true,
            ProcessRefreshKind::nothing(),
        );
        let alive: Vec<Pid> = kill_slice
            .iter()
            .copied()
            .filter(|pid| {
                system.process(*pid).is_some_and(|process| {
                    process.start_time() == target_start_times[pid]
                        && process.status() != ProcessStatus::Zombie
                })
            })
            .collect();
        if alive.is_empty() || Instant::now() >= deadline {
            residual_pids = alive.iter().map(|pid| pid.as_u32()).collect();
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    CleanupReport {
        matched_roots,
        descendants,
        killed,
        residual: residual_pids.len(),
        residual_pids,
        elapsed: started.elapsed(),
    }
}

/// Test if any live process still matches any of `patterns` (excluding self).
///
/// Used by the update-shutdown path to verify an earlier termination pass
/// actually completed before handing control off to the NSIS installer.
pub fn has_matching_processes(patterns: &[ProcessPattern]) -> bool {
    has_matching_processes_with_roots(patterns, &[])
}

pub fn has_matching_processes_with_roots(
    patterns: &[ProcessPattern],
    protected_roots: &[PathBuf],
) -> bool {
    !find_matching_processes_with_roots(patterns, protected_roots).is_empty()
}

pub fn find_matching_processes_with_roots(
    patterns: &[ProcessPattern],
    protected_roots: &[PathBuf],
) -> Vec<ProcessMatch> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_exe(sysinfo::UpdateKind::Always),
    );
    let self_pid = Pid::from_u32(std::process::id());
    let norm_patterns: Vec<String> = patterns.iter().map(|p| normalize(p.pattern)).collect();
    let norm_roots = normalized_roots(protected_roots);
    let mut matches = Vec::new();
    for (pid, proc) in system.processes() {
        if *pid == self_pid {
            continue;
        }
        let cmd_raw: String = proc
            .cmd()
            .iter()
            .map(|os| os.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let cmd_norm = normalize(&cmd_raw);
        let exe = proc.exe().map(|p| p.to_string_lossy().into_owned());
        let exe_norm = exe.as_deref().map(normalize_path_boundary);
        if cmd_raw.is_empty() && exe_norm.is_none() {
            continue;
        }
        if let Some(reason) =
            process_match_reason(&cmd_norm, exe_norm.as_deref(), &norm_patterns, &norm_roots)
        {
            matches.push(ProcessMatch {
                pid: pid.as_u32(),
                name: proc.name().to_string_lossy().into_owned(),
                reason,
                exe,
                cmd: cmd_raw,
            });
        }
    }
    matches
}

pub fn find_live_processes_by_pid(pids: &[u32]) -> Vec<ProcessMatch> {
    let sysinfo_pids: Vec<Pid> = pids.iter().copied().map(Pid::from_u32).collect();
    if sysinfo_pids.is_empty() {
        return Vec::new();
    }

    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&sysinfo_pids),
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_exe(sysinfo::UpdateKind::Always),
    );
    sysinfo_pids
        .iter()
        .filter_map(|pid| {
            system
                .process(*pid)
                // Linux keeps a killed orphan visible as a zombie until its
                // reaper runs. It owns no executable resources and cannot be
                // killed again, so it must not block shutdown convergence.
                .filter(|proc| proc.status() != ProcessStatus::Zombie)
                .map(|proc| {
                    let cmd_raw: String = proc
                        .cmd()
                        .iter()
                        .map(|os| os.to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join(" ");
                    ProcessMatch {
                        pid: pid.as_u32(),
                        name: proc.name().to_string_lossy().into_owned(),
                        reason: "residual descendant after kill".to_string(),
                        exe: proc.exe().map(|p| p.to_string_lossy().into_owned()),
                        cmd: cmd_raw,
                    }
                })
        })
        .collect()
}

/// Query whether a specific PID corresponds to a MyAgents process.
/// Uses the executable path (`GetModuleFileNameExW` underneath), so it is
/// reliable whether the process was spawned via shortcut, installer, or
/// direct path. Case-insensitive substring match to absorb Windows
/// filesystem case quirks (`MyAgents` vs `myagents`).
pub fn is_myagents_pid(pid: u32) -> bool {
    let mut system = System::new();
    let only: [Pid; 1] = [Pid::from_u32(pid)];
    // remove_dead_processes=true so that a dead PID returns None from
    // system.process() below rather than a stale entry.
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&only),
        true,
        ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always),
    );
    let Some(proc) = system.process(Pid::from_u32(pid)) else {
        return false;
    };
    // Prefer exe_path (full path) — sysinfo falls back to name() internally.
    let haystack: String = proc
        .exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| proc.name().to_string_lossy().into_owned());
    let lower = haystack.to_ascii_lowercase();
    lower.contains("myagents")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "spawned by the startup cleanup regression test"]
    fn startup_cleanup_test_process() {
        if std::env::var_os("MYAGENTS_STARTUP_CLEANUP_TEST_CHILD").is_some() {
            std::thread::sleep(Duration::from_secs(120));
        }
    }

    fn spawn_cleanup_test_process(
        scope: Option<&StartupProcessScope>,
        legacy_home: Option<&Path>,
    ) -> crate::process_cmd::ChildTree {
        let mut command =
            crate::process_cmd::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--ignored",
                "--exact",
                "--skip=--myagents-sidecar",
                "process_cleanup::tests::startup_cleanup_test_process",
            ])
            .env("MYAGENTS_STARTUP_CLEANUP_TEST_CHILD", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        match scope {
            Some(scope) => command.env(STARTUP_OWNER_ENV, scope.stamp()),
            None => command.env_remove(STARTUP_OWNER_ENV),
        };
        command.env_remove("MYAGENTS_SIDECAR_ID");
        if let Some(home) = legacy_home {
            let home_key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
            command
                .env(home_key, home)
                .env("MYAGENTS_SIDECAR_ID", "legacy");
        }
        crate::process_cmd::spawn_tree(&mut command).expect("spawn cleanup test child")
    }

    fn test_generation(pid: u32) -> AppGeneration {
        let pid = Pid::from_u32(pid);
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        let started_at = system.process(pid).expect("test process").start_time();
        assert!(started_at > 0);
        AppGeneration { pid, started_at }
    }

    #[test]
    fn startup_cleanup_only_terminates_owned_stale_generation() {
        let acceptance_data = tempfile::tempdir().expect("acceptance data");
        let main_data = tempfile::tempdir().expect("main data");
        let current = test_generation(std::process::id());
        let stale = AppGeneration {
            // Simulate the old PID having already been reused by this process.
            started_at: current.started_at - 60,
            ..current
        };
        let acceptance_home = acceptance_data
            .path()
            .canonicalize()
            .expect("acceptance home");
        let main_home = main_data.path().canonicalize().expect("main home");
        let acceptance_scope_dir = acceptance_home.join(".agenteam");
        let startup_scope = StartupProcessScope::new(&acceptance_scope_dir, current);
        let stale_scope = StartupProcessScope::new(&acceptance_scope_dir, stale);
        let foreign_scope = StartupProcessScope::new(&main_home.join(".agenteam"), stale);
        let mut owned = spawn_cleanup_test_process(Some(&stale_scope), None);
        let mut foreign = spawn_cleanup_test_process(Some(&foreign_scope), None);
        let mut current_child = spawn_cleanup_test_process(Some(&startup_scope), None);
        let mut unmarked = spawn_cleanup_test_process(None, Some(&main_home));
        let mut live_owner = spawn_cleanup_test_process(None, None);
        let live_scope =
            StartupProcessScope::new(&acceptance_scope_dir, test_generation(live_owner.id()));
        let mut live_child = spawn_cleanup_test_process(Some(&live_scope), None);

        assert!(kill_stale_processes_in_scope(&startup_scope).is_err());
        assert!(owned
            .try_wait()
            .expect("owned process still inspectable")
            .is_none());
        assert!(live_child
            .try_wait()
            .expect("live owner's process")
            .is_none());
        live_owner.kill_and_wait().expect("retire previous owner");

        let mut legacy_writer = spawn_cleanup_test_process(None, Some(&acceptance_home));
        assert!(matches!(
            kill_stale_processes_in_scope(&startup_scope),
            Err("a prior Sidecar in this data directory has no owner stamp")
        ));
        assert!(legacy_writer.try_wait().expect("legacy writer").is_none());
        legacy_writer.kill_and_wait().expect("retire legacy writer");

        let report = kill_stale_processes_in_scope(&startup_scope).expect("scoped cleanup");
        assert!(report.total_targets() >= 2, "{report:?}");
        assert_eq!(report.residual, 0);
        assert!(owned.try_wait().expect("stale child").is_some());
        assert!(live_child
            .try_wait()
            .expect("newly orphaned child")
            .is_some());
        for child in [&mut foreign, &mut current_child, &mut unmarked] {
            assert!(child.try_wait().expect("out-of-scope child").is_none());
        }
    }

    #[test]
    fn root_match_accepts_windows_separator_variants() {
        let roots = normalized_roots(&[PathBuf::from(
            r"C:\Users\alice\AppData\Local\MyAgents\resources",
        )]);
        assert_eq!(
            process_match_reason(
                "c:/users/alice/appdata/local/myagents/resources/nodejs/node.exe server-dist.js",
                Some("c:/users/alice/appdata/local/myagents/resources/nodejs/node.exe"),
                &[],
                &roots,
            )
            .as_deref(),
            Some("exe under 'c:/users/alice/appdata/local/myagents/resources'")
        );
    }

    #[test]
    fn root_match_strips_extended_length_prefix() {
        let roots = normalized_roots(&[PathBuf::from(
            r"\\?\C:\Users\alice\AppData\Local\MyAgents\resources",
        )]);
        assert!(process_match_reason(
            "node c:/users/alice/appdata/local/myagents/resources/plugin-bridge-dist.mjs",
            None,
            &[],
            &roots,
        )
        .is_some());
    }

    #[test]
    fn root_match_does_not_match_prefix_siblings_by_exe() {
        let roots = normalized_roots(&[PathBuf::from(
            r"C:\Users\alice\AppData\Local\MyAgents\resources",
        )]);
        assert!(process_match_reason(
            "",
            Some("c:/users/alice/appdata/local/myagents/resources-old/node.exe"),
            &[],
            &roots,
        )
        .is_none());
    }

    #[test]
    fn root_match_does_not_match_prefix_siblings_by_cmd() {
        let roots = normalized_roots(&[PathBuf::from(
            r"C:\Users\alice\AppData\Local\MyAgents\resources",
        )]);
        assert!(process_match_reason(
            "node c:/users/alice/appdata/local/myagents/resources-old/server-dist.js",
            None,
            &[],
            &roots,
        )
        .is_none());
    }

    #[test]
    fn root_match_does_not_require_command_line_when_exe_matches() {
        let roots = normalized_roots(&[PathBuf::from(
            r"C:\Users\alice\AppData\Local\MyAgents\resources",
        )]);
        assert!(process_match_reason(
            "",
            Some("c:/users/alice/appdata/local/myagents/resources/nodejs/node.exe"),
            &[],
            &roots,
        )
        .is_some());
    }

    #[test]
    fn root_match_accepts_file_url_command_arguments() {
        let roots = normalized_roots(&[PathBuf::from(
            r"C:\Users\alice\AppData\Local\MyAgents\resources",
        )]);
        assert!(process_match_reason(
            "node --import=file:///c:/users/alice/appdata/local/myagents/resources/tsx-runtime/loader.mjs",
            None,
            &[],
            &roots,
        )
        .is_some());
    }
}
