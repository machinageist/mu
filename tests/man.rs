// Author: machinageist
// Date: 2026-11-09
// Description: Man pages in roff source, with man and mdoc macros, render as documents
// Notes: Fixtures are written for mu, not copied from system pages; exact output at one width

mod common;

use common::{assert_invariants, plain};
use mu::{Format, Options, detect, present_outlined, sniff};

const MAN_FIXTURE: &str = include_str!("fixtures/sample.1");
const MDOC_FIXTURE: &str = include_str!("fixtures/sample.mdoc");
const WIDTH: usize = 60;

// The man-macro fixture at sixty columns
const MAN_EXPECTED: &str = "SAMPLE(1)\n═════════\n\nNAME\n────\n\nsample - show a page of text\n\nSYNOPSIS\n────────\n\nsample [-v] [-w width] file...\n\nDESCRIPTION\n───────────\n\nsample prints each file to standard output, with “curly\nquotes” and an em—dash.\n\nA second paragraph that is long enough to wrap onto another\nline at sixty columns.\n\nOptions\n\n-v\n    Print more.\n\n-w width\n    Wrap at width columns.\n\n    The default is 80.\n\n• A bullet.\n\n• Another bullet.\n\nEXAMPLES\n────────\n\n╭───────────────────────────────────────────────────────────\n│ sample -w 40 notes.txt | less\n╰───────────────────────────────────────────────────────────\n\n    Indented note.\n\nSEE ALSO\n────────\n\ncat(1), the sample site (https://example.org/sample).\n";

// The mdoc fixture at sixty columns
const MDOC_EXPECTED: &str = "SAMPLE(1)\n═════════\n\nNAME\n────\n\nsample — show a page of text\n\nSYNOPSIS\n────────\n\nsample [-v] [-w width] [file ...]\n\nDESCRIPTION\n───────────\n\nThe sample utility prints each file.\n\n-v\n    Print more.\n\n-w width\n    Wrap at width columns (default 80).\n\n• A bullet.\n\n• Another bullet.\n\nEXAMPLES\n────────\n\n╭───────────────────────────────────────────────────────────\n│ sample -w 40 notes.txt\n╰───────────────────────────────────────────────────────────\n\nEXIT STATUS\n───────────\n\nThe sample utility exits 0 on success, and >0 if an error\noccurs.\n\nSEE ALSO\n────────\n\ncat(1), the sample site (https://example.org/sample)\n";

#[test]
fn man_macros_render_exactly() {
    assert_eq!(plain(MAN_FIXTURE, Format::Man, WIDTH), MAN_EXPECTED);
}

#[test]
fn mdoc_macros_render_exactly() {
    assert_eq!(plain(MDOC_FIXTURE, Format::Man, WIDTH), MDOC_EXPECTED);
}

#[test]
fn pages_are_detected_by_extension_and_content() {
    assert_eq!(detect(Some("sample.1"), MAN_FIXTURE), Format::Man);
    assert_eq!(detect(Some("sample.mdoc"), MDOC_FIXTURE), Format::Man);
    assert_eq!(sniff(MAN_FIXTURE), Format::Man);
    assert_eq!(sniff(MDOC_FIXTURE), Format::Man);
}

#[test]
fn sections_fill_the_outline() {
    let rendered = present_outlined(MAN_FIXTURE, Format::Man, &Options::new(WIDTH, false));
    let titles: Vec<(u8, &str)> = rendered
        .headings
        .iter()
        .map(|heading| (heading.level, heading.text.as_str()))
        .collect();
    assert_eq!(
        titles,
        [
            (1, "SAMPLE(1)"),
            (2, "NAME"),
            (2, "SYNOPSIS"),
            (2, "DESCRIPTION"),
            (3, "Options"),
            (2, "EXAMPLES"),
            (2, "SEE ALSO"),
        ]
    );
}

#[test]
fn malformed_roff_still_renders() {
    let out = plain(
        ".TH\n.RE\n.El\n.TP\n.IP\n\\f\n.nf\nunclosed \\(",
        Format::Man,
        WIDTH,
    );
    assert!(out.contains("unclosed"), "{out}");
}

#[test]
fn man_pages_keep_the_output_invariants() {
    assert_invariants(MAN_FIXTURE, Format::Man);
    assert_invariants(MDOC_FIXTURE, Format::Man);
}
