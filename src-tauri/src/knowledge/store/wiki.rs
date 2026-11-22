//! The Store publishes normalized wiki Markdown through the same staged
//! source-directory transaction as file ingestion. Search reads that version
//! through the standard chunker; the named wiki file is available for browsing.

use super::*;
use crate::knowledge::chunker::{chunk_markdown, ChunkError};
use crate::workspace_files::path_safety::{metadata_is_link_like, validate_item_name};
use sha2::{Digest, Sha256};
use KnowledgeErrorCode::KbAccessDenied;

pub(crate) const MAX_WIKI_PAGE_BYTES: u64 = 1024 * 1024;
const MAX_WIKI_PAGES: usize = 2000;
const LOG_PAGE: &str = "log.md";

#[derive(Deserialize, Serialize)]
struct WikiPublication {
    page: String,
    source_id: KnowledgeSourceId,
    content_hash: String,
}

fn publication_path(kb: &str) -> String {
    format!("{kb}/.wiki-publication.json")
}

fn validate_wiki_path(path: &str) -> StoreResult<()> {
    if !Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        || path.len() > 512
        || path.split('/').any(|part| {
            part.starts_with('.') || part.ends_with('.') || validate_item_name(part).is_err()
        })
    {
        return Err(KbInvalidInput);
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LintReport {
    version: u32,
    generated_at: String,
    items: Vec<LintItem>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LintItem {
    kind: LintKind,
    page: String,
    message: String,
    refs: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum LintKind {
    Contradiction,
    Stale,
    Orphan,
    MissingCitation,
}

fn parse_lint_report(kb_id: &str, bytes: &[u8]) -> StoreResult<(LintReport, String)> {
    let report: LintReport = serde_json::from_slice(bytes).map_err(|_| KbInvalidInput)?;
    let generated = chrono::DateTime::parse_from_rfc3339(&report.generated_at)
        .ok()
        .filter(|date| date.offset().local_minus_utc() == 0)
        .ok_or(KbInvalidInput)?;
    if report.version != 1 {
        return Err(KbInvalidInput);
    }
    for item in &report.items {
        validate_wiki_path(&item.page)?;
        if item.message.trim().is_empty()
            || item.message.chars().any(char::is_control)
            || (item.refs.is_empty()
                && matches!(item.kind, LintKind::Contradiction | LintKind::Stale))
            || item.refs.iter().any(|value| {
                parse_knowledge_ref(value).is_none_or(|reference| reference.kb_id != kb_id)
            })
        {
            return Err(KbInvalidInput);
        }
    }
    Ok((report, generated.format("%Y%m%dT%H%M%S%.9fZ").to_string()))
}

impl StoreFiles {
    // The source-directory rename commits a page. If the process dies after
    // that rename, finish the named projection from the committed Markdown.
    pub(super) fn recover_wiki_publication(&self, kb: &str) -> StoreResult<()> {
        let journal = publication_path(kb);
        let Some(Some(pending)) = self.read_json::<Option<WikiPublication>>(&journal)? else {
            return Ok(());
        };
        validate_wiki_path(&pending.page)?;
        if !is_knowledge_source_id(&pending.source_id) {
            return Err(KbStoreCorrupt);
        }
        if let Some(source) = self.read_source(kb, &pending.source_id)? {
            if source.facts.kind != SourceKind::Wiki
                || source.display_name.as_deref() != Some(pending.page.as_str())
            {
                return Err(KbStoreCorrupt);
            }
            if source.facts.content_hash.as_deref() == Some(&pending.content_hash) {
                let markdown = self
                    .read_text(
                        &format!("{kb}/sources/{}/content.md", source.id),
                        MAX_WIKI_PAGE_BYTES,
                    )?
                    .ok_or(KbStoreCorrupt)?;
                if format!("{:x}", Sha256::digest(markdown.as_bytes())) != pending.content_hash {
                    return Err(KbStoreCorrupt);
                }
                let target = self.wiki_target(kb, &pending.page)?;
                self.write_text(&target, &markdown)?;
            }
        }
        self.write_json(&journal, &Option::<WikiPublication>::None)
    }

    fn check_wiki_page_capacity(&self, kb: &str, target: &str) -> StoreResult<()> {
        if self.root.join(target).is_file() {
            return Ok(());
        }
        let relative = format!("{kb}/wiki");
        let (root, _handle) = ensure_workspace_directory_no_follow(&self.root, &relative)
            .map_err(|_| KbInvalidInput)?;
        let mut queue = vec![root.clone()];
        let mut pages = 0;
        while let Some(directory) = queue.pop() {
            for entry in fs::read_dir(&directory).map_err(|_| KbStoreUnavailable)? {
                let entry = entry.map_err(|_| KbStoreUnavailable)?;
                let metadata =
                    fs::symlink_metadata(entry.path()).map_err(|_| KbStoreUnavailable)?;
                if metadata_is_link_like(&metadata) {
                    continue;
                }
                if metadata.is_dir() {
                    queue.push(entry.path());
                } else if metadata.is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                    && !(directory == root && entry.file_name().eq_ignore_ascii_case(LOG_PAGE))
                {
                    pages += 1;
                    if pages >= MAX_WIKI_PAGES {
                        return Err(KbLimitExceeded);
                    }
                }
            }
        }
        Ok(())
    }

    fn wiki_target(&self, kb: &str, page: &str) -> StoreResult<String> {
        let relative = format!("{kb}/wiki/{page}");
        let parent = relative.rsplit_once('/').ok_or(KbInvalidInput)?.0;
        ensure_workspace_directory_no_follow(&self.root, parent).map_err(|_| KbInvalidInput)?;
        match fs::symlink_metadata(self.root.join(&relative)) {
            Ok(meta) if !meta.is_file() || metadata_is_link_like(&meta) => Err(KbInvalidInput),
            Ok(_) => Ok(relative),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(relative),
            Err(_) => Err(KbStoreUnavailable),
        }
    }

    fn wiki_source(&self, kb: &str, page: &str) -> StoreResult<Option<SourceRecord>> {
        let Some(parent) = self.source_parent(kb)? else {
            return Ok(None);
        };
        self.recover_source_publications(&parent)?;
        let key = page.to_lowercase();
        let (mut count, mut existing) = (0, None);
        for id in ingest::valid_kb_source_folders(&parent)? {
            let record = self.require_record(kb, &id)?;
            if record.source.facts.kind != SourceKind::Wiki {
                continue;
            }
            count += 1;
            if let Some(name) = record.source.display_name.as_deref() {
                if name.to_lowercase() == key {
                    if name != page || existing.is_some() {
                        return Err(KbInvalidInput);
                    }
                    existing = Some(record);
                }
            }
        }
        if existing.is_none() && count >= MAX_WIKI_PAGES {
            return Err(KbLimitExceeded);
        }
        Ok(existing)
    }
}

impl KnowledgeStore {
    pub(crate) fn write_wiki_page<G>(
        &self,
        kb: &str,
        page: &str,
        text: &str,
        authorize: impl FnOnce(&KnowledgeBase) -> StoreResult<G>,
    ) -> StoreResult<KnowledgeSource> {
        validate_wiki_path(page)?;
        if page.eq_ignore_ascii_case(LOG_PAGE) {
            return Err(KbInvalidInput);
        }
        if text.len() as u64 > MAX_WIKI_PAGE_BYTES {
            return Err(KbLimitExceeded);
        }
        self.locked_kb(kb, None, |files, _| {
            let library = files.read_kb(kb)?;
            if !library.wiki_enabled {
                return Err(KbAccessDenied);
            }
            files.recover_wiki_publication(kb)?;
            let previous = files.wiki_source(kb, page)?;
            let replacing = previous.is_some();
            let mut record = if let Some(record) = previous {
                record
            } else {
                let parent = files.root.join(kb).join("sources");
                ingest::check_source_limit(&parent)?;
                SourceRecord {
                    source: KnowledgeSource {
                        id: allocate_id(&parent, |_| false, create_knowledge_source_id)?,
                        display_name: Some(page.into()),
                        facts: SourceFacts {
                            kind: SourceKind::Wiki,
                            current_path: None,
                            content_hash: None,
                            file_fingerprint: None,
                            status: SourceStatus::Ready,
                        },
                        metadata: BTreeMap::new(),
                    },
                    ingest: None,
                    curation_pending: false,
                }
            };
            let locale = crate::i18n::locale_from_config(&files.app.join("config.json"));
            chunk_markdown(kb, &record.source.id, page, text, locale).map_err(
                |error| match error {
                    ChunkError::GraphemeExceedsBudget(_) => KnowledgeErrorCode::KbChunkUnsplittable,
                    ChunkError::InvalidReference => KbStoreCorrupt,
                    ChunkError::InsufficientContextSpace => KbLimitExceeded,
                },
            )?;
            let stage = files.prepare_import(kb, text.len() as u64)?;
            let stage_name = stage
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(KbStoreCorrupt)?;
            files.write_text(&format!("{kb}/sources/{stage_name}/content.md"), text)?;
            let content_hash = format!("{:x}", Sha256::digest(text.as_bytes()));
            record.source.facts.content_hash = Some(content_hash.clone());
            let _permit = authorize(&library)?;
            let target = files.wiki_target(kb, page)?;
            files.check_wiki_page_capacity(kb, &target)?;
            files.write_json(
                &publication_path(kb),
                &WikiPublication {
                    page: page.to_string(),
                    source_id: record.source.id.clone(),
                    content_hash,
                },
            )?;
            files.publish_source_version(kb, &record, &stage, replacing)?;
            self.source_upserted(kb, &record.source);
            files.write_text(&target, text)?;
            files.write_json(&publication_path(kb), &Option::<WikiPublication>::None)?;
            Ok(record.source)
        })
    }

    pub(crate) fn append_wiki_log<G>(
        &self,
        kb: &str,
        message: &str,
        authorize: impl FnOnce(&KnowledgeBase) -> StoreResult<G>,
    ) -> StoreResult<()> {
        if message.trim().is_empty()
            || message.len() > 4096
            || message
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(KbInvalidInput);
        }
        self.locked_kb(kb, None, |files, _| {
            let library = files.read_kb(kb)?;
            if !library.wiki_enabled {
                return Err(KbAccessDenied);
            }
            let _permit = authorize(&library)?;
            let path = files.wiki_target(kb, LOG_PAGE)?;
            let prior = files
                .read_text(&path, MAX_WIKI_PAGE_BYTES)?
                .unwrap_or_default();
            let text = format!(
                "{prior}\n## {}\n\n{}\n",
                Utc::now().format("%Y-%m-%d"),
                message.trim()
            );
            if text.len() as u64 > MAX_WIKI_PAGE_BYTES {
                return Err(KbLimitExceeded);
            }
            files.write_text(&path, &text)
        })
    }

    pub(crate) fn save_wiki_lint_report<G>(
        &self,
        kb: &str,
        bytes: &[u8],
        authorize: impl FnOnce(&KnowledgeBase) -> StoreResult<G>,
    ) -> StoreResult<usize> {
        let (report, stamp) = parse_lint_report(kb, bytes)?;
        self.locked_kb(kb, None, |files, _| {
            let library = files.read_kb(kb)?;
            if !library.wiki_enabled {
                return Err(KbAccessDenied);
            }
            let _permit = authorize(&library)?;
            let path = files.wiki_target(kb, &format!("lint/{stamp}.json"))?;
            files.write_json(&path, &report)?;
            Ok(report.items.len())
        })
    }
}
