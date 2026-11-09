// Author: machinageist
// Date: 2026-11-09
// Description: Org-mode presenter tests with exact expected output plus layout invariants
// Notes: Exact cases run with color off at width 40

mod common;

use common::{assert_invariants, colored, plain};
use mu::Format;

const FIXTURE: &str = include_str!("fixtures/sample.org");
const KITCHEN_SINK: &str = include_str!("fixtures/kitchen-sink.md");
const README: &str = include_str!("../README.md");
const WIDTH: usize = 40;

// Render Org with color off at the test width
fn org(input: &str) -> String {
    plain(input, Format::Org, WIDTH)
}

#[test]
fn fixture_passes_invariants() {
    assert_invariants(FIXTURE, Format::Org);
}

#[test]
fn sniff_detects_org_signals() {
    assert_eq!(mu::sniff(FIXTURE), Format::Org);
    assert_eq!(mu::sniff("#+title: x\n\nbody\n"), Format::Org);
    assert_eq!(
        mu::sniff("text\n#+BEGIN_SRC sh\nls\n#+END_SRC\n"),
        Format::Org
    );
    assert_eq!(mu::sniff("* Head\n:PROPERTIES:\n:END:\n"), Format::Org);
    assert_eq!(
        mu::sniff("* Head\nSee [[https://a.b][here]].\n"),
        Format::Org
    );
}

#[test]
fn sniff_leaves_markdown_alone() {
    assert_ne!(mu::sniff(KITCHEN_SINK), Format::Org);
    assert_ne!(mu::sniff(README), Format::Org);
    assert_ne!(mu::sniff("* one\n* two\n"), Format::Org);
    assert_ne!(mu::sniff("* one\n* [two](https://x.y)\n"), Format::Org);
}

#[test]
fn title_becomes_h1_and_shifts_headings() {
    let out = org("#+TITLE: Notes\n#+AUTHOR: machinageist\n#+OPTIONS: toc:nil\n* Intro\nBody.\n");
    assert_eq!(out, "Notes\n═════\n\nmachinageist\n\nIntro\n─────\n\nBody.\n");
}

#[test]
fn headings_without_title_start_at_h1() {
    let out = org("* One\n** Two\n******** Deep\n");
    assert_eq!(out, "One\n═══\n\nTwo\n───\n\nDeep\n");
}

#[test]
fn heading_keeps_todo_priority_and_tags() {
    let out = org("* TODO [#A] Fix router   :net:urgent:\n");
    assert_eq!(
        out,
        "TODO [#A] Fix router :net:urgent:\n═════════════════════════════════\n"
    );
}

#[test]
fn drawers_and_planning_lines_are_dropped() {
    let input = "* Task\nSCHEDULED: <2026-11-12 Thu>\n:PROPERTIES:\n:ID: 42\n:END:\n:LOGBOOK:\nCLOCK: x\n:END:\nKept.\n";
    assert_eq!(org(input), "Task\n════\n\nKept.\n");
}

#[test]
fn unterminated_drawer_is_text() {
    assert_eq!(org(":NOTES:\nhello\n"), ":NOTES: hello\n");
}

#[test]
fn comments_are_dropped() {
    let input = "# hidden\nshown\n\n#+BEGIN_COMMENT\nsecret\n#+END_COMMENT\n#+CAPTION: gone\n";
    assert_eq!(org(input), "shown\n");
}

#[test]
fn emphasis_markers_style_text() {
    let out = org("*b* /i/ _u_ +s+ =v= ~c~");
    assert_eq!(out, "b i u s v c\n");
    let color = colored("*b* +s+ ~c~", Format::Org, WIDTH);
    assert!(color.contains("\x1b[0;1mb"), "{color:?}");
    assert!(color.contains("\x1b[0;9ms"), "{color:?}");
}

#[test]
fn emphasis_needs_word_boundaries() {
    let input = "a*b*c /usr/bin/ x_y_z 1+2+3 (*ok*).";
    assert_eq!(org(input), "a*b*c /usr/bin/ x_y_z 1+2+3 (ok).\n");
    assert_eq!(
        org("* not closed and * spaced *"),
        "not closed and * spaced *\n═════════════════════════\n"
    );
    assert_eq!(org("text *not closed"), "text *not closed\n");
}

#[test]
fn links_show_text_then_url() {
    let out = org("See [[https://a.io][docs]] and [[https://b.io]].");
    assert_eq!(out, "See docs (https://a.io) and\nhttps://b.io.\n");
}

#[test]
fn image_links_render_as_images() {
    assert_eq!(org("[[file:img/cat.png]]"), "[image: img/cat.png]\n");
}

