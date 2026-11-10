// Author: machinageist
// Date: 2026-11-10
// Description: PDF text through pdftotext, page by page, from a file and from stdin
// Notes: The fixture is a hand-written two-page PDF; tests that need pdftotext skip without it

use std::io::Write;
use std::process::{Command, Stdio};

use mu::{Format, sniff_bytes};

const BIN: &str = env!("CARGO_BIN_EXE_mu");
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.pdf");
const PDF: &[u8] = include_bytes!("fixtures/sample.pdf");
const EXIT_FAILURE: i32 = 1;
const EXPECTED: &str =
    "Hello from page one\n\n─── page 2 ─────────────────────────────\n\nSecond page\n";
const MISSING_TOOL: &str = "mu: -: reading PDF needs pdftotext from poppler: `brew install poppler` or `apt install poppler-utils`\n";

// Check whether pdftotext can run here
fn has_pdftotext() -> bool {
    Command::new("pdftotext")
        .arg("-v")
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

// Run mu with a PATH, feeding stdin, and return (exit code, stdout, stderr)
fn run(args: &[&str], stdin: &[u8], path: Option<&str>) -> (i32, String, String) {
    let mut command = Command::new(BIN);
    command
        .args(["-w", "40", "--color", "never"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let mut child = command.spawn().expect("spawn mu");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for mu");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn magic_bytes_name_and_media_type_resolve() {
    assert_eq!(sniff_bytes(PDF), Some(Format::Pdf));
    assert_eq!(Format::from_path("paper.pdf"), Some(Format::Pdf));
    assert_eq!(
        Format::from_media_type("application/pdf"),
        Some(Format::Pdf)
    );
    assert_eq!(Format::from_name("PDF"), Some(Format::Pdf));
    assert!(Format::Pdf.is_binary() && Format::Pdf.is_wide());
}

#[test]
fn pages_print_with_rules_from_file_and_stdin() {
    if !has_pdftotext() {
        eprintln!("skipped: pdftotext is not installed");
        return;
    }
    let from_file = run(&[FIXTURE], b"", None);
    assert_eq!(from_file, (0, EXPECTED.to_owned(), String::new()));
    assert_eq!(run(&[], PDF, None), from_file);
    let (_, toc, _) = run(&["--toc", FIXTURE], b"", None);
    assert_eq!(toc, "Page 1\nPage 2\n");
}

#[test]
fn missing_pdftotext_is_explained() {
    let (code, out, err) = run(&[], PDF, Some(""));
    assert_eq!(code, EXIT_FAILURE);
    assert!(out.is_empty());
    assert_eq!(err, MISSING_TOOL);
}

#[test]
fn a_broken_pdf_reports_the_tool_error() {
    if !has_pdftotext() {
        return;
    }
    let (code, _, err) = run(&["-f", "pdf"], b"not a pdf", None);
    assert_eq!(code, EXIT_FAILURE);
    assert!(err.starts_with("mu: -: pdftotext failed"), "{err}");
}
