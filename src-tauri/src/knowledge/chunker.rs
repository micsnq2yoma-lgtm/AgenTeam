use crate::i18n::SupportedLocale;
use pulldown_cmark::{Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};
mod table;
pub use table::{TableColumn, TableLocation};
#[cfg(test)]
mod tests;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

const TARGET: usize = 6_000;
const LIMIT: usize = 10_000;
const OVERLAP: usize = 800;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunk {
    pub chunk_id: String,
    pub ordinal: usize,
    pub heading_path: Vec<String>,
    pub page: Option<u32>,
    pub text: String,
    pub prefix: String,
    pub byte_range: [usize; 2],
    pub utf16_range: [usize; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableLocation>,
}

fn ideograph(c: char) -> bool {
    matches!(c as u32, 0x2e80..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff |
        0x20000..=0x323af | 0xff66..=0xff9d)
}

// Tenths of a token. A Latin word weighs 13, a CJK character or non-space
// symbol weighs 10. Word boundaries, row ends and highlights stay on UTF-8 boundaries.
fn units(s: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut word = false;
    s.char_indices().filter_map(move |(byte, c)| {
        let in_word = !ideograph(c) && (c.is_alphanumeric() || c == '_');
        let weight = if in_word && !word {
            13
        } else if c.is_whitespace() || in_word {
            0
        } else {
            10
        };
        word = in_word;
        (weight != 0).then_some((byte, weight))
    })
}

fn suffix(text: &str, budget: usize) -> &str {
    let positions: Vec<_> = units(text).collect();
    let mut weight = 0;
    let mut start = text.len();
    for (index, cost) in positions.into_iter().rev() {
        if weight + cost > budget {
            break;
        }
        weight += cost;
        start = index;
    }
    if !text.is_ascii() && start < text.len() {
        start = text
            .grapheme_indices(true)
            .map(|(offset, _)| offset)
            .find(|&offset| offset >= start)
            .unwrap_or(text.len());
    }
    &text[start..]
}

// Charge a multi-scalar grapheme together. A preferred paragraph size may
// be exceeded by one indivisible grapheme, but the hard limit still applies.
fn grapheme_units(text: &str) -> Vec<(usize, usize)> {
    if text.is_ascii() {
        return units(text).collect();
    }
    let mut scalars = units(text).peekable();
    let mut result = Vec::new();
    for (start, grapheme) in text.grapheme_indices(true) {
        let end = start + grapheme.len();
        let mut total = 0;
        while scalars.peek().is_some_and(|&(offset, _)| offset < end) {
            total += scalars.next().unwrap().1;
        }
        if total != 0 {
            result.push((start, total));
        }
    }
    result
}

struct ChunkBuilder<'a> {
    markdown: &'a str,
    source: &'a str,
    kb: &'a str,
    locale: SupportedLocale,
    excerpt_budget: usize,
    title: &'a str,
    headings: Vec<(usize, String)>,
    page: Option<u32>,
    counts: HashMap<(Vec<String>, [u8; 32]), u64>,
    chunks: Vec<KnowledgeChunk>,
    previous: Option<Range<usize>>,
    byte: usize,
    utf16: usize,
}

