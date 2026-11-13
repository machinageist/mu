# mu — feature spec

mu is short for *markup/markdown utility*.

Long-lived behaviour and invariants. Update this when behaviour ships.

## Purpose

A terminal presenter that turns markup and data into readable, styled, wrapped text so the reader never has to leave the terminal or parse raw syntax by eye. Small, fast, few dependencies, no config file.

## Inputs

- A file path, or stdin when the path is `-` or absent (`cat x | mu`, `curl … | mu`, `mu < x`)
- An `http://` or `https://` URL, fetched with the system `curl` (see URLs)
- Bare `mu` at an interactive prompt prints usage instead of waiting on stdin
- Input is read as bytes. EPUB and PDF are converted from bytes once at load time; every other format is decoded as UTF-8 with invalid bytes replaced

## Formats

| Format | Kind | Extensions |
|---|---|---|
| Markdown (CommonMark + GFM) | document | `.md` `.markdown` `.mdown` `.mkd` |
| HTML | document | `.html` `.htm` `.xhtml` |
| reStructuredText | document | `.rst` `.rest` |
| Org-mode | document | `.org` |
| AsciiDoc | document | `.adoc` `.asciidoc` `.asc` |
| JSON | data | `.json` `.jsonl` `.geojson` |
| YAML | data | `.yaml` `.yml` |
| TOML | data | `.toml` |
| XML | data | `.xml` `.plist` `.xsd` `.xsl` `.xslt` `.rss` `.atom` `.svg` `.opml` |
| CSV | table | `.csv` |
| TSV | table | `.tsv` `.tab` |
| Man pages (roff, `man` and `mdoc` macros) | document | `.1`–`.9` with optional letters, `.man`, `.mdoc` |
| Jupyter notebooks (nbformat 4) | document, built as Markdown | `.ipynb` |
| EPUB books | document, built as HTML | `.epub` |
| PDF, through poppler's `pdftotext` | text as laid out on the page, one outline entry per page | `.pdf` |
| Formatted man output (`man ls \| mu`) | text, overstrikes as bold and underline | sniffed only |
| Source code | verbatim text | `.c` `.h` `.cpp` `.rs` `.py` `.js` `.go` `.sh` and about 80 more; `-f code` or `-f text` for anything |

Document formats share one layout engine. Each one gets the same headings, lists, quotes, tables, code frames, links and pager. Data formats are syntax-highlighted. Source code is shown line for line as written, with no Markdown interpretation, and long lines hard-wrap like data. Source files and fenced code blocks are syntax-highlighted by language when color is on (extension, `#!` line or info string); highlighting only changes styles, so color output with styling removed equals plain output. Valid JSON is also re-indented, and XML is re-indented when its tags balance and no element mixes text with child elements.

- Source URL highlighting follows a supported source `Content-Type`, then the path extension, then the shebang. A shebang may follow a UTF-8 BOM and use `env` options or assignments. Unlabelled document code blocks also use a shebang; an explicitly unknown block language stays plain.
- Lexers use per-language block-comment nesting and hash-comment boundaries; quoted strings in preprocessor directives do not start comments. Known prefixed constants, such as PowerShell booleans and Scheme `#t`/`#f`, retain the constant style without consuming longer variable or identifier names.
- HTML and EPUB code blocks accept `language-`/`lang-` classes and `data-language`/`data-lang` hints on block ancestors, `pre`, or nested `code`; bare known language classes work on `pre` and `code`. A nested `code` hint takes precedence over its surrounding blocks. AsciiDoc source languages accept paired quotes and existing attribute substitutions.
- HTML headings omit decorative fragment permalink icons and URLs; meaningful fragment-linked labels retain their inline formatting. External heading links and ordinary paragraph links follow the hyperlink policy below, retaining printed destinations when OSC 8 is disabled or the target is ineligible. Missing or mismatched heading tags recover locally, and hidden content does not suppress the document-title fallback.

Tables (CSV and TSV) go through the same table engine as Markdown tables:
- The first record is the header, and numeric columns are right-aligned.
- Quoting follows RFC 4180, and a CSV's delimiter is detected.
- With an automatic width they use the full terminal width.
- Columns shrink no narrower than 10 (or their natural width). Beyond that the table overflows, which the pager scrolls sideways.

