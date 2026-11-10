// Author: machinageist
// Date: 2026-11-09
// Description: Input formats, detection by extension or content, and dispatch to a renderer
// Notes: Document formats emit pulldown-cmark events into the shared layout engine;
// data formats emit styled lines into the data printer

mod asciidoc;
mod csv;
mod html;
mod org;
mod rst;

use std::path::Path;

use crate::data;
use crate::render::{Options, Rendered, render_events, render_markdown, render_table_events};

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
    Csv,
    Tsv,
    Xml,
}

// Names accepted by --format, first one is canonical
const NAMES: [(Format, &[&str]); 11] = [
    (Format::Markdown, &["markdown", "md"]),
    (Format::Html, &["html", "htm"]),
    (Format::Rst, &["rst", "restructuredtext"]),
    (Format::Org, &["org"]),
    (Format::AsciiDoc, &["asciidoc", "adoc"]),
    (Format::Json, &["json"]),
    (Format::Yaml, &["yaml", "yml"]),
    (Format::Toml, &["toml"]),
    (Format::Csv, &["csv"]),
    (Format::Tsv, &["tsv"]),
    (Format::Xml, &["xml", "plist"]),
];

// File extensions, matched case-insensitively
const EXTENSIONS: [(Format, &[&str]); 11] = [
    (Format::Markdown, &["md", "markdown", "mdown", "mkd"]),
    (Format::Html, &["html", "htm", "xhtml"]),
    (Format::Rst, &["rst", "rest"]),
    (Format::Org, &["org"]),
    (Format::AsciiDoc, &["adoc", "asciidoc", "asc"]),
    (Format::Json, &["json", "jsonl", "geojson"]),
    (Format::Yaml, &["yaml", "yml"]),
    (Format::Toml, &["toml"]),
    (Format::Csv, &["csv"]),
    (Format::Tsv, &["tsv", "tab"]),
    (
        Format::Xml,
        &[
            "xml", "plist", "xsd", "xsl", "xslt", "rss", "atom", "svg", "opml",
        ],
    ),
];

// Media types from a Content-Type header, matched without parameters, case-insensitively
const MEDIA_TYPES: [(Format, &[&str]); 11] = [
    (Format::Markdown, &["text/markdown", "text/x-markdown"]),
    (Format::Html, &["text/html", "application/xhtml+xml"]),
    (Format::Rst, &["text/x-rst", "text/prs.fallenstein.rst"]),
    (Format::Org, &["text/org", "text/x-org"]),
    (Format::AsciiDoc, &["text/asciidoc", "text/x-asciidoc"]),
    (
        Format::Json,
        &[
            "application/json",
            "application/x-ndjson",
            "application/ld+json",
        ],
    ),
    (
        Format::Yaml,
        &[
            "application/yaml",
            "application/x-yaml",
            "text/yaml",
            "text/x-yaml",
        ],
    ),
    (Format::Toml, &["application/toml", "text/x-toml"]),
    (Format::Csv, &["text/csv"]),
    (Format::Tsv, &["text/tab-separated-values"]),
    (Format::Xml, &["application/xml", "text/xml"]),
];

// Structured-syntax suffix for JSON media types such as application/problem+json
const JSON_SUFFIX: &str = "+json";
// Structured-syntax suffix for XML types such as image/svg+xml; xhtml is matched as HTML first
const XML_SUFFIX: &str = "+xml";
const URL_SCHEME_SEPARATOR: &str = "://";
const URL_PATH_START: char = '/';
const URL_PATH_ENDS: [char; 2] = ['?', '#'];

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

    // Look up a Content-Type value such as `text/html; charset=utf-8`
    pub fn from_media_type(content_type: &str) -> Option<Format> {
        let essence = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        MEDIA_TYPES
            .iter()
            .find(|(_, types)| types.contains(&essence.as_str()))
            .map(|(format, _)| *format)
            .or_else(|| essence.ends_with(JSON_SUFFIX).then_some(Format::Json))
            .or_else(|| essence.ends_with(XML_SUFFIX).then_some(Format::Xml))
    }

    // Look up the extension of a URL's path, ignoring the host, query and fragment
    pub fn from_url_path(url: &str) -> Option<Format> {
        let after_scheme = url
            .split_once(URL_SCHEME_SEPARATOR)
            .map_or(url, |(_, rest)| rest);
        let path = &after_scheme[after_scheme.find(URL_PATH_START)?..];
        let path = path.split(URL_PATH_ENDS).next().unwrap_or_default();
        Format::from_path(path)
    }

    // Tables that use the full terminal width rather than the reading-width cap
    pub fn is_wide(self) -> bool {
        matches!(self, Format::Csv | Format::Tsv)
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
    } else if data::xml::sniff(head) {
        Format::Xml
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
        // Tables last: only input nothing else claimed is checked for even columns
        csv::sniff(head).unwrap_or(Format::Markdown)
    }
}

// Render input in the given format
pub fn present(input: &str, format: Format, options: &Options) -> String {
    present_outlined(input, format, options).text
}

// Render input in the given format, keeping the document's headings; data formats have none
pub fn present_outlined(input: &str, format: Format, options: &Options) -> Rendered {
    let no_headings = |text: String| Rendered {
        text,
        headings: Vec::new(),
    };
    match format {
        Format::Markdown => render_markdown(input, options),
        Format::Html => render_events(html::events(input), options, input.len()),
        Format::Rst => render_events(rst::events(input), options, input.len()),
        Format::Org => render_events(org::events(input), options, input.len()),
        Format::AsciiDoc => render_events(asciidoc::events(input), options, input.len()),
        Format::Json => no_headings(data::print(&data::json::highlight(input), options)),
        Format::Yaml => no_headings(data::print(&data::yaml::highlight(input), options)),
        Format::Toml => no_headings(data::print(&data::toml::highlight(input), options)),
        Format::Xml => no_headings(data::print(&data::xml::highlight(input), options)),
        Format::Csv | Format::Tsv => render_table_events(
            csv::events(input, format),
            options,
            input.len(),
            csv::CSV_MIN_COLUMN,
        ),
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
    fn media_types_resolve_with_parameters_and_suffixes() {
        assert_eq!(
            Format::from_media_type("text/html; charset=UTF-8"),
            Some(Format::Html)
        );
        assert_eq!(
            Format::from_media_type("Application/JSON"),
            Some(Format::Json)
        );
        assert_eq!(
            Format::from_media_type("application/problem+json"),
            Some(Format::Json)
        );
        assert_eq!(
            Format::from_media_type("text/markdown"),
            Some(Format::Markdown)
        );
        assert_eq!(Format::from_media_type("text/plain"), None);
        assert_eq!(Format::from_media_type("application/octet-stream"), None);
    }

    #[test]
    fn url_paths_resolve_by_path_not_host() {
        assert_eq!(Format::from_url_path("https://example.org"), None);
        assert_eq!(Format::from_url_path("https://example.org/"), None);
        assert_eq!(
            Format::from_url_path("https://raw.githubusercontent.com/o/r/HEAD/README.md"),
            Some(Format::Markdown)
        );
        assert_eq!(
            Format::from_url_path("https://x.io/data.json?page=2#top"),
            Some(Format::Json)
        );
        assert_eq!(
            Format::from_url_path("https://x.io/notes.org"),
            Some(Format::Org)
        );
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
