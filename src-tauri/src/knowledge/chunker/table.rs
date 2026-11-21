use super::super::types::{format_knowledge_ref, KnowledgeRef};
use super::{units, ChunkBuilder, ChunkError, LIMIT, TARGET};
use crate::i18n::SupportedLocale;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

// All limits use the chunker's common tenths-of-a-token estimator.
pub(super) const HEADER_LIMIT: usize = LIMIT / 4;
const CONTEXT_LIMIT: usize = LIMIT - TARGET;
const LABEL_LIMIT: usize = 480;
const LABEL_START: usize = 320;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumn {
    pub column_index: usize,
    pub name_byte_range: [usize; 2],
    pub first_header_chunk_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableLocation {
    pub first_header_chunk_id: String,
    // Store the directory once, on the first header chunk. Every other fragment
    // can resolve it without copying arbitrarily long names into its metadata.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<TableColumn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<[usize; 2]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Templates {
    truncated_header: String,
    row_part: String,
    cell_part: String,
    long_column: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Common {
    knowledge_table: Templates,
}

fn labels(locale: SupportedLocale) -> &'static Templates {
    static ZH: LazyLock<Common> = LazyLock::new(|| {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/renderer/i18n/locales/zh-CN/common.json"
        )))
        .expect("checked-in zh-CN common resource")
    });
    static EN: LazyLock<Common> = LazyLock::new(|| {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/renderer/i18n/locales/en-US/common.json"
        )))
        .expect("checked-in en-US common resource")
    });
    match locale {
        SupportedLocale::ZhCn => &ZH.knowledge_table,
        SupportedLocale::EnUs => &EN.knowledge_table,
    }
}

fn render(template: &str, values: &[(&str, &str)]) -> String {
    let mut result = String::new();
    let mut rest = template;
    while let Some((literal, field)) = rest.split_once("{{") {
        let (key, tail) = field.split_once("}}").expect("closed resource placeholder");
        result.push_str(literal);
        result.push_str(
            values
                .iter()
                .find(|(name, _)| *name == key)
                .expect("resource parameter")
                .1,
        );
        rest = tail;
    }
    result.push_str(rest);
    result
}

fn cost(text: &str) -> usize {
    units(text).map(|(_, n)| n).sum()
}

#[derive(Clone)]
struct Cell {
    syntax: Range<usize>,
    value: Range<usize>,
}

// The parser has already recognized a GFM table. Preserve source delimiters
// and escaped pipes; a rendered TableCell omits that information.
fn cells(md: &str, row: &Range<usize>) -> Vec<Cell> {
    let s = &md[row.clone()];
    let left = row.start + s.len() - s.trim_start_matches([' ', '\t']).len();
    let right = row.start + s.trim_end_matches([' ', '\t']).len();
    let mut separators = Vec::new();
    let mut escaped = false;
    for (offset, b) in s.bytes().enumerate() {
        if b == b'|' && !escaped {
            separators.push(row.start + offset);
        }
        escaped = b == b'\\' && !escaped;
    }
    let mut value_start = row.start;
    let mut first = 0;
    if separators.first() == Some(&left) {
        value_start = left + 1;
        first = 1;
    }
    let mut fields = Vec::new();
    let mut raw_start = row.start;
    for &pipe in &separators[first..] {
        let last = pipe + 1 == right;
        let end = if last { row.end } else { pipe };
        fields.push(Cell {
            syntax: raw_start..end,
            value: trim_cell(md, value_start..pipe),
        });
        raw_start = pipe;
        value_start = pipe + 1;
        if last {
            return fields;
        }
    }
    if value_start <= row.end {
        fields.push(Cell {
            syntax: raw_start..row.end,
            value: trim_cell(md, value_start..row.end),
        });
    }
    fields
}

fn trim_cell(md: &str, range: Range<usize>) -> Range<usize> {
    let raw = &md[range.clone()];
    let trimmed = raw.trim_matches([' ', '\t']);
    if trimmed.is_empty() {
        return range.end..range.end;
    }
    let from = range.start + raw.len() - raw.trim_start_matches([' ', '\t']).len();
    from..from + trimmed.len()
}

