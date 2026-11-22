use super::*;
use tempfile::tempdir;

const TEMPLATE: &str = include_str!("../../../src/shared/default-user.md");

/// `text` with `body` written under `heading`, after the chapter's hint comment
/// as in a real profile.
fn with_body(text: &str, heading: &str, body: &str) -> String {
    let marker = format!("## {heading}\n\n<!--");
    let start = text.find(&marker).unwrap_or_else(|| panic!("{heading}"));
    let end = start + text[start..].find("-->\n").unwrap() + "-->\n".len();
    format!("{}\n{body}\n{}", &text[..end], &text[end..])
}

fn section(text: &str, heading: &str) -> Vec<String> {
    let (_, sections) = parse(text);
    chapter_body(
        sections
            .iter()
            .find(|section| section.heading == heading)
            .unwrap(),
    )
}

fn workspace(root: &Path, name: &str, text: Option<&str>) -> PathBuf {
    let path = root.join(name);
    if let Some(text) = text {
        fs::create_dir_all(path.join(".claude/rules")).unwrap();
        fs::write(path.join(USER_FILE), text).unwrap();
    } else {
        fs::create_dir_all(&path).unwrap();
    }
    path
}

#[test]
fn only_files_with_every_fixed_chapter_once_take_part() {
    assert!(uses_fixed_chapters(TEMPLATE));
    assert!(!uses_fixed_chapters(
        "# USER.md\n\n## Context\n\n- old style\n"
    ));
    assert!(!uses_fixed_chapters(
        &TEMPLATE.replace("## 偏好", "## 偏好 (renamed)")
    ));
    assert!(!uses_fixed_chapters(&format!(
        "{TEMPLATE}\n## 偏好\n\nduplicate\n"
    )));
    // A `## ` line in fenced code is content, not a chapter.
    assert!(uses_fixed_chapters(&format!(
        "{TEMPLATE}\n```\n## 偏好\n```\n"
    )));
}

#[test]
fn merging_copies_the_person_chapters_and_nothing_else() {
    let primary = with_body(TEMPLATE, "沟通风格", "简短直接，不说客套话。");
    let primary = with_body(&primary, "偏好", "- 周报用表格");
    let primary = with_body(&primary, "近期观察", "- (2026-11-01) 主助理的观察");
    let primary = with_body(&primary, "来源概况", "- 简历").replace("- 名字：", "- 名字：主人");
    let target = with_body(TEMPLATE, "近期观察", "- (2026-11-02) 副手的观察")
        .replace("- 名字：", "- 名字：副手");
    let merged = merge_profile(&primary, &target).expect("chapters differ");

    assert_eq!(section(&merged, "沟通风格"), ["简短直接，不说客套话。"]);
    assert_eq!(section(&merged, "偏好"), ["- 周报用表格"]);
    // The target keeps its own chapters.
    assert!(merged.contains("- 名字：副手") && !merged.contains("主人"));
    assert!(merged.contains("副手的观察") && !merged.contains("主助理的观察"));
    assert!(!section(&merged, "来源概况")
        .iter()
        .any(|line| line.contains("简历")));
    // Every synced chapter's ownership hint survives.
    for chapter in SYNCED_CHAPTERS {
        let (_, sections) = parse(&merged);
        let found = sections
            .iter()
            .find(|section| section.heading == chapter)
            .unwrap();
        let (from, to) = leading_comment(&found.lines);
        assert!(to > from, "{chapter} lost its hint");
    }
    assert!(uses_fixed_chapters(&merged));
    // Syncing again is a no-op.
    assert_eq!(merge_profile(&primary, &merged), None);
}

#[test]
fn an_empty_primary_chapter_never_blanks_a_filled_target() {
    let target = with_body(TEMPLATE, "偏好", "- 已有偏好");
    assert_eq!(merge_profile(TEMPLATE, &target), None);
}

#[test]
fn legacy_files_and_crlf_are_handled() {
    let legacy = "# USER.md\n\n## Context\n\n- old\n";
    assert_eq!(merge_profile(TEMPLATE, legacy), None);
    assert_eq!(merge_profile(legacy, TEMPLATE), None);
    let primary = with_body(TEMPLATE, "偏好", "- 表格");
    let crlf = TEMPLATE.replace('\n', "\r\n");
    let merged = merge_profile(&primary, &crlf).unwrap();
    assert!(
        !merged.replace("\r\n", "").contains('\n'),
        "every newline stays CRLF"
    );
}

