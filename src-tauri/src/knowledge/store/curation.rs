// Curation state owned by the Store: which sources still wait for a curation
// round, which categories the user has approved, and the context a curator
// reads before it works. Rules text is user data; this module only parses the
// category list out of it and appends to it when the user approves a category.

use super::*;

/// One curation round handles at most this many sources.
pub const CURATION_BATCH_LIMIT: usize = 50;
pub(super) const MAX_PENDING_CATEGORIES: usize = 50;
const MAX_CATEGORY_CHARS: usize = 200;
pub(super) const MAX_CONTEXT_TEXT_BYTES: u64 = 256 * 1024;
pub(super) const RULES_FILE: &str = "ORGANIZE_RULES.md";
pub(super) const SCHEMA_FILE: &str = "WIKI_SCHEMA.md";
const WIKI_INDEX: &str = "wiki/index.md";
const CATEGORY_SECTION_ZH: &str = "分类体系";
const CATEGORY_SECTION_EN: &str = "Categories";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCurationSource {
    pub source_id: KnowledgeSourceId,
    pub reference: String,
    pub title: String,
    pub status: SourceStatus,
}

/// Everything a curator needs to start a round. Text fields are library data,
/// so the caller presents them as untrusted content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurationContext {
    pub kb_id: KnowledgeBaseId,
    /// `None` when the file does not exist (a library created before K7).
    pub rules: Option<String>,
    pub wiki_schema: Option<String>,
    pub wiki_index: Option<String>,
    /// Categories approved in `ORGANIZE_RULES.md`.
    pub categories: Vec<String>,
    /// Categories proposed by a curator and awaiting the user.
    pub pending_categories: Vec<String>,
    pub wiki_enabled: bool,
    pub pending: Vec<PendingCurationSource>,
    /// True when more sources are waiting than this round returns.
    pub more_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CategoryProposal {
    /// Already approved; nothing changed.
    Allowed,
    /// Already waiting for the user; nothing changed.
    AlreadyPending,
    Proposed,
}

fn rules_lines(rules: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut fence = None;
    rules.lines().enumerate().filter_map(move |(index, line)| {
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() > 3 || trimmed.starts_with('\t') {
            return None;
        }
        if let Some(marker @ (b'`' | b'~')) = trimmed.bytes().next() {
            let count = trimmed.bytes().take_while(|byte| *byte == marker).count();
            if count >= 3 {
                match fence {
                    None => fence = Some((marker, count)),
                    Some((open, length))
                        if open == marker
                            && count >= length
                            && trimmed[count..].trim().is_empty() =>
                    {
                        fence = None;
                    }
                    Some(_) => {}
                }
                return None;
            }
        }
        fence.is_none().then_some((index, trimmed))
    })
}

/// The approved category in a bullet matching the active section's delimiter
/// (``- `path`：description`` under `## 分类体系`, ``- `path`: description`` under `## Categories`).
fn category_bullet(line: &str, english_section: bool) -> Option<&str> {
    let (path, tail) = line.strip_prefix("- `")?.split_once('`')?;
    let description = if english_section {
        tail.strip_prefix(':')?
    } else {
        tail.strip_prefix('：')?
    };
    (!description.trim().is_empty()).then_some(path)
}

fn section_title(line: &str) -> Option<&str> {
    let (header, title) = line.split_once(' ')?;
    matches!(header, "#" | "##").then_some(title.trim())
}

fn category_section_lang(title: &str) -> Option<bool> {
    if title == CATEGORY_SECTION_ZH {
        Some(false)
    } else if title.eq_ignore_ascii_case(CATEGORY_SECTION_EN) {
        Some(true)
    } else {
        None
    }
}

/// Categories listed under `## 分类体系` or `## Categories`; code blocks and other sections are ignored.
pub(super) fn allowed_categories(rules: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut section_english: Option<bool> = None;
    for (_, line) in rules_lines(rules) {
        if let Some(title) = section_title(line) {
            section_english = if line.starts_with("## ") {
                category_section_lang(title)
            } else {
                None
            };
        } else if let Some(english) = section_english {
            if let Some(path) = category_bullet(line, english) {
                if valid_category(path) && !found.iter().any(|known| known == path) {
                    found.push(path.to_string());
                }
            }
        }
    }
    found
}

/// A relative `/`-separated path of non-empty segments, no traversal or control characters.
pub(super) fn valid_category(path: &str) -> bool {
    !path.is_empty()
        && path.chars().count() <= MAX_CATEGORY_CHARS
        && path.trim() == path
        && !path.chars().any(char::is_control)
        && !path.contains('`')
        && path.split('/').all(|segment| {
            !segment.is_empty() && segment.trim() == segment && segment != "." && segment != ".."
        })
}

