//! One-way sync of the shared user-profile chapters from the primary assistant
//! to the other assistants.
//!
//! The profile is a workspace file (`.claude/rules/03-USER.md`), so there is no
//! App-level profile store. The primary assistant's file is the authority for
//! the chapters that describe the person (background, communication style,
//! working style, preferences, writing style). Every other assistant whose
//! USER file uses the fixed chapters receives those chapters; its own basic
//! information, recent observations and source summary are never touched.
//!
//! The primary is the earliest registered assistant (order in `agents[]`, which
//! is append-only) whose USER file has the fixed chapters, so a legacy
//! free-form profile never becomes the source of a sync it cannot feed.
//!
//! A target's previous file is saved beside it before every overwrite, so a hand
//! edit to a synced chapter is recoverable even when it was made after an
//! earlier sync the same day.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::workspace_files::path_safety::{
    read_workspace_file_no_follow, write_workspace_file_no_follow,
};

/// Chapters that describe the person and are shared between assistants.
pub const SYNCED_CHAPTERS: [&str; 5] = ["背景", "沟通风格", "工作方式", "偏好", "写作风格"];
/// Chapters every file must have to take part in the sync, in either role.
const OWN_CHAPTERS: [&str; 3] = ["基本信息", "近期观察", "来源概况"];
pub const USER_FILE: &str = ".claude/rules/03-USER.md";
const MAX_PROFILE_BYTES: u64 = 256 * 1024;
const KEPT_BACKUPS: usize = 5;
pub const SYNC_INTERVAL_SECS: u64 = 600;

struct Section {
    heading: String,
    lines: Vec<String>,
}

fn fence_open(line: &str, open: Option<&str>) -> (Option<String>, bool) {
    let trimmed = line.trim_start_matches(' ');
    let marker = (line.len() - trimmed.len() <= 3)
        .then(|| {
            ["```", "~~~"]
                .into_iter()
                .find(|marker| trimmed.starts_with(marker))
        })
        .flatten();
    match (marker, open) {
        (Some(marker), None) => (Some(marker.to_string()), true),
        (Some(marker), Some(open)) if marker == open => (None, true),
        (Some(_), Some(open)) => (Some(open.to_string()), true),
        (None, open) => (open.map(str::to_string), false),
    }
}

/// Preamble lines and `## ` sections; a `## ` line inside a code fence is content.
fn parse(text: &str) -> (Vec<String>, Vec<Section>) {
    let mut preamble = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut fence: Option<String> = None;
    for line in text.split('\n') {
        let (next, is_fence_line) = fence_open(line, fence.as_deref());
        let heading = (!is_fence_line && next.is_none())
            .then(|| line.strip_prefix("## "))
            .flatten()
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty());
        fence = next;
        match heading {
            Some(heading) => sections.push(Section {
                heading,
                lines: vec![line.to_string()],
            }),
            None => match sections.last_mut() {
                Some(section) => section.lines.push(line.to_string()),
                None => preamble.push(line.to_string()),
            },
        }
    }
    (preamble, sections)
}

fn render(preamble: &[String], sections: &[Section]) -> String {
    let mut lines: Vec<&str> = preamble.iter().map(String::as_str).collect();
    lines.extend(
        sections
            .iter()
            .flat_map(|section| section.lines.iter().map(String::as_str)),
    );
    lines.join("\n")
}

/// Index of the leading `<!-- … -->` hint block right after the heading, as a
/// line range into `lines[1..]` (inclusive start, exclusive end).
fn leading_comment(lines: &[String]) -> (usize, usize) {
    let body = &lines[1..];
    let Some(start) = body.iter().position(|line| !line.trim().is_empty()) else {
        return (0, 0);
    };
    if !body[start].trim_start().starts_with("<!--") {
        return (0, 0);
    }
    match body[start..].iter().position(|line| line.contains("-->")) {
        Some(end) => (start, start + end + 1),
        None => (0, 0),
    }
}

/// The chapter's content without its ownership hint and surrounding blank lines.
fn chapter_body(section: &Section) -> Vec<String> {
    let (from, to) = leading_comment(&section.lines);
    let mut body: Vec<String> = section.lines[1..]
        .iter()
        .enumerate()
        .filter(|(index, _)| *index < from || *index >= to)
        .map(|(_, line)| line.clone())
        .collect();
    while body.first().is_some_and(|line| line.trim().is_empty()) {
        body.remove(0);
    }
    while body.last().is_some_and(|line| line.trim().is_empty()) {
        body.pop();
    }
    body
}

/// True when the text has the fixed chapters this sync knows about.
pub fn uses_fixed_chapters(text: &str) -> bool {
    let (_, sections) = parse(text);
    SYNCED_CHAPTERS
        .iter()
        .chain(OWN_CHAPTERS.iter())
        .all(|chapter| {
            sections
                .iter()
                .filter(|section| section.heading == *chapter)
                .count()
                == 1
        })
}

