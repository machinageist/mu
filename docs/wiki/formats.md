---
title: Formats
group: Guide
order: 2
summary: Every format mu reads, and how it picks one
---

Seven document formats share one layout engine. Four data formats share one printer. Two table formats, CSV and TSV, go through the same table engine as Markdown tables. Source code is shown verbatim. EPUB books and PDF are converted from their bytes first: a book becomes HTML, a PDF becomes page text.

## Supported formats {#supported}

| Format | Kind | Extensions | Recognised in piped input by |
|---|---|---|---|
| Markdown (CommonMark + GFM) | document | `.md` `.markdown` `.mdown` `.mkd` | the fallback for anything else |
| HTML | document | `.html` `.htm` `.xhtml` | `<!doctype html>`, `<html>`, `<head>` or `<body>` at the start |
| reStructuredText | document | `.rst` `.rest` | `.. directive::` lines, `.. _target:`, over-and-underlined titles, `` `text <url>`_ `` links, `:role:` |
| Org-mode | document | `.org` | `#+KEYWORD:` or `#+BEGIN_` lines, `:PROPERTIES:`, or `*` headings with Org links |
| AsciiDoc | document | `.adoc` `.asciidoc` `.asc` | `= Title` first line, `:attr:` header lines, `[source,…]` blocks, `\|===` tables |
| Man pages (roff) | document | `.1` to `.9`, also with letters (`.3p`, `.1ssl`), `.man` `.mdoc` | a `.TH` or `.Dd` request before any text |
| Jupyter notebook | document | `.ipynb` | a JSON object with `"cells"` and `"cell_type"` |
| EPUB book | document, built as HTML | `.epub` | a zip whose first entry is `mimetype` holding `application/epub+zip` |
| PDF | page text, through `pdftotext` | `.pdf` | `%PDF-` in the first 1 KB |
| JSON | data | `.json` `.jsonl` `.geojson` | starts with `{` or `[` and parses as JSON |
| YAML | data | `.yaml` `.yml` | mostly `key: value` / `- item` lines with a top-level key or `---` |
| TOML | data | `.toml` | mostly `[table]` and `key = value` lines |
| XML | data | `.xml` `.plist` `.svg` `.rss` `.atom` `.xsd` `.xsl` `.xslt` `.opml` | an `<?xml` declaration, a non-HTML `<!DOCTYPE`, or a well-known root element |
| CSV | table | `.csv` | 3+ lines with the same number of `,` or `;` fields |
| TSV | table | `.tsv` `.tab` | 3+ lines with the same number of tab-separated fields |
| Source code | verbatim text | `.c` `.h` `.cpp` `.rs` `.py` `.js` `.ts` `.go` `.java` `.sh` `.sql` `.diff` and about 70 more | a `#!` first line |

XML, CSV and TSV were added in 0.6.0. Source code, man pages and Jupyter notebooks were added in 0.7.0. EPUB and PDF were added in 0.8.0.

## Format selection {#selection}

The first rule that gives an answer wins:

