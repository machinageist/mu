// Author: machinageist
// Date: 2026-11-09
// Description: YAML highlighting preserves text exactly, colors each token class and sniffs conservatively
// Notes: Color checks look for a token wrapped in its style's escape; SGR codes mirror style.rs

mod common;

use common::{assert_invariants, colored, plain};
use mu::{Format, sniff};

const FIXTURE: &str = include_str!("fixtures/sample.yaml");
const KITCHEN_SINK: &str = include_str!("fixtures/kitchen-sink.md");
const README: &str = include_str!("../README.md");
const WIDE: usize = 120;

// Escapes for the data styles
const SGR_KEY: &str = "\x1b[0;1;34m";
const SGR_STRING: &str = "\x1b[0;32m";
const SGR_NUMBER: &str = "\x1b[0;35m";
const SGR_LITERAL: &str = "\x1b[0;33m";
const SGR_COMMENT: &str = "\x1b[0;2;3m";
const SGR_PUNCT: &str = "\x1b[0;2m";
const SGR_META: &str = "\x1b[0;36m";

// Assert a token is rendered in a style
fn assert_styled(out: &str, sgr: &str, token: &str) {
    let needle = format!("{sgr}{token}");
    assert!(out.contains(&needle), "missing {needle:?} in {out:?}");
}

#[test]
fn text_is_preserved_exactly() {
    assert_eq!(plain(FIXTURE, Format::Yaml, WIDE), FIXTURE);
}

#[test]
fn crlf_line_endings_are_dropped() {
    let out = plain("a: 1\r\nb: [x, y]\r\n", Format::Yaml, WIDE);
    assert_eq!(out, "a: 1\nb: [x, y]\n");
}

#[test]
fn malformed_input_passes_through() {
    let input = "a: \"unclosed\n  b: ]]}\n- ? : |\n: :\n'\n\"\\";
    let mut expected = input.to_owned();
    expected.push('\n');
    assert_eq!(plain(input, Format::Yaml, WIDE), expected);
}

#[test]
fn keys_values_and_comments_are_colored() {
    let out = colored("name: CI  # the name\nport: 8080\n", Format::Yaml, WIDE);
    assert_styled(&out, SGR_KEY, "name");
    assert_styled(&out, SGR_PUNCT, ":");
    assert_styled(&out, SGR_STRING, "CI");
    assert_styled(&out, SGR_COMMENT, "# the name");
    assert_styled(&out, SGR_NUMBER, "8080");
}

#[test]
fn literals_and_numbers_are_classified() {
    let input = "a: TRUE\nb: off\nc: ~\nd: 0x1F\ne: -.inf\nf: 1.5e3\ng: 1.2.3\n";
    let out = colored(input, Format::Yaml, WIDE);
    for literal in ["TRUE", "off", "~"] {
        assert_styled(&out, SGR_LITERAL, literal);
    }
    for number in ["0x1F", "-.inf", "1.5e3"] {
        assert_styled(&out, SGR_NUMBER, number);
    }
    assert_styled(&out, SGR_STRING, "1.2.3");
}

#[test]
fn hash_inside_quotes_or_words_is_not_a_comment() {
    let out = colored("a: \"x # y\"\nb: c#d\n", Format::Yaml, WIDE);
    assert_styled(&out, SGR_STRING, "\"x # y\"");
    assert_styled(&out, SGR_STRING, "c#d");
    assert!(!out.contains(SGR_COMMENT), "{out:?}");
}

#[test]
fn block_scalar_bodies_are_strings() {
    let input = "run: |\n  echo a: b # c\n  - x\nnext: 1\n";
    let out = colored(input, Format::Yaml, WIDE);
    assert_styled(&out, SGR_PUNCT, "|");
    assert_styled(&out, SGR_STRING, "echo a: b # c");
    assert_styled(&out, SGR_STRING, "- x");
    assert_styled(&out, SGR_KEY, "next");
}

#[test]
fn block_scalar_in_a_sequence_item_ends_at_a_sibling_key() {
    let input = "- run: >-\n    one\n  name: two\n";
    let out = colored(input, Format::Yaml, WIDE);
    assert_styled(&out, SGR_STRING, "one");
    assert_styled(&out, SGR_KEY, "name");
}

#[test]
fn multi_line_quoted_strings_continue() {
    let input = "a: \"one\n  two\"\nb: 1\n";
    let out = colored(input, Format::Yaml, WIDE);
    assert_styled(&out, SGR_STRING, "two\"");
    assert_styled(&out, SGR_KEY, "b");
}

#[test]
fn anchors_aliases_tags_and_merge_keys_are_meta() {
    let input = "base: &b\n  x: 1\nuse:\n  <<: *b\n  y: !!str 2\n";
    let out = colored(input, Format::Yaml, WIDE);
    for meta in ["&b", "<<", "*b", "!!str"] {
        assert_styled(&out, SGR_META, meta);
    }
}

#[test]
fn flow_collections_mark_keys_and_punctuation() {
    let out = colored("a: {k: v, \"q\": [1, x]}\n", Format::Yaml, WIDE);
    assert_styled(&out, SGR_PUNCT, "{");
    assert_styled(&out, SGR_KEY, "k");
    assert_styled(&out, SGR_KEY, "\"q\"");
    assert_styled(&out, SGR_NUMBER, "1");
    assert_styled(&out, SGR_STRING, "x");
}

#[test]
fn document_markers_and_entries_are_punctuation() {
    let out = colored("---\n- a\n...\n", Format::Yaml, WIDE);
    assert_styled(&out, SGR_PUNCT, "---");
    assert_styled(&out, SGR_PUNCT, "-");
    assert_styled(&out, SGR_PUNCT, "...");
}

#[test]
fn fixture_keeps_the_invariants() {
    assert_invariants(FIXTURE, Format::Yaml);
}

#[test]
fn yaml_documents_are_sniffed() {
    assert_eq!(sniff(FIXTURE), Format::Yaml);
    let compose = "services:\n  web:\n    image: nginx\n    ports:\n      - \"80:80\"\n";
    assert_eq!(sniff(compose), Format::Yaml);
    let manifest =
        "# Source: chart/x.yaml\napiVersion: v1\nkind: Service\nmetadata:\n  name: web\n";
    assert_eq!(sniff(manifest), Format::Yaml);
    let header = "# lint config\n\nfirst: false\nrules:\n  - a\n";
    assert_eq!(sniff(header), Format::Yaml);
}

#[test]
fn markdown_and_prose_are_not_yaml() {
    let front_matter = "---\ntitle: x\n---\n\n# Heading\n\nProse here.\n";
    let prose = "Note: this is a sentence.\nAnother: line of prose here.\n";
    let list = "# Shopping\n\n## Dairy\n\n- milk\n- eggs\n- butter: salted\n";
    let paragraph = "key: value\nother: thing\nmore: stuff\n\nThen a paragraph of prose.\n";
    for input in [KITCHEN_SINK, README, front_matter, prose, list, paragraph] {
        assert_ne!(sniff(input), Format::Yaml, "{input:?}");
    }
}