// Consume a prefix once. Even a boundary-free cell is linear in its length.
// Split only between extended graphemes. Formatting whitespace stays on one
// side so concatenating successive owned ranges recovers every input byte.
fn cut(
    text: &str,
    target: usize,
    hard: usize,
    min_semantic: Option<usize>,
) -> Result<usize, ChunkError> {
    let mut marks = units(text).peekable();
    let mut words = text
        .split_word_bound_indices()
        .map(|(i, s)| i + s.len())
        .peekable();
    let mut used = 0;
    let mut last = 0;
    let mut last_cost = 0;
    let mut preferred = [0; 3];
    for (i, g) in text.grapheme_indices(true) {
        let end = i + g.len();
        while marks.peek().is_some_and(|(at, _)| *at < end) {
            used += marks.next().unwrap().1;
        }
        if used > target {
            if min_semantic.is_some_and(|min| last_cost < min && used >= min && used <= hard) {
                return Ok(end);
            }
            if last == 0 {
                return if used <= hard {
                    Ok(end)
                } else {
                    Err(ChunkError::InsufficientContextSpace)
                };
            }
            if min_semantic.is_some() {
                for pos in preferred {
                    if pos != 0 {
                        return Ok(pos);
                    }
                }
            }
            return Ok(last);
        }
        last = end;
        last_cost = used;
        while words.peek().is_some_and(|&pos| pos <= end) {
            if words.next() == Some(end) && min_semantic.is_some_and(|min| used >= min) {
                preferred[2] = end;
            }
        }
        if min_semantic.is_none_or(|min| used < min) {
            continue;
        }
        if matches!(g, "。" | "！" | "？" | "；" | "." | "!" | "?" | ";") {
            preferred[1] = end;
        }
        if matches!(g, "\n" | "\r" | "\r\n")
            || (g == ">"
                && ["<br>", "<br/>", "<br />"]
                    .iter()
                    .any(|tag| text[..end].ends_with(tag)))
        {
            preferred[0] = end;
        }
    }
    Ok(text.len())
}

fn reference(kb: &str, source: &str, id: &str) -> Result<String, ChunkError> {
    format_knowledge_ref(&KnowledgeRef {
        kb_id: kb.into(),
        source_id: source.into(),
        page: None,
        chunk_id: Some(id.into()),
    })
    .ok_or(ChunkError::InvalidReference)
}

fn shown_name(
    md: &str,
    column: &TableColumn,
    locale: SupportedLocale,
    kb: &str,
    source: &str,
) -> Result<String, ChunkError> {
    let [start, end] = column.name_byte_range;
    let name = &md[start..end];
    if cost(name) <= LABEL_LIMIT {
        return Ok(name.into());
    }
    // A single grapheme can itself cost more than the display limit. Keep the
    // original in the header and use the ellipsis/ref with an empty preview.
    let n = cut(name, LABEL_START, LABEL_START, None).unwrap_or(0);
    let link = reference(kb, source, &column.first_header_chunk_id)?;
    Ok(render(
        &labels(locale).long_column,
        &[
            ("start", &name[..n]),
            ("index", &column.column_index.to_string()),
            ("ref", &link),
        ],
    ))
}

fn excerpt(names: &[String], budget: usize, reference: &str, locale: SupportedLocale) -> String {
    let mut line = String::new();
    let mut used = 10;
    let mut shown = 0;
    for name in names {
        let weight = cost(name) + 10;
        if used + weight > budget {
            break;
        }
        line.push_str("| ");
        line.push_str(name);
        line.push(' ');
        used += weight;
        shown += 1;
    }
    if shown != 0 {
        line.push_str("|\n");
    }
    line.push_str(&render(
        &labels(locale).truncated_header,
        &[
            ("shown", &shown.to_string()),
            ("total", &names.len().to_string()),
            ("ref", reference),
        ],
    ));
    line
}

