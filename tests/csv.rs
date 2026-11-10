// Author: machinageist
// Date: 2026-11-09
// Description: CSV and TSV render as box-drawn tables through the shared table engine
// Notes: Data tables keep columns at least 10 wide and overflow rather than crushing them

mod common;

use common::{colored, plain, strip_sgr};
use mu::Format;
use unicode_width::UnicodeWidthStr;

const CSV: &str = include_str!("fixtures/sample.csv");
const TSV: &str = include_str!("fixtures/sample.tsv");

#[test]
fn csv_renders_as_a_table_with_numbers_right_aligned() {
    assert_eq!(
        plain(CSV, Format::Csv, 70),
        "\
┌───────────┬─────────┬────────────┬──────────┬──────────────────────┐
│ city      │ country │ population │ area_km2 │ note                 │
├───────────┼─────────┼────────────┼──────────┼──────────────────────┤
│ Tokyo     │ Japan   │   37400068 │     2194 │ Largest metro area,  │
│           │         │            │          │ by most counts       │
│ Delhi     │ India   │   28514000 │     1484 │                      │
│ Shanghai  │ China   │   25582000 │     6341 │ Coastal; \"Pearl of   │
│           │         │            │          │ the Orient\"          │
│ São Paulo │ Brazil  │   21650000 │     1521 │ Southern hemisphere  │
└───────────┴─────────┴────────────┴──────────┴──────────────────────┘
"
    );
}

#[test]
fn tsv_renders_as_a_table() {
    assert_eq!(
        plain(TSV, Format::Tsv, 80),
        "\
┌─────────┬──────────┬───────┬───────────────┐
│ tool    │ language │ stars │ license       │
├─────────┼──────────┼───────┼───────────────┤
│ ripgrep │ Rust     │ 48000 │ MIT/Unlicense │
│ fzf     │ Go       │ 65000 │ MIT           │
│ jq      │ C        │ 30000 │ MIT           │
└─────────┴──────────┴───────┴───────────────┘
"
    );
}

#[test]
fn tables_fit_when_they_can_and_overflow_evenly_when_they_cannot() {
    for width in 30..=120 {
        let out = plain(CSV, Format::Csv, width);
        let widths: Vec<usize> = out.lines().map(UnicodeWidthStr::width).collect();
        assert!(
            widths.iter().all(|line| *line == widths[0]),
            "ragged table at {width}"
        );
        // Natural widths 9, 7, 10, 8 and 34 plus 16 columns of borders and padding
        let narrowest = 9 + 7 + 10 + 8 + 10 + 16;
        if width >= narrowest {
            assert!(widths[0] <= width, "{} wide at {width}", widths[0]);
        } else {
            assert_eq!(widths[0], narrowest, "columns crushed below 10 at {width}");
        }
    }
}

#[test]
fn color_output_matches_plain_and_closes_styles() {
    for width in [40, 60, 100] {
        let color = colored(CSV, Format::Csv, width);
        assert_eq!(strip_sgr(&color), plain(CSV, Format::Csv, width));
        for line in color.lines() {
            if let Some(last) = line.rfind('\x1b') {
                assert!(line[last..].starts_with("\x1b[0m"), "{line:?}");
            }
        }
    }
}

#[test]
fn hostile_cells_are_scrubbed() {
    let hostile = "a,b\n\"\x1b]52;c;ZXZpbA==\x07\",\"\x1b[2J\"\n";
    for out in [
        plain(hostile, Format::Csv, 60),
        strip_sgr(&colored(hostile, Format::Csv, 60)),
    ] {
        assert!(!out.contains('\x1b') && !out.contains('\x07'), "{out:?}");
    }
}

#[test]
fn names_extensions_and_media_types_resolve() {
    assert_eq!(Format::from_name("CSV"), Some(Format::Csv));
    assert_eq!(Format::from_name("tsv"), Some(Format::Tsv));
    assert_eq!(Format::from_path("data/x.csv"), Some(Format::Csv));
    assert_eq!(Format::from_path("x.tab"), Some(Format::Tsv));
    assert_eq!(
        Format::from_media_type("text/csv; header=present"),
        Some(Format::Csv)
    );
    assert_eq!(
        Format::from_media_type("text/tab-separated-values"),
        Some(Format::Tsv)
    );
    assert!(Format::Csv.is_wide() && Format::Tsv.is_wide() && !Format::Json.is_wide());
}

#[test]
fn empty_input_prints_nothing() {
    assert_eq!(plain("", Format::Csv, 60), "");
    assert_eq!(plain("\n\n", Format::Tsv, 60), "");
}