/// Appends `path` to the category section, creating the section when absent.
pub(super) fn with_category(rules: &str, path: &str) -> String {
    let lines: Vec<&str> = rules.lines().collect();
    let mut insertion = None;
    let mut english_section = false;
    for (index, line) in rules_lines(rules) {
        if let Some(title) = section_title(line) {
            if insertion.is_some() {
                break;
            }
            if line.starts_with("## ") {
                if let Some(english) = category_section_lang(title) {
                    english_section = english;
                    insertion = Some(index + 1);
                }
            }
        } else if insertion.is_some() && category_bullet(line, english_section).is_some() {
            insertion = Some(index + 1);
        }
    }
    let bullet = if english_section {
        format!("- `{path}`: Confirmed by user")
    } else {
        format!("- `{path}`：用户确认新增")
    };
    let mut out: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
    match insertion {
        Some(at) => out.insert(at, bullet),
        None => {
            if out.last().is_some_and(|line| !line.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(format!("## {CATEGORY_SECTION_ZH}"));
            out.push(String::new());
            out.push(bullet);
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

impl StoreFiles {
    pub(super) fn read_text(&self, relative: &str, max_bytes: u64) -> StoreResult<Option<String>> {
        match fs::symlink_metadata(self.root.join(relative)) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(KbStoreUnavailable),
            Ok(_) => {}
        }
        let (_, bytes) =
            read_workspace_file_no_follow(&self.app, &format!("knowledge/{relative}"), max_bytes)
                .map_err(|_| KbStoreUnavailable)?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| KbStoreCorrupt)
    }

    /// A library without rules approves nothing, so an agent cannot invent categories.
    pub(super) fn approved_categories(&self, kb_id: &str) -> StoreResult<Vec<String>> {
        Ok(self
            .read_text(&format!("{kb_id}/{RULES_FILE}"), MAX_CONTEXT_TEXT_BYTES)?
            .map(|rules| allowed_categories(&rules))
            .unwrap_or_default())
    }

    fn pending_curation(
        &self,
        kb_id: &str,
        limit: usize,
    ) -> StoreResult<(Vec<PendingCurationSource>, bool)> {
        let Some(parent) = self.source_parent(kb_id)? else {
            return Ok((Vec::new(), false));
        };
        let mut found = Vec::new();
        for entry in fs::read_dir(parent).map_err(|_| KbStoreUnavailable)? {
            let entry = entry.map_err(|_| KbStoreUnavailable)?;
            let Some(id) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if !is_knowledge_source_id(&id) {
                continue;
            }
            let record = self.require_record(kb_id, &id)?;
            if record.curation_pending && record.source.facts.status == SourceStatus::Ready {
                found.push(PendingCurationSource {
                    reference: format!("kb://{kb_id}/{id}"),
                    title: record.source.display_title().to_string(),
                    source_id: id,
                    status: record.source.facts.status,
                });
            }
        }
        found.sort_by(|a, b| a.source_id.cmp(&b.source_id));
        let more = found.len() > limit;
        found.truncate(limit);
        Ok((found, more))
    }
}

impl KnowledgeStore {
    /// Sources waiting for curation, oldest id first, at most [`CURATION_BATCH_LIMIT`].
    #[cfg(test)]
    pub(crate) fn pending_curation(
        &self,
        kb_id: &str,
    ) -> StoreResult<(Vec<PendingCurationSource>, bool)> {
        self.locked_kb(kb_id, None, |files, _| {
            files.read_kb(kb_id)?;
            files.pending_curation(kb_id, CURATION_BATCH_LIMIT)
        })
    }

    pub(crate) fn curation_context<G>(
        &self,
        kb_id: &str,
        authorize_pending: impl FnOnce(&KnowledgeBase) -> StoreResult<Option<G>>,
    ) -> StoreResult<CurationContext> {
        self.locked_kb(kb_id, None, |files, _| {
            let kb = files.read_kb(kb_id)?;
            let permit = authorize_pending(&kb)?;
            let rules =
                files.read_text(&format!("{kb_id}/{RULES_FILE}"), MAX_CONTEXT_TEXT_BYTES)?;
            let (pending, more_pending) = if permit.is_some() {
                files.pending_curation(kb_id, CURATION_BATCH_LIMIT)?
            } else {
                (Vec::new(), false)
            };
            Ok(CurationContext {
                kb_id: kb_id.into(),
                categories: rules.as_deref().map(allowed_categories).unwrap_or_default(),
                rules,
                wiki_schema: files
                    .read_text(&format!("{kb_id}/{SCHEMA_FILE}"), MAX_CONTEXT_TEXT_BYTES)?,
                wiki_index: files.read_text(
                    &format!("{kb_id}/{WIKI_INDEX}"),
                    super::wiki::MAX_WIKI_PAGE_BYTES,
                )?,
                pending_categories: kb.pending_categories,
                wiki_enabled: kb.wiki_enabled,
                pending,
                more_pending,
            })
        })
    }

    /// A curator proposes a category; only the user can approve it.
    pub fn propose_category<G>(
        &self,
        kb_id: &str,
        path: &str,
        reason: &str,
        authorize: impl FnOnce(&KnowledgeBase) -> StoreResult<G>,
    ) -> StoreResult<CategoryProposal> {
        if !valid_category(path)
            || reason.trim().is_empty()
            || reason.len() > 4096
            || reason.chars().any(char::is_control)
        {
            return Err(KbInvalidInput);
        }
        self.locked_kb(kb_id, None, |files, _| {
            let mut kb = files.read_kb(kb_id)?;
            let _permit = authorize(&kb)?;
            if files
                .approved_categories(kb_id)?
                .iter()
                .any(|known| known == path)
            {
                return Ok(CategoryProposal::Allowed);
            }
            if kb.pending_categories.iter().any(|known| known == path) {
                return Ok(CategoryProposal::AlreadyPending);
            }
            if kb.pending_categories.len() >= MAX_PENDING_CATEGORIES {
                return Err(KbLimitExceeded);
            }
            kb.pending_categories.push(path.to_string());
            kb.pending_category_reasons
                .insert(path.to_string(), reason.trim().into());
            files.write_json(&format!("{kb_id}/kb.json"), &kb)?;
            self.emit_ui("kb-upserted", kb_id, None);
            Ok(CategoryProposal::Proposed)
        })
    }

    /// The user approves a proposed category: it joins the approved list in
    /// `ORGANIZE_RULES.md` and leaves the pending list. The rules file is
    /// written first, so an interrupted approval is repeated harmlessly.
    pub fn confirm_category(&self, kb_id: &str, path: &str) -> StoreResult<()> {
        if !valid_category(path) {
            return Err(KbInvalidInput);
        }
        self.locked_kb(kb_id, None, |files, _| {
            let mut kb = files.read_kb(kb_id)?;
            if !kb.pending_categories.iter().any(|known| known == path) {
                return Err(KbNotFound);
            }
            let rules_path = format!("{kb_id}/{RULES_FILE}");
            let fallback =
                library_templates(explicit_config_locale(&files.app.join("config.json")))[0].1;
            let rules = files
                .read_text(&rules_path, MAX_CONTEXT_TEXT_BYTES)?
                .unwrap_or_else(|| fallback.to_string());
            if !allowed_categories(&rules).iter().any(|known| known == path) {
                let updated = with_category(&rules, path);
                if updated.len() as u64 > MAX_CONTEXT_TEXT_BYTES {
                    return Err(KbLimitExceeded);
                }
                if !allowed_categories(&updated)
                    .iter()
                    .any(|known| known == path)
                {
                    return Err(KbInvalidInput);
                }
                files.write_text(&rules_path, &updated)?;
            }
            kb.pending_categories.retain(|known| known != path);
            kb.pending_category_reasons.remove(path);
            files.write_json(&format!("{kb_id}/kb.json"), &kb)?;
            self.emit_ui("kb-upserted", kb_id, None);
            Ok(())
        })
    }

    /// The user declines a proposed category.
    pub fn dismiss_category(&self, kb_id: &str, path: &str) -> StoreResult<()> {
        self.locked_kb(kb_id, None, |files, _| {
            let mut kb = files.read_kb(kb_id)?;
            let before = kb.pending_categories.len();
            kb.pending_categories.retain(|known| known != path);
            if kb.pending_categories.len() == before {
                return Err(KbNotFound);
            }
            kb.pending_category_reasons.remove(path);
            files.write_json(&format!("{kb_id}/kb.json"), &kb)?;
            self.emit_ui("kb-upserted", kb_id, None);
            Ok(())
        })
    }
}

#[cfg(test)]
impl KnowledgeStore {
    /// Test seam for a source that finished ingestion: queue it for curation.
    pub(crate) fn mark_curation_pending(&self, kb_id: &str, source_id: &str) -> StoreResult<()> {
        self.locked_kb(kb_id, Some(source_id), |files, _| {
            let mut record = files.require_record(kb_id, source_id)?;
            record.curation_pending = true;
            files.write_json(&source_path(kb_id, source_id), &record)
        })
    }
}
