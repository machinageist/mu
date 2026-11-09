// Author: machinageist
// Date: 2026-11-09
// Description: The CLI honours --format and rejects unknown format names
// Notes: Drives the built binary so flag parsing and exit codes are covered end to end

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_mu");
const EXIT_USAGE: i32 = 2;

// Run mu with args, feeding stdin, and return (exit code, stdout)
fn run(args: &[&str], stdin: &str) -> (i32, String) {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn mu");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for mu");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn stdin_markdown_still_renders() {
    let (code, out) = run(&["-w", "40"], "# Hi\n");
    assert_eq!(code, 0);
    assert_eq!(out, "Hi\n══\n");
}

#[test]
fn format_flag_overrides_detection() {
    let (code, out) = run(&["--format=markdown", "-w", "40"], "# Hi\n");
    assert_eq!(code, 0);
    assert_eq!(out, "Hi\n══\n");
}

#[test]
fn unknown_format_is_a_usage_error() {
    let (code, _) = run(&["-f", "docx"], "x");
    assert_eq!(code, EXIT_USAGE);
}
