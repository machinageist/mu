// Author: machinageist
// Description: Hyperlink policy through the real CLI, with deterministic environment and piped output.
// Notes: A subprocess proves emitted bytes, not terminal-emulator clickability.

use std::io::Write;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_mu");
const DOCUMENT: &str = "[Guide](https://example.com/guide)\n";
const FALLBACK: &str = "Guide (https://example.com/guide)\n";
const LINKED: &str = "\x1b]8;;https://example.com/guide\x1b\\Guide\x1b]8;;\x1b\\\n";

fn run(args: &[&str], input: &str, environment: &[(&str, &str)]) -> Output {
    let mut command = Command::new(BIN);
    command
        .env_clear()
        .env("TERM", "xterm-256color")
        .env("TERM_PROGRAM", "WezTerm")
        .args(["--paging=never", "--images=never", "--width=80"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("spawn mu");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait for mu")
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "mu failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 output")
}

#[test]
fn links_never_accepts_both_value_forms_and_preserves_plain_fallback() {
    for args in [
        vec!["--links=never", "--color=never"],
        vec!["--links", "never", "--color", "never"],
    ] {
        assert_eq!(stdout(run(&args, DOCUMENT, &[])), FALLBACK);
    }
}

#[test]
fn links_always_overrides_a_pipe_plain_color_and_no_color() {
    for args in [
        vec!["--links=always", "--color=never"],
        vec!["--links", "always", "--color", "never"],
    ] {
        assert_eq!(
            stdout(run(&args, DOCUMENT, &[("NO_COLOR", "1"), ("TERM", "dumb")])),
            LINKED
        );
    }
}

#[test]
fn auto_and_never_do_not_emit_osc8_to_a_pipe_even_with_color_forced() {
    for args in [
        vec!["--color=always"],
        vec!["--links=auto", "--color=always"],
        vec!["--links=never", "--color=always"],
        vec!["--links=auto", "--color=never"],
    ] {
        let text = stdout(run(&args, DOCUMENT, &[]));
        assert!(!text.contains("\x1b]8;"));
        assert!(text.contains("(https://example.com/guide)"));
    }
}

#[test]
fn invalid_and_missing_link_modes_are_usage_errors() {
    for (args, message) in [
        (vec!["--links"], "--links needs a value"),
        (vec!["--links="], "invalid links mode ''"),
        (vec!["--links=sometimes"], "invalid links mode 'sometimes'"),
        (vec!["--links", "ALWAYS"], "invalid links mode 'ALWAYS'"),
    ] {
        let output = run(&args, "", &[]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
    }
}

#[test]
fn forced_links_do_not_put_terminal_controls_in_the_outline() {
    let source = "# [Guide](https://example.com/guide)\n";
    let plain = stdout(run(
        &["--links=never", "--color=never", "--toc"],
        source,
        &[],
    ));
    let forced = stdout(run(
        &["--links=always", "--color=never", "--toc"],
        source,
        &[],
    ));
    assert_eq!(forced, plain);
    assert!(!forced.contains('\x1b'));
}

#[test]
fn forced_links_cannot_replay_source_osc_controls_in_the_cli() {
    let source =
        "<p><a href='https://example.com/&#27;]8;;evil&#7;'>safe&#27;]52;c;payload&#7;</a></p>";
    let text = stdout(run(
        &["--format=html", "--links=always", "--color=never"],
        source,
        &[],
    ));
    assert!(!text.contains('\x1b'));
    assert!(!text.contains('\x07'));
    assert!(text.contains('\u{fffd}'));
}

#[test]
fn help_documents_link_controls_and_epub_navigation() {
    let help = stdout(run(&["--help"], "", &[]));
    for expected in [
        "--links <WHEN>",
        "Shift-Tab",
        "selected EPUB link",
        "Backspace",
    ] {
        assert!(help.contains(expected), "missing help: {expected}");
    }
}