#[test]
fn bullets_numbers_and_nesting() {
    let input = "- one\n  - inner\n- two\n\n1. first\n2) second\n";
    assert_eq!(
        org(input),
        "• one\n  ◦ inner\n• two\n\n1. first\n2. second\n"
    );
}

#[test]
fn ordered_list_keeps_start_number() {
    assert_eq!(org("3. three\n4. four\n"), "3. three\n4. four\n");
}

#[test]
fn checkboxes_become_task_markers() {
    let input = "- [X] done\n- [ ] todo\n- [-] partial\n";
    assert_eq!(org(input), "☑ done\n☐ todo\n☐ partial\n");
}

#[test]
fn description_items_bold_the_term() {
    let input = "- VLAN :: virtual LAN\n- MTU ::\n  max frame\n";
    assert_eq!(org(input), "• VLAN virtual LAN\n• MTU max frame\n");
    let color = colored("- VLAN :: x", Format::Org, WIDTH);
    assert!(color.contains("\x1b[0;1mVLAN"), "{color:?}");
}

#[test]
fn blank_lines_make_a_loose_list() {
    let input = "- one\n\n- two\n";
    assert_eq!(org(input), "• one\n\n• two\n");
}

#[test]
fn list_item_continuation_joins_its_text() {
    let input = "- first line\n  continues here\nafter\n";
    assert_eq!(org(input), "• first line continues here\n\nafter\n");
}

#[test]
fn src_block_becomes_framed_code() {
    let input = "#+begin_src rust :tangle no\nfn main() {}\n,* escaped\n#+end_src\n";
    assert_eq!(
        org(input),
        "╭─ rust ────────────────────────────────\n│ fn main() {}\n│ * escaped\n╰───────────────────────────────────────\n"
    );
}

#[test]
fn example_and_fixed_width_are_code() {
    let input = "#+BEGIN_EXAMPLE\n  $ ls\n#+END_EXAMPLE\n: out\n";
    assert_eq!(
        org(input),
        "╭───────────────────────────────────────\n│ $ ls\n╰───────────────────────────────────────\n\n╭───────────────────────────────────────\n│ out\n╰───────────────────────────────────────\n"
    );
}

#[test]
fn quote_verse_and_special_blocks() {
    let input = "#+BEGIN_QUOTE\nwise words\n#+END_QUOTE\n#+BEGIN_VERSE\nline one\nline two\n#+END_VERSE\n#+begin_note\nheads up\n#+end_note\n#+BEGIN_CENTER\nmiddle\n#+END_CENTER\n";
    assert_eq!(
        org(input),
        "│ wise words\n\nline one\nline two\n\n│ Note\n│ heads up\n\nmiddle\n"
    );
}

#[test]
fn table_with_rule_has_head_and_drops_formula() {
    let input = "| a | b |\n|---+---|\n| 1 | 2 |\n#+TBLFM: $2=$1\n";
    assert_eq!(
        org(input),
        "┌───┬───┐\n│ a │ b │\n├───┼───┤\n│ 1 │ 2 │\n└───┴───┘\n"
    );
}

#[test]
fn table_without_rule_uses_first_row_as_head() {
    let input = "| a | b |\n| 1 |\n";
    assert_eq!(
        org(input),
        "┌───┬───┐\n│ a │ b │\n├───┼───┤\n│ 1 │   │\n└───┴───┘\n"
    );
}

#[test]
fn rule_needs_five_dashes() {
    assert_eq!(org("-----"), format!("{}\n", "─".repeat(WIDTH)));
    assert_eq!(org("----"), "----\n");
}

#[test]
fn footnotes_reference_and_define() {
    let input = "Claim.[fn:1]\n\n[fn:1] Source.\n";
    assert_eq!(org(input), "Claim.[1]\n\n[1] Source.\n");
}

#[test]
fn malformed_input_renders_without_panic() {
    let inputs = [
        "#+BEGIN_SRC\nnever closed",
        "#+END_QUOTE\nstray end",
        "| unclosed table\n|-",
        "[[unclosed link",
        "[fn:]",
        "* ",
        "*",
        "- [X]",
        "-",
        "1.",
        "::",
        "* TODO",
        "* :tag:",
        "=",
        "*é* /ü/ ~日本~",
        "#+BEGIN_\n#+begin_quote\n- a\n  #+begin_src\n",
        "",
    ];
    for input in inputs {
        let _ = org(input);
        let _ = colored(input, Format::Org, 30);
    }
    assert_eq!(
        org("#+BEGIN_SRC\nnever closed"),
        "╭───────────────────────────────────────\n│ never closed\n╰───────────────────────────────────────\n"
    );
}
