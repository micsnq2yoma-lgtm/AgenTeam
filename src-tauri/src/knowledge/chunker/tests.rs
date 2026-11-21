use super::{chunk_markdown as chunk, KnowledgeChunk, SupportedLocale};
const KB: &str = "kb_0123456789ab";
fn chunk_markdown(source: &str, title: &str, input: &str) -> Vec<KnowledgeChunk> {
    chunk(KB, source, title, input, SupportedLocale::EnUs).unwrap()
}
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

const SOURCE: &str = "src_0123456789abcdef";
const TITLE: &str = "Field notes";

macro_rules! fixtures {
    ($($name:literal),+ $(,)?) => {
        [$(($name, include_str!(concat!("fixtures/", $name, ".md")))),+]
    };
}

#[test]
fn markdown_goldens_match_all_public_fields() {
    let snapshots: BTreeMap<String, Value> =
        serde_json::from_str(include_str!("fixtures/golden.json")).unwrap();
    let cases = fixtures![
        "chinese-long",
        "english",
        "mixed",
        "hierarchy",
        "large-table",
        "long-code",
        "pdf-pages",
        "plain",
        "empty",
        "headings-only",
        "unicode",
        "list",
        "duplicates",
        "blockquote",
        "inline-heading",
        "code-markers",
    ];
    assert_eq!(cases.len(), snapshots.len());
    for (name, markdown) in cases {
        // Fixture byte ranges target LF on every checkout, including Windows autocrlf.
        let markdown = markdown.replace("\r\n", "\n");
        let actual = serde_json::to_value(chunk_markdown(SOURCE, TITLE, &markdown)).unwrap();
        assert_eq!(actual, snapshots[name], "{name}");
    }
}

fn ids(markdown: &str) -> Vec<String> {
    chunk_markdown(SOURCE, TITLE, markdown)
        .into_iter()
        .map(|chunk| chunk.chunk_id)
        .collect()
}

#[test]
fn expanding_the_first_paragraph_preserves_every_later_reference() {
    let original = "# Garden\n\nShort.\n\nGate closed.\n\nRows numbered.\n";
    let expanded = original.replace("Short.", &"果".repeat(2300));
    let before = ids(original);
    let after = ids(&expanded);
    assert_eq!(before, ids(original));
    assert_eq!(before.len(), 3);
    assert!(after.len() > before.len());
    assert_eq!(before[1..], after[after.len() - 2..]);
    assert!(!after.contains(&before[0]));
}

#[test]
fn inserting_a_paragraph_only_introduces_one_reference() {
    let original = ids("North gate.\n\nSouth gate.");
    let edited = ids("North gate.\n\nWest gate.\n\nSouth gate.");
    assert_eq!(original.len(), 2);
    assert_eq!(edited.len(), 3);
    assert_eq!(original[0], edited[0]);
    assert_eq!(original[1], edited[2]);
    assert!(!original.contains(&edited[1]));
}

#[test]
fn occurrence_counts_are_local_to_identical_body_and_heading_path() {
    let before = ids("# A\n\nRepeat.\n\nOld note.\n\nRepeat.\n\n# B\n\nRepeat.");
    let after = ids("# A\n\nNew note.\n\nRepeat.\n\nRepeat.\n\n# B\n\nRepeat.");
    assert_eq!(before.len(), 4);
    assert_eq!(after.len(), 4);
    assert_eq!(
        [&before[0], &before[2], &before[3]],
        [&after[1], &after[2], &after[3]]
    );
    assert_eq!(before.iter().collect::<HashSet<_>>().len(), before.len());
    assert!(!after.contains(&before[1]));
}

#[test]
fn editing_body_retains_source_but_removes_the_original_chunk_id() {
    let before = ids("# A\n\nLeave the gate open.\n\nCheck the latch.\n");
    let after = ids("# A\n\nClose the gate.\n\nCheck the latch.\n");
    assert_eq!(before.len(), 2);
    assert_eq!(after.len(), 2);
    assert!(!after.contains(&before[0]));
    assert_eq!(before[1], after[1]);
}