#[test]
fn reconcile_syncs_followers_keeps_a_backup_and_leaves_legacy_files_alone() {
    let root = tempdir().unwrap();
    let legacy_text = "# USER.md\n\n## Context\n\n- written by hand\n";
    let first = workspace(root.path(), "first-legacy", Some(legacy_text));
    let primary = workspace(
        root.path(),
        "primary",
        Some(&with_body(TEMPLATE, "偏好", "- 周报用表格")),
    );
    let follower_text = with_body(TEMPLATE, "近期观察", "- (2026-11-02) 副手的观察");
    let follower = workspace(root.path(), "follower", Some(&follower_text));
    let none = workspace(root.path(), "no-profile", None);
    let identical = workspace(root.path(), "identical", Some(TEMPLATE));

    let order = vec![
        first.clone(),
        primary.clone(),
        follower.clone(),
        none.clone(),
        identical.clone(),
    ];
    let report = reconcile(&order, "20261122");
    // The earliest assistant has a legacy file, so the first structured one is the primary.
    assert_eq!(report.primary.as_ref(), Some(&primary));
    assert_eq!(report.updated, vec![follower.clone(), identical.clone()]);

    assert_eq!(
        fs::read_to_string(first.join(USER_FILE)).unwrap(),
        legacy_text
    );
    let synced = fs::read_to_string(follower.join(USER_FILE)).unwrap();
    assert_eq!(section(&synced, "偏好"), ["- 周报用表格"]);
    assert!(synced.contains("副手的观察"));
    assert_eq!(
        fs::read_to_string(follower.join(".claude/rules/03-USER.md.bak-20261122")).unwrap(),
        follower_text
    );
    assert!(!none.join(USER_FILE).exists());

    // A second pass changes nothing and adds no backup.
    let again = reconcile(&order, "20261123");
    assert!(again.updated.is_empty());
    assert!(!follower
        .join(".claude/rules/03-USER.md.bak-20261123")
        .exists());
}

#[test]
fn the_oldest_backup_of_a_day_is_kept_and_only_five_backups_remain() {
    let root = tempdir().unwrap();
    let primary = workspace(root.path(), "primary", Some(TEMPLATE));
    let follower = workspace(root.path(), "follower", Some(TEMPLATE));
    let rules = follower.join(".claude/rules");
    for day in 1..=7 {
        let text = with_body(TEMPLATE, "偏好", &format!("- 版本 {day}"));
        fs::write(primary.join(USER_FILE), &text).unwrap();
        reconcile(
            &[primary.clone(), follower.clone()],
            &format!("202611{day:02}"),
        );
        // A second change on the same day does not replace that day's backup.
        let later = with_body(TEMPLATE, "偏好", &format!("- 版本 {day}b"));
        fs::write(primary.join(USER_FILE), later).unwrap();
        reconcile(
            &[primary.clone(), follower.clone()],
            &format!("202611{day:02}"),
        );
    }
    let mut backups: Vec<String> = fs::read_dir(&rules)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("03-USER.md.bak-"))
        .collect();
    backups.sort();
    assert_eq!(backups.len(), KEPT_BACKUPS);
    assert_eq!(backups.first().unwrap(), "03-USER.md.bak-20261103");
    let day_seven = fs::read_to_string(rules.join("03-USER.md.bak-20261107")).unwrap();
    assert!(
        day_seven.contains("版本 6b"),
        "the day's backup is the file as it was at the first change that day"
    );
}

#[cfg(unix)]
#[test]
fn a_target_that_is_a_symlink_is_never_written_through() {
    let root = tempdir().unwrap();
    let primary = workspace(
        root.path(),
        "primary",
        Some(&with_body(TEMPLATE, "偏好", "- 表格")),
    );
    let outside = root.path().join("outside.md");
    fs::write(&outside, TEMPLATE).unwrap();
    let follower = workspace(root.path(), "follower", None);
    fs::create_dir_all(follower.join(".claude/rules")).unwrap();
    std::os::unix::fs::symlink(&outside, follower.join(USER_FILE)).unwrap();
    let report = reconcile(&[primary, follower], "20261122");
    assert!(report.updated.is_empty());
    assert_eq!(fs::read_to_string(&outside).unwrap(), TEMPLATE);
}