struct View {
    names: Vec<String>,
    excerpt: String,
}
fn view(
    columns: &[TableColumn],
    wide: bool,
    raw: &str,
    first: &str,
    builder: &ChunkBuilder<'_>,
    locale: SupportedLocale,
    budget: usize,
) -> Result<View, ChunkError> {
    let names = columns
        .iter()
        .map(|c| shown_name(builder.markdown, c, locale, builder.kb, builder.source))
        .collect::<Result<Vec<_>, _>>()?;
    let excerpt = if wide {
        excerpt(&names, budget, first, locale)
    } else if columns.iter().zip(&names).any(|(column, name)| {
        let [start, end] = column.name_byte_range;
        name != &builder.markdown[start..end]
    }) {
        let delimiter = raw.rsplit_once('\n').map_or("", |(_, line)| line);
        format!("| {} |\n{delimiter}", names.join(" | "))
    } else {
        raw.to_owned()
    };
    Ok(View { names, excerpt })
}

fn header_spans(
    md: &str,
    header: Range<usize>,
    row: Range<usize>,
    delimiter: Range<usize>,
) -> Result<Vec<Range<usize>>, ChunkError> {
    if cost(&md[header.clone()]) <= LIMIT {
        return Ok(vec![header]);
    }
    let fields = cells(md, &row).into_iter().chain(cells(md, &delimiter));
    let mut spans = Vec::new();
    let mut start = header.start;
    let mut end = start;
    let mut used = 0;
    for field in fields {
        let raw = &md[end..field.syntax.end];
        let weight = cost(raw);
        if used + weight > LIMIT && start < end {
            spans.push(start..end);
            start = end;
            used = 0;
        }
        if weight > LIMIT {
            while start < field.syntax.end {
                let tail = &md[start..field.syntax.end];
                let split = cut(tail, LIMIT, LIMIT, None)?;
                end = start + split;
                if end < field.syntax.end {
                    spans.push(start..end);
                    start = end;
                } else {
                    used = cost(&md[start..end]);
                    break;
                }
            }
        } else {
            used += weight;
            end = field.syntax.end;
        }
    }
    if start < header.end {
        spans.push(start..header.end);
    }
    Ok(spans)
}

#[derive(Debug)]
struct Piece {
    own: Range<usize>,
    column: Option<usize>,
    cell_part: Option<[usize; 2]>,
}

// Decimal row/part numbers are a single token run regardless of digit count.
// Compare the actual templates across locales once, keeping body boundaries
// independent of display language or the chosen excerpt length.
#[derive(Clone, Copy, Default)]
struct CellContext {
    full: usize,
    locator: usize,
}

fn cell_marker(
    locale: SupportedLocale,
    column: &str,
    row: &str,
    part: &str,
    total: &str,
) -> String {
    render(
        &labels(locale).cell_part,
        &[
            ("row", row),
            ("column", column),
            ("part", part),
            ("total", total),
        ],
    )
}

fn row_parts(
    md: &str,
    row: Range<usize>,
    contexts: &[CellContext],
    extra_column_context: CellContext,
) -> Result<Vec<Piece>, ChunkError> {
    let mut result = Vec::new();
    let mut pending: Option<Range<usize>> = None;
    let mut used = 0;
    for (idx, cell) in cells(md, &row).into_iter().enumerate() {
        let overhead = contexts.get(idx).unwrap_or(&extra_column_context);
        let weight = cost(&md[cell.syntax.clone()]);
        if (used + weight > TARGET || weight + overhead.full > LIMIT) && pending.is_some() {
            result.push(Piece {
                own: pending.take().unwrap(),
                column: None,
                cell_part: None,
            });
            used = 0;
        }
        if weight + overhead.full > LIMIT && !cell.value.is_empty() {
            let first = result.len();
            let mut at = cell.value.start;
            let body_budget = LIMIT - overhead.full;
            while at < cell.value.end {
                let n = cut(
                    &md[at..cell.value.end],
                    body_budget,
                    LIMIT - overhead.locator,
                    Some(TARGET),
                )?;
                result.push(Piece {
                    own: at..at + n,
                    column: (idx < contexts.len()).then_some(idx),
                    cell_part: None,
                });
                at += n;
            }
            let count = result.len() - first;
            for (part, p) in result[first..].iter_mut().enumerate() {
                p.cell_part = Some([part + 1, count]);
            }
        } else {
            pending = Some(pending.map_or(cell.syntax.clone(), |p| p.start..cell.syntax.end));
            used += weight;
        }
    }
    if let Some(own) = pending {
        result.push(Piece {
            own,
            column: None,
            cell_part: None,
        });
    }
    Ok(result)
}