EPUB books are read in-process:
- `META-INF/container.xml` names the package, whose spine gives the reading order. The package's title and creators head the document.
- Each XHTML spine item's body is joined into one HTML document for the HTML presenter. Namespace aliases are resolved in chapter scope before extracting the body; foreign names do not become HTML links/images, and hidden/raw subtrees stay inert across namespace boundaries. Recovered unclosed elements are closed within their chapter before joining, so malformed markup cannot swallow a later chapter's boundary. Other or missing items are skipped.
- CDATA remains literal text in chapters and package metadata. Processing instructions and complete declarations, including quoted/internal-subset content, are not rendered as HTML actions. Hidden elements expose neither text nor destinations.
- Images (`img src`, SVG `image href`) resolve against the chapter's path in the zip and are inlined as `data:` URIs; a missing image becomes literal alt text, never newly interpreted markup or actions.
- Relative chapter paths and fragments resolve within readable spine chapters. Chapter-top anchors, element IDs and legacy named anchors are scoped by chapter, so repeated fragment IDs in different chapters cannot collide. URI path/fragment escapes are decoded during resolution; paths cannot escape the book's root. External HTTP(S)/mailto links retain their destinations and use the normal hyperlink policy.
- Internal link labels are shown without ZIP paths or generated anchor IDs. Only targets that resolve to rendered anchors in their own chapter are jumpable; empty chapters, trailing anchors without a visible destination, missing or hidden fragments and unsupported/ambiguous targets remain readable, without borrowing a following chapter's output. The EPUB is not exposed as invented local file URIs, and internal navigation never opens a file, runs an external opener or touches the network.
- Zip limits: 10,000 entries, 64 MB per entry, 256 MB in all, sizes checked against the directory. Zip64, encryption and multi-disk archives are refused. Failures read `mu: FILE: not a readable EPUB: <reason>`, exit 1.

PDF goes through `pdftotext -layout -enc UTF-8 - -`, with no shell:
- Pages split on form feeds, trailing blank lines dropped, printed through the data printer at the full terminal width, with a dim `page N` rule between pages and a `Page N` outline entry each.
- Without `pdftotext` the error names poppler and how to install it, exit 1. A `pdftotext` failure reports its stderr.

## Format selection, first match wins

1. `-f/--format <name>` flag
2. For a URL, the response `Content-Type`. `text/plain` and `application/octet-stream` are skipped
3. File extension (case-insensitive), or for a URL the extension of its path, never its host
4. Magic bytes for the binary formats: `%PDF-` in the first 1 KB is PDF, and a zip whose first entry is `mimetype` holding `application/epub+zip` is EPUB
5. Content sniffing on the first 8 KB: used for stdin and for unknown or missing extensions. The order is formatted man output (overstrikes), a `#!` shebang (source code), Jupyter, JSON, man page, HTML, XML, Org, RST, AsciiDoc, TOML, YAML, then CSV/TSV, and each rule is conservative
6. Fallback: Markdown, which shows plain text correctly too

`tests/sniff.rs` pins every fixture to its format. A sniffing change must keep it green.

## Links

- `--links auto|always|never`, default `auto`, controls external OSC 8 hyperlinks, separately from SGR color and images. `never` emits no OSC 8. `always` explicitly enables them even with `--color never`, nonempty `NO_COLOR`, non-terminal stdout or an unknown environment.
- Auto requires terminal stdout, resolved color enabled and a recognized environment: exact `TERM` values `xterm-kitty` or `xterm-ghostty`; exact `TERM_PROGRAM` values `iTerm.app`, `WezTerm` or `ghostty`; or a nonempty digits-only `VTE_VERSION` parsing as `u32` and at least `5000`. A set `TMUX` or `STY`, `TERM=dumb`, or a `TERM` beginning with `screen` or `tmux` disables auto regardless of other markers. Unknown environments fall back. This is a conservative heuristic: no capability handshake, terminal queries or detector subprocesses.
- Only semantic document links with validated absolute `http`, `https` or `mailto` destinations can emit OSC 8. A usable hyperlink shows its label without a redundant printed `(URL)` suffix. Otherwise retain the existing printed destination and non-repeated autolinks. An empty external link label outside HTML/EPUB headings still exposes its destination; empty heading anchors remain invisible rather than creating outline noise. Existing HTML heading-permalink cleanup and EPUB internal-label rendering remain exceptions to printed fragment URLs.
- Relative paths, local files, fragments outside EPUB and unsupported schemes are not actionable. Links do not inherit `--base` or image resolution. URLs merely appearing in source files, code blocks or data are not auto-linkified.
- Validate before encoding: literal controls (including ESC, BEL, C1 and line breaks), backslashes, unsafe directional characters, malformed percent escapes and invalid/unsupported destinations are rejected, not stripped into different actionable URIs. Rejection preserves readable, scrubbed fallback text and does not fail the document or trigger fetching, execution or file opening.
- Adapters preserve the source scheme for semantic links; local-file prefixes must not be removed to manufacture actionable HTTP(S)/mailto links. Targets containing a replacement character are rejected as ambiguous after parser or UTF-8 repair rather than activated under a changed spelling.
- Encode Unicode and unsafe URI bytes as percent-encoded UTF-8 to produce a portable ASCII URI. The encoded URI payload is at most 8192 bytes. OSC 8 uses the ESC-backslash string terminator, not BEL, and closes before newlines, padding, gutters, borders, status bars and pager exit. Wrapping, clipping and inline styles preserve the destination only on its label.

## Library metadata

