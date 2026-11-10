// Author: machinageist
// Date: 2026-11-09
// Description: Content sniffing across every format, each fixture against all the others
// Notes: Per-format tests only check their own format against Markdown; this catches
// one format's rule claiming another format's files

use mu::{Format, detect, sniff};

// Each fixture and the format sniffing must pick for it
const FIXTURES: [(&str, &str, Format); 15] = [
    (
        "kitchen-sink.md",
        include_str!("fixtures/kitchen-sink.md"),
        Format::Markdown,
    ),
    ("README.md", include_str!("../README.md"), Format::Markdown),
    (
        "CHANGELOG.md",
        include_str!("../CHANGELOG.md"),
        Format::Markdown,
    ),
    (
        "sample.html",
        include_str!("fixtures/sample.html"),
        Format::Html,
    ),
    (
        "sample.rst",
        include_str!("fixtures/sample.rst"),
        Format::Rst,
    ),
    (
        "sample.org",
        include_str!("fixtures/sample.org"),
        Format::Org,
    ),
    (
        "sample.adoc",
        include_str!("fixtures/sample.adoc"),
        Format::AsciiDoc,
    ),
    (
        "sample.json",
        include_str!("fixtures/sample.json"),
        Format::Json,
    ),
    (
        "sample.yaml",
        include_str!("fixtures/sample.yaml"),
        Format::Yaml,
    ),
    (
        "sample.toml",
        include_str!("fixtures/sample.toml"),
        Format::Toml,
    ),
    (
        "sample.csv",
        include_str!("fixtures/sample.csv"),
        Format::Csv,
    ),
    (
        "sample.tsv",
        include_str!("fixtures/sample.tsv"),
        Format::Tsv,
    ),
    (
        "sample.plist",
        include_str!("fixtures/sample.plist"),
        Format::Xml,
    ),
    (
        "sample.rss",
        include_str!("fixtures/sample.rss"),
        Format::Xml,
    ),
    (
        "sample.sh",
        include_str!("fixtures/sample.sh"),
        Format::Code,
    ),
];

#[test]
fn every_fixture_sniffs_as_its_own_format() {
    for (name, input, format) in FIXTURES {
        assert_eq!(sniff(input), format, "{name}");
    }
}

#[test]
fn extension_matches_content_for_every_fixture() {
    for (name, input, format) in FIXTURES {
        assert_eq!(detect(Some(name), input), format, "{name}");
    }
}
