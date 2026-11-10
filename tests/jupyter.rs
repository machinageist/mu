// Author: machinageist
// Date: 2026-11-09
// Description: Jupyter notebooks render their prose, code and outputs as one document
// Notes: Exact output runs with images off, so the PNG output shows as its alt text

mod common;

use common::{assert_invariants, plain};
use mu::{Format, Options, detect, present_outlined, sniff};

const FIXTURE: &str = include_str!("fixtures/sample.ipynb");
const WIDTH: usize = 60;
const NBFORMAT_3: &str = "{\"nbformat\": 3, \"worksheets\": [{\"cells\": []}]}";

// The fixture notebook at sixty columns, images off
const EXPECTED: &str = "Sample notebook\n═══════════════\n\nA notebook with every kind of cell and output.\n\nSetup\n─────\n\nIn [1]:\n\n╭─ python ──────────────────────────────────────────────────\n│ import data\n│ rows = data.load(\"sample.csv\")\n│ print(f\"loaded {len(rows)} rows\")\n╰───────────────────────────────────────────────────────────\n\n╭───────────────────────────────────────────────────────────\n│ loaded 3 rows\n╰───────────────────────────────────────────────────────────\n\nIn [2]:\n\n╭─ python ──────────────────────────────────────────────────\n│ [r.id for r in rows]\n╰───────────────────────────────────────────────────────────\n\n╭───────────────────────────────────────────────────────────\n│ [1, 2, 3]\n╰───────────────────────────────────────────────────────────\n\nResults\n───────\n\nThe chart, with a code span:\n\nIn [3]:\n\n╭─ python ──────────────────────────────────────────────────\n│ plot(rows)\n╰───────────────────────────────────────────────────────────\n\n[image: output]\n\nstderr:\n\n╭───────────────────────────────────────────────────────────\n│ warning: small sample\n╰───────────────────────────────────────────────────────────\n\nIn [4]:\n\n╭─ python ──────────────────────────────────────────────────\n│ 1 / 0\n╰───────────────────────────────────────────────────────────\n\n╭───────────────────────────────────────────────────────────\n│ ZeroDivisionError: division by zero\n│ ----------------------------------------------------------\n┆ -----------------\n│ ZeroDivisionError: division by zero\n╰───────────────────────────────────────────────────────────\n\nIn [ ]:\n\n╭─ python ──────────────────────────────────────────────────\n│ # not run yet\n╰───────────────────────────────────────────────────────────\n\n╭───────────────────────────────────────────────────────────\n│ raw ``` text\n╰───────────────────────────────────────────────────────────\n";

#[test]
fn notebook_renders_exactly() {
    assert_eq!(plain(FIXTURE, Format::Jupyter, WIDTH), EXPECTED);
}

#[test]
fn notebook_is_detected_by_extension_and_content() {
    assert_eq!(detect(Some("sample.ipynb"), FIXTURE), Format::Jupyter);
    assert_eq!(sniff(FIXTURE), Format::Jupyter);
    assert_eq!(
        Format::from_media_type("application/x-ipynb+json"),
        Some(Format::Jupyter)
    );
}

#[test]
fn markdown_headings_fill_the_outline() {
    let rendered = present_outlined(FIXTURE, Format::Jupyter, &Options::new(WIDTH, false));
    let titles: Vec<&str> = rendered
        .headings
        .iter()
        .map(|heading| heading.text.as_str())
        .collect();
    assert_eq!(titles, ["Sample notebook", "Setup", "Results"]);
}

#[test]
fn other_versions_show_as_json() {
    assert_eq!(
        plain(NBFORMAT_3, Format::Jupyter, WIDTH),
        plain(NBFORMAT_3, Format::Json, WIDTH)
    );
}

#[test]
fn notebook_keeps_the_output_invariants() {
    assert_invariants(FIXTURE, Format::Jupyter);
}
