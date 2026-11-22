use super::*;
use crate::knowledge::store::MAX_WIKI_PAGE_BYTES;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Command {
    CurationContext {
        #[serde(rename = "kbId")]
        kb_id: String,
    },
    WikiWrite {
        #[serde(rename = "kbId")]
        kb_id: String,
        path: String,
        file: String,
    },
    WikiLog {
        #[serde(rename = "kbId")]
        kb_id: String,
        message: String,
    },
    WikiLintReport {
        #[serde(rename = "kbId")]
        kb_id: String,
        file: String,
    },
    TaxonomyPropose {
        #[serde(rename = "kbId")]
        kb_id: String,
        category: String,
        reason: String,
    },
}

pub(super) fn is_write(action: &str) -> bool {
    matches!(
        action,
        "wiki-write" | "wiki-log" | "wiki-lint-report" | "taxonomy-propose"
    )
}

impl Command {
    pub(super) fn decode(action: &str, mut request: Value) -> StoreResult<Self> {
        let fields = request.as_object_mut().ok_or(KbInvalidInput)?;
        if fields.insert("action".into(), json!(action)).is_some() {
            return Err(KbInvalidInput);
        }
        decode(request)
    }

    pub(super) fn kb(&self) -> &str {
        match self {
            Self::CurationContext { kb_id }
            | Self::WikiWrite { kb_id, .. }
            | Self::WikiLog { kb_id, .. }
            | Self::WikiLintReport { kb_id, .. }
            | Self::TaxonomyPropose { kb_id, .. } => kb_id,
        }
    }

    pub(super) fn run(
        self,
        store: &KnowledgeStore,
        authority: &WriteAuthority,
    ) -> StoreResult<Value> {
        let input_bytes = |kb: &str, file: &str| -> StoreResult<Vec<u8>> {
            store.check_agent_write(kb, |settings| authority.pin(settings))?;
            read_workspace_file_strict(&authority.caller.workspace_path, file, MAX_WIKI_PAGE_BYTES)
                .map_err(|_| KbInvalidInput)
        };
        match self {
            Self::CurationContext { kb_id } => {
                let context = store.curation_context(&kb_id, |kb| {
                    if authority.curator.is_none() {
                        return Ok(None);
                    }
                    match authority.pin_access(kb, true) {
                        Ok(guard) => Ok(Some(guard)),
                        Err(KbAccessDenied) => Ok(None),
                        Err(code) => Err(code),
                    }
                })?;
                let mut value = serde_json::to_value(context).map_err(|_| KbStoreUnavailable)?;
                value["ok"] = json!(true);
                Ok(value)
            }
            Self::WikiWrite { kb_id, path, file } => {
                let bytes = input_bytes(&kb_id, &file)?;
                let text = std::str::from_utf8(&bytes).map_err(|_| KbInvalidInput)?;
                let source = store.write_wiki_page(&kb_id, &path, text, |kb| authority.pin(kb))?;
                Ok(json!({"ok": true, "sourceId": source.id}))
            }
            Self::WikiLog { kb_id, message } => {
                store.append_wiki_log(&kb_id, &message, |kb| authority.pin(kb))?;
                Ok(json!({"ok": true}))
            }
            Self::WikiLintReport { kb_id, file } => {
                let bytes = input_bytes(&kb_id, &file)?;
                let count = store.save_wiki_lint_report(&kb_id, &bytes, |kb| authority.pin(kb))?;
                Ok(json!({"ok": true, "items": count}))
            }
            Self::TaxonomyPropose {
                kb_id,
                category,
                reason,
            } => {
                let status =
                    store.propose_category(&kb_id, &category, &reason, |kb| authority.pin(kb))?;
                Ok(json!({"ok": true, "status": status}))
            }
        }
    }
}
