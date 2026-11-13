// Author: machinageist
// Date: 2026-11-09
// Description: Input formats, detection by extension or content, and dispatch to a renderer
// Notes: Document formats emit pulldown-cmark events into the shared layout engine;
// data formats emit styled lines into the data printer

mod asciidoc;
mod csv;
mod html;
mod jupyter;
mod man;
mod org;
mod rst;

use std::path::Path;

use crate::book;
use crate::data;
use crate::highlight;
use crate::render::{
    Options, Rendered, render_annotated_events, render_events, render_markdown, render_table_events,
};

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
    Code,
    Man,
    Jupyter,
    Epub,
    Pdf,
}

// Names accepted by --format, first one is canonical
const NAMES: [(Format, &[&str]); 16] = [
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
    (Format::Code, &["code", "text"]),
    (Format::Man, &["man", "roff", "mdoc"]),
    (Format::Jupyter, &["ipynb", "jupyter", "notebook"]),
    (Format::Epub, &["epub"]),
    (Format::Pdf, &["pdf"]),
];

// File extensions, matched case-insensitively
const EXTENSIONS: [(Format, &[&str]); 16] = [
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
    (
        Format::Code,
        &[
            // C family
            "c",
            "h",
            "cc",
            "cpp",
            "cxx",
            "c++",
            "hh",
            "hpp",
            "hxx",
            "m",
            "mm",
            // Systems
            "rs",
            "go",
            "zig",
            "nim",
            "asm",
            "s",
            // Scripting
            "py",
            "pyi",
            "rb",
            "pl",
            "pm",
            "php",
            "lua",
            "r",
            "jl",
            // Web
            "js",
            "mjs",
            "cjs",
            "jsx",
            "ts",
            "tsx",
            "vue",
            "svelte",
            "css",
            "scss",
            "sass",
            "less",
            // JVM and .NET
            "java",
            "kt",
            "kts",
            "scala",
            "groovy",
            "gradle",
            "cs",
            "fs",
            // Functional and other compiled
            "hs",
            "ml",
            "mli",
            "ex",
            "exs",
            "erl",
            "clj",
            "el",
            "lisp",
            "scm",
            "dart",
            "swift",
            // Shell
            "sh",
            "bash",
            "zsh",
            "fish",
            "ksh",
            "ps1",
            "bat",
            "cmd",
            // Other
            "sql",
            "diff",
            "patch",
            "vim",
            "mk",
            "cmake",
            "proto",
            "graphql",
            "tf",
            "hcl",
            "nix",
            "ini",
            "cfg",
            "conf",
            "properties",
        ],
    ),
    (Format::Man, &["man", "mdoc"]),
    (Format::Jupyter, &["ipynb"]),
    (Format::Epub, &["epub"]),
    (Format::Pdf, &["pdf"]),
];

// Media types from a Content-Type header, matched without parameters, case-insensitively
const MEDIA_TYPES: [(Format, &[&str]); 16] = [
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
    (
        Format::Code,
        &[
            "text/x-c",
            "text/x-csrc",
            "text/x-chdr",
            "text/x-c++src",
            "text/x-c++hdr",
            "text/x-python",
            "text/x-rust",
            "text/x-go",
            "text/x-java",
            "text/x-java-source",
            "text/javascript",
            "application/javascript",
            "application/x-javascript",
            "text/x-sh",
            "application/x-sh",
            "text/x-shellscript",
            "text/x-diff",
            "text/x-patch",
            "text/css",
            "application/sql",
        ],
    ),
    (Format::Man, &["text/troff", "application/x-troff-man"]),
    (Format::Jupyter, &["application/x-ipynb+json"]),
    (Format::Epub, &["application/epub+zip"]),
    (Format::Pdf, &["application/pdf"]),
];

// Structured-syntax suffix for JSON media types such as application/problem+json
const JSON_SUFFIX: &str = "+json";
// Structured-syntax suffix for XML types such as image/svg+xml; xhtml is matched as HTML first
const XML_SUFFIX: &str = "+xml";
// Man page sections, and the longest suffix such as `3pm` or `1ssl`
const MAN_SECTIONS: &str = "123456789";
const MAX_MAN_SUFFIX: usize = 6;
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
            .or_else(|| is_man_section(&ext).then_some(Format::Man))
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

    // Tables and PDF pages use the full terminal width rather than the reading-width cap
    pub fn is_wide(self) -> bool {
        matches!(self, Format::Csv | Format::Tsv | Format::Pdf)
    }

    // Formats read from bytes and converted with `extract` before presenting
    pub fn is_binary(self) -> bool {
        matches!(self, Format::Epub | Format::Pdf)
    }

    // Canonical name, as shown in help and errors
    pub fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|(format, _)| *format == self)
            .map_or("markdown", |(_, names)| names[0])
    }
}

// A man page section suffix: a digit 1-9, optionally followed by letters (`3p`, `1ssl`)
fn is_man_section(ext: &str) -> bool {
    let mut chars = ext.chars();
    chars
        .next()
        .is_some_and(|first| MAN_SECTIONS.contains(first))
        && ext.len() <= MAX_MAN_SUFFIX
        && chars.all(|ch| ch.is_ascii_lowercase())
}

