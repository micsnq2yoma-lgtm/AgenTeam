use super::super::super::types::parse_knowledge_ref;
use super::*;
use unicode_segmentation::UnicodeSegmentation;

const KB: &str = "kb_0123456789ab";
const SRC: &str = "src_0123456789abcdef";
fn table(names: &[String], rows: &[Vec<String>]) -> String {
    let line = |cells: &[String]| format!("| {} |", cells.join(" | "));
    let mut md = vec![line(names), line(&vec!["---".into(); names.len()])];
    md.extend(rows.iter().map(|r| line(r)));
    md.join("\n")
}
fn chunks(md: &str, locale: SupportedLocale) -> Vec<super::super::KnowledgeChunk> {
    super::super::chunk_markdown(KB, SRC, "Notes", md, locale).unwrap()
}
fn prefix<'a>(chunk: &'a super::super::KnowledgeChunk, md: &str) -> &'a str {
    let [a, b] = chunk.byte_range;
    chunk
        .text
        .strip_suffix(&md[a..b])
        .unwrap()
        .trim_end_matches('\n')
}
fn raw<'a>(chunk: &super::super::KnowledgeChunk, md: &'a str) -> &'a str {
    let [a, b] = chunk.byte_range;
    &md[a..b]
}
fn long_names(n: usize) -> Vec<String> {
    (1..=n)
        .map(|i| format!("列{i}{}", "梨".repeat(19)))
        .collect()
}
fn case_md(names: &[String], cell: String) -> String {
    let mut row = vec!["data".into(); names.len()];
    row[0] = cell;
    table(names, &[row])
}

#[test]
fn width_threshold_and_complete_headers_have_resolvable_first_refs() {
    let small = case_md(&long_names(6), "梨".repeat(2400));
    let special = case_md(&long_names(13), "梨".repeat(2400));
    assert!(cost(special.lines().next().unwrap()) >= 2500);
    let regular = chunks(&small, SupportedLocale::ZhCn);
    assert!(regular
        .iter()
        .skip(1)
        .any(|c| prefix(c, &small).contains("| ---")));
    for locale in [SupportedLocale::ZhCn, SupportedLocale::EnUs] {
        let cs = chunks(&special, locale);
        let root = &cs[0];
        let first_ref = format!("kb://{KB}/{SRC}#{}", root.chunk_id);
        let parsed = parse_knowledge_ref(&first_ref).unwrap();
        assert_eq!(parsed.chunk_id.as_deref(), Some(root.chunk_id.as_str()));
        assert_eq!(parsed.kb_id, KB);
        assert_eq!(parsed.source_id, SRC);
        assert_eq!(
            raw(root, &special),
            special.lines().take(2).collect::<Vec<_>>().join("\n")
        );
        let p = prefix(&cs[1], &special);
        assert!(p.contains(&first_ref));
        assert!(p.contains(if locale == SupportedLocale::ZhCn {
            "共 13 列"
        } else {
            "of 13 columns"
        }));
        assert!(p.contains("列1"));
        assert!(!p.contains(&long_names(13)[12]));
        assert!(cs.iter().all(|c| cost(&c.text) <= LIMIT));
    }
}

#[test]
fn huge_header_preserves_source_and_column_directory_across_multiple_chunks() {
    let names = long_names(80);
    let md = case_md(&names, "short".into());
    let cs = chunks(&md, SupportedLocale::EnUs);
    let header = md.lines().take(2).collect::<Vec<_>>().join("\n");
    let fragments = cs
        .iter()
        .take_while(|c| c.byte_range[0] < header.len())
        .collect::<Vec<_>>();
    assert!(fragments.len() > 1);
    assert!(fragments.iter().all(|c| cost(&c.text) <= LIMIT));
    assert_eq!(
        fragments.iter().map(|c| raw(c, &md)).collect::<String>(),
        header
    );
    let root = cs[0].table.as_ref().unwrap();
    for (idx, col) in root.columns.iter().enumerate() {
        assert_eq!(col.column_index, idx + 1);
        let [a, b] = col.name_byte_range;
        assert_eq!(&md[a..b], names[idx]);
        let owner = cs
            .iter()
            .find(|c| c.chunk_id == col.first_header_chunk_id)
            .unwrap();
        assert!(owner.byte_range[0] <= a && a < owner.byte_range[1]);
        assert!(b <= owner.byte_range[1], "whole header cells should fit");
    }
    assert!(prefix(cs.last().unwrap(), &md).contains(&root.first_header_chunk_id));
}