- The library never detects terminal capabilities or reads the environment for hyperlink policy. `Options.links: bool` is explicit and defaults to `false`, including in `Options::new(width, color)`; it is independent of `Options.color`.
- `render` and `present` return text. `present_outlined(input, format, &options)` returns `Rendered { text, headings, anchors, links }`.
- `Anchor { id: String, line: usize, column: usize }` identifies an internal destination, and `NavigationLink { target: String, label: String, line: usize, column: usize }` identifies its source label. Positions use zero-based rendered output lines and display columns, not source lines or byte offsets. Metadata is recomputed at each render width and kept separate from terminal escape bytes; external OSC 8 destinations are not navigation entries.
- For an EPUB, call `extract(bytes, Format::Epub)` and pass the returned HTML to `present_outlined` with `Format::Epub` to retain chapter navigation metadata. Data and source formats have no internal navigation metadata.

## Invariants, all formats

- No output line is wider than the wrap width, except the documented overflow cases
- At a fixed hyperlink policy, stripping SGR from color output gives exactly the plain output. Changing hyperlink policy may change visible layout by removing or restoring printed URL suffixes
- Every styled line resets its style and closes its hyperlink before its newline, so the pager can draw any window of lines
- No control character from the input reaches the output. The only escapes mu writes are its own: SGR codes, validated and encoded OSC 8 hyperlinks, 24-bit colors for half-block images, and Kitty or iTerm2 image sequences with payloads mu encodes
- Malformed input never panics or errors. It renders on a best-effort basis
- A closed pipe is a success. Exit codes: 0 ok, 1 read/write error, fetch failure or HTTP status of 400 or more, 2 usage error

## Pager

- Pages only when stdout is a terminal. It reads keys from `/dev/tty` and waits on it with `select()`, because macOS `poll()` rejects `/dev/tty`
- A resize re-renders at the new width when the width is automatic, keeping the reader's place proportionally. Any search and the outline follow the new layout
- **Search** (`/` `?` `n` `N` `Esc`):
  - Literal substrings with smart case, matched against each line with SGR and OSC 8 controls removed. Hidden destination bytes are not searchable. A match never spans two lines
  - `n` and `N` wrap round the ends. A match already on screen does not scroll the view
  - Highlighting only inserts SGR. With styling removed, a highlighted line equals the original, and it still resets before its newline
- **Sideways** (`h` `l` `←` `→`): half a window per step, limited by the widest line. Lines are cut by display column with their SGR and OSC 8 state kept, and a wide character cut in half becomes spaces. The status bar shows `col N` while scrolled. A search jump scrolls to its match, and an outline jump returns to column 1
- **Outline** (`o`):
  - The renderer records each heading's level, plain text and first output line. Plain labels come from semantic text, not wrapped lines, so resizing never inserts spaces inside long words
  - The list opens on the section holding the top line, and `Enter` puts the heading's line at the top, clamped at the end of the document
  - `--toc` prints the same outline, indented two spaces per level below the shallowest. Data formats have no headings
- **EPUB chapter links** (`Tab` / `Shift-Tab`, `Enter`, `Backspace`):
  - Cycle forward/backward through resolvable internal links in document order, scrolling the selected source label into view vertically and horizontally and showing its label with `Enter follow` in the status bar. Following a link also reveals its destination column in an overflowing table
  - `Enter` follows the selected internal link; without a selection it still scrolls one line. `Backspace` returns through internal-jump history; in the search prompt it retains its editing behavior
  - Targets are chapter-scoped anchors mapped to rendered lines, recomputed on resize/reflow. Only resolvable targets enter selection; missing or hidden targets cannot jump to an unrelated chapter
  - Works independently of OSC 8 and SGR color, including `--links never` and `--color never`. Terminal-emulator mouse gestures apply to external OSC 8 links, not these in-pager jumps. No external opener or network access is used


- An image alone in its paragraph or block is drawn. Inline images, images inside active external/internal links, failures, color off and `--images never` all show `[image: alt]`; linked alt text retains its destination
- The aspect ratio is exact to within half a cell, using the terminal's reported cell pixels or an assumed 1:2 cell. Images are never enlarged, never wider than the content column and never taller than the screen minus 2 rows
- No image network access unless `--images fetch`, which a URL input implies unless `--images` is given. Remote loads go through the system `curl`, limited to http(s), including on redirect
- Decode limits: 20 MB per source, 64 megapixels, 16384 px per side
- The pager always uses half blocks. Native protocols are used only when writing straight to a supporting terminal outside tmux

## URLs

- An argument starting with `http://` or `https://` (any case) is a URL. Anything else is a path
- Fetched with the system `curl`, no shell, the URL one argument after `--`. Protocols are limited to http(s), including on redirect. Redirects are followed. Connect timeout 10 s
- URL options (`-H -X -d -A -u -k -i -I --timeout --curl --no-rewrite`) are a usage error without a URL. `--curl` arguments pass to curl verbatim
- GitHub `blob`, GitLab `-/blob` and Gist page URLs are fetched as raw files unless `--no-rewrite`. The query and fragment are dropped
- `-i` shows the final response's status and headers as a table before the body. `-I` shows only that table. Redirect, 1xx and proxy-tunnel header blocks are never shown
- Status 400 or more: the body is rendered, then `mu: <url>: HTTP <code>` on stderr, exit 1. Transport failure: curl's message, exit 1
- The pager title is the URL as typed. A resize re-renders without fetching again
