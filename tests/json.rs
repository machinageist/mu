// Author: machinageist
// Date: 2026-11-09
// Description: JSON is re-indented when valid, highlighted as written when not, and sniffed safely
// Notes: Exact-output tests run with color off; one color test checks keys and strings differ

mod common;

use common::{assert_invariants, colored, plain};
use mu::{Format, sniff};

const FIXTURE: &str = include_str!("fixtures/sample.json");
const WIDTH: usize = 80;
const DEEP_LEVELS: usize = 10_000;

// Render JSON with color off at the default test width
fn show(input: &str) -> String {
    plain(input, Format::Json, WIDTH)
}

#[test]
fn minified_object_is_indented() {
    let out = show(r#"{"a":1,"b":[true,false,null],"c":{"d":"x"}}"#);
    assert_eq!(
        out,
        "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    false,\n    null\n  ],\n  \"c\": {\n    \"d\": \"x\"\n  }\n}\n"
    );
}

#[test]
fn empty_containers_stay_inline() {
    assert_eq!(show("{}"), "{}\n");
    assert_eq!(show("[ ]"), "[]\n");
    assert_eq!(
        show(r#"{"a":{},"b":[]}"#),
        "{\n  \"a\": {},\n  \"b\": []\n}\n"
    );
}

#[test]
fn nested_arrays_indent_per_level() {
    assert_eq!(
        show("[[1,2],[[]],[[3]]]"),
        "[\n  [\n    1,\n    2\n  ],\n  [\n    []\n  ],\n  [\n    [\n      3\n    ]\n  ]\n]\n"
    );
}

#[test]
fn escapes_and_numbers_are_preserved() {
    let out = show("[\"a\\\"b\\\\c\\u00e9\\n\", -1.5e+10, 0.0, 2E-3]");
    assert_eq!(
        out,
        "[\n  \"a\\\"b\\\\c\\u00e9\\n\",\n  -1.5e+10,\n  0.0,\n  2E-3\n]\n"
    );
}

#[test]
fn ndjson_values_each_get_their_own_block() {
    let out = show("{\"a\":1}\n{\"b\":2}\n3\n");
    assert_eq!(out, "{\n  \"a\": 1\n}\n{\n  \"b\": 2\n}\n3\n");
}

#[test]
fn invalid_json_is_shown_as_written() {
    let input = "{\"a\": 1,\n  \"b\": [2, oops\n}";
    assert_eq!(show(input), "{\"a\": 1,\n  \"b\": [2, oops\n}\n");
    let truncated = "{\"a\": [1,\n  2";
    assert_eq!(show(truncated), "{\"a\": [1,\n  2\n");
}

#[test]
fn deep_nesting_does_not_crash() {
    let deep = format!("{}{}", "[".repeat(DEEP_LEVELS), "]".repeat(DEEP_LEVELS));
    let out = show(&deep);
    assert!(out.starts_with('['));
    assert!(sniff(&deep) == Format::Json);
    let unclosed = "{\"a\":".repeat(DEEP_LEVELS);
    assert!(!show(&unclosed).is_empty());
}

#[test]
fn keys_and_strings_get_different_colors() {
    let out = colored(r#"{"key":"value"}"#, Format::Json, WIDTH);
    let key_at = out.find("\"key\"").expect("key");
    let value_at = out.find("\"value\"").expect("value");
    let key_sgr = &out[out[..key_at].rfind('\x1b').expect("key sgr")..key_at];
    let value_sgr = &out[out[..value_at].rfind('\x1b').expect("value sgr")..value_at];
    assert_ne!(key_sgr, value_sgr);
}

#[test]
fn fixture_passes_invariants() {
    assert_invariants(FIXTURE, Format::Json);
    assert!(show(FIXTURE).lines().count() > 80);
}

#[test]
fn sniffing_accepts_json_and_rejects_markdown() {
    assert_eq!(sniff(FIXTURE), Format::Json);
    assert_eq!(sniff("{\"a\":1}\n{\"a\":2}\n"), Format::Json);
    assert_eq!(sniff("\u{feff}  [1, 2"), Format::Json);
    for markdown in [
        include_str!("fixtures/kitchen-sink.md"),
        include_str!("../README.md"),
        "[link]: https://x.io\n",
        "[x] done\n",
    ] {
        assert_ne!(sniff(markdown), Format::Json, "{markdown:.40}");
    }
}