impl ChunkBuilder<'_> {
    fn ordinary_table(
        &mut self,
        header: Range<usize>,
        rows: &[Range<usize>],
        weights: &[usize],
        head_cost: usize,
    ) {
        let md = self.markdown;
        let raw = &md[header.clone()];
        let mut left = weights.iter().sum::<usize>();
        let mut row = 0;
        self.previous = None;
        loop {
            let first = self.byte <= header.start && row == 0;
            let limit = if left + head_cost <= LIMIT {
                LIMIT
            } else {
                TARGET
            };
            let start = if first { header.start } else { rows[row].start };
            let mut end = if first { header.end } else { start };
            let mut used = if first { head_cost } else { 0 };
            let initial = row;
            while row < rows.len() {
                let w = weights[row];
                if row > initial && used + w > limit {
                    break;
                }
                if !first && row > initial && used + w + head_cost > LIMIT {
                    break;
                }
                used += w;
                left -= w;
                end = rows[row].end;
                row += 1;
            }
            self.emit(start..end, if first { "" } else { raw });
            if row == rows.len() {
                break;
            }
        }
    }

    fn table_emit(&mut self, own: Range<usize>, context: &str, location: TableLocation) {
        self.previous = None;
        self.emit(own, context);
        self.chunks.last_mut().unwrap().table = Some(location);
    }

    pub(super) fn table(&mut self, span: Range<usize>) -> Result<(), ChunkError> {
        let md = self.markdown;
        let mut offset = span.start;
        let mut rows = Vec::new();
        for line in md[span].split_inclusive('\n') {
            rows.push(offset..offset + line.trim_end_matches(['\r', '\n']).len());
            offset += line.len();
        }
        let header = rows[0].start..rows[1].end;
        let raw = &md[header.clone()];
        let head_cost = cost(raw);
        let wide = head_cost > HEADER_LIMIT;
        let body = &rows[2..];
        let weights = body
            .iter()
            .map(|r| cost(&md[r.clone()]))
            .collect::<Vec<_>>();
        let fields = cells(md, &rows[0]);
        let repeats_long_name = head_cost + weights.iter().sum::<usize>() > LIMIT
            && fields
                .iter()
                .any(|field| cost(&md[field.value.clone()]) > LABEL_LIMIT);
        // The ordinary path retains the exact source layout and its golden IDs.
        if !wide && !repeats_long_name && weights.iter().all(|&w| w + head_cost <= LIMIT) {
            self.ordinary_table(header, body, &weights, head_cost);
            return Ok(());
        }
        let first_index = self.chunks.len();
        let heads = header_spans(md, header, rows[0].clone(), rows[1].clone())?;
        for piece in &heads {
            self.table_emit(piece.clone(), "", TableLocation::default());
        }
        let first_id = self.chunks[first_index].chunk_id.clone();
        let first_ref = reference(self.kb, self.source, &first_id)?;
        let columns = fields
            .iter()
            .enumerate()
            .map(|(n, field)| {
                let offset = field.value.start.min(rows[0].end.saturating_sub(1));
                let idx = heads
                    .partition_point(|h| h.end <= offset)
                    .min(heads.len() - 1);
                TableColumn {
                    column_index: n + 1,
                    name_byte_range: [field.value.start, field.value.end],
                    first_header_chunk_id: self.chunks[first_index + idx].chunk_id.clone(),
                }
            })
            .collect::<Vec<_>>();
        for chunk in &mut self.chunks[first_index..] {
            chunk.table.as_mut().unwrap().first_header_chunk_id = first_id.clone();
        }
        let rendered = view(
            &columns,
            wide,
            raw,
            &first_ref,
            self,
            self.locale,
            self.excerpt_budget.min(HEADER_LIMIT),
        )?;
        // Use the same structural plan for any rendering language or excerpt budget.
        let mut allowance = 0;
        let mut contexts = vec![CellContext::default(); columns.len()];
        let mut extra_column_context = CellContext::default();
        for locale in [SupportedLocale::ZhCn, SupportedLocale::EnUs] {
            let canonical = view(&columns, wide, raw, &first_ref, self, locale, HEADER_LIMIT)?;
            let header_cost = cost(&canonical.excerpt);
            allowance = allowance.max(header_cost);
            for (context, name) in contexts.iter_mut().zip(canonical.names) {
                let marker_cost = cost(&cell_marker(locale, &name, "1", "1", "1"));
                context.full = context
                    .full
                    .max((header_cost + marker_cost).min(CONTEXT_LIMIT));
                context.locator = context.locator.max(marker_cost);
            }
            let row_marker = render(
                &labels(locale).row_part,
                &[("row", "1"), ("part", "1"), ("total", "1")],
            );
            let row_cost = cost(&row_marker);
            extra_column_context.locator = extra_column_context.locator.max(row_cost);
            extra_column_context.full = extra_column_context
                .full
                .max((header_cost + row_cost).min(CONTEXT_LIMIT));
        }
        let root = TableLocation {
            first_header_chunk_id: first_id,
            ..Default::default()
        };
        let mut pending: Option<Range<usize>> = None;
        let mut used = 0;
        for (idx, (row, &weight)) in body.iter().zip(&weights).enumerate() {
            if pending.is_some() && used + weight > TARGET {
                self.table_emit(pending.take().unwrap(), &rendered.excerpt, root.clone());
                used = 0;
            }
            if weight + allowance <= LIMIT {
                pending = Some(pending.map_or(row.clone(), |p| p.start..row.end));
                used += weight;
                continue;
            }
            let pieces = row_parts(md, row.clone(), &contexts, extra_column_context)?;
            let total = pieces.len();
            for (j, part) in pieces.into_iter().enumerate() {
                let indices = part.cell_part.unwrap_or([j + 1, total]);
                let row_num = (idx + 1).to_string();
                let segment = indices[0].to_string();
                let count = indices[1].to_string();
                let column = part
                    .column
                    .map(|n| rendered.names[n].as_str())
                    .unwrap_or("");
                let vars = [
                    ("row", row_num.as_str()),
                    ("column", column),
                    ("part", segment.as_str()),
                    ("total", count.as_str()),
                ];
                let lang = labels(self.locale);
                let mark = if part.column.is_some() {
                    cell_marker(self.locale, column, &row_num, &segment, &count)
                } else {
                    render(&lang.row_part, &vars)
                };
                let available =
                    CONTEXT_LIMIT.min(LIMIT.saturating_sub(cost(&md[part.own.clone()])));
                let ctx = fit_context(
                    &rendered.excerpt,
                    part.column.map(|_| column),
                    mark,
                    available,
                )?;
                let loc = TableLocation {
                    row: Some(idx + 1),
                    column_index: part.column.map(|n| n + 1),
                    part: Some(indices),
                    ..root.clone()
                };
                self.table_emit(part.own, &ctx, loc);
            }
        }
        if let Some(own) = pending {
            self.table_emit(own, &rendered.excerpt, root);
        }
        self.chunks[first_index].table.as_mut().unwrap().columns = columns;
        Ok(())
    }
}

fn fit_context(
    full: &str,
    column: Option<&str>,
    marker: String,
    available: usize,
) -> Result<String, ChunkError> {
    let complete = format!("{full}\n{marker}");
    if cost(&complete) <= available {
        return Ok(complete);
    }
    if let Some(name) = column {
        let own = format!("| {name} |\n{marker}");
        if cost(&own) <= available {
            return Ok(own);
        }
    }
    if cost(&marker) <= available {
        Ok(marker)
    } else {
        Err(ChunkError::InsufficientContextSpace)
    }
}
