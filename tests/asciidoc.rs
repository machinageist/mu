// Author: machinageist
// Date: 2026-11-09
// Description: AsciiDoc presenter tests with exact expected output plus layout invariants
// Notes: Exact cases run with color off at width 40; the fixture sweeps the shared invariants

mod common;

use common::{assert_invariants, colored, plain};
use mu::{Format, Options, render};

const FIXTURE: &str = include_str!("fixtures/sample.adoc");
const KITCHEN_SINK: &str = include_str!("fixtures/kitchen-sink.md");
const README: &str = include_str!("../README.md");
const WIDTH: usize = 40;

// Render AsciiDoc with color off at the test width
fn adoc(input: &str) -> String {
    plain(input, Format::AsciiDoc, WIDTH)
}

// Render Markdown with color off at the test width
fn markdown(input: &str) -> String {
    render(
        input,
        &Options {
            width: WIDTH,
            color: false,
        },
    )
}

#[test]
fn fixture_passes_invariants() {
    assert_invariants(FIXTURE, Format::AsciiDoc);
}

#[test]
fn fixture_renders_in_color_without_panicking() {
    assert!(colored(FIXTURE, Format::AsciiDoc, 80).contains("Widget Forge"));
}

#[test]
fn sniffs_asciidoc_but_not_markdown() {
    assert_eq!(mu::sniff(FIXTURE), Format::AsciiDoc);
    assert_eq!(mu::sniff(":toc:\n\nSome text."), Format::AsciiDoc);
    assert_eq!(
        mu::sniff("Intro\n\n[source,sh]\n----\nls\n----"),
        Format::AsciiDoc
    );
    assert_eq!(mu::sniff("Intro\n\n|===\n|a |b\n|==="), Format::AsciiDoc);
    assert_ne!(mu::sniff(KITCHEN_SINK), Format::AsciiDoc);
    assert_ne!(mu::sniff(README), Format::AsciiDoc);
    assert_ne!(mu::sniff("# Title\n\n---\n\ntext"), Format::AsciiDoc);
}

#[test]
fn header_is_dropped_and_attributes_substitute() {
    let input = "= Title\nAuthor Name\nv1.0\n:name: World\n\nHello {name} and {missing}.";
    assert_eq!(adoc(input), "Title\n═════\n\nHello World and {missing}.\n");
}

#[test]
fn attributes_set_mid_document_and_unset() {
    let input = ":a: one\n\n{a}\n\n:a!:\n\n{a} {nbsp}x";
    assert_eq!(adoc(input), "one\n\n{a} x\n");
}

#[test]
fn sections_and_markdown_headings() {
    let out = adoc("== Two\n\n=== Three\n\n# Markdown style");
    assert_eq!(out, "Two\n───\n\nThree\n\nMarkdown style\n══════════════\n");
}

#[test]
fn inline_markup_respects_word_boundaries() {
    let input = "*bold* **un**bold _it_ snake_case_name a*b*c `mono` +pass+ #hl# H~2~O x^2^";
    assert_eq!(
        adoc(input),
        "bold unbold it snake_case_name a*b*c\nmono pass hl H2O x2\n"
    );
}

#[test]
fn inline_markup_matches_markdown_styles() {
    let ansi = colored("*b* _i_ `c`", Format::AsciiDoc, WIDTH);
    let md = render(
        "**b** *i* `c`",
        &Options {
            width: WIDTH,
            color: true,
        },
    );
    assert_eq!(ansi, md);
}

#[test]
fn unclosed_markers_stay_literal() {
    let input = "*unclosed bold and _unclosed italic";
    assert_eq!(adoc(input), "*unclosed bold and _unclosed italic\n");
}

#[test]
fn escaped_markers_stay_literal() {
    assert_eq!(adoc(r"\*not bold* and \{name}"), "*not bold* and {name}\n");
}

#[test]
fn links_and_cross_references() {
    let input = "https://a.io[text] link:doc.adoc[guide] https://bare.io. \
                 <<sec,Section>> <<plain>> xref:id[label]";
    assert_eq!(
        adoc(input),
        "text (https://a.io) guide (doc.adoc)\nhttps://bare.io. Section plain label\n"
    );
}

#[test]
fn hard_break_at_line_end() {
    assert_eq!(
        adoc("line one +\nline two\nline three"),
        "line one\nline two line three\n"
    );
}

#[test]
fn source_block_gets_language_label() {
    let input = "[source,rust]\n----\nfn main() {}\n----";
    assert_eq!(adoc(input), markdown("```rust\nfn main() {}\n```"));
}

#[test]
fn literal_and_indented_blocks_are_code() {
    let frame = "╭───────────────────────────────────────\n";
    let bottom = "╰───────────────────────────────────────\n";
    assert_eq!(
        adoc("....\nliteral  text\n...."),
        format!("{frame}│ literal  text\n{bottom}")
    );
    assert_eq!(
        adoc(" indented\n   more"),
        format!("{frame}│ indented\n│   more\n{bottom}")
    );
}