#[test]
fn large_rows_use_whole_cells_and_do_not_split_rows_that_fit_with_the_header() {
    let row = vec!["梨".repeat(395), "桃".repeat(395), "枣".repeat(395)];
    let names = vec!["A".into(), "B".into(), "C".into()];
    let md = table(&names, std::slice::from_ref(&row));
    let cs = chunks(&md, SupportedLocale::ZhCn);
    let parts = &cs[1..];
    assert_eq!(parts.len(), 3);
    for (idx, (part, original)) in parts.iter().zip(&row).enumerate() {
        assert!(raw(part, &md).contains(original));
        assert!(prefix(part, &md).contains(&format!("第 {}/3 段", idx + 1)));
        assert!(cost(&part.text) <= LIMIT);
    }
    let short = vec!["梨".repeat(810)];
    let md = table(&["name".into()], &[short.clone(), short.clone(), short]);
    let cs = chunks(&md, SupportedLocale::EnUs);
    assert_eq!(cs.len(), 3);
    for part in cs {
        assert!(raw(&part, &md).contains(&"梨".repeat(810)));
    }
}

#[test]
fn chinese_english_and_unspaced_graphemes_reassemble_exactly() {
    let zh = (0..170)
        .map(|i| format!("白天记录第{i}筐苹果；夜间检查温湿度。"))
        .collect::<String>();
    let en = (0..240)
        .map(|i| format!("Record {i} arrived at the orchard. "))
        .collect::<String>()
        .trim_end()
        .to_string();
    let unicode = "𠮷👨‍👩‍👧‍👦e\u{301}梨🧑🏽‍🌾".repeat(250);
    for value in [&zh, &en, &unicode] {
        let md = table(&["Orchard".into()], &[vec![value.clone()]]);
        let cs = chunks(&md, SupportedLocale::EnUs);
        let parts = cs
            .iter()
            .filter(|c| c.table.as_ref().unwrap().column_index == Some(1))
            .collect::<Vec<_>>();
        assert!(parts.len() > 1);
        let mut graphemes = value
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        graphemes.push(value.len());
        let mut cursor = 0;
        for (i, p) in parts.iter().enumerate() {
            let body = raw(p, &md);
            cursor += body.len();
            assert!(graphemes.binary_search(&cursor).is_ok());
            assert_eq!(p.table.as_ref().unwrap().part, Some([i + 1, parts.len()]));
            assert!(prefix(p, &md).contains(&format!(
                "(Row 1 · column “Orchard” · part {}/{})",
                i + 1,
                parts.len()
            )));
            assert!(cost(prefix(p, &md)) <= CONTEXT_LIMIT && cost(&p.text) <= LIMIT);
        }
        assert_eq!(
            parts.iter().map(|p| raw(p, &md)).collect::<String>(),
            *value
        );
    }
}

#[test]
fn priorities_keep_breaks_then_sentences_then_words_and_preserve_the_short_tail() {
    let cells = [
        (
            "梨".repeat(280) + "\n" + &"桃".repeat(230) + "。" + &"枣".repeat(150),
            "\n",
        ),
        (
            "梨".repeat(280) + "<br/>" + &"桃".repeat(230) + "！" + &"枣".repeat(150),
            "<br/>",
        ),
        (
            "梨".repeat(280) + "；" + &"桃".repeat(285) + " words " + &"枣".repeat(150),
            "；",
        ),
        ("word ".repeat(600), " "),
    ];
    for (value, ending) in cells {
        let n = cut(&value, TARGET, TARGET, Some(0)).unwrap();
        assert!(value[..n].ends_with(ending));
        assert_eq!(
            cut("短尾。", TARGET, TARGET, Some(TARGET)).unwrap(),
            "短尾。".len()
        );
    }
}

