---
title: Formats
group: Guide
order: 2
summary: The eight formats and how mu picks one
---

Five document formats share one layout engine. Three data formats share one printer.

## Supported formats {#supported}

| Format | Kind | Extensions | Recognised in piped input by |
|---|---|---|---|
| Markdown (CommonMark + GFM) | document | `.md` `.markdown` `.mdown` `.mkd` | the fallback for anything else |
| HTML | document | `.html` `.htm` `.xhtml` | `<!doctype html>`, `<html>`, `<head>` or `<body>` at the start |
| reStructuredText | document | `.rst` `.rest` | `.. directive::` lines, `.. _target:`, over-and-underlined titles, `` `text <url>`_ `` links, `:role:` |
| Org-mode | document | `.org` | `#+KEYWORD:` or `#+BEGIN_` lines, `:PROPERTIES:`, or `*` headings with Org links |
| AsciiDoc | document | `.adoc` `.asciidoc` `.asc` | `= Title` first line, `:attr:` header lines, `[source,…]` blocks, `\|===` tables |
| JSON | data | `.json` `.jsonl` `.geojson` | starts with `{` or `[` and parses as JSON |
| YAML | data | `.yaml` `.yml` | mostly `key: value` / `- item` lines with a top-level key or `---` |
| TOML | data | `.toml` | mostly `[table]` and `key = value` lines |

## Format selection {#selection}

The first rule that gives an answer wins:

1. The `-f` / `--format <name>` flag.
2. For a URL, the response `Content-Type`. `text/plain` and `application/octet-stream` say nothing and are skipped.
3. The file extension, case-insensitive. For a URL, the extension of its path, never its host.
4. Content sniffing on the first 8 KB. This is used for stdin and for unknown or missing extensions such as `.txt`.
5. Markdown, which also shows plain text correctly.

## Content sniffing {#sniffing}

Sniffing runs the rules below in a fixed order, most specific first. Every rule is strict, so uncertain input falls through to Markdown.

| # | Format | Rule |
|---|---|---|
| 1 | JSON | Starts with `{` or `[` and tokenizes as JSON so far |
| 2 | HTML | A doctype, `html`, `head` or `body` tag comes first |
| 3 | Org | `#+KEYWORD` or `#+BEGIN_` line, a `PROPERTIES` drawer, or a star heading plus a described or URL link |
| 4 | RST | `.. directive::` or `.. _target:` line, an over-and-underlined title after a blank line, or link/role syntax with no Markdown headings or fences |
| 5 | AsciiDoc | `= Title` first line, `:attr:` header, source block attribute plus delimiter, or a table fence |
| 6 | TOML | 2+ lines, 80% are tables or `key = value`, and at least one `key = value` |
| 7 | YAML | 3+ lines, 80% YAML-shaped, a key, a top-level key or `---` or item mapping, and no fences, quotes or tables |
| — | Markdown | Everything else |

> [!TIP]
> If mu guesses wrong, pass `-f`. `tests/sniff.rs` sniffs every fixture against every format, so one format's rule can't quietly claim another format's files.

The flow is drawn as [logic map 3](/wiki/logic-maps#map-3).

## Documents {#documents}

Each document format is translated into the same block and inline structure as Markdown, as a stream of `pulldown-cmark` events. Headings, lists, quotes, tables, code frames, links and admonitions therefore look the same whatever the source.

- **HTML** drops `script`, `style`, `head` and similar non-content, and decodes entities.
- **reStructuredText** and **AsciiDoc** admonitions, and Org `#+BEGIN_NOTE`-style blocks, render like GitHub alerts.
- **Org** drawers and planning lines are hidden. `#+TITLE`, TODO keywords and tags show on headings.

## Data formats {#data}

Data is colored by token: keys, strings, numbers, booleans and null, comments, punctuation, YAML anchors and tags, and TOML table headers.

- Valid **JSON** and NDJSON are re-indented with two spaces. Strings and numbers are kept exactly as written.
- Invalid JSON, JSON nested deeper than 256 levels, and all **YAML** and **TOML** are colored exactly as written and never reformatted.
- Long lines hard-wrap behind a dim `┆` continuation bar.

## Parser coverage {#parser-coverage}

The non-Markdown parsers are hand-written and tolerant. They cover the subset that real READMEs and pages use, not the full specs.

| Format | Covered | Not covered |
|---|---|---|
| HTML | Headings, paragraphs, lists, quotes, `pre` code, tables, links, images, entities | Table cell spans |
| RST | Titles, inline markup and links, lists, definition and field lists, literal and `code-block` blocks, admonitions, simple and grid tables, images | Cell spans, line blocks, letter/roman enumerators |
| Org | Headings with TODO and tags, emphasis, links, lists and checkboxes, src/quote/example blocks, admonition blocks, tables, footnotes | More than one header row per table |
| AsciiDoc | Title and attributes, sections, inline markup and links, lists and checklists, source/listing/quote/example/sidebar blocks, admonitions, tables, images | Cell spans, footnotes |
