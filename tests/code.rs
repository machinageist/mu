// Author: machinageist
// Date: 2026-11-09
// Description: Source files are shown verbatim, never read as Markdown
// Notes: Drives detection by extension and by shebang, then checks lines survive unchanged

mod common;

use common::{assert_invariants, plain};
use mu::{Format, detect};

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