#[test]
fn a_1001_token_cell_reserves_capacity_and_keeps_the_actual_tail() {
    let value = "梨".repeat(1001);
    for names in [vec!["品种".into()], long_names(16)] {
        let md = case_md(&names, value.clone());
        let cs = chunks(&md, SupportedLocale::ZhCn);
        let parts: Vec<_> = cs
            .iter()
            .filter(|c| c.table.as_ref().and_then(|t| t.column_index) == Some(1))
            .collect();
        assert_eq!(parts.len(), 2);
        for (i, part) in parts.iter().enumerate() {
            assert_eq!(part.table.as_ref().unwrap().part, Some([i + 1, 2]));
            assert!(cost(prefix(part, &md)) <= CONTEXT_LIMIT);
            assert!(cost(&part.text) <= LIMIT);
        }
        let first = raw(parts[0], &md);
        let last = raw(parts[1], &md);
        assert!(cost(first) >= TARGET);
        assert!(cost(last) < TARGET);
        assert_eq!(first.to_owned() + last, value);
    }
}

#[test]
fn an_early_sentence_boundary_cannot_leave_a_short_nonterminal_cell_part() {
    let value = "梨".repeat(120) + "。" + &"红苹果，".repeat(650);
    let md = case_md(&["果园".into()], value.clone());
    let cs = chunks(&md, SupportedLocale::ZhCn);
    let parts: Vec<_> = cs
        .iter()
        .filter(|c| c.table.as_ref().and_then(|t| t.column_index) == Some(1))
        .collect();
    assert!(parts.len() >= 3);
    for c in &parts[..parts.len() - 1] {
        assert!(cost(raw(c, &md)) >= TARGET);
    }
    assert_eq!(
        parts.into_iter().map(|c| raw(c, &md)).collect::<String>(),
        value
    );
}

#[test]
fn changing_only_one_unique_body_preserves_later_part_ids() {
    let value = (0..120)
        .map(|i| format!("第{i}条记录在秋季果园采收苹果，核对当天重量。"))
        .collect::<String>();
    let md = case_md(&["Fruit".into()], value);
    let cs = chunks(&md, SupportedLocale::ZhCn);
    let [a, b] = cs[2].byte_range;
    let edited = md[..a].to_owned() + &md[a..b].replacen("苹果", "柑橘", 1) + &md[b..];
    let after = chunks(&edited, SupportedLocale::ZhCn);
    assert!(cs.len() >= 4);
    assert_eq!(cs.len(), after.len());
    for (i, (before, after)) in cs.iter().zip(&after).enumerate() {
        assert_eq!(before.chunk_id == after.chunk_id, i != 2);
    }
}

#[test]
fn shortening_the_excerpt_or_switching_languages_never_changes_ids() {
    let md = case_md(&long_names(16), "梨".repeat(2400));
    let wide = super::super::chunk_with_excerpt(
        KB,
        SRC,
        "Notes",
        &md,
        SupportedLocale::EnUs,
        HEADER_LIMIT,
    )
    .unwrap();
    let slim =
        super::super::chunk_with_excerpt(KB, SRC, "笔记", &md, SupportedLocale::ZhCn, 300).unwrap();
    assert_ne!(wide[1].text, slim[1].text);
    for (a, b) in wide.iter().zip(&slim) {
        assert_eq!(a.chunk_id, b.chunk_id);
        assert_eq!(a.byte_range, b.byte_range);
    }
    assert_eq!(wide.len(), slim.len());
}

#[test]
fn both_excerpt_and_locator_use_the_same_long_column_label() {
    let family = "👩‍🌾";
    let combining = "e\u{301}";
    for long in [
        "梨".repeat(1200),
        "梨".repeat(31) + family + &"梨".repeat(100),
        "梨".repeat(31) + combining + &"梨".repeat(100),
    ] {
        let md = case_md(&[long.clone(), "枣".repeat(170)], "苹果".repeat(1000));
        for locale in [SupportedLocale::ZhCn, SupportedLocale::EnUs] {
            let cs = chunks(&md, locale);
            let col = &cs[0].table.as_ref().unwrap().columns[0];
            assert_eq!(&md[col.name_byte_range[0]..col.name_byte_range[1]], long);
            let label = shown_name(&md, col, locale, KB, SRC).unwrap();
            let (preview, _) = label.split_once('…').unwrap();
            let boundaries = long
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            assert!(boundaries.contains(&preview.len()));
            assert!(cost(&label) <= 800);
            assert!(label.contains(&col.first_header_chunk_id));
            let part = cs
                .iter()
                .find(|c| c.table.as_ref().unwrap().column_index == Some(1))
                .unwrap();
            let context = prefix(part, &md);
            // The display name appears once in the excerpt, once in the cell locator.
            assert_eq!(context.matches(&label).count(), 2);
            assert!(context.contains(if locale == SupportedLocale::EnUs {
                "Row 1"
            } else {
                "第 1 行"
            }));
            assert!(cs.iter().all(|c| cost(&c.text) <= LIMIT));
        }
    }
}