#[test]
fn updating_the_table_header_keeps_continuation_row_ids() {
    let original = include_str!("fixtures/large-table.md").replace("\r\n", "\n");
    let updated = original.replacen("果实", "品种", 1);
    let before = chunk_markdown(SOURCE, TITLE, &original);
    let after = chunk_markdown(SOURCE, TITLE, &updated);
    assert_eq!(before.len(), 3);
    assert_eq!(before.len(), after.len());
    assert_ne!(before[0].chunk_id, after[0].chunk_id);
    for (before, after) in before[1..].iter().zip(&after[1..]) {
        assert_eq!(before.chunk_id, after.chunk_id);
        assert!(before.text.starts_with("| 号 | 果实 |\n"));
        assert!(after.text.starts_with("| 号 | 品种 |\n"));
    }
}

#[test]
fn ids_ignore_display_context_line_endings_and_trailing_whitespace() {
    let clean = "# Trees\n\n## Page 7\n\nPear harvest.\nApple harvest.\n";
    let formatted = "# Trees\r\n\r\n## Page 13\r\n\r\nPear harvest.  \r\nApple harvest.\t \r\n";
    let original = chunk_markdown(SOURCE, TITLE, clean);
    let repaged = chunk_markdown(SOURCE, "New title", formatted);
    assert_eq!(original.len(), 1);
    assert_eq!(repaged.len(), 1);
    assert_eq!(original[0].chunk_id, repaged[0].chunk_id);
    assert_eq!((original[0].page, repaged[0].page), (Some(7), Some(13)));
    assert_eq!(repaged[0].prefix, "New title › Trees");
    assert_ne!(original[0].text, repaged[0].text);
    assert_ne!(original[0].byte_range, repaged[0].byte_range);
    let other_source = chunk_markdown("src_fedcba9876543210", TITLE, clean);
    assert_ne!(original[0].chunk_id, other_source[0].chunk_id);
    assert_ne!(
        original[0].chunk_id,
        ids(&clean.replace("# Trees", "# Hedges"))[0]
    );
}

// Independent oracle for the test alphabet only. Each ASCII word costs 1.3,
// each Han ideograph, emoji, combining mark or punctuation mark costs 1.
fn sample_token_tenths(text: &str) -> usize {
    let mut total = 0;
    let mut in_word = false;
    for c in text.chars() {
        let han = matches!(c, '梨' | '树' | '𠮷' | '界');
        let word = !han && (c.is_alphanumeric() || c == '_');
        if word && !in_word {
            total += 13;
        } else if !word && !c.is_whitespace() {
            total += 10;
        }
        in_word = word;
    }
    total
}

struct SampleRng(u64);

impl SampleRng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

#[test]
fn one_thousand_seeded_unicode_cases_respect_budgets_and_source_coordinates() {
    let mut rng = SampleRng(0x35b29d70e814c69f);
    let alphabet = [
        "word",
        "123",
        "_a",
        "é",
        "e\u{301}",
        "роса",
        "梨",
        "树",
        "界",
        "𠮷",
        "🌱",
        "👩‍🌾",
        ".",
        ",",
        "-",
        "|",
        "*",
        "#",
        "@",
        "\u{200b}",
        " ",
        "\t",
        "\u{00a0}",
        "\n",
        "\r\n",
    ];
    let prose = [
        "word ",
        "123 ",
        "e\u{301}",
        "梨",
        "树",
        "界",
        "𠮷",
        "🌱",
        "👩‍🌾",
        ".",
    ];
    for case in 0..1000 {
        let count = if case % 4 == 0 {
            1500 + rng.below(2200)
        } else {
            rng.below(120)
        };
        // Exactly 125 cases are long single paragraphs. They must exercise the
        // hard-limit splitter, independently of the random Markdown punctuation.
        let one_paragraph = case % 8 == 0;
        let tokens: &[&str] = if one_paragraph { &prose } else { &alphabet };
        let mut markdown = String::new();
        for _ in 0..count {
            markdown.push_str(tokens[rng.below(tokens.len())]);
        }
        let chunks = chunk_markdown(SOURCE, TITLE, &markdown);
        let mut end = 0;
        let mut seen = HashSet::new();
        if one_paragraph {
            assert!(sample_token_tenths(&markdown) > 10_000);
            assert!(chunks.len() > 1);
        }
        for (index, chunk) in chunks.iter().enumerate() {
            let [from, to] = chunk.byte_range;
            assert!(from >= end && from < to, "case {case}, chunk {index}");
            let own = markdown
                .get(from..to)
                .expect("UTF-8 scalar boundaries inside source");
            assert!(chunk.text.ends_with(own), "case {case}, chunk {index}");
            assert_eq!(
                chunk.utf16_range,
                [
                    markdown[..from].encode_utf16().count(),
                    markdown[..to].encode_utf16().count()
                ],
                "case {case}, chunk {index}"
            );
            assert!(
                sample_token_tenths(&chunk.text) <= 10_000,
                "case {case}, chunk {index}"
            );
            assert_eq!(chunk.ordinal, index);
            assert!(seen.insert(&chunk.chunk_id));
            end = to;
        }
    }
}

