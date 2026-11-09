// Author: machinageist
// Date: 2026-11-09
// Description: TOML is highlighted as written, sniffed conservatively and keeps the output invariants
// Notes: Color assertions look for exact SGR sequences built from the STYLE_DATA_* theme

mod common;

use common::{assert_invariants, colored, plain};
use mu::Format;

const FIXTURE: &str = include_str!("fixtures/sample.toml");
const CARGO_MANIFEST: &str = include_str!("../Cargo.toml");
const KITCHEN_SINK: &str = include_str!("fixtures/kitchen-sink.md");
const README: &str = include_str!("../README.md");
const WIDTH: usize = 100;

// Theme sequences as the printer writes them
const KEY: &str = "\x1b[0;1;34m";
const STRING: &str = "\x1b[0;32m";
const NUMBER: &str = "\x1b[0;35m";
const LITERAL: &str = "\x1b[0;33m";
const COMMENT: &str = "\x1b[0;2;3m";
const PUNCT: &str = "\x1b[0;2m";
const SECTION: &str = "\x1b[0;1;36m";
const RESET: &str = "\x1b[0m";

// Render one TOML snippet with color on
fn paint(input: &str) -> String {
    colored(input, Format::Toml, WIDTH)
}

#[test]
fn text_is_preserved_exactly() {
    let input = "[a.b]\nx = 1 # c\ns = \"\"\"\nk = v # no\n\"\"\"\n";
    assert_eq!(plain(input, Format::Toml, WIDTH), input);
    let expected: String = FIXTURE.lines().map(|line| format!("{line}\n")).collect();
    assert_eq!(plain(FIXTURE, Format::Toml, WIDTH), expected);
}

#[test]
fn carriage_returns_are_dropped() {
    assert_eq!(
        plain("a = 1\r\nb = 2\r\n", Format::Toml, WIDTH),
        "a = 1\nb = 2\n"
    );
}

#[test]
fn headers_are_sections_whole() {
    assert_eq!(
        paint("[a.\"b c\"]\n"),
        format!("{SECTION}[a.\"b c\"]{RESET}\n")
    );
    assert_eq!(paint("[[bin]]\n"), format!("{SECTION}[[bin]]{RESET}\n"));
}

#[test]
fn keys_values_and_punctuation_get_their_styles() {
    assert_eq!(
        paint("name = \"mu\"\n"),
        format!("{KEY}name{RESET} {PUNCT}={RESET} {STRING}\"mu\"{RESET}\n")
    );
    assert_eq!(
        paint("a.\"b\" = 0x1F\n"),
        format!("{KEY}a{PUNCT}.{KEY}\"b\"{RESET} {PUNCT}={RESET} {NUMBER}0x1F{RESET}\n")
    );
    assert_eq!(
        paint("t = { x = true }\n"),
        format!(
            "{KEY}t{RESET} {PUNCT}={RESET} {PUNCT}{{{RESET} {KEY}x{RESET} {PUNCT}={RESET} \
             {LITERAL}true{RESET} {PUNCT}}}{RESET}\n"
        )
    );
}

#[test]
fn comments_are_dimmed_outside_strings_only() {
    assert_eq!(
        paint("x = '#' # note\n"),
        format!("{KEY}x{RESET} {PUNCT}={RESET} {STRING}'#'{RESET} {COMMENT}# note{RESET}\n")
    );
}

#[test]
fn escaped_quotes_stay_inside_basic_strings() {
    assert_eq!(
        paint("s = \"a\\\"b # c\"\n"),
        format!("{KEY}s{RESET} {PUNCT}={RESET} {STRING}\"a\\\"b # c\"{RESET}\n")
    );
}

#[test]
fn multi_line_strings_hide_comments_and_keys() {
    let out = paint("s = \"\"\"\nkey = value # not a comment\n\"\"\"\nnext = 1\n");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[1],
        format!("{STRING}key = value # not a comment{RESET}")
    );
    assert_eq!(lines[2], format!("{STRING}\"\"\"{RESET}"));
    assert!(
        lines[3].starts_with(&format!("{KEY}next")),
        "{:?}",
        lines[3]
    );
    let literal = paint("s = '''\nk = 'v' # x\n'''\n");
    assert_eq!(
        literal.lines().nth(1),
        Some(format!("{STRING}k = 'v' # x{RESET}").as_str())
    );
}

#[test]
fn numbers_and_dates_are_classified() {
    for number in [
        "+17",
        "-5",
        "1_000",
        "0o755",
        "0b101",
        "6.626e-34",
        "-inf",
        "nan",
    ] {
        let out = paint(&format!("n = {number}\n"));
        assert!(
            out.contains(&format!("{NUMBER}{number}{RESET}")),
            "{number}: {out:?}"
        );
    }
    for date in [
        "1979-05-27T07:32:00Z",
        "1979-05-27 07:32:00",
        "1979-05-27",
        "07:32:00",
    ] {
        let out = paint(&format!("d = {date}\n"));
        assert!(
            out.contains(&format!("{LITERAL}{date}{RESET}")),
            "{date}: {out:?}"
        );
    }
}

#[test]
fn multi_line_arrays_keep_value_context() {
    let out = paint("a = [\n    [1, 2],\n    \"x\",\n]\n[t]\n");
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[1].contains(&format!("{NUMBER}1{PUNCT},")),
        "{:?}",
        lines[1]
    );
    assert!(
        lines[2].contains(&format!("{STRING}\"x\"")),
        "{:?}",
        lines[2]
    );
    assert_eq!(lines[4], format!("{SECTION}[t]{RESET}"));
}

#[test]
fn malformed_input_does_not_panic() {
    for input in [
        "[",
        "[[a",
        "a = \"open",
        "a = '''",
        "x = \"\\",
        "= = ]]}{",
        "a.\n.b = é",
    ] {
        assert_eq!(plain(input, Format::Toml, WIDTH), format!("{input}\n"));
    }
}

#[test]
fn fixture_and_manifest_keep_invariants() {
    assert_invariants(FIXTURE, Format::Toml);
    assert_invariants(CARGO_MANIFEST, Format::Toml);
}

#[test]
fn toml_is_sniffed() {
    assert_eq!(mu::sniff(FIXTURE), Format::Toml);
    assert_eq!(mu::sniff(CARGO_MANIFEST), Format::Toml);
    let pyproject = "[project]\nname = \"x\"\ndependencies = [\n    \"requests\",\n]\n";
    assert_eq!(mu::sniff(pyproject), Format::Toml);
}

#[test]
fn markdown_and_prose_are_not_sniffed_as_toml() {
    assert_ne!(mu::sniff(KITCHEN_SINK), Format::Toml);
    assert_ne!(mu::sniff(README), Format::Toml);
    assert_ne!(
        mu::sniff("[ref]: https://x.io\n[other]: https://y.io\n"),
        Format::Toml
    );
    assert_ne!(mu::sniff("a = b is an equation\n"), Format::Toml);
    assert_ne!(mu::sniff("- [x] done\n- [ ] todo\n"), Format::Toml);
    assert_ne!(
        mu::sniff("[intro]\nSome words here.\nx = 1\nMore prose.\n"),
        Format::Toml
    );
}
