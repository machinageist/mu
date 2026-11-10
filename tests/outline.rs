// Author: machinageist
// Date: 2026-11-09
// Description: Headings recorded by the renderer point at the lines that show them, in every document format
// Notes: Swept across widths so wrapping, gutters and heading rules cannot shift the line numbers

mod common;

use common::strip_sgr;
use std::process::{Command, Stdio};

use mu::{Format, Options, present, present_outlined};

const BIN: &str = env!("CARGO_BIN_EXE_mu");

const SWEEP_WIDTHS: std::ops::RangeInclusive<usize> = 30..=120;
const DOCUMENTS: [(&str, Format); 5] = [
    ("kitchen-sink.md", Format::Markdown),
    ("sample.html", Format::Html),
    ("sample.rst", Format::Rst),
    ("sample.org", Format::Org),
    ("sample.adoc", Format::AsciiDoc),
];
const DATA: [(&str, Format); 3] = [
    ("sample.json", Format::Json),
    ("sample.yaml", Format::Yaml),
    ("sample.toml", Format::Toml),
];

// Read a fixture by file name
fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).expect("read fixture")
}

#[test]
fn heading_lines_show_their_heading_at_every_width() {
    for (name, format) in DOCUMENTS {
        let input = fixture(name);
        for width in SWEEP_WIDTHS {
            for color in [false, true] {
                let rendered = present_outlined(&input, format, &Options::new(width, color));
                assert!(!rendered.headings.is_empty(), "{name}: no headings");
                let lines: Vec<&str> = rendered.text.lines().collect();
                for heading in &rendered.headings {
                    let shown = strip_sgr(lines[heading.line]);
                    let shown = shown.trim();
                    assert!(
                        !shown.is_empty() && heading.text.starts_with(shown),
                        "{name} at {width} (color {color}): line {} is {shown:?}, heading {:?}",
                        heading.line,
                        heading.text
                    );
                    assert!((1..=6).contains(&heading.level));
                }
            }
        }
    }
}

#[test]
fn outlined_text_is_exactly_the_presented_text() {
    for (name, format) in DOCUMENTS.into_iter().chain(DATA) {
        let input = fixture(name);
        let options = Options::new(80, true);
        assert_eq!(
            present_outlined(&input, format, &options).text,
            present(&input, format, &options),
            "{name}"
        );
    }
}

#[test]
fn data_formats_have_no_headings() {
    for (name, format) in DATA {
        let rendered = present_outlined(&fixture(name), format, &Options::new(80, false));
        assert!(rendered.headings.is_empty(), "{name}");
    }
}

#[test]
fn markdown_headings_keep_level_and_text() {
    let rendered = present_outlined(
        "# Top\n\ntext\n\n## Second *part*\n\n### Third\n",
        Format::Markdown,
        &Options::new(40, false),
    );
    let outline: Vec<(u8, &str)> = rendered
        .headings
        .iter()
        .map(|heading| (heading.level, heading.text.as_str()))
        .collect();
    assert_eq!(outline, [(1, "Top"), (2, "Second part"), (3, "Third")]);
    assert_eq!(rendered.headings[0].line, 0);
}

// Run mu on a fixture with color off and return (exit code, stdout)
fn run_toc(name: &str) -> (i32, String) {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(BIN)
        .args(["--toc", "--color", "never", "-w", "40", &path])
        .stdin(Stdio::null())
        .output()
        .expect("run mu");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn toc_prints_the_markdown_outline() {
    assert_eq!(
        run_toc("kitchen-sink.md"),
        (
            0,
            "mdread kitchen sink\n  Lists\n    Quotes and alerts\n      Code\n        Table\n          Misc\n"
                .to_owned()
        )
    );
}

#[test]
fn toc_works_for_html_and_is_empty_for_data() {
    let (code, out) = run_toc("sample.html");
    assert_eq!(code, 0);
    assert!(
        out.starts_with("Rendering HTML in the Terminal\n  Lists\n"),
        "{out}"
    );
    assert_eq!(run_toc("sample.json"), (0, String::new()));
}
