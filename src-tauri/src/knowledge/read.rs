use std::ops::Range;
use std::sync::LazyLock;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use super::chunker::{page_heading, top_level_headings};
use super::store::{KnowledgeStore, StoreResult};
use super::types::{
    format_knowledge_ref, parse_knowledge_ref, KnowledgeErrorCode::*, KnowledgeRef,
};

const PAGE_BYTES: usize = 64 * 1024;
const CURSOR_LENGTH: usize = 54;
const CURSOR_BYTES: usize = 40;
static CURSOR_KEY: LazyLock<[u8; 32]> = LazyLock::new(|| {
    let mut key = [0; 32];
    key[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    key[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    key
});

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadRequest {
    pub reference: String,
    pub page: Option<u32>,
    pub heading: Option<String>,
    pub cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadPage {
    reference: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
enum Selection<'a> {
    EntireSource,
    Page(u32),
    Heading(&'a str),
}

impl<'a> Selection<'a> {
    fn of(request: &'a ReadRequest, reference: &KnowledgeRef) -> StoreResult<Self> {
        if request
            .page
            .zip(reference.page)
            .is_some_and(|(page, reference)| page != reference)
        {
            return Err(KbInvalidInput);
        }
        match (request.page.or(reference.page), request.heading.as_deref()) {
            (None, Some(heading)) if !heading.trim().is_empty() => Ok(Self::Heading(heading)),
            (_, Some(_)) | (Some(0), None) => Err(KbInvalidInput),
            (Some(page), None) => Ok(Self::Page(page)),
            _ => Ok(Self::EntireSource),
        }
    }

    fn range(&self, text: &str) -> StoreResult<Range<usize>> {
        let mut headings = top_level_headings(text);
        let (start, end) = match self {
            Self::EntireSource => return Ok(0..text.len()),
            Self::Page(page) => {
                let start = headings
                    .find(|(level, name, _)| page_heading(*level, name) == Some(*page))
                    .ok_or(KbNotFound)?
                    .2;
                let next = headings.find(|(level, name, _)| page_heading(*level, name).is_some());
                (start, next)
            }
            Self::Heading(title) => {
                let (level, _, start) = headings
                    .find(|(level, name, _)| {
                        name.as_str() == *title && page_heading(*level, name).is_none()
                    })
                    .ok_or(KbNotFound)?;
                let next = headings
                    .find(|(next, name, _)| *next <= level && page_heading(*next, name).is_none());
                (start, next)
            }
        };
        Ok(start..end.map_or(text.len(), |(_, _, offset)| offset))
    }
}

fn cursor_mac(binding: &[u8], offset: &[u8]) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(CURSOR_KEY.as_slice())
        .expect("HMAC accepts the process cursor key");
    mac.update(b"kb-read-v1\0");
    mac.update(binding);
    mac.update(offset);
    mac
}

fn encode_cursor(binding: &[u8], offset: usize) -> String {
    let offset = (offset as u64).to_be_bytes();
    let signature = cursor_mac(binding, &offset).finalize().into_bytes();
    let mut cursor = [0; CURSOR_BYTES];
    cursor[..8].copy_from_slice(&offset);
    cursor[8..].copy_from_slice(&signature);
    URL_SAFE_NO_PAD.encode(cursor)
}

fn decode_cursor(binding: &[u8], cursor: &str) -> StoreResult<usize> {
    if cursor.len() != CURSOR_LENGTH {
        return Err(KbInvalidInput);
    }
    let bytes = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| KbInvalidInput)?;
    let decoded: [u8; CURSOR_BYTES] = bytes.try_into().map_err(|_| KbInvalidInput)?;
    let (offset, signature) = decoded.split_at(8);
    cursor_mac(binding, offset)
        .verify_slice(signature)
        .map_err(|_| KbInvalidInput)?;
    let offset = u64::from_be_bytes(offset.try_into().map_err(|_| KbInvalidInput)?);
    usize::try_from(offset).map_err(|_| KbInvalidInput)
}

fn page_slice<'a>(
    text: &'a str,
    range: Range<usize>,
    cursor: Option<&str>,
    binding: &[u8],
) -> StoreResult<(&'a str, Option<String>)> {
    let start = cursor
        .map(|cursor| decode_cursor(binding, cursor))
        .transpose()?
        .unwrap_or(range.start);
    if !text.is_char_boundary(start) || start < range.start || start > range.end {
        return Err(KbInvalidInput);
    }
    let end = text.floor_char_boundary(range.end.min(start + PAGE_BYTES));
    let next = (end < range.end).then(|| encode_cursor(binding, end));
    Ok((&text[start..end], next))
}

pub(crate) fn read_page(
    store: &KnowledgeStore,
    session: (&str, u64),
    request: ReadRequest,
) -> StoreResult<ReadPage> {
    let mut reference = parse_knowledge_ref(&request.reference).ok_or(KbInvalidInput)?;
    if reference.chunk_id.is_some() {
        return Err(KbInvalidInput);
    }
    let selection = Selection::of(&request, &reference)?;
    let source = store.get_source(&reference.kb_id, &reference.source_id)?;
    let hash = source.facts.content_hash.as_deref().ok_or(KbNotFound)?;
    let text = store.read_published_content(&reference.kb_id, &source)?;
    let range = selection.range(&text)?;
    if let Selection::Page(page) = selection {
        reference.page = Some(page);
    }
    let reference = format_knowledge_ref(&reference).ok_or(KbInvalidInput)?;
    let binding = serde_json::to_vec(&(session, &reference, hash, &selection))
        .map_err(|_| KbStoreUnavailable)?;
    let (content, next_cursor) = page_slice(&text, range, request.cursor.as_deref(), &binding)?;
    Ok(ReadPage {
        reference,
        content: content.to_owned(),
        next_cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_read_cursors_preserve_utf8_markdown_and_reject_tampering() {
        let text = format!("# Title\n{}\n", "果🧭".repeat(30_000));
        let binding = b"the exact Session, generation, content version and selector";
        let mut actual = String::new();
        let mut cursor = None;
        loop {
            let (page, next) =
                page_slice(&text, 0..text.len(), cursor.as_deref(), binding).unwrap();
            assert!(page.len() <= PAGE_BYTES);
            actual.push_str(page);
            let Some(next) = next else { break };
            assert_eq!(decode_cursor(b"wrong binding", &next), Err(KbInvalidInput));
            let mut tampered = next.clone().into_bytes();
            tampered[0] = if tampered[0] == b'A' { b'B' } else { b'A' };
            let tampered = String::from_utf8(tampered).unwrap();
            assert_eq!(decode_cursor(binding, &tampered), Err(KbInvalidInput));
            cursor = Some(next);
        }
        assert_eq!(actual, text);
    }

    #[test]
    fn knowledge_read_selects_real_headings_and_pdf_pages() {
        let text =
            "## Page 1\n\n# Forest\n\n```md\n## Page 7\n```\n\n## Page 2\n\n# Orchard\n\nPears.\n";
        let page = Selection::Page(2).range(text).unwrap();
        assert_eq!(&text[page], "## Page 2\n\n# Orchard\n\nPears.\n");
        let section = Selection::Heading("Orchard").range(text).unwrap();
        assert_eq!(&text[section], "# Orchard\n\nPears.\n");
        assert_eq!(Selection::Page(7).range(text), Err(KbNotFound));
    }
}
