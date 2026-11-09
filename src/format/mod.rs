// Author: machinageist
// Date: 2026-11-09
// Description: Input formats, detection by extension or content, and dispatch to a renderer
// Notes: Document formats emit pulldown-cmark events into the shared layout engine;
// data formats emit styled lines into the data printer

mod asciidoc;
mod html;
mod org;
mod rst;

use std::path::Path;

use crate::data;
use crate::render::{Options, render, render_events};

// Sniffing looks only at the head of the input, cut back to a char boundary
const SNIFF_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Html,
    Rst,
    Org,
    AsciiDoc,
    Json,
    Yaml,
    Toml,
}

// Names accepted by --format, first one is canonical
const NAMES: [(Format, &[&str]); 8] = [
    (Format::Markdown, &["markdown", "md"]),
    (Format::Html, &["html", "htm"]),
    (Format::Rst, &["rst", "restructuredtext"]),
    (Format::Org, &["org"]),
    (Format::AsciiDoc, &["asciidoc", "adoc"]),
    (Format::Json, &["json"]),
    (Format::Yaml, &["yaml", "yml"]),
    (Format::Toml, &["toml"]),
];

// File extensions, matched case-insensitively
const EXTENSIONS: [(Format, &[&str]); 8] = [
    (Format::Markdown, &["md", "markdown", "mdown", "mkd"]),
    (Format::Html, &["html", "htm", "xhtml"]),
    (Format::Rst, &["rst", "rest"]),
    (Format::Org, &["org"]),
    (Format::AsciiDoc, &["adoc", "asciidoc", "asc"]),
    (Format::Json, &["json", "jsonl", "geojson"]),
    (Format::Yaml, &["yaml", "yml"]),
    (Format::Toml, &["toml"]),
];

impl Format {
    // Look up a --format value
    pub fn from_name(name: &str) -> Option<Format> {
        let name = name.to_ascii_lowercase();
        NAMES
            .iter()
            .find(|(_, names)| names.contains(&name.as_str()))
            .map(|(format, _)| *format)
    }

    // Look up a file's extension
    pub fn from_path(path: &str) -> Option<Format> {
        let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
        EXTENSIONS
            .iter()
            .find(|(_, exts)| exts.contains(&ext.as_str()))
            .map(|(format, _)| *format)
    }

    // Canonical name, as shown in help and errors
    pub fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|(format, _)| *format == self)
            .map_or("markdown", |(_, names)| names[0])
    }
}

// Pick a format from the path's extension, else from the content, else Markdown
pub fn detect(path: Option<&str>, input: &str) -> Format {
    path.and_then(Format::from_path)
        .unwrap_or_else(|| sniff(input))
}

// Guess a format from the head of the content, conservatively
pub fn sniff(input: &str) -> Format {
    let mut end = input.len().min(SNIFF_BYTES);
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    let head = &input[..end];
    // Most specific signals first; YAML last because loose prose can look like it
    if data::json::sniff(head) {
        Format::Json
    } else if html::sniff(head) {
        Format::Html
    } else if org::sniff(head) {
        Format::Org
    } else if rst::sniff(head) {
        Format::Rst
    } else if asciidoc::sniff(head) {
        Format::AsciiDoc
    } else if data::toml::sniff(head) {
        Format::Toml
    } else if data::yaml::sniff(head) {
        Format::Yaml
    } else {
        Format::Markdown
    }
}

// Render input in the given format
pub fn present(input: &str, format: Format, options: &Options) -> String {
    match format {
        Format::Markdown => render(input, options),
        Format::Html => render_events(html::events(input), options, input.len()),
        Format::Rst => render_events(rst::events(input), options, input.len()),
        Format::Org => render_events(org::events(input), options, input.len()),
        Format::AsciiDoc => render_events(asciidoc::events(input), options, input.len()),
        Format::Json => data::print(&data::json::highlight(input), options),
        Format::Yaml => data::print(&data::yaml::highlight(input), options),
        Format::Toml => data::print(&data::toml::highlight(input), options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_extensions_resolve() {
        assert_eq!(Format::from_name("ADOC"), Some(Format::AsciiDoc));
        assert_eq!(Format::from_name("yml"), Some(Format::Yaml));
        assert_eq!(Format::from_name("nope"), None);
        assert_eq!(Format::from_path("a/b/README.RST"), Some(Format::Rst));
        assert_eq!(Format::from_path("Cargo.toml"), Some(Format::Toml));
        assert_eq!(Format::from_path("notes"), None);
        assert_eq!(Format::from_path("x.txt"), None);
    }

    #[test]
    fn every_format_has_a_canonical_name_that_round_trips() {
        for (format, _) in NAMES {
            assert_eq!(Format::from_name(format.name()), Some(format));
        }
    }

    #[test]
    fn extension_beats_content() {
        assert_eq!(detect(Some("x.md"), "{\"a\": 1}"), Format::Markdown);
    }

    #[test]
    fn plain_prose_falls_back_to_markdown() {
        assert_eq!(sniff("Just some words.\n\nAnd more."), Format::Markdown);
        assert_eq!(sniff(""), Format::Markdown);
    }
}
