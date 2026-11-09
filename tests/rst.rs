// Author: machinageist
// Date: 2026-11-09
// Description: reStructuredText presenter tests with exact expected output plus layout invariants
// Notes: Exact cases run with color off at a fixed width

mod common;

use common::{assert_invariants, colored, plain};
use mu::Format;

const FIXTURE: &str = include_str!("fixtures/sample.rst");
const WIDTH: usize = 40;

// Render RST with color off at the test width
fn rst(input: &str) -> String {
    plain(input, Format::Rst, WIDTH)
}

#[test]
fn heading_levels_follow_adornment_order() {
    let input = "====\nTop\n====\n\nA\n---\n\nB\n~~~\n\nC\n---\n\nText.\n";
    assert_eq!(rst(input), "Top\n═══\n\nA\n─\n\nB\n\nC\n─\n\nText.\n");
}

#[test]
fn literal_block_after_double_colon() {
    let input = "Example::\n\n    code here\n\nAfter ::\n\n  x\n\n::\n\n  y\n";
    assert_eq!(
        rst(input),
        "Example:\n\n╭───────────────────────────────────────\n│ code here\n╰───────────────────────────────────────\n\nAfter\n\n╭───────────────────────────────────────\n│ x\n╰───────────────────────────────────────\n\n╭───────────────────────────────────────\n│ y\n╰───────────────────────────────────────\n"
    );
}

#[test]
fn code_block_directive_keeps_language_and_skips_options() {
    let input = ".. code-block:: python\n   :linenos:\n\n   x = 1\n";
    assert_eq!(
        rst(input),
        "╭─ python ──────────────────────────────\n│ x = 1\n╰───────────────────────────────────────\n"
    );
}

#[test]
fn admonitions_become_alerts() {
    let input =
        ".. note:: Hi there.\n\n.. danger::\n   Hot.\n\n.. admonition:: Title\n\n   Body.\n";
    assert_eq!(
        rst(input),
        "│ Note\n│ Hi there.\n\n│ Caution\n│ Hot.\n\n│ Title\n│\n│ Body.\n"
    );
}

#[test]
fn simple_table_with_header() {
    let input =
        "=====  =====\nA      B\n=====  =====\n1      2\nx      long\n       tail\n=====  =====\n";
    assert_eq!(
        rst(input),
        "┌───┬───────────┐\n│ A │ B         │\n├───┼───────────┤\n│ 1 │ 2         │\n│ x │ long tail │\n└───┴───────────┘\n"
    );
}

#[test]
fn grid_table_with_header() {
    let input = "+---+-------+\n| a | b     |\n+===+=======+\n| 1 | two   |\n|   | lines |\n+---+-------+\n";
    assert_eq!(
        rst(input),
        "┌───┬───────────┐\n│ a │ b         │\n├───┼───────────┤\n│ 1 │ two lines │\n└───┴───────────┘\n"
    );
}

#[test]
fn inline_markup() {
    let input = "Some *em*, **strong**, ``lit``, `site <http://u>`_, :func:`f`, `ref`_, 2*3*4.\n";
    assert_eq!(
        rst(input),
        "Some em, strong, lit, site (http://u),\nf, ref, 2*3*4.\n"
    );
}

#[test]
fn lists_tight_nested_and_enumerated() {
    let input = "- a\n- b\n\n  * c\n\n- d\n\n#. one\n#. two\n";
    assert_eq!(rst(input), "• a\n• b\n  ◦ c\n• d\n\n1. one\n2. two\n");
}

#[test]
fn blank_separated_items_are_loose() {
    assert_eq!(rst("- a\n\n- b\n"), "• a\n\n• b\n");
}

#[test]
fn definition_and_field_lists() {
    let input = "term : type\n   Its meaning.\n\n:Author: Jane\n";
    assert_eq!(rst(input), "• term\n  Its meaning.\n\nAuthor: Jane\n");
}

#[test]
fn block_quote_and_transition() {
    let input = "Para.\n\n   Quoted.\n\n----\n\nEnd.\n";
    assert_eq!(
        rst(input),
        "Para.\n\n│ Quoted.\n\n────────────────────────────────────────\n\nEnd.\n"
    );
}

#[test]
fn comments_and_targets_drop_and_unknown_directives_show() {
    let input = ".. a comment\n   more\n\n.. _x: http://x\n\n.. foo:: bar\n   body\n\nPara.\n";
    assert_eq!(
        rst(input),
        "╭─ foo ─────────────────────────────────\n│ bar\n│ body\n╰───────────────────────────────────────\n\nPara.\n"
    );
}

#[test]
fn image_directive_shows_alt() {
    let input = ".. image:: pic.png\n   :alt: A cat\n";
    assert_eq!(rst(input), "[image: A cat]\n");
}

#[test]
fn malformed_input_does_not_panic() {
    let deep: String = (0..200).map(|n| format!("{}x\n", " ".repeat(n))).collect();
    let inputs = [
        "",
        "::",
        "..",
        ".. ::",
        "+--+\n| a | b | c |\n+",
        "=====  ==\n",
        "===  ===\nunterminated é table\n",
        "- \n-\n#.\n(1)\n(",
        "`unclosed <http://x`_ **bold *em ``code",
        ":role:`x\n:a:\n:",
        "===\n\n===\nTitle\n",
        "é\n=\nü\n--\n",
        "\t\tTabbed\n\t\tblock::\n",
        deep.as_str(),
    ];
    for input in inputs {
        let _ = plain(input, Format::Rst, 30);
        let _ = colored(input, Format::Rst, 30);
    }
}

#[test]
fn fixture_holds_invariants() {
    assert_invariants(FIXTURE, Format::Rst);
}

#[test]
fn sniff_detects_rst_but_not_markdown() {
    assert_eq!(mu::sniff(FIXTURE), Format::Rst);
    assert_eq!(mu::sniff(".. note:: x\n"), Format::Rst);
    assert_eq!(mu::sniff("=====\nTitle\n=====\n\nBody"), Format::Rst);
    assert_eq!(mu::sniff("See `docs <http://x>`_ now."), Format::Rst);
    assert_eq!(mu::sniff("Call :func:`run` now."), Format::Rst);
    assert_ne!(
        mu::sniff(include_str!("fixtures/kitchen-sink.md")),
        Format::Rst
    );
    assert_ne!(mu::sniff(include_str!("../README.md")), Format::Rst);
    assert_eq!(mu::sniff("Title\n=====\n\nBody"), Format::Markdown);
    assert_eq!(mu::sniff("Intro\n\n=====\nTitle\n=====\n"), Format::Rst);
    // Markdown that quotes RST syntax is still Markdown
    assert_eq!(
        mu::sniff("# Docs\n\nRST links look like `` `text <url>`_ `` and roles like `:ref:`.\n"),
        Format::Markdown
    );
    // An AsciiDoc listing under its block attribute is not an RST title
    assert_ne!(
        mu::sniff("Intro\n\n[source,sh]\n----\nls\n----"),
        Format::Rst
    );
}