impl ChunkBuilder<'_> {
    fn emit(&mut self, span: Range<usize>, repeated: &str) {
        let own = &self.markdown[span.clone()];
        let path: Vec<_> = self.headings.iter().map(|(_, s)| s.clone()).collect();
        let mut normalized = Sha256::new();
        for (n, line) in own.split('\n').enumerate() {
            if n != 0 {
                normalized.update(b"\n");
            }
            normalized.update(line.trim_end().as_bytes());
        }
        let body: [u8; 32] = normalized.finalize().into();
        let occurrence = self.counts.entry((path.clone(), body)).or_default();
        let mut id = Sha256::new();
        id.update(self.source.as_bytes());
        id.update((path.len() as u32).to_be_bytes());
        for part in &path {
            id.update((part.len() as u64).to_be_bytes());
            id.update(part.as_bytes());
        }
        id.update(body);
        id.update(occurrence.to_be_bytes());
        *occurrence += 1;
        let digest = id.finalize();
        let mut text = String::new();
        if !repeated.is_empty() {
            text.push_str(repeated);
            text.push('\n');
        } else if let Some(previous) = &self.previous {
            let body_cost = units(own).map(|(_, weight)| weight).sum();
            let overlap = suffix(
                &self.markdown[previous.clone()],
                OVERLAP.min(LIMIT.saturating_sub(body_cost)),
            );
            if !overlap.is_empty() {
                text.push_str(overlap);
                text.push('\n');
            }
        }
        text.push_str(own);
        self.utf16 += self.markdown[self.byte..span.start].encode_utf16().count();
        let start = self.utf16;
        self.utf16 += own.encode_utf16().count();
        self.byte = span.end;
        self.chunks.push(KnowledgeChunk {
            chunk_id: format!(
                "{:016x}",
                u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 has eight bytes"))
            ),
            ordinal: self.chunks.len(),
            heading_path: path.clone(),
            page: self.page,
            text,
            prefix: std::iter::once(self.title)
                .chain(path.iter().map(String::as_str))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" › "),
            byte_range: [span.start, span.end],
            utf16_range: [start, self.utf16],
            table: None,
        });
        self.previous = Some(span);
    }

    // Small structural blocks have a stable boundary. For a long block prefer
    // complete lines. An oversized single line is split at token boundaries.
    fn block(&mut self, span: Range<usize>) {
        let block = self.markdown[span.clone()].trim_end_matches(['\r', '\n']);
        if block.is_empty() {
            return;
        }
        let positions = grapheme_units(block);
        let mut sum = 0;
        let cumulative: Vec<_> = positions
            .iter()
            .map(|&(_, weight)| {
                sum += weight;
                sum
            })
            .collect();
        let mut from = 0;
        let mut first: usize = 0;
        while from < block.len() {
            let hard = if self.previous.is_some() {
                LIMIT - OVERLAP
            } else {
                LIMIT
            };
            let before = first.checked_sub(1).map_or(0, |i| cumulative[i]);
            let budget = if sum - before <= hard { hard } else { TARGET };
            let after = cumulative
                .partition_point(|&n| n <= before + budget)
                .max(first.saturating_add(1));
            let tentative = positions.get(after).map_or(block.len(), |&(at, _)| at);
            let end = if tentative < block.len() {
                block[from..tentative]
                    .rfind('\n')
                    .map_or(tentative, |n| from + n + 1)
            } else {
                tentative
            };
            let own_end = from + block[from..end].trim_end_matches(['\r', '\n']).len();
            if own_end > from {
                self.emit(span.start + from..span.start + own_end, "");
            }
            from = end;
            first = positions.partition_point(|&(pos, _)| pos < from);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkPosition {
    pub row: usize,
    pub column: usize,
    pub byte_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkError {
    InvalidReference,
    GraphemeExceedsBudget(ChunkPosition),
    InsufficientContextSpace,
}

// Validate the entire source, including headings and other text a Markdown
// parser can omit. Each byte accounts for at most 13 units, so ordinary
// graphemes need no second tokenization pass.
fn validate_grapheme_budget(markdown: &str) -> Result<(), ChunkError> {
    if markdown.is_ascii() {
        return Ok(());
    }
    for (byte_offset, grapheme) in markdown.grapheme_indices(true) {
        if grapheme.len() <= LIMIT / 13 || units(grapheme).map(|(_, n)| n).sum::<usize>() <= LIMIT {
            continue;
        }
        let preceding = &markdown[..byte_offset];
        let current_line = preceding.rsplit('\n').next().unwrap_or_default();
        return Err(ChunkError::GraphemeExceedsBudget(ChunkPosition {
            row: 1 + preceding.bytes().filter(|&b| b == b'\n').count(),
            column: 1 + current_line.chars().count(),
            byte_offset,
        }));
    }
    Ok(())
}

pub fn chunk_markdown(
    kb_id: &str,
    source_id: &str,
    title: &str,
    markdown: &str,
    locale: SupportedLocale,
) -> Result<Vec<KnowledgeChunk>, ChunkError> {
    chunk_with_excerpt(
        kb_id,
        source_id,
        title,
        markdown,
        locale,
        table::HEADER_LIMIT,
    )
}
fn chunk_with_excerpt(
    kb_id: &str,
    source_id: &str,
    title: &str,
    markdown: &str,
    locale: SupportedLocale,
    excerpt_budget: usize,
) -> Result<Vec<KnowledgeChunk>, ChunkError> {
    if !super::types::is_knowledge_base_id(kb_id)
        || !super::types::is_knowledge_source_id(source_id)
    {
        return Err(ChunkError::InvalidReference);
    }
    validate_grapheme_budget(markdown)?;
    let mut builder = ChunkBuilder {
        markdown,
        source: source_id,
        kb: kb_id,
        locale,
        excerpt_budget,
        title,
        headings: Vec::new(),
        page: None,
        counts: HashMap::new(),
        chunks: Vec::new(),
        previous: None,
        byte: 0,
        utf16: 0,
    };
    let has_heading = collect_blocks(&mut builder, false)?;
    if builder.chunks.is_empty() && has_heading {
        // A text-only PDF can convert entirely to headings. Replay only
        // sources without body chunks so existing references stay stable.
        builder.headings.clear();
        builder.page = None;
        collect_blocks(&mut builder, true)?;
    }
    Ok(builder.chunks)
}

fn collect_blocks(
    builder: &mut ChunkBuilder<'_>,
    headings_as_content: bool,
) -> Result<bool, ChunkError> {
    let mut has_heading = false;
    let mut events = Parser::new_ext(builder.markdown, Options::ENABLE_TABLES).into_offset_iter();
    while let Some((event, span)) = events.next() {
        let Event::Start(tag) = event else {
            if matches!(event, Event::Rule | Event::Html(_)) {
                builder.block(span);
            }
            continue;
        };
        let mut depth = 1;
        let mut name = String::new();
        let is_heading = matches!(&tag, Tag::Heading { .. });
        for (child, _) in events.by_ref() {
            match child {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Event::Text(t) | Event::Code(t) if is_heading => name.push_str(&t),
                Event::SoftBreak | Event::HardBreak if is_heading => name.push(' '),
                _ => {}
            }
        }
        match tag {
            Tag::Heading { level, .. } => {
                let level = level as usize;
                let page = if level == 2 {
                    name.strip_prefix("Page ")
                        .and_then(|n| n.trim().parse::<u32>().ok())
                        .filter(|&n| n > 0)
                } else {
                    None
                };
                builder.previous = None;
                if let Some(n) = page {
                    builder.page = Some(n);
                } else {
                    let has_text = !name.trim().is_empty();
                    has_heading |= has_text;
                    builder.headings.retain(|(depth, _)| *depth < level);
                    builder.headings.push((level, name));
                    if headings_as_content && has_text {
                        builder.block(span);
                    }
                }
            }
            Tag::Table(_) => builder.table(span)?,
            _ => builder.block(span),
        }
    }
    Ok(has_heading)
}
