//! Host egress admission. Only the policy is persisted; a previous process's
//! successful check is never evidence for a new launch.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};
use tauri::Emitter;
use tokio::sync::watch;

const CHECK_INTERVAL: Duration = Duration::from_secs(15);
const PROBE_TIMEOUT: Duration = Duration::from_secs(6);
const EVIDENCE_TTL: Duration = Duration::from_secs(25);
const MAX_IP_BODY: usize = 64;
const EVENT: &str = "egress-restriction-changed";

#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Policy {
    enabled: bool,
    #[serde(default)]
    expected_ip: String,
}

impl Policy {
    fn from_input(enabled: bool, input: &str) -> Result<Self, String> {
        let expected_ip = match public_ip(input) {
            Some(ip) => ip.to_string(),
            // Disabling must also recover a malformed stored address.
            None if !enabled => String::new(),
            None => return Err("invalid_expected_ip".into()),
        };
        Ok(Self {
            enabled,
            expected_ip,
        })
    }
}

fn public_ip(literal: &str) -> Option<IpAddr> {
    let ip = literal.trim().parse::<IpAddr>().ok()?;
    let public = match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, d] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_documentation()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 198 && (18..=19).contains(&b))
                || (a == 192 && b == 0 && c == 0 && d != 9 && d != 10))
        }
        IpAddr::V6(v6) => {
            // Global unicast excludes loopback, unspecified, ULA, link-local,
            // multicast and IPv4-mapped forms. Exclude documentation prefixes.
            let [a, b, ..] = v6.segments();
            (a & 0xe000) == 0x2000
                && (a, b) != (0x2001, 0x0db8)
                && !(a == 0x3fff && (b & 0xf000) == 0)
        }
    };
    public.then_some(ip)
}

fn config_path() -> Result<PathBuf, String> {
    crate::app_dirs::myagents_data_dir()
        .map(|dir| dir.join("config.json"))
        .ok_or_else(|| "egress_config_unavailable".into())
}

fn load_policy(path: &Path) -> Result<Policy, String> {
    // A missing file is normal on first launch. I/O errors must not inherit
    // read_config_json's missing-file default via Path::exists().
    if !path.try_exists().map_err(|_| "egress_config_unavailable")? {
        return Ok(Policy::default());
    }
    let root = crate::config_io::read_config_json(path)?;
    let object = root.as_object().ok_or("egress_config_unavailable")?;
    match object.get("egressRestriction") {
        Some(policy) => {
            serde_json::from_value(policy.clone()).map_err(|_| "egress_config_unavailable".into())
        }
        None => Ok(Policy::default()),
    }
}

