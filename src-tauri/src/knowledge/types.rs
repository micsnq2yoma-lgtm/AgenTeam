//! Pure Knowledge Base wire contracts and reference codecs.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub type KnowledgeBaseId = String;
pub type KnowledgeSourceId = String;
pub type KnowledgeJobId = String;
pub type KnowledgePlanId = String;
pub type KnowledgeChunkId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeMetadataOrigin {
    Agent,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeMetadataField<T> {
    pub value: T,
    pub origin: KnowledgeMetadataOrigin,
    /// ISO 8601 timestamp.
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeErrorCode {
    KbNotFound,
    KbAccessDenied,
    KbFieldLocked,
    KbLimitExceeded,
    KbSourceUnsupported,
    KbIndexing,
    KbCategoryNotAllowed,
    KbInvalidInput,
    KbStoreCorrupt,
    KbStoreUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRef {
    pub kb_id: KnowledgeBaseId,
    pub source_id: KnowledgeSourceId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<KnowledgeChunkId>,
}

const MAX_REF_LENGTH: usize = 74;

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_calendar_date(value: &str) -> bool {
    value.len() == 8
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && !value.starts_with("0000")
        && NaiveDate::parse_from_str(value, "%Y%m%d").is_ok()
}

pub fn is_knowledge_base_id(value: &str) -> bool {
    value
        .strip_prefix("kb_")
        .is_some_and(|hex| is_lower_hex(hex, 12))
}

pub fn is_knowledge_source_id(value: &str) -> bool {
    value
        .strip_prefix("src_")
        .is_some_and(|hex| is_lower_hex(hex, 16))
}

pub fn is_knowledge_chunk_id(value: &str) -> bool {
    is_lower_hex(value, 16)
}

fn is_dated_id(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|suffix| suffix.split_once('_'))
        .is_some_and(|(date, hex)| is_calendar_date(date) && is_lower_hex(hex, 12))
}

pub fn is_knowledge_job_id(value: &str) -> bool {
    is_dated_id(value, "job_")
}

pub fn is_knowledge_plan_id(value: &str) -> bool {
    is_dated_id(value, "plan_")
}

fn format_entropy(entropy: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(entropy.len() * 2);
    for &byte in entropy {
        hex.push(char::from(HEX[usize::from(byte >> 4)]));
        hex.push(char::from(HEX[usize::from(byte & 0xf)]));
    }
    hex
}

/// The caller supplies entropy. The Store owns collision handling.
pub fn create_knowledge_base_id(entropy: [u8; 6]) -> KnowledgeBaseId {
    format!("kb_{}", format_entropy(&entropy))
}

pub fn create_knowledge_source_id(entropy: [u8; 8]) -> KnowledgeSourceId {
    format!("src_{}", format_entropy(&entropy))
}

pub fn create_knowledge_job_id(date: &str, entropy: [u8; 6]) -> Option<KnowledgeJobId> {
    is_calendar_date(date).then(|| format!("job_{date}_{}", format_entropy(&entropy)))
}

pub fn create_knowledge_plan_id(date: &str, entropy: [u8; 6]) -> Option<KnowledgePlanId> {
    is_calendar_date(date).then(|| format!("plan_{date}_{}", format_entropy(&entropy)))
}

/// Parse the literal reference without URL decoding or normalization.
pub fn parse_knowledge_ref(value: &str) -> Option<KnowledgeRef> {
    if value.len() > MAX_REF_LENGTH {
        return None;
    }
    let target = value.strip_prefix("kb://")?;
    let (target, chunk_id) = match target.split_once('#') {
        Some((target, chunk_id)) if is_knowledge_chunk_id(chunk_id) => (target, Some(chunk_id)),
        Some(_) => return None,
        None => (target, None),
    };
    let (path, page) = match target.split_once('?') {
        Some((path, query)) => {
            let digits = query.strip_prefix("page=")?;
            if digits.starts_with('0') || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            (path, Some(digits.parse::<u32>().ok()?))
        }
        None => (target, None),
    };
    let (kb_id, source_id) = path.split_once('/')?;
    if !is_knowledge_base_id(kb_id) || !is_knowledge_source_id(source_id) {
        return None;
    }
    Some(KnowledgeRef {
        kb_id: kb_id.to_owned(),
        source_id: source_id.to_owned(),
        page,
        chunk_id: chunk_id.map(str::to_owned),
    })
}

pub fn format_knowledge_ref(reference: &KnowledgeRef) -> Option<String> {
    let KnowledgeRef {
        kb_id,
        source_id,
        page,
        chunk_id,
    } = reference;
    if !is_knowledge_base_id(kb_id)
        || !is_knowledge_source_id(source_id)
        || *page == Some(0)
        || chunk_id
            .as_deref()
            .is_some_and(|id| !is_knowledge_chunk_id(id))
    {
        return None;
    }
    let mut value = format!("kb://{kb_id}/{source_id}");
    if let Some(page) = page {
        value.push_str(&format!("?page={page}"));
    }
    if let Some(chunk_id) = chunk_id {
        value.push('#');
        value.push_str(chunk_id);
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[derive(Deserialize)]
    struct RefCase {
        input: String,
        expected: Option<KnowledgeRef>,
    }

    #[test]
    fn knowledge_refs_match_shared_fixture() {
        let cases: Vec<RefCase> = serde_json::from_str(include_str!(
            "../../../src/shared/knowledge/__fixtures__/refs.json"
        ))
        .unwrap();
        assert!(cases.len() >= 30);
        for RefCase { input, expected } in cases {
            assert_eq!(parse_knowledge_ref(&input), expected, "{input:?}");
            if let Some(reference) = expected {
                assert_eq!(
                    format_knowledge_ref(&reference).as_deref(),
                    Some(input.as_str())
                );
            }
        }
    }

    #[test]
    fn knowledge_ref_formatter_rejects_invalid_parts() {
        let reference = KnowledgeRef {
            kb_id: "kb_0123456789ab".into(),
            source_id: "src_0123456789abcdef".into(),
            page: None,
            chunk_id: None,
        };
        for invalid in [
            KnowledgeRef {
                kb_id: "kb_0123456789AB".into(),
                ..reference.clone()
            },
            KnowledgeRef {
                source_id: "../src_0123456789abcdef".into(),
                ..reference.clone()
            },
            KnowledgeRef {
                page: Some(0),
                ..reference.clone()
            },
            KnowledgeRef {
                chunk_id: Some("1234?x=1".into()),
                ..reference.clone()
            },
        ] {
            assert_eq!(format_knowledge_ref(&invalid), None);
        }
    }

    #[test]
    fn knowledge_id_generators_produce_100_000_unique_ids_per_kind() {
        let mut seen = HashSet::with_capacity(400_000);
        for counter in 0..100_000u64 {
            let source_entropy = counter.to_be_bytes();
            let short_entropy = source_entropy[2..].try_into().unwrap();
            let base_id = create_knowledge_base_id(short_entropy);
            let source_id = create_knowledge_source_id(source_entropy);
            let job_id = create_knowledge_job_id("20261121", short_entropy).unwrap();
            let plan_id = create_knowledge_plan_id("20261121", short_entropy).unwrap();
            assert!(is_knowledge_base_id(&base_id));
            assert!(is_knowledge_source_id(&source_id));
            assert!(is_knowledge_job_id(&job_id));
            assert!(is_knowledge_plan_id(&plan_id));
            for id in [base_id, source_id, job_id, plan_id] {
                assert!(seen.insert(id));
            }
        }
        assert_eq!(seen.len(), 400_000);
    }

    #[test]
    fn knowledge_dated_ids_require_calendar_dates() {
        for invalid in [
            "00000101",
            "19000229",
            "20260229",
            "20261131",
            "20261301",
            "2026-11-21",
        ] {
            assert_eq!(create_knowledge_job_id(invalid, [0; 6]), None);
            assert_eq!(create_knowledge_plan_id(invalid, [0; 6]), None);
            assert!(!is_knowledge_job_id(&format!("job_{invalid}_000000000000")));
            assert!(!is_knowledge_plan_id(&format!(
                "plan_{invalid}_000000000000"
            )));
        }
        assert_eq!(
            create_knowledge_plan_id("20000229", [0xab; 6]).as_deref(),
            Some("plan_20000229_abababababab")
        );
    }

    #[test]
    fn knowledge_metadata_and_error_wire_names() {
        let field = KnowledgeMetadataField {
            value: "summary",
            origin: KnowledgeMetadataOrigin::User,
            updated_at: "2026-11-21T00:00:00.000Z".into(),
        };
        assert_eq!(
            serde_json::to_value(field).unwrap(),
            serde_json::json!({
                "value": "summary",
                "origin": "user",
                "updatedAt": "2026-11-21T00:00:00.000Z"
            })
        );
        let codes = [
            (KnowledgeErrorCode::KbNotFound, "KB_NOT_FOUND"),
            (KnowledgeErrorCode::KbAccessDenied, "KB_ACCESS_DENIED"),
            (KnowledgeErrorCode::KbFieldLocked, "KB_FIELD_LOCKED"),
            (KnowledgeErrorCode::KbLimitExceeded, "KB_LIMIT_EXCEEDED"),
            (
                KnowledgeErrorCode::KbSourceUnsupported,
                "KB_SOURCE_UNSUPPORTED",
            ),
            (KnowledgeErrorCode::KbIndexing, "KB_INDEXING"),
            (
                KnowledgeErrorCode::KbCategoryNotAllowed,
                "KB_CATEGORY_NOT_ALLOWED",
            ),
            (KnowledgeErrorCode::KbInvalidInput, "KB_INVALID_INPUT"),
            (KnowledgeErrorCode::KbStoreCorrupt, "KB_STORE_CORRUPT"),
            (
                KnowledgeErrorCode::KbStoreUnavailable,
                "KB_STORE_UNAVAILABLE",
            ),
        ];
        for (code, wire) in codes {
            assert_eq!(serde_json::to_value(code).unwrap(), serde_json::json!(wire));
            assert_eq!(
                serde_json::from_value::<KnowledgeErrorCode>(serde_json::json!(wire)).unwrap(),
                code
            );
        }
    }
}
