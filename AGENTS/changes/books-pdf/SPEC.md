# Change: EPUB and PDF

Ships as **0.8.0** with syntax highlighting (`AGENTS/changes/highlighting/`). Decided with machinageist on 2026-11-09: EPUB is read in-process, and PDF goes through poppler's `pdftotext` when it is installed.

## Outcome

- `mu book.epub` reads a whole book as one document: chapters in reading order, images drawn, and chapter headings in the outline.
- `mu paper.pdf` shows a PDF's text laid out as on the page, with a rule and an outline entry for each page.
- Both work from a file, from stdin, and from a URL.

## Formats

| Format | Names for `-f` | Extensions | Media types | Magic bytes |
|---|---|---|---|---|
| EPUB | `epub` | `.epub` | `application/epub+zip` | a zip whose first entry is `mimetype` holding `application/epub+zip` |
| PDF | `pdf` | `.pdf` | `application/pdf` | `%PDF-` within the first 1024 bytes |

## Bytes before text

- **Reading:** input is read as bytes. Magic bytes are checked after `-f`, the media type and the extension, and before content sniffing.
- **EPUB and PDF** are converted from their bytes once, at load time.
- **Every other format** is decoded as before, with invalid UTF-8 replaced.
- **Library:**
  - New `Format::Epub` and `Format::Pdf`, plus `Format::is_binary()`.
  - New `mu::extract(bytes, format) -> Result<String, String>` turns an EPUB into HTML or a PDF into page text.
  - New `mu::sniff_bytes(bytes) -> Option<Format>`.
  - `present` on these formats takes the extracted text.

## EPUB

- **Zip reader:**
  - Built in. Entries are found through the central directory, stored or deflated, and inflated with `miniz_oxide`, which `png` already pulls in.
  - Zip64, encryption and multi-disk archives are refused with an error.
- **Limits:**
  - at most 10,000 entries
  - each entry at most 64 MB uncompressed
  - all entries together at most 256 MB
  - an entry whose inflated size differs from what the directory declares is an error
- **Package:**
  - `META-INF/container.xml` names the package document, whose spine gives the reading order.
  - The package's `dc:title` and `dc:creator` head the document as a title and a byline.
- **Chapters:**
  - Each spine item's `<body>` is joined into one HTML document, rendered by the HTML presenter. Headings, lists, tables, links and code all work as for HTML, and headings fill the outline.
  - Items that are not XHTML are skipped, and so are missing items.
- **Images:** `<img src>` and SVG `<image href>`/`<image xlink:href>` are resolved against the chapter's path inside the zip and inlined as `data:` URIs, so they draw through the image pipeline. An image missing from the book shows its alt text.
- **Paths:** `../` segments and percent-escapes in hrefs are resolved, and a `#fragment` is dropped.
- **Errors:** a file that isn't a zip, has no package document or has an empty spine fails with `mu: FILE: not a readable EPUB: <reason>` and exit code 1.

## PDF

- **Extraction:** runs `pdftotext -layout -enc UTF-8 - -`, with the PDF on stdin and the text read from stdout. No shell is involved.
- **Errors:**
  - Without `pdftotext`, mu fails with "reading PDF needs pdftotext from poppler: `brew install poppler` or `apt install poppler-utils`", exit 1.
  - A failure from `pdftotext` is reported with its stderr.
- **Output:**
  - Pages are split on form feeds and printed as written, through the data printer, so long lines hard-wrap behind the `┆` bar. The auto width is not capped, since the layout came from the page.
  - Between pages there is a dim rule labelled `page N`.
  - Trailing blank lines on each page are dropped.
- **Outline:** one entry per page, `Page N`, at the line where the page starts.

## Acceptance criteria

- **Zip:** unit tests cover stored and deflated entries, the central directory, the size limits, and corrupt input (truncated, bad sizes, Zip64).
- **EPUB:** a small fixture EPUB, built by the test from parts, has two chapters, a nested `../` image path, a missing image and metadata. Tests check exact output with images off, an outline of chapter headings, and that the image becomes a `data:` URI.
- **PDF:**
  - The page splitting and outline are tested on text with form feeds.
  - An integration test makes a PDF with a minimal hand-written fixture and runs `pdftotext`. It is skipped when `pdftotext` is not installed.
  - The missing-tool error is tested by running with an empty `PATH`.
- **Detection:** magic bytes, extensions, media types and `-f` names resolve. `tests/sniff.rs` is unchanged, since these formats are binary.
- **CLI:** `mu book.epub` and `cat book.epub | mu` match. A PDF over a URL works through the test server.
- **Unchanged:** all existing fixtures are byte-identical.

## Out of scope

- PDF without `pdftotext`: no built-in parser
- Reflowing PDF text, PDF images, and PDF outlines and bookmarks
- EPUB CSS, footnote popups, fixed-layout EPUBs, and DRM
- Gzipped man pages, though the inflate code now exists. A follow-up