fn save_policy(path: &Path, policy: &Policy) -> Result<(), String> {
    crate::config_io::with_config_lock(path, false, |root| {
        let object = root.as_object_mut().ok_or("egress_config_unavailable")?;
        object.insert("egressRestriction".into(), serde_json::json!(policy));
        Ok(())
    })
    .map(|_| ())
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CheckState {
    Disabled,
    Checking,
    Matched,
    Mismatch,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EgressRestrictionStatus {
    pub enabled: bool,
    pub expected_ip: String,
    pub observed_ip: Option<String>,
    pub state: CheckState,
    pub lock_epoch: u64,
    pub policy_epoch: u64,
}

impl EgressRestrictionStatus {
    pub fn is_allowed(&self) -> bool {
        matches!(self.state, CheckState::Disabled | CheckState::Matched)
    }

    pub fn must_lock(&self) -> bool {
        matches!(self.state, CheckState::Mismatch | CheckState::Unavailable)
    }
}

// Instant excludes suspended time on some platforms. Checking both clocks
// prevents a sleeping computer from waking with stale permission.
#[derive(Clone, Copy)]
struct SampleTime {
    monotonic: Instant,
    wall: SystemTime,
}

impl SampleTime {
    fn now() -> Self {
        Self {
            monotonic: Instant::now(),
            wall: SystemTime::now(),
        }
    }

    fn is_fresh_at(self, now: Self, limit: Duration) -> bool {
        now.monotonic
            .checked_duration_since(self.monotonic)
            .is_some_and(|age| age < limit)
            && now
                .wall
                .duration_since(self.wall)
                .is_ok_and(|age| age < limit)
    }
}

struct Gate {
    status: EgressRestrictionStatus,
    last_sample: Option<SampleTime>,
    // Start of the latest completed attempt, including negative results. A
    // manual retry after a network change needs a connection opened after it.
    last_attempt: Option<SampleTime>,
}

impl Gate {
    fn new(policy: Result<Policy, String>) -> Self {
        let mut gate = Self {
            status: EgressRestrictionStatus {
                enabled: false,
                expected_ip: String::new(),
                observed_ip: None,
                state: CheckState::Disabled,
                lock_epoch: 0,
                policy_epoch: 0,
            },
            last_sample: None,
            last_attempt: None,
        };
        gate.replace(policy);
        gate
    }

    fn replace(&mut self, policy: Result<Policy, String>) {
        let policy = policy.unwrap_or_else(|_| Policy {
            enabled: true,
            expected_ip: String::new(),
        });
        if self.status.enabled == policy.enabled && self.status.expected_ip == policy.expected_ip {
            return;
        }
        self.status.enabled = policy.enabled;
        self.status.expected_ip = policy.expected_ip;
        self.status.policy_epoch += 1;
        self.status.observed_ip = None;
        self.last_sample = None;
        self.last_attempt = None;
        if !self.status.enabled {
            self.transition(CheckState::Disabled);
        } else if public_ip(&self.status.expected_ip).is_some() {
            self.transition(CheckState::Checking);
        } else {
            self.transition(CheckState::Unavailable);
        }
    }

    fn transition(&mut self, next: CheckState) {
        let was_locked = self.status.must_lock();
        self.status.state = next;
        if self.status.must_lock() && !was_locked {
            self.status.lock_epoch += 1;
        }
    }

    fn expire(&mut self, now: SampleTime) {
        if self.status.state == CheckState::Matched
            && !self
                .last_sample
                .is_some_and(|at| at.is_fresh_at(now, EVIDENCE_TTL))
        {
            self.status.observed_ip = None;
            self.transition(CheckState::Unavailable);
        }
    }

    fn finish(
        &mut self,
        epoch: u64,
        expected: IpAddr,
        sample: Option<IpAddr>,
        start: SampleTime,
        now: SampleTime,
    ) {
        if self.status.policy_epoch != epoch {
            return;
        }
        self.expire(now);
        self.last_attempt = Some(start);
        let observed = sample.filter(|_| start.is_fresh_at(now, PROBE_TIMEOUT));
        self.status.observed_ip = observed.map(|ip| ip.to_string());
        let state = match observed {
            Some(ip) if ip == expected => {
                self.last_sample = Some(start);
                CheckState::Matched
            }
            Some(_) => CheckState::Mismatch,
            None => CheckState::Unavailable,
        };
        self.transition(state);
    }
}

struct Authority {
    gate: Mutex<Gate>,
    changed: watch::Sender<EgressRestrictionStatus>,
}

static AUTHORITY: LazyLock<Authority> = LazyLock::new(|| {
    #[cfg(not(test))]
    let gate = Gate::new(config_path().and_then(|path| load_policy(&path)));
    // Unit/integration test binaries must not inherit the developer's private
    // egress configuration. Disk bootstrap is covered with temporary fixtures.
    #[cfg(test)]
    let gate = Gate::new(Ok(Policy::default()));
    let (changed, _) = watch::channel(gate.status.clone());
    Authority {
        gate: Mutex::new(gate),
        changed,
    }
});
static PROBE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static SETTINGS_WRITER: Mutex<()> = Mutex::new(());

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn with_gate<T>(action: impl FnOnce(&mut Gate) -> T) -> T {
    let mut gate = lock(&AUTHORITY.gate);
    gate.expire(SampleTime::now());
    let result = action(&mut gate);
    AUTHORITY.changed.send_if_modified(|last| {
        if *last == gate.status {
            false
        } else {
            *last = gate.status.clone();
            true
        }
    });
    result
}

pub fn subscribe_access() -> watch::Receiver<EgressRestrictionStatus> {
    AUTHORITY.changed.subscribe()
}

pub fn is_allowed() -> bool {
    with_gate(|gate| gate.status.is_allowed())
}

pub fn ensure_allowed() -> Result<(), String> {
    is_allowed()
        .then_some(())
        .ok_or_else(|| "egress_restriction_locked".into())
}

pub fn restriction_enabled() -> bool {
    with_gate(|gate| gate.status.enabled)
}

/// Exit IP for a model turn that is about to start. Unlike [`required_exit_ip`]
/// this never trusts evidence older than `max_age`: a stale or missing sample
/// triggers a new probe first. `Ok(None)` means no restriction is configured.
pub async fn fresh_exit_ip(max_age: Duration) -> Result<Option<IpAddr>, String> {
    if !restriction_enabled() {
        return Ok(None);
    }
    let stale = with_gate(|gate| {
        !gate
            .last_sample
            .is_some_and(|at| at.is_fresh_at(SampleTime::now(), max_age))
    });
    if stale {
        refresh(true).await;
    }
    required_exit_ip()
}

pub fn required_exit_ip() -> Result<Option<IpAddr>, String> {
    with_gate(|gate| {
        if !gate.status.enabled {
            return Ok(None);
        }
        if !gate.status.is_allowed() {
            return Err("egress_restriction_locked".into());
        }
        gate.status
            .expected_ip
            .parse()
            .map(Some)
            .map_err(|_| "invalid_expected_ip".into())
    })
}

fn parse_observation(body: &[u8]) -> Option<IpAddr> {
    if body.len() > MAX_IP_BODY {
        return None;
    }
    public_ip(std::str::from_utf8(body).ok()?)
}

async fn observe(expected: IpAddr) -> Option<IpAddr> {
    let url = if expected.is_ipv4() {
        "https://api.ipify.org/"
    } else {
        "https://api6.ipify.org/"
    };
    let ip = tokio::time::timeout(PROBE_TIMEOUT, host_probe(url))
        .await
        .ok()??;
    (ip.is_ipv4() == expected.is_ipv4()).then_some(ip)
}

#[cfg(not(windows))]
async fn host_probe(url: &'static str) -> Option<IpAddr> {
    // External HTTPS probe deliberately uses the host's default client policy.
    // A fresh client observes changed proxy settings and cannot reuse an
    // already-connected socket from a previous network.
    #[allow(clippy::disallowed_methods)]
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let mut response = client
        .get(url)
        .header("Cache-Control", "no-cache, no-store")
        .send()
        .await
        .ok()?;
    if response.status() != reqwest::StatusCode::OK {
        return None;
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > MAX_IP_BODY - body.len() {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    parse_observation(&body)
}

#[cfg(windows)]
async fn host_probe(url: &'static str) -> Option<IpAddr> {
    // A timed-out synchronous OS proxy/PAC lookup may still be winding down.
    // Hold the permit in the blocking worker so retries never pile up.
    static WORKER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
    let permit = WORKER.try_acquire().ok()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        windows_probe(url)
    })
    .await
    .ok()?
}

#[cfg(windows)]
fn windows_probe(url: &str) -> Option<IpAddr> {
    use std::{ffi::c_void, ptr};
    use windows_sys::Win32::Networking::WinInet::*;

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: constructed only from a non-null, owned WinINet handle.
            unsafe {
                InternetCloseHandle(self.0);
            }
        }
    }
    fn owned(raw: *mut c_void) -> Option<Handle> {
        if raw.is_null() {
            None
        } else {
            Some(Handle(raw))
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    // SAFETY: strings supplied to WinINet are NUL-terminated UTF-16; all
    // handles and buffers live until synchronous calls return. PRECONFIG
    // follows the current user's Windows proxy/PAC settings. No app proxy
    // overlay, proxy fallback request, shared cookies, or redirect is used.
    unsafe {
        let session = owned(InternetOpenW(
            wide("AgenTeam").as_ptr(),
            INTERNET_OPEN_TYPE_PRECONFIG,
            ptr::null(),
            ptr::null(),
            0,
        ))?;
        let ms = PROBE_TIMEOUT.as_millis() as u32;
        for option in [
            INTERNET_OPTION_CONNECT_TIMEOUT,
            INTERNET_OPTION_SEND_TIMEOUT,
            INTERNET_OPTION_RECEIVE_TIMEOUT,
        ] {
            if InternetSetOptionW(
                session.0,
                option,
                (&ms as *const u32).cast(),
                size_of::<u32>() as u32,
            ) == 0
            {
                return None;
            }
        }
        let flags = INTERNET_FLAG_SECURE
            | INTERNET_FLAG_RELOAD
            | INTERNET_FLAG_NO_CACHE_WRITE
            | INTERNET_FLAG_NO_AUTO_REDIRECT
            | INTERNET_FLAG_NO_AUTH
            | INTERNET_FLAG_NO_COOKIES
            | INTERNET_FLAG_NO_UI;
        let headers = wide("Cache-Control: no-cache, no-store\r\nPragma: no-cache\r\n");
        let request = owned(InternetOpenUrlW(
            session.0,
            wide(url).as_ptr(),
            headers.as_ptr(),
            (headers.len() - 1) as u32,
            flags,
            0,
        ))?;
        let mut code = 0_u32;
        let mut len = size_of::<u32>() as u32;
        if HttpQueryInfoW(
            request.0,
            HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
            (&mut code as *mut u32).cast(),
            &mut len,
            ptr::null_mut(),
        ) == 0
            || code != 200
        {
            return None;
        }
        let mut body = [0_u8; MAX_IP_BODY + 1];
        let mut used = 0;
        loop {
            let mut n = 0;
            if InternetReadFile(
                request.0,
                body[used..].as_mut_ptr().cast(),
                (body.len() - used) as u32,
                &mut n,
            ) == 0
            {
                return None;
            }
            if n == 0 {
                return parse_observation(&body[..used]);
            }
            used += n as usize;
            if used > MAX_IP_BODY {
                return None;
            }
        }
    }
}

async fn refresh(force: bool) -> EgressRestrictionStatus {
    let requested = Instant::now();
    let before = cmd_get_egress_restriction();
    if !before.enabled {
        return before;
    }
    let _probe = PROBE.lock().await;
    let target = with_gate(|gate| {
        if !gate.status.enabled
            || gate.last_attempt.is_some_and(|at| {
                // A concurrent caller already finished a fresh check for this policy.
                at.monotonic >= requested
                    || (!force && at.is_fresh_at(SampleTime::now(), CHECK_INTERVAL))
            })
        {
            return None;
        }
        Some((
            gate.status.policy_epoch,
            public_ip(&gate.status.expected_ip)?,
        ))
    });
    if let Some((epoch, expected)) = target {
        let start = SampleTime::now();
        let sample = observe(expected).await;
        with_gate(|gate| gate.finish(epoch, expected, sample, start, SampleTime::now()));
    }
    cmd_get_egress_restriction()
}

pub async fn verify_for_startup() -> bool {
    refresh(true).await.is_allowed()
}

pub fn start_monitor(app: tauri::AppHandle) {
    let mut updates = subscribe_access();
    let mut policy_updates = subscribe_access();
    tauri::async_runtime::spawn(async move {
        while updates.changed().await.is_ok() {
            let status = updates.borrow_and_update().clone();
            let _ = app.emit(EVENT, status);
        }
    });
    tauri::async_runtime::spawn(async move {
        loop {
            let current = policy_updates.borrow_and_update().clone();
            if current.enabled {
                refresh(false).await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else if policy_updates.changed().await.is_err() {
                return;
            }
        }
    });
}

#[tauri::command]
pub fn cmd_get_egress_restriction() -> EgressRestrictionStatus {
    with_gate(|gate| gate.status.clone())
}

#[tauri::command]
pub async fn cmd_set_egress_restriction(
    enabled: bool,
    expected_ip: String,
) -> Result<EgressRestrictionStatus, String> {
    let policy = Policy::from_input(enabled, &expected_ip)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _writer = lock(&SETTINGS_WRITER);
        save_policy(&config_path()?, &policy)?;
        with_gate(|gate| gate.replace(Ok(policy)));
        Ok::<_, String>(())
    })
    .await
    .map_err(|err| err.to_string())??;
    Ok(refresh(true).await)
}

#[tauri::command]
pub async fn cmd_recheck_egress_restriction() -> EgressRestrictionStatus {
    refresh(true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(start: SampleTime, seconds: u64) -> SampleTime {
        let elapsed = Duration::from_secs(seconds);
        SampleTime {
            monotonic: start.monotonic + elapsed,
            wall: start.wall + elapsed,
        }
    }

    fn policy(ip: &str) -> Policy {
        Policy::from_input(true, ip).unwrap()
    }

    #[test]
    fn input_requires_one_ip_and_canonicalizes_equivalent_ipv6() {
        for bad in [
            "",
            "node.example",
            "https://8.8.8.8",
            "8.8.8.8:443",
            "192.0.2.0/24",
            "[::1]",
            "8.8.8.8\n1.1.1.1",
        ] {
            assert!(Policy::from_input(true, bad).is_err());
        }
        assert_eq!(
            policy(" 2606:4700:4700:0:0:0:0:1111 ").expected_ip,
            "2606:4700:4700::1111"
        );
        assert_eq!(
            Policy::from_input(false, "bad stored setting")
                .unwrap()
                .expected_ip,
            ""
        );
    }

    #[test]
    fn rejects_nonpublic_literals_in_settings_and_probe_results() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.9",
            "192.168.3.1",
            "0.0.0.0",
            "169.254.1.2",
            "100.64.1.2",
            "198.18.0.1",
            "224.1.1.1",
            "255.255.255.255",
            "::",
            "::1",
            "fd00::1",
            "fe80::1",
            "ff01::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
        ] {
            assert!(Policy::from_input(true, address).is_err(), "{address}");
            assert!(parse_observation(address.as_bytes()).is_none(), "{address}");
        }
    }

    #[test]
    fn only_fresh_matching_observation_grants_access() {
        let ip: IpAddr = "1.1.1.1".parse().unwrap();
        let mut gate = Gate::new(Ok(policy("1.1.1.1")));
        let epoch = gate.status.policy_epoch;
        let t = SampleTime::now();
        assert!(!gate.status.is_allowed());
        gate.finish(epoch, ip, None, t, t);
        assert_eq!(gate.status.state, CheckState::Unavailable);
        gate.finish(epoch, ip, Some("9.9.9.9".parse().unwrap()), t, t);
        assert_eq!(gate.status.state, CheckState::Mismatch);
        gate.finish(epoch, ip, Some(ip), t, t);
        assert!(gate.status.is_allowed());
        let seen = gate.status.lock_epoch;
        gate.expire(after(t, 24));
        assert!(gate.status.is_allowed());
        gate.expire(after(t, 25));
        assert!(gate.status.must_lock());
        assert!(gate.status.lock_epoch > seen);
        assert_eq!(gate.status.observed_ip, None);
    }

    #[test]
    fn rejects_samples_crossing_sleep_timeout_or_policy_replacement() {
        let mut gate = Gate::new(Ok(policy("1.1.1.1")));
        let epoch = gate.status.policy_epoch;
        let t = SampleTime::now();
        let ip = gate.status.expected_ip.parse().unwrap();
        let wake = SampleTime {
            monotonic: t.monotonic,
            wall: t.wall + Duration::from_secs(60),
        };
        gate.finish(epoch, ip, Some(ip), t, wake);
        assert!(gate.status.must_lock());
        gate.replace(Ok(policy("9.9.9.9")));
        gate.finish(epoch, ip, Some(ip), t, t);
        assert_eq!(gate.status.state, CheckState::Checking);
        gate.replace(Ok(Policy::default()));
        gate.finish(epoch, ip, None, t, t);
        assert_eq!(gate.status.state, CheckState::Disabled);
    }

    #[test]
    fn policy_storage_preserves_other_keys_and_malformed_data_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        assert!(Gate::new(load_policy(&path)).status.is_allowed());
        std::fs::write(&path, r#"{"proxySettings":{"enabled":true}}"#).unwrap();
        save_policy(&path, &policy("1.1.1.1")).unwrap();
        let root = crate::config_io::read_config_json(&path).unwrap();
        assert_eq!(root["proxySettings"]["enabled"], true);
        assert_eq!(root["egressRestriction"]["expectedIp"], "1.1.1.1");
        assert_eq!(
            Gate::new(load_policy(&path)).status.state,
            CheckState::Checking
        );
        for malformed in [
            "bad json",
            "[]",
            r#"{"egressRestriction":{}}"#,
            r#"{"egressRestriction":{"enabled":true,"expectedIp":"bad"}}"#,
        ] {
            std::fs::write(&path, malformed).unwrap();
            assert!(Gate::new(load_policy(&path)).status.must_lock());
        }
    }

    #[tokio::test]
    async fn model_turn_admission_is_open_without_a_configured_restriction() {
        // The test authority starts with the default (disabled) policy.
        assert_eq!(fresh_exit_ip(Duration::ZERO).await, Ok(None));
        assert!(crate::proxy_config::ensure_model_egress_admitted().is_ok());
    }

    #[test]
    fn echo_response_must_be_a_single_bounded_ip_literal() {
        let ip: IpAddr = "1.1.1.1".parse().unwrap();
        assert_eq!(parse_observation(b"1.1.1.1\n"), Some(ip));
        for body in [
            b"" as &[u8],
            b"1.1.1.1\n9.9.9.9",
            b"<html>login</html>",
            &[b' '; MAX_IP_BODY + 1],
        ] {
            assert_eq!(parse_observation(body), None);
        }
    }
}