#[test]
fn label_boundary_is_inclusive_and_rendering_treats_column_text_as_literal() {
    for n in [48, 49] {
        let input = "梨".repeat(n);
        let col = TableColumn {
            column_index: 2,
            name_byte_range: [0, input.len()],
            first_header_chunk_id: "abcdef0123456789".into(),
        };
        let shown = shown_name(&input, &col, SupportedLocale::ZhCn, KB, SRC).unwrap();
        if n == 48 {
            assert_eq!(shown, input);
        } else {
            assert!(shown.starts_with(&("梨".repeat(32) + "…（第 2 列")));
        }
    }
    assert_eq!(
        render("{{column}} {{row}}", &[("column", "{{row}}"), ("row", "7")]),
        "{{row}} 7"
    );
}

#[test]
fn progressively_smaller_contexts_always_leave_sixty_percent_for_body() {
    let marker = "（第 1 行 · 「梨」列 · 第 1/2 段）";
    let full = "桃".repeat(405);
    let compact = fit_context(&full, Some("梨"), marker.into(), CONTEXT_LIMIT).unwrap();
    assert_eq!(compact, format!("| 梨 |\n{marker}"));
    let bare = fit_context(&full, Some("梨"), marker.into(), cost(marker)).unwrap();
    assert_eq!(bare, marker);
    assert!(LIMIT - cost(&compact) >= TARGET);
}

#[test]
fn exactly_one_quarter_of_the_budget_still_uses_the_full_header() {
    for (n, wide) in [(243, false), (244, true)] {
        let md = table(&["梨".repeat(n)], &[vec!["ok".into()]]);
        let header = md.lines().take(2).collect::<Vec<_>>().join("\n");
        assert_eq!(cost(&header), (n + 7) * 10);
        let cs = chunks(&md, SupportedLocale::ZhCn);
        assert_eq!(cs[0].table.is_some(), wide);
    }
}

#[test]
fn a_narrow_repeated_header_also_uses_the_shared_long_column_name() {
    let name = "梨".repeat(49);
    let md = table(
        std::slice::from_ref(&name),
        &[vec!["桃".repeat(700)], vec!["枣".repeat(700)]],
    );
    let result = chunks(&md, SupportedLocale::ZhCn);
    let root = &result[0];
    let column = &root.table.as_ref().unwrap().columns[0];
    let display = shown_name(&md, column, SupportedLocale::ZhCn, KB, SRC).unwrap();
    assert!(root.text.contains(&name));
    for row in &result[1..] {
        assert!(prefix(row, &md).contains(&display));
        assert!(!row.text.contains(&name));
    }
}

#[test]
fn escaped_pipes_and_empty_columns_keep_the_source_column_number() {
    let md = table(
        &["A\\|B".into(), String::new(), "梨".repeat(320)],
        &[vec!["x\\|y".into(), "z".into(), "桃".repeat(2200)]],
    );
    let cs = chunks(&md, SupportedLocale::ZhCn);
    let root = cs[0].table.as_ref().unwrap();
    assert_eq!(root.columns.len(), 3);
    let cols = &root.columns;
    assert_eq!(
        &md[cols[0].name_byte_range[0]..cols[0].name_byte_range[1]],
        "A\\|B"
    );
    assert_eq!(cols[1].name_byte_range[0], cols[1].name_byte_range[1]);
    let parts = cs
        .iter()
        .filter(|c| c.table.as_ref().unwrap().column_index == Some(3))
        .collect::<Vec<_>>();
    assert!(parts.len() > 1);
    assert_eq!(
        parts.iter().map(|p| raw(p, &md)).collect::<String>(),
        "桃".repeat(2200)
    );
    for (i, p) in parts.iter().enumerate() {
        let context = prefix(p, &md);
        assert!(context.contains("第 3 列"));
        let locator = context.lines().last().unwrap();
        assert!(cost(locator) <= 800);
        assert!(locator.contains(&format!("第 {}/{} 段", i + 1, parts.len())));
    }
}
