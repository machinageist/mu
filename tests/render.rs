// Author: machinageist
// Date: 2026-11-09
// Description: End-to-end rendering tests with exact expected output plus layout invariants
// Notes: Exact cases run with color off; invariants sweep widths over the kitchen-sink fixture

use mu::{Options, render};
use unicode_width::UnicodeWidthStr;

const KITCHEN_SINK: &str = include_str!("fixtures/kitchen-sink.md");
const ESC: char = '\x1b';

// Render with color off at a fixed width
fn plain(markdown: &str, width: usize) -> String {
    render(
        markdown,
        &Options {
            width,
            color: false,
        },
    )
}

// Render with color on at a fixed width
fn colored(markdown: &str, width: usize) -> String {
    render(markdown, &Options { width, color: true })
}

// Remove SGR sequences of the form ESC [ ... m
fn strip_sgr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == ESC {
            for inner in chars.by_ref() {
                if inner == 'm' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[test]
fn paragraph_wraps_at_width() {
    let out = plain("one two three four five six seven eight nine ten", 20);
    assert_eq!(out, "one two three four\nfive six seven eight\nnine ten\n");
}

#[test]
fn long_word_is_broken_at_width() {
    let out = plain("abcdefghijklmnopqrstuvwxyz0123456789", 20);
    assert_eq!(out, "abcdefghijklmnopqrst\nuvwxyz0123456789\n");
}

#[test]
fn blocks_are_separated_by_one_blank_line() {
    let out = plain("# Title\n\nBody text.\n\n---\n\nAfter.", 30);
    assert_eq!(
        out,
        "Title\n═════\n\nBody text.\n\n──────────────────────────────\n\nAfter.\n"
    );
}

#[test]
fn tight_list_has_no_blank_lines_and_nests() {
    let out = plain("- a\n  - b\n    - c\n- d\n\n1. x\n2. y", 30);
    assert_eq!(out, "• a\n  ◦ b\n    ▪ c\n• d\n\n1. x\n2. y\n");
}

#[test]
fn loose_list_keeps_blank_lines_between_items() {
    let out = plain("- a\n\n- b", 30);
    assert_eq!(out, "• a\n\n• b\n");
}

#[test]
fn list_item_continuation_aligns_under_text() {
    let out = plain("- alpha beta gamma delta epsilon", 20);
    assert_eq!(out, "• alpha beta gamma\n  delta epsilon\n");
}

#[test]
fn task_items_replace_the_bullet() {
    let out = plain("- [ ] todo\n- [x] done", 30);
    assert_eq!(out, "☐ todo\n☑ done\n");
}

#[test]
fn quote_bar_runs_through_blank_lines() {
    let out = plain("> one\n>\n> two", 30);
    assert_eq!(out, "│ one\n│\n│ two\n");
}

#[test]
fn alert_shows_its_label() {
    let out = plain("> [!TIP]\n> Use it.", 30);
    assert_eq!(out, "│ Tip\n│ Use it.\n");
}

#[test]
fn code_block_is_framed_with_tabs_expanded() {
    let out = plain("```sh\n\techo hi\n```", 20);
    assert_eq!(
        out,
        "╭─ sh ──────────────\n│     echo hi\n╰───────────────────\n"
    );
}

#[test]
fn long_code_line_continues_with_dashed_bar() {
    let out = plain("```\n0123456789abcdefghijXYZ\n```", 22);
    assert_eq!(
        out,
        "╭─────────────────────\n│ 0123456789abcdefghij\n┆ XYZ\n╰─────────────────────\n"
    );
}

#[test]
fn table_aligns_columns() {
    let out = plain(
        "| L | C | R |\n|:--|:-:|--:|\n| a | b | c |\n| long | mid | 1 |",
        40,
    );
    let expected = "\
┌──────┬─────┬───┐
│ L    │  C  │ R │
├──────┼─────┼───┤
│ a    │  b  │ c │
│ long │ mid │ 1 │
└──────┴─────┴───┘
";
    assert_eq!(out, expected);
}

#[test]
fn links_show_url_but_autolinks_do_not_repeat_it() {
    let out = plain("[docs](https://d.io) and <https://a.io>", 60);
    assert_eq!(out, "docs (https://d.io) and https://a.io\n");
}

#[test]
fn images_render_alt_text() {
    let out = plain("![logo](l.png) ![](x.png)", 60);
    assert_eq!(out, "[image: logo] [image]\n");
}

#[test]
fn html_comments_are_hidden() {
    let out = plain("before\n\n<!-- secret -->\n\nafter", 30);
    assert_eq!(out, "before\n\nafter\n");
}

#[test]
fn br_tag_breaks_the_line() {
    let out = plain("one<br>two", 30);
    assert_eq!(out, "one\ntwo\n");
}

#[test]
fn footnotes_render_reference_and_definition() {
    let out = plain("Claim[^n].\n\n[^n]: Source.", 30);
    assert_eq!(out, "Claim[n].\n\n[n] Source.\n");
}

#[test]
fn front_matter_only_at_top_of_document() {
    let top = plain("---\nk: v\n---\n\nBody", 24);
    assert!(top.starts_with("╭─ front matter ───"), "{top}");
    assert!(top.contains("│ k: v\n"), "{top}");

    let middle = plain("Intro\n\n---\nk: v\n---\n", 24);
    assert!(!middle.contains("front matter"), "{middle}");
}

#[test]
fn control_characters_cannot_reach_the_terminal() {
    let hostile =
        "title\x1b]52;c;ZXZpbA==\x07 text\n\n```\nrm\x1b[2J\n```\n\n| a\x1b[31m |\n|---|\n| b |";
    for out in [plain(hostile, 40), strip_sgr(&colored(hostile, 40))] {
        assert!(!out.contains(ESC), "escape leaked: {out:?}");
        assert!(!out.contains('\x07'), "bell leaked: {out:?}");
    }
}

#[test]
fn empty_input_renders_nothing() {
    assert_eq!(plain("", 40), "");
}

#[test]
fn no_line_exceeds_the_width() {
    for width in 30..=120 {
        for line in plain(KITCHEN_SINK, width).lines() {
            assert!(
                line.width() <= width,
                "width {width}: {line:?} is {} wide",
                line.width()
            );
        }
    }
}

#[test]
fn color_output_matches_plain_output_once_stripped() {
    for width in [30, 47, 60, 80, 100] {
        assert_eq!(
            strip_sgr(&colored(KITCHEN_SINK, width)),
            plain(KITCHEN_SINK, width),
            "width {width}"
        );
    }
}

#[test]
fn every_styled_line_is_reset_before_its_newline() {
    for line in colored(KITCHEN_SINK, 60).lines() {
        if let Some(last_sgr) = line.rfind(ESC) {
            assert!(
                line[last_sgr..].starts_with("\x1b[0m"),
                "style left open: {line:?}"
            );
        }
    }
}