1. The `-f` / `--format <name>` flag. Names are `markdown`, `html`, `rst`, `org`, `asciidoc`, `json`, `yaml`, `toml`, `csv`, `tsv`, `xml`, `man`, `ipynb`, `epub`, `pdf` and `code`, plus the aliases `md`, `htm`, `restructuredtext`, `adoc`, `yml`, `plist`, `roff`, `mdoc`, `jupyter`, `notebook` and `text`.
2. For a URL, the response `Content-Type`. `text/plain` and `application/octet-stream` say nothing and are skipped. See [URLs](/wiki/urls#url-format).
3. The file extension, case-insensitive. For a URL, the extension of its path, never its host.
4. Magic bytes for the binary formats: `%PDF-` in the first 1 KB is PDF, and a zip that starts with a `mimetype` entry holding `application/epub+zip` is EPUB.
5. Content sniffing on the first 8 KB. This is used for stdin and for unknown or missing extensions such as `.txt`.
6. Markdown, which also shows plain text correctly.

> [!NOTE]
> An `.svg` file is XML, so `mu logo.svg` shows its source, highlighted. An SVG is drawn as a picture only when a document embeds it. See [Images](/wiki/images).

## Content sniffing {#sniffing}

Sniffing runs the rules below in a fixed order, most specific first. Every rule is strict, so uncertain input falls through to Markdown.

| # | Format | Rule |
|---|---|---|
| 1 | Man output | 8+ backspace overstrikes, as `man ls \| mu` sends, or SGR bold and underline under a `LS(1) … LS(1)` header line |
| 2 | Source code | The input starts with `#!`, like a script |
| 3 | Jupyter | A JSON object whose head has `"cells"` and `"cell_type"` |
| 4 | JSON | Starts with `{` or `[` and tokenizes as JSON so far |
| 5 | Man page | Skipping comments, blank lines, other requests and `.de` macro definitions, a `.TH` or `.Dd` request comes before any text |
| 6 | HTML | A doctype, `html`, `head` or `body` tag comes first |
| 7 | XML | An `<?xml` declaration with no `<html` after it, a `<!DOCTYPE` that isn't HTML, or a first element named `plist`, `svg`, `rss`, `feed`, `urlset`, `sitemapindex`, `project` or `opml` |
| 8 | Org | `#+KEYWORD` or `#+BEGIN_` line, a `PROPERTIES` drawer, or a star heading plus a described or URL link |
| 9 | RST | `.. directive::` or `.. _target:` line, an over-and-underlined title after a blank line, or link/role syntax with no Markdown headings or fences |
| 10 | AsciiDoc | `= Title` first line, `:attr:` header, source block attribute plus delimiter, or a table fence |
| 11 | TOML | 2+ lines, 80% are tables or `key = value`, and at least one `key = value` |
| 12 | YAML | 3+ lines, 80% YAML-shaped, a key, a top-level key or `---` or item mapping, and no fences, quotes or tables |
| 13 | TSV, then CSV | See [below](#table-sniffing) |
| — | Markdown | Everything else |

> [!TIP]
> If mu guesses wrong, pass `-f`. `tests/sniff.rs` sniffs every fixture against every format, so one format's rule can't quietly claim another format's files.

### Tables in piped input {#table-sniffing}

CSV and TSV are checked last, so only input that nothing else claimed is looked at for even columns:

- The input must not start with `#`, `-`, `*`, `>`, `|`, `<`, `{`, `[` or `=`, which begin other formats.
- It needs at least 3 non-blank lines. The last line of a cut-off head is ignored, since it may be partial.
- If every line has the same number of tab-separated fields, 2 or more, it's TSV.
- Otherwise `,` and then `;` are tried the same way, with RFC 4180 quoting. A split into just 2 fields doesn't count when the delimiter is followed by a space somewhere, because that reads like prose such as `one, two`.

The flow is drawn as [logic map 3](/wiki/logic-maps#map-3).

## Documents {#documents}

Each document format is translated into the same block and inline structure as Markdown, as a stream of `pulldown-cmark` events. Headings, lists, quotes, tables, code frames, links and admonitions therefore look the same whatever the source.

- **HTML** drops `script`, `style`, `head` and similar non-content, and decodes entities.
- **reStructuredText** and **AsciiDoc** admonitions, and Org `#+BEGIN_NOTE`-style blocks, render like GitHub alerts.
- **Org** drawers and planning lines are hidden. `#+TITLE`, TODO keywords and tags show on headings.

Every document format records its headings, which feed the pager's [outline](/wiki/pager#outline) and `--toc`.

Semantic links in documents share [the OSC 8 policy](/wiki/usage#links): eligible absolute HTTP(S)/mailto destinations can make labels clickable without printed URL suffixes; otherwise the printed fallback remains. Source, code and data URLs are not auto-linkified. EPUB internal chapter links instead use [pager keyboard navigation](/wiki/pager#chapter-links).

## Data formats {#data}

Data is colored by token: keys, strings, numbers, booleans and null, comments, punctuation, YAML anchors and tags, and TOML table headers.

- Valid **JSON** and NDJSON are re-indented with two spaces. Strings and numbers are kept exactly as written.
- Invalid JSON, JSON nested deeper than 256 levels, and all **YAML** and **TOML** are colored exactly as written and never reformatted.
- Long lines hard-wrap behind a dim `┆` continuation bar.

## XML {#xml}

XML covers property lists, SVG source, RSS and Atom feeds, sitemaps, OPML and anything else with tags. mu colors element names, attribute names, attribute values, entity references such as `&amp;`, comments, the `<?xml` declaration and doctype, and CDATA sections.

It's re-indented two spaces per level, one node per line, when the whole document allows it:

- every tag closes, in order, and nothing broken is left open;
- nothing but markup sits outside the root element;
- no element mixes text with child elements;
- nesting is no deeper than 256 levels.

An element that holds only text stays on one line, and whitespace between a tag's attributes is squeezed to one space. Anything else, such as XHTML-like prose, is colored exactly as written, so nothing is lost.

```term Info.plist — mu
<cy><?xml version="1.0"?></cy>
<d><</d><bl>plist</bl> <cy>version</cy><d>=</d><g>"1.0"</g><d>></d>
  <d><</d><bl>dict</bl><d>></d>
    <d><</d><bl>key</bl><d>></d>Name<d></</d><bl>key</bl><d>></d>
    <d><</d><bl>string</bl><d>></d>mu <y>&amp;</y> co<d></</d><bl>string</bl><d>></d>
    <d><</d><bl>key</bl><d>></d>Tags<d></</d><bl>key</bl><d>></d>
    <d><</d><bl>array</bl><d>></d>
      <d><</d><bl>string</bl><d>></d>cli<d></</d><bl>string</bl><d>></d>
    <d></</d><bl>array</bl><d>></d>
  <d></</d><bl>dict</bl><d>></d>
<d></</d><bl>plist</bl><d>></d>
```

That's the output for a plist that arrived on one line. Binary plists (`bplist00`) aren't decoded.

## Source code {#code}

Source files are shown line for line as written. Without this, a `.c` or `.py` file would fall back to Markdown, which joins lines and reads `*` and `_` as emphasis.

- Over 80 extensions are recognised: the C family, Rust, Go, Python, Ruby, JavaScript and TypeScript, CSS, JVM and .NET languages, shell scripts, SQL, diffs, and config such as `.ini` and `.conf`.
- A piped script that starts with `#!` is recognised by that line.
- `-f code`, or `-f text`, shows any input this way.
- Code is syntax-highlighted when color is on. The language comes from a source URL's supported `Content-Type`, then the extension, or from the `#!` line of a script. A leading UTF-8 BOM and `env` options or assignments do not prevent script highlighting. See [Rendering](/wiki/rendering#highlighting) for the languages. Long lines hard-wrap behind a `┆` bar, like data.

## Man pages {#man}

Man pages in roff source are laid out like any other document. mu reads the macros, not the full roff language, so it needs no `groff` or `mandoc` installed.

- Both macro sets are understood: `man` (Linux and GNU pages) and `mdoc` (BSD and macOS pages, which start with `.Dd`).
- `.TH` or `.Dt` becomes the title, such as `LS(1)`. Sections and subsections become headings and fill the [outline](/wiki/pager#outline).
- Option lists (`.TP`, `.IP`, `.Bl -tag`) become a term with its description indented under it. Bullet and numbered lists become lists. `.RS`/`.RE` indent.
- Examples (`.nf`, `.EX`, `.Bd -literal`) become code blocks, kept as written.
- Fonts (`\fB`, `.B`, `.BR`…) become bold and italic, and escapes such as `\-`, `\(em` and `\[u00E9]` become their characters. mdoc's `.Fl`, `.Ar`, `.Op`, `.Xr`, `.Nm` and friends read the way `man` shows them: `.Op Fl v` is `[-v]`.
- `.UR`/`.UE`, `.MT`/`.ME` and `.Lk` are links.
- Requests mu doesn't know are skipped, and the text after them still shows. Macro definitions (`.de`) are skipped, not run.

### Piped from man {#man-output}

`man ls | mu` works too. When `man` writes to a pipe it marks bold as `X` backspace `X` and underline as `_` backspace `X`, which shows up as junk in most tools. mu turns these back into bold and underline and keeps `man`'s own layout, line for line. Lines that are entirely bold become the outline: sections at the margin, subsections indented up to 3 columns. SGR styling, from `groff -T utf8` or `MAN_KEEP_FORMATTING=1`, is understood the same way.

Pages under `/usr/share/man` work directly: `mu /usr/share/man/man1/ls.1`. Compressed pages (`ls.1.gz`) aren't read yet, so use `gunzip -c ls.1.gz | mu`. A page that is only `.so other.1` shows "See other.1".

## Jupyter notebooks {#jupyter}

A notebook (`.ipynb`, nbformat 4) reads as one document, top to bottom:

- Markdown cells are rendered as Markdown, and their headings fill the [outline](/wiki/pager#outline). Images attached to a cell (`attachment:plot.png`) are drawn.
- Code cells show an italic `In [n]:` line with the execution count (`In [ ]:` if the cell never ran), then the code in a block labelled with the notebook's language.
- Outputs follow their cell. Printed text and results are code blocks, and `stderr` is labelled. Errors show the exception and traceback with the terminal colors removed. Markdown output is rendered.
- Plots and other PNG, JPEG or SVG outputs go through the [image pipeline](/wiki/images), so they draw as pictures, or show as `[image: output]` with `--images never`.
- HTML-only output, such as a pandas table with no text version, is shown as its HTML source.
- Other notebook versions, and files that aren't valid notebooks, are shown as JSON.

## EPUB books {#epub}

A book (`.epub`) reads as one document, in the order its spine gives:

- The book's title is the first heading, and its authors follow in italics.
- Each chapter's body goes through the [HTML presenter](#documents), so headings, lists, tables, quotes and code look as they do for HTML. Chapter headings fill the [outline](/wiki/pager#outline).
- Pictures are taken from the book and drawn like any other [image](/wiki/images), or show their alt text with `--images never`. A picture missing from the book shows its alt text. Cover pages built from an SVG `<image>` are drawn too.
- Links to other places in the book show their labels, without ZIP paths or generated anchor IDs. In the pager, `Tab` / `Shift-Tab` select them, `Enter` follows and `Backspace` returns, independently of OSC 8 and color, including `--links never`. For short books, use `--paging always`.
- Relative chapter paths resolve within readable spine chapters; fragment-only links stay in their own chapter. IDs are chapter-scoped, so the same fragment name in two chapters does not collide. Destinations track rendered output lines through reflow. Missing chapters, missing or hidden fragments and other unresolved targets remain readable but non-jumpable.
- Internal navigation uses no external opener or network access, and does not fabricate file URIs for ZIP members. Terminal-emulator mouse clicks apply to external OSC 8 links, not chapter jumps. Links that leave the book (`http`, `https`, `mailto`) follow [the normal hyperlink policy](/wiki/usage#links), including printed URL fallback.
- The zip is read with limits: at most 10,000 entries, 64 MB per entry and 256 MB in all. Zip64, encrypted and multi-disk archives are refused. A broken book fails with `mu: FILE: not a readable EPUB: <reason>` and exit code 1.

Apple Books keeps books as unpacked folders. Zip one first, `mimetype` first and stored: `cd Book.epub && zip -X0 ../book.epub mimetype && zip -Xr ../book.epub . -x mimetype`.

## PDF {#pdf}

mu has no PDF parser of its own. It runs poppler's `pdftotext -layout`, sending the PDF on stdin, so the text keeps the page's columns and spacing:

- Pages are printed as written, with long lines hard-wrapped behind the `┆` bar. Like tables, PDF uses the full terminal width rather than the 100-column cap.
- A dim `page N` rule separates pages, and each page is an outline entry, `Page N`.
- Without `pdftotext`, mu fails with a message naming the package: `brew install poppler` or `apt install poppler-utils`. A PDF `pdftotext` can't read reports its error.
- Pages are text only: no pictures, and scanned pages with no text layer come out empty.

## Tables {#tables}

CSV and TSV are shown as box-drawn tables through the same engine as Markdown tables, so cells get the same scrubbing, wrapping and borders.

- The first record is the header. Rows with fewer fields are padded with empty cells.
- A column whose non-empty cells below the header are all numbers is right-aligned. A number may have a sign, a fraction, an exponent and a trailing `%`.
- CSV quoting follows RFC 4180: a quoted field can hold the delimiter, a doubled `""` quote and line breaks. A line break inside a cell reads as a space.
- A CSV's delimiter is detected: the first of `,`, `;`, tab and `|` that splits the opening records evenly into 2 or more fields. TSV splits on every tab and has no quoting.
- A byte order mark at the start and blank lines are skipped.

```term tools.csv — mu
┌──────────────────┬──────────┬───────┬───────────────┐
│ <bold>tool</bold>             │ <bold>language</bold> │ <bold>stars</bold> │ <bold>first release</bold> │
├──────────────────┼──────────┼───────┼───────────────┤
│ mu               │ Rust     │   120 │          2026 │
│ bat, a cat clone │ Rust     │ 48000 │          2018 │
│ less             │ C        │   900 │          1984 │
└──────────────────┴──────────┴───────┴───────────────┘
```

Tables use the full terminal width rather than the 100-column reading cap. Columns shrink widest-first to fit, but never below 10 columns, or their natural width if that's less. A table that still doesn't fit runs past the right edge, and the pager [scrolls it sideways](/wiki/pager#sideways) with `h` and `l`.

## Parser coverage {#parser-coverage}

The non-Markdown parsers are hand-written and tolerant. They cover the subset that real READMEs and pages use, not the full specs.

| Format | Covered | Not covered |
|---|---|---|
| HTML | Headings, paragraphs, lists, quotes, `pre` code, tables, links, images, entities | Table cell spans |
| RST | Titles, inline markup and links, lists, definition and field lists, literal and `code-block` blocks, admonitions, simple and grid tables, images | Cell spans, line blocks, letter/roman enumerators |
| Org | Headings with TODO and tags, emphasis, links, lists and checkboxes, src/quote/example blocks, admonition blocks, tables, footnotes | More than one header row per table |
| AsciiDoc | Title and attributes, sections, inline markup and links, lists and checklists, source/listing/quote/example/sidebar blocks, admonitions, tables, images | Cell spans, footnotes |
| XML | Elements, attributes, entities, comments, declarations, doctypes, CDATA | Binary plists (`bplist00`) |