/// `target` with the synced chapters taken from `primary`; `None` when nothing
/// would change. A chapter the primary left empty is skipped, so an unfinished
/// primary never blanks a filled target. Hint comments, the target's own
/// chapters and everything outside the synced chapters stay byte-for-byte.
pub fn merge_profile(primary: &str, target: &str) -> Option<String> {
    let primary = primary.replace("\r\n", "\n");
    let crlf = target.contains("\r\n");
    let target = target.replace("\r\n", "\n");
    if !uses_fixed_chapters(&primary) || !uses_fixed_chapters(&target) {
        return None;
    }
    let (_, primary_sections) = parse(&primary);
    let (preamble, mut sections) = parse(&target);
    let mut changed = false;
    for chapter in SYNCED_CHAPTERS {
        let source = primary_sections
            .iter()
            .find(|section| section.heading == chapter)
            .map(chapter_body)
            .unwrap_or_default();
        if source.is_empty() {
            continue;
        }
        let section = sections
            .iter_mut()
            .find(|section| section.heading == chapter)
            .expect("fixed chapters were checked");
        if chapter_body(section) == source {
            continue;
        }
        let (from, to) = leading_comment(&section.lines);
        let mut lines = vec![section.lines[0].clone(), String::new()];
        if to > from {
            lines.extend(section.lines[1 + from..1 + to].iter().cloned());
            lines.push(String::new());
        }
        lines.extend(source);
        lines.push(String::new());
        // The next heading keeps its blank line, as in the shipped template.
        section.lines = lines;
        changed = true;
    }
    changed.then(|| {
        let text = render(&preamble, &sections);
        if crlf {
            text.replace('\n', "\r\n")
        } else {
            text
        }
    })
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_profile(workspace: &Path) -> Option<Vec<u8>> {
    read_workspace_file_no_follow(workspace, USER_FILE, MAX_PROFILE_BYTES)
        .ok()
        .map(|(_, bytes)| bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncReport {
    pub primary: Option<PathBuf>,
    pub updated: Vec<PathBuf>,
    /// Targets skipped because the file changed while the sync was running.
    pub deferred: Vec<PathBuf>,
}

/// A backup name for `stamp` that does not exist yet: a second overwrite in the
/// same second gets a numeric suffix instead of replacing the first copy.
fn free_backup_name(target: &Path, stamp: &str) -> String {
    let base = format!("{USER_FILE}.bak-{stamp}");
    let mut name = base.clone();
    let mut n = 2;
    while target.join(&name).exists() {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

fn prune_backups(workspace: &Path) {
    let Some(rules) = workspace.join(USER_FILE).parent().map(Path::to_path_buf) else {
        return;
    };
    let prefix = "03-USER.md.bak-";
    let Ok(entries) = fs::read_dir(rules) else {
        return;
    };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        .map(|entry| entry.path())
        .collect();
    backups.sort();
    while backups.len() > KEPT_BACKUPS {
        let _ = fs::remove_file(backups.remove(0));
    }
}

/// Sync `workspaces` (in registration order). `stamp` (sortable, e.g.
/// `YYYYMMDD-HHMMSS`) names the backup of each file that is about to change.
pub fn reconcile(workspaces: &[PathBuf], stamp: &str) -> SyncReport {
    let mut report = SyncReport::default();
    let profiles: Vec<(&PathBuf, Vec<u8>)> = workspaces
        .iter()
        .filter_map(|path| read_profile(path).map(|bytes| (path, bytes)))
        .filter(|(_, bytes)| std::str::from_utf8(bytes).is_ok_and(uses_fixed_chapters))
        .collect();
    let Some((primary, primary_bytes)) = profiles.first() else {
        return report;
    };
    report.primary = Some((*primary).clone());
    let primary_text = String::from_utf8_lossy(primary_bytes).into_owned();
    for (target, bytes) in profiles.iter().skip(1) {
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let Some(merged) = merge_profile(&primary_text, text) else {
            continue;
        };
        // Skip rather than overwrite when another flow wrote the file meanwhile.
        if read_profile(target).map(|current| digest(&current)) != Some(digest(bytes)) {
            report.deferred.push((*target).clone());
            continue;
        }
        let backup = free_backup_name(target, stamp);
        if write_workspace_file_no_follow(target, &backup, bytes).is_err() {
            continue;
        }
        if write_workspace_file_no_follow(target, USER_FILE, merged.as_bytes()).is_ok() {
            prune_backups(target);
            report.updated.push((*target).clone());
        }
    }
    report
}

/// Workspaces of the registered assistants in registration order, skipping
/// archived ones and duplicates.
fn assistant_workspaces() -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    crate::im::read_agent_configs_from_disk()
        .into_iter()
        .filter(|agent| {
            !agent.resolved_workspace_path.is_empty()
                && !crate::im::is_agent_workspace_archived(agent)
        })
        .map(|agent| PathBuf::from(agent.resolved_workspace_path))
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

/// `shareUserProfile` in config.json; sharing is on unless the user turned it off.
fn enabled() -> bool {
    dirs::home_dir()
        .map(|home| home.join(".agenteam").join("config.json"))
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| {
            serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')).ok()
        })
        .and_then(|config| {
            config
                .get("shareUserProfile")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(true)
}

/// Run one reconcile pass; failures only log, they never affect an assistant.
pub fn run_once() {
    if !enabled() {
        return;
    }
    let workspaces = assistant_workspaces();
    if workspaces.len() < 2 {
        return;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let report = reconcile(&workspaces, &stamp);
    if !report.updated.is_empty() {
        crate::ulog_info!(
            "[profile-sync] shared profile chapters synced to {} assistant workspace(s)",
            report.updated.len()
        );
    }
}

/// Startup pass, then one pass every [`SYNC_INTERVAL_SECS`].
pub fn start_background_sync() {
    tauri::async_runtime::spawn(async {
        loop {
            let _ = tauri::async_runtime::spawn_blocking(run_once).await;
            tokio::time::sleep(std::time::Duration::from_secs(SYNC_INTERVAL_SECS)).await;
        }
    });
}

#[cfg(test)]
mod tests;