#[cfg(not(debug_assertions))]
#[test]
#[ignore = "release-only 5 MiB performance gate"]
fn five_mib_markdown_chunks_in_less_than_one_second() {
    use std::time::{Duration, Instant};

    const BYTES: usize = 5 * 1024 * 1024;
    let paragraph =
        "Orchard workers inspect the cold room. 果园记录当日收成，检查温度和湿度。\n".repeat(80);
    let section = format!("# Orchard log\n\n{paragraph}\n");
    let mut markdown = section.repeat(BYTES / section.len());
    // Finish the last paragraph with ASCII so the fixture is exactly five MiB.
    let remaining = BYTES - markdown.len();
    markdown.push_str(&"pear ".repeat(remaining / 5));
    markdown.push_str(&"x".repeat(remaining % 5));
    assert_eq!(markdown.len(), BYTES);

    let start = Instant::now();
    let chunks = chunk_markdown(SOURCE, TITLE, std::hint::black_box(&markdown));
    let elapsed = start.elapsed();
    assert!(chunks.len() > 100);
    println!(
        "{} bytes, {} chunks, {:.3} ms",
        markdown.len(),
        chunks.len(),
        elapsed.as_secs_f64() * 1000.0
    );
    assert!(elapsed < Duration::from_secs(1), "5 MiB took {elapsed:?}");
}
#[test]
fn reports_the_start_of_an_unsplittable_grapheme_in_any_markdown_context() {
    let bad = format!("a{}", "\u{0301}".repeat(1000));
    let near_limit = format!("a{}", "\u{0301}".repeat(998));
    for prefix in ["# Heading\n\n😄 ", "| 栏 |\r\n| --- |\r\n| ", "## "] {
        let md = format!("{prefix}{bad}");
        let last_line = prefix.rsplit('\n').next().unwrap();
        let expected = super::ChunkPosition {
            row: prefix.matches('\n').count() + 1,
            column: last_line.chars().count() + 1,
            byte_offset: prefix.len(),
        };
        assert_eq!(
            chunk(KB, SOURCE, TITLE, &md, SupportedLocale::EnUs),
            Err(super::ChunkError::GraphemeExceedsBudget(expected))
        );
    }
    assert!(super::validate_grapheme_budget(&near_limit).is_ok());
}

#[test]
fn a_long_valid_grapheme_in_prose_is_kept_whole_even_after_a_previous_block() {
    let cluster = "a".to_string() + &"\u{0301}".repeat(995);
    let input = format!("{}\n\n{cluster}{}", "梨".repeat(300), "桃".repeat(601));
    let pieces = chunk_markdown(SOURCE, TITLE, &input);
    let at = input.find(&cluster).unwrap();
    assert!(pieces
        .iter()
        .any(|c| c.byte_range[0] <= at && at + cluster.len() <= c.byte_range[1]));
    for c in pieces {
        assert!(super::units(&c.text).map(|(_, n)| n).sum::<usize>() <= super::LIMIT);
    }
}
