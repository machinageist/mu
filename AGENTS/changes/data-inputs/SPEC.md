# Change: CSV, TSV and XML inputs

Ships as **0.6.0**, the second release on `FEATURE/ROADMAP.md`.

## Outcome

`mu data.csv`, `mu data.tsv`, `mu feed.xml` and `mu Info.plist` are shown in readable form:
- tabular files as box-drawn tables
- XML highlighted, and re-indented when that is safe

Tables too wide for the window scroll sideways in the pager instead of being crushed.

## Formats

| Format | Names for `-f` | Extensions | Media types |
|---|---|---|---|
| CSV | `csv` | `.csv` | `text/csv` |
| TSV | `tsv` | `.tsv` `.tab` | `text/tab-separated-values` |
| XML | `xml`, `plist` | `.xml` `.plist` `.xsd` `.xsl` `.xslt` `.rss` `.atom` `.svg` `.opml` | `application/xml`, `text/xml`, and any `+xml` type except `application/xhtml+xml`, which stays HTML |

`.svg` now shows the SVG's source. Images inside documents are unaffected.

## CSV and TSV

- **Parsing:**
  - CSV follows RFC 4180. A field starting with `"` is quoted until the next lone `"`, `""` inside it is one quote, and delimiters and line breaks inside quotes are part of the field.
  - Text after a closing quote is kept as written, so malformed files still show.
  - `CRLF` and `LF` both end a record, and a byte order mark is ignored.
  - TSV splits on tabs only and has no quoting.
- **Delimiter:** a CSV's delimiter is detected from its first 20 records: the first of `,` `;` `\t` `|` that gives every one of those records the same number of fields, at least 2. Otherwise it is `,`.
- **Shape:**
  - The first record is the header.
  - Records with fewer fields are padded with empty cells, and the table is as wide as its longest record.
  - Blank lines are skipped.
  - Line breaks inside a field show as spaces.
  - Cell text is literal: no Markdown is interpreted, and control characters are scrubbed as everywhere else.
- **Alignment:** a column whose non-empty body cells are all numbers is right-aligned. A number is an optional sign, digits with an optional fraction or exponent, and an optional trailing `%`. All other columns are left-aligned.
- **Drawing:**
  - Tables use the shared table engine and look like Markdown tables, with a bold header row and box borders.
  - Columns shrink widest-first to fit the width, but never below 10 columns, or a column's natural width if that is less.
  - A table that still doesn't fit is drawn wider than the width. In the pager it scrolls sideways (see Pager), and when printing it overflows.
- **Width:** when the width is automatic, CSV and TSV use the full terminal width rather than the 100-column reading cap. `-w` still wins.
- **Outline:** none. `--toc` prints nothing.
- **Content sniffing:** this runs last, after YAML and before the Markdown fallback, and only when nothing else matched.
  - **Records:** at least 3 non-empty records in the head. When there are more than 3, the last one is ignored because it may be cut off.
  - **Shape:** the first record must not start with `#` `-` `*` `>` `|` `<` `{` `[` or `=`.
  - **TSV:** every record has the same number of tab-separated fields, at least 2.
  - **CSV:** the same rule with `,` or `;`, plus either at least 3 fields per record, or no delimiter followed by a space anywhere. This keeps comma-heavy prose out.

## XML

- **Tokens:** the tokenizer covers the input exactly, with no bytes lost: declarations and processing instructions (`<?…?>`), comments, CDATA, `<!DOCTYPE …>` including an internal subset, start, end and self-closing tags, and text. Anything malformed becomes text.
- **Highlighting:**

  | Part | Style |
  |---|---|
  | Element names | `data.key` |
  | Attribute names | `data.meta` |
  | Attribute values | `data.string` |
  | `<` `>` `/` `=` | `data.punct` |
  | Entities (`&amp;`) | `data.literal` |
  | Comments | `data.comment` |
  | Declarations and DOCTYPE | `data.meta` |
  | CDATA | markers in `data.punct`, content in `data.string` |
  | Text | plain |

- **Re-indenting:** the whole document is re-indented only when all of these hold:
  - tags balance with matching names
  - nesting is at most 256 deep
  - no element mixes non-blank text with child elements or comments
  - no non-blank text sits outside the root

  Otherwise it is highlighted exactly as written, like invalid JSON.
- **Re-indented layout:**
  - One node per line, indented two spaces per level.
  - An element holding only text (or CDATA) stays on one line: `<name a="1">text</name>`. Its text is kept verbatim, and any line breaks in it are kept.
  - Blank text between tags is dropped.
  - Runs of whitespace between attributes become one space. Attribute values are never changed.
  - An empty `<a></a>` stays as written.
- **Content sniffing:** this runs after HTML. The head, ignoring a byte order mark and leading blank space, must meet one of these:
  - it starts with `<?xml` and has no `<html` anywhere in the head
  - it starts with a `<!DOCTYPE` that is not `html`
  - its first element is one of `plist` `svg` `rss` `feed` `urlset` `sitemapindex` `project` `opml`

  Generic roots are not sniffed, because many READMEs open with `<div align="center">`.
- **Outline:** none.

## Pager: horizontal scrolling

- **Keys:** `l` and `→` scroll right by half the window width, and `h` and `←` scroll back. Scrolling works only while some line is wider than the window, and stops when the widest line's end reaches the right edge.
- **Status bar:** while scrolled, it shows `col N`, the first visible column, before the line range.
- **Searching:** a match that lands outside the visible columns scrolls sideways, so the match starts a quarter of the way into the window, clamped. `Esc` does not reset sideways scrolling, but a jump from the outline returns to column 1.
- **Resizing:** a resize or re-layout keeps the column offset, clamped to the new widest line.
- **Styling:** cutting a line at a column keeps its SGR state. A wide character cut in half becomes spaces. With styling removed, the visible part of a line equals the original text from that column on.

## Library

- **New variants:** `Format` gains `Csv`, `Tsv` and `Xml`. Adding variants breaks exhaustive matches in library code, which is acceptable before 1.0.
- **Wide formats:** `Format::is_wide()` is true for CSV and TSV, so the CLI knows when to lift the width cap.

## Acceptance criteria

- **CSV parsing:** unit tests cover quotes, `""`, quoted delimiters and line breaks, `CRLF`, a BOM, ragged rows, blank lines, delimiter detection (`,` `;` tab `|`), and numeric alignment.
- **Table width:** fixture tests for CSV and TSV check exact output at one width, a column minimum of 10 when shrinking, and that the table stays within the width when it fits. Color output with styling removed equals plain output.
- **XML:** unit tests cover every token kind, re-indenting a plist and an RSS fixture with exact output, the fallback for mixed content and for unbalanced and too-deep input, and that lossless tokenizing reproduces the input exactly.
- **Sniffing:** `tests/sniff.rs` gains CSV, TSV and XML fixtures. Every fixture still detects only as its own format, and prose with commas and a README opening `<div align="center">` stay Markdown.
- **Detection:** media types, extensions and `-f` names resolve, with tests.
- **Horizontal scroll:** unit tests cover the column cut (SGR kept, wide characters, offset past the end), the scroll limits, the status `col N`, and search scrolling sideways to a match. The pty test scrolls a wide CSV with `l` and `h`.
- **Unchanged:** existing fixtures render byte-identical.

## Out of scope

- Binary plists (`bplist00`)
- XML namespaces as anything but part of the name, DTD validation, and entity expansion beyond showing entities
- CSV options: a header-less mode, choosing the delimiter, or picking columns. The delimiter becomes a config key in 0.9.0
- An outline for CSV or XML
