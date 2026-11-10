// Author: machinageist
// Date: 2026-11-09
// Description: Source files are shown verbatim, never read as Markdown
// Notes: Drives detection by extension and by shebang, then checks lines survive unchanged

mod common;

use common::{assert_invariants, plain, strip_sgr};
use mu::{Format, Options, detect, present};

const C_FIXTURE: &str = include_str!("fixtures/sample.c");
const SH_FIXTURE: &str = include_str!("fixtures/sample.sh");
const WIDTH: usize = 80;
const TAB: &str = "\t";
const TAB_AS_SPACES: &str = "    ";

// Expected output for verbatim source: tabs expanded, every line kept
fn verbatim(input: &str) -> String {
    input
        .lines()
        .map(|line| format!("{}\n", line.replace(TAB, TAB_AS_SPACES)))
        .collect()
}

#[test]
fn c_file_is_detected_as_code() {
    assert_eq!(detect(Some("sample.c"), C_FIXTURE), Format::Code);
    assert_eq!(detect(Some("include/sample.h"), C_FIXTURE), Format::Code);
}

#[test]
fn c_file_lines_are_unchanged() {
    assert_eq!(plain(C_FIXTURE, Format::Code, WIDTH), verbatim(C_FIXTURE));
}

#[test]
fn shebang_script_on_stdin_is_code() {
    assert_eq!(detect(None, SH_FIXTURE), Format::Code);
    assert_eq!(plain(SH_FIXTURE, Format::Code, WIDTH), verbatim(SH_FIXTURE));
}

#[test]
fn long_lines_wrap_behind_the_bar() {
    let line = "x".repeat(30);
    assert_eq!(
        plain(&line, Format::Code, 20),
        format!("{}\n┆ {}\n", "x".repeat(20), "x".repeat(10))
    );
}

#[test]
fn code_keeps_the_output_invariants() {
    assert_invariants(C_FIXTURE, Format::Code);
}

// Render source with a language and color on
fn highlighted(input: &str, language: &str, width: usize) -> String {
    let options = Options {
        language: Some(language.to_owned()),
        ..Options::new(width, true)
    };
    present(input, Format::Code, &options)
}

#[test]
fn source_is_highlighted_by_language() {
    let out = highlighted(C_FIXTURE, "c", WIDTH);
    assert!(out.contains("\x1b[0;1;35mstatic\x1b[0m"), "{out}");
    assert!(out.contains("\x1b[0;96m#include <stdio.h>"), "{out}");
}

#[test]
fn highlighting_never_changes_the_text() {
    for width in [20, 47, 80] {
        assert_eq!(
            strip_sgr(&highlighted(C_FIXTURE, "c", width)),
            plain(C_FIXTURE, Format::Code, width)
        );
        assert_eq!(
            strip_sgr(&highlighted(SH_FIXTURE, "sh", width)),
            plain(SH_FIXTURE, Format::Code, width)
        );
    }
}

#[test]
fn shebang_picks_the_language_without_an_extension() {
    let out = present(SH_FIXTURE, Format::Code, &Options::new(WIDTH, true));
    assert!(out.contains("\x1b[0;1;35mfor\x1b[0m"), "{out}");
}

#[test]
fn unknown_language_stays_plain() {
    let out = highlighted(C_FIXTURE, "nope", WIDTH);
    assert!(!out.contains("\x1b[0;1;35m"), "{out}");
}

// A document with fenced blocks in several languages, including wide and tabbed lines
const FENCED: &str = "# Code\n\n```rust\nfn main() {\n\tlet s = \"a very long string that will need to wrap inside the frame\"; // é\n}\n```\n\n```python\n@dec\ndef f(): return None\n```\n\n```diff\n-old\n+new\n```\n\n```json\n{\"a\": [1, true]}\n```\n\n```unknown\nplain\n```\n";

#[test]
fn fenced_blocks_are_highlighted_without_changing_text() {
    for width in [30, 60, 100] {
        let color = mu::render(FENCED, &Options::new(width, true));
        let plain = mu::render(FENCED, &Options::new(width, false));
        assert_eq!(strip_sgr(&color), plain, "width {width}");
    }
    let color = mu::render(FENCED, &Options::new(80, true));
    assert!(color.contains("\x1b[0;1;35mfn\x1b[0m"), "{color}");
    assert!(color.contains("\x1b[0;92m+new\x1b[0m"), "{color}");
}