#[test]
fn block_title_is_bold_paragraph_before_block() {
    let out = adoc(".Title\n----\ncode\n----");
    assert_eq!(out, markdown("**Title**\n\n```\ncode\n```"));
}

#[test]
fn admonition_paragraph_form() {
    assert_eq!(
        adoc("NOTE: Inline note\ntext."),
        "│ Note\n│ Inline note text.\n"
    );
    assert_eq!(
        adoc("NOTE: x"),
        markdown("> [!NOTE]\n> x"),
        "same events as a GitHub alert"
    );
}

#[test]
fn admonition_block_forms() {
    assert_eq!(
        adoc("[WARNING]\n====\nBlock warning.\n===="),
        "│ Warning\n│ Block warning.\n"
    );
    assert_eq!(adoc("[TIP]\nParagraph tip."), "│ Tip\n│ Paragraph tip.\n");
}

#[test]
fn quote_with_attribution() {
    let input = "[quote, Ada, Notes]\n____\nQuoted.\n____";
    assert_eq!(adoc(input), "│ Quoted.\n│\n│ — Ada, Notes\n");
}

#[test]
fn example_and_sidebar_are_quotes() {
    assert_eq!(adoc("====\nExample.\n===="), "│ Example.\n");
    assert_eq!(adoc("****\nSide.\n****"), "│ Side.\n");
}

#[test]
fn table_first_row_is_header() {
    let input = "|===\n|A |B\n\n|1 |2\n|3 |4\n|===";
    assert_eq!(
        adoc(input),
        markdown("| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |")
    );
}

#[test]
fn table_one_cell_per_line_uses_cols() {
    let input = "[cols=\"1,1\"]\n|===\n|A\n|B\n|1\n|2\n|===";
    assert_eq!(
        adoc(input),
        "┌───┬───┐\n│ A │ B │\n├───┼───┤\n│ 1 │ 2 │\n└───┴───┘\n"
    );
}

#[test]
fn bullet_lists_nest_by_marker_count() {
    assert_eq!(
        adoc("* a\n** b\n*** c\n* d"),
        markdown("- a\n  - b\n    - c\n- d")
    );
}

#[test]
fn ordered_lists_nest() {
    assert_eq!(adoc(". one\n. two\n.. sub"), "1. one\n2. two\n   1. sub\n");
}

#[test]
fn different_list_after_blank_line_is_separate() {
    assert_eq!(adoc("* a\n\n. b"), "• a\n\n1. b\n");
}

#[test]
fn checklist_items() {
    assert_eq!(
        adoc("* [x] done\n* [ ] todo"),
        markdown("- [x] done\n- [ ] todo")
    );
}

#[test]
fn description_list_bolds_terms() {
    let input = "term:: desc\nother::\n  next line";
    assert_eq!(adoc(input), "• term\n  desc\n• other\n  next line\n");
}

#[test]
fn list_continuation_attaches_block() {
    let input = "* item\n+\n----\ncode\n----\n* next";
    assert_eq!(
        adoc(input),
        "• item\n  ╭─────────────────────────────────────\n  │ code\n  ╰─────────────────────────────────────\n• next\n"
    );
}

#[test]
fn rule_kept_page_break_dropped() {
    assert_eq!(
        adoc("'''\n\n<<<\n\nafter"),
        format!("{}\n\nafter\n", "─".repeat(WIDTH))
    );
}

#[test]
fn includes_and_comments_are_dropped() {
    assert_eq!(adoc("include::other.adoc[]\nkept"), "kept\n");
    assert_eq!(adoc("////\nhidden\n////\n// also hidden\nshown"), "shown\n");
}

#[test]
fn images_show_alt_text() {
    let input = "image::pic.png[Alt text]\n\nimage:icon.svg[]";
    assert_eq!(adoc(input), "[image: Alt text]\n\n[image]\n");
}

#[test]
fn unterminated_blocks_run_to_end() {
    let frame = "╭───────────────────────────────────────\n";
    let bottom = "╰───────────────────────────────────────\n";
    assert_eq!(
        adoc("----\nunterminated"),
        format!("{frame}│ unterminated\n{bottom}")
    );
    assert_eq!(adoc("____\nopen quote"), "│ open quote\n");
    assert_eq!(adoc("|===\n|a |b"), "┌───┬───┐\n│ a │ b │\n└───┴───┘\n");
}

#[test]
fn hostile_nesting_does_not_overflow() {
    let deep_quotes = ">".repeat(5000) + " x";
    let deep_inline = "*_".repeat(5000) + "x" + &"_*".repeat(5000);
    let chained = "* a\n+\n".repeat(5000);
    for input in [
        deep_quotes,
        deep_inline,
        chained,
        "|===\n|===\n[".to_owned(),
    ] {
        let _ = adoc(&input);
    }
}