// The extensions shown as source code, each of which has a highlighter
#[cfg(test)]
pub(crate) fn code_extensions() -> &'static [&'static str] {
    EXTENSIONS
        .iter()
        .find(|(format, _)| *format == Format::Code)
        .map_or(&[], |(_, extensions)| extensions)
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
    if data::overstrike::sniff(head) {
        Format::Man
    } else if data::code::sniff(head) {
        Format::Code
    } else if jupyter::sniff(head) {
        Format::Jupyter
    } else if data::json::sniff(head) {
        Format::Json
    } else if man::sniff(head) {
        Format::Man
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
        ..Rendered::default()
    };
    match format {
        Format::Markdown => render_markdown(input, options),
        Format::Html => render_events(html::events(input), options, input.len()),
        Format::Epub => {
            render_annotated_events(html::annotated_events(input), options, input.len())
        }
        Format::Pdf => book::pdf::present(input, options),
        Format::Rst => render_events(rst::events(input), options, input.len()),
        Format::Org => render_events(org::events(input), options, input.len()),
        Format::AsciiDoc => render_events(asciidoc::events(input), options, input.len()),
        Format::Man if data::overstrike::is_formatted(input) => man_output(input, options),
        Format::Jupyter => match jupyter::markdown(input) {
            Some(markdown) => render_markdown(&markdown, options),
            None => no_headings(data::print(&data::json::highlight(input), options)),
        },
        Format::Man => render_events(man::events(input), options, input.len()),
        Format::Json => no_headings(data::print(&data::json::highlight(input), options)),
        Format::Yaml => no_headings(data::print(&data::yaml::highlight(input), options)),
        Format::Toml => no_headings(data::print(&data::toml::highlight(input), options)),
        Format::Xml => no_headings(data::print(&data::xml::highlight(input), options)),
        Format::Code => no_headings(data::print(&code_lines(input, options), options)),
        Format::Csv | Format::Tsv => render_table_events(
            csv::events(input, format),
            options,
            input.len(),
            csv::CSV_MIN_COLUMN,
        ),
    }
}

// Source lines, highlighted when color is on and the language is known
fn code_lines(input: &str, options: &Options) -> Vec<data::StyledLine> {
    if !options.color {
        return data::code::lines(input);
    }
    let language = options
        .language
        .as_deref()
        .and_then(highlight::language)
        .or_else(|| highlight::language_from_shebang(input));
    match language {
        Some(language) => highlight::highlight(
            input.trim_start_matches(data::code::BYTE_ORDER_MARK),
            language,
        ),
        None => data::code::lines(input),
    }
}

// Print formatted man output, mapping its headings to the lines they land on
fn man_output(input: &str, options: &Options) -> Rendered {
    let (lines, mut headings) = data::overstrike::lines(input);
    let (text, starts) = data::print_counted(&lines, options);
    for heading in &mut headings {
        heading.line = starts[heading.line];
    }
    Rendered {
        text,
        headings,
        ..Rendered::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_media_type_has_a_highlighter() {
        for (_, types) in MEDIA_TYPES
            .iter()
            .filter(|(format, _)| *format == Format::Code)
        {
            for media_type in *types {
                assert!(highlight::language(media_type).is_some(), "{media_type}");
            }
        }
    }

    #[test]
    fn names_and_extensions_resolve() {
        assert_eq!(Format::from_name("ADOC"), Some(Format::AsciiDoc));
        assert_eq!(Format::from_name("yml"), Some(Format::Yaml));
        assert_eq!(Format::from_name("nope"), None);
        assert_eq!(Format::from_path("a/b/README.RST"), Some(Format::Rst));
        assert_eq!(Format::from_path("Cargo.toml"), Some(Format::Toml));
        assert_eq!(Format::from_path("notes"), None);
        assert_eq!(Format::from_path("x.txt"), None);
        assert_eq!(Format::from_path("src/main.C"), Some(Format::Code));
        assert_eq!(Format::from_path("lib.h"), Some(Format::Code));
        assert_eq!(Format::from_path("build.sh"), Some(Format::Code));
        assert_eq!(Format::from_name("text"), Some(Format::Code));
        assert_eq!(Format::from_path("ls.1"), Some(Format::Man));
        assert_eq!(Format::from_path("printf.3p"), Some(Format::Man));
        assert_eq!(Format::from_path("openssl.1ssl"), Some(Format::Man));
        assert_eq!(Format::from_path("page.mdoc"), Some(Format::Man));
        assert_eq!(Format::from_path("backup.0"), None);
        assert_eq!(Format::from_path("x.1.bak"), None);
        assert_eq!(Format::from_name("roff"), Some(Format::Man));
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
        assert_eq!(Format::from_media_type("text/x-csrc"), Some(Format::Code));
        assert_eq!(Format::from_media_type("text/troff"), Some(Format::Man));
        assert_eq!(
            Format::from_media_type("application/x-ipynb+json"),
            Some(Format::Jupyter)
        );
        assert_eq!(Format::from_path("analysis.ipynb"), Some(Format::Jupyter));
        assert_eq!(Format::from_name("notebook"), Some(Format::Jupyter));
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
