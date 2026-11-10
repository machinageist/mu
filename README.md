# mu

**mu** is the *markup/markdown utility*: a small, fast terminal presenter for Markdown, HTML, reStructuredText, Org-mode, AsciiDoc, man pages, Jupyter notebooks, JSON, YAML, TOML, CSV, TSV and XML, and shows source code as written. It reads a file, stdin or a URL, works out the format, wraps text to the terminal width, and prints styled output: headings, lists, quotes, tables, framed code blocks and links for documents, syntax coloring for data, and box-drawn tables for CSV and TSV. Documents taller than the screen open in a built-in pager. Pictures in documents are drawn in the terminal. It is a single ~2.4 MB binary with no config file.

```sh
mu README.md
mu https://example.com/                    # HTML, with its images
mu https://api.github.com/repos/rust-lang/rust   # JSON, re-indented
mu https://github.com/machinageist/mu/blob/main/README.md   # the raw file, not the page
mu -I https://example.com/                 # response headers as a table
curl -s https://example.com/doc.md | mu    # pipes still work
mu Cargo.toml
mu -f rst < docs/index.txt
mu -w 72 notes.org
mu --images fetch page.html                # also download remote images
mu --paging never CHANGELOG.md | grep -i fix
```

## Install

```sh
cargo install --git https://github.com/machinageist/mu
# or, from a checkout
cargo install --path .
```

Requires Rust 1.85+ (edition 2024). Linux and macOS (terminal width comes from the `TIOCGWINSZ` ioctl).

## Usage

```text
mu [OPTIONS] [FILE | URL]

Reads FILE, or stdin when FILE is - or absent, or fetches an http(s) URL.

  -f, --format <NAME>  Force a format: markdown, html, rst, org, asciidoc, json, yaml, toml
      --images <MODE>  auto, fetch or never (default: auto; fetch also downloads http(s) images)
      --base <DIR|URL> Resolve relative image paths here (default: the file's folder)
      --toc            Print the document's outline (its headings) and exit
  -w, --width <N>      Wrap at N columns (default: terminal width, capped at 100)
      --color <WHEN>   auto, always or never (default: auto, honours NO_COLOR)
      --paging <WHEN>  auto, always or never (default: auto, pages when taller than the screen)
  -h, --help
  -V, --version
```

- **Format:** `-f` if given, else (for a URL) the response `Content-Type`, else the file or URL path extension, else a look at the content (this is how piped input such as `curl … | mu` is recognised), else Markdown. See [Formats](#formats).
- **Width:** the terminal width (probed on stdout, then stderr, then stdin, so piping into a pager still sees the real width), capped at 100 columns for readable line lengths. Falls back to 80 when no terminal is attached. `-w` overrides both, minimum 20.
- **Color:** on when stdout is a terminal and `NO_COLOR` is unset. Use `--color always` to keep styling when piping elsewhere.
- **Paging:** with `auto`, output that would push the prompt off screen opens in the pager; shorter output prints straight to the terminal (like `less -F`). Output to a pipe or file is never paged, whatever the setting.
- **Exit codes:** 0 success (including a closed pipe, e.g. `| head`), 1 read/write error, fetch failure or HTTP error status, 2 usage error. Run with no arguments at an interactive prompt, it prints usage instead of waiting on stdin.

## URLs

`mu <url>` fetches an `http://` or `https://` URL with the system `curl` and presents the response like a file. There is no HTTP or TLS code in mu itself; curl is run directly, without a shell, with the URL as a single argument after `--`. Redirects are followed, but only to http(s).

```text
  -H, --header <H>      Add a request header (repeatable)
  -X, --request <M>     Use another request method
  -d, --data <D>        Send data in the request body (repeatable)
  -A, --user-agent <UA> Set the User-Agent (default: mu/VERSION)
  -u, --user <U:P>      Send credentials
  -k, --insecure        Skip TLS certificate checks
  -i, --include         Show the response status and headers as a table, then the body
  -I, --head            Show the response status and headers only
      --timeout <SECS>  Give up after this long (connecting always times out after 10 s)
      --curl <ARG>      Pass ARG straight to curl (repeatable), e.g. --curl --http1.1
      --no-rewrite      Fetch the URL as typed
```

These options are a usage error without a URL. mu's `-w` is still the wrap width, not curl's write-out.

- **Format:** `-f`, else the `Content-Type` (`text/plain` and `application/octet-stream` say nothing and are skipped), else the URL path's extension, else the content, else Markdown.
- **Raw files:** a GitHub `…/blob/REF/PATH`, GitLab `…/-/blob/REF/PATH` or Gist page URL is fetched as the raw file it shows, so `mu` presents the document rather than the site's HTML around it. `--no-rewrite` turns this off.
- **Images:** a fetched page loads its remote images as if `--images fetch` were given, resolving relative paths against the final URL. Pass `--images auto` or `never` to stop that.
- **Errors:** for an HTTP status of 400 or more the body is still shown, then `mu: <url>: HTTP <code>` goes to stderr and mu exits 1. A failed connection prints curl's message and exits 1.

## Pager

Built in, no `less` needed. It takes over the screen (alternate screen, so your scrollback is untouched on exit) and reads keys from `/dev/tty`, so `curl … | mu` pages too.

| Keys | Action |
|---|---|
| `j` `↓` `Enter` / `k` `↑` | Scroll one line down / up |
| `space` `f` `PgDn` `Ctrl-F` / `b` `PgUp` `Ctrl-B` | Scroll one page down / up |
| `d` `Ctrl-D` / `u` `Ctrl-U` | Scroll half a page down / up |
| `g` `Home` `<` / `G` `End` `>` | Jump to top / bottom |
| Mouse wheel | Scroll (sent as arrow keys, so text selection still works) |
| `/` / `?` | Search forward / backward. Type the text, `Enter` to search, `Esc` to cancel. An empty search repeats the last one |
| `n` / `N` | Next / previous match, wrapping round the ends |
| `Esc` | Clear search highlighting |
| `o` | Outline: the document's headings. `j` `k` and the scroll keys move, `Enter` jumps there, `o` `q` `Esc` close |
| `h` `←` / `l` `→` | Scroll sideways by half a window, for lines wider than the screen such as wide tables |
| `q` `Ctrl-C` | Quit |

**Search** matches the text as it appears on screen, with styling removed. It is a literal match, not a regular expression, and a lowercase query ignores case while any capital letter makes it case-sensitive. Every visible match is shown in reverse video, and the current one is also bold. The status bar shows the query, the match number and count (`3/17`), and `wrapped` when a jump went round the end. A match never spans two lines.

**Outline** works for every document format, and opens on the heading of the section you are reading. `mu --toc FILE` prints the same outline without paging.

The bottom row shows the file name, the visible line range, how far through you are, and the key hints. Resizing the window re-renders the document at the new width and keeps your place proportionally; with `-w` the layout stays fixed and only the window changes. Lines wider than the window are clipped rather than wrapped.

## Formats

| Format | Extensions | Recognised in piped input by | Shown as |
|---|---|---|---|
| Markdown | `.md` `.markdown` `.mdown` `.mkd` | the fallback for anything else | document |
| HTML | `.html` `.htm` `.xhtml` | `<!doctype html>`, `<html>`, `<head>` or `<body>` at the start | document |
| reStructuredText | `.rst` `.rest` | `.. directive::` lines, `.. _target:`, over-and-underlined titles, `` `text <url>`_ `` links, `:role:` | document |
| Org-mode | `.org` | `#+KEYWORD:` or `#+BEGIN_` lines, `:PROPERTIES:`, or `*` headings with Org links | document |
| AsciiDoc | `.adoc` `.asciidoc` `.asc` | `= Title` first line, `:attr:` header lines, `[source,…]` blocks, `\|===` tables | document |
| JSON | `.json` `.jsonl` `.geojson` | starts with `{` or `[` and parses as JSON | data, re-indented when valid |
| YAML | `.yaml` `.yml` | mostly `key: value` / `- item` lines with a top-level key or `---` | data |
| TOML | `.toml` | mostly `[table]` and `key = value` lines | data |
| XML | `.xml` `.plist` `.svg` `.rss` `.atom` `.xsd` `.xsl` `.xslt` `.opml` | an `<?xml` declaration, a non-HTML `<!DOCTYPE`, or a `plist`, `svg`, `rss`, `feed`, `urlset`, `sitemapindex`, `project` or `opml` root | data, re-indented when safe |
| CSV | `.csv` | 3+ lines with the same number of `,` or `;` fields (checked last, so prose stays Markdown) | table |
| TSV | `.tsv` `.tab` | 3+ lines with the same number of tab-separated fields | table |
| Man pages | `.1`–`.9` (and `.3p`, `.1ssl`…) `.man` `.mdoc` | a `.TH` or `.Dd` first request, after comments and macro definitions | document |
| Jupyter notebook | `.ipynb` | a JSON object with `"cells"` and `"cell_type"` (checked before JSON) | document: prose, code, outputs and plots |
| Man output (`man ls \| mu`) | — | backspace overstrikes, checked first | text with bold and underline, as `man` laid it out |
| Source code | `.c` `.h` `.cpp` `.rs` `.py` `.js` `.ts` `.go` `.java` `.sh` `.sql` `.diff` and about 70 more | a `#!` first line (checked first) | text, verbatim |

Detection from content is deliberately conservative: when in doubt it picks Markdown, which shows plain text correctly too. Use `-f` when it guesses wrong.

**Documents** share one layout engine: each format is translated into the same block and inline structure as Markdown, so headings, lists, quotes, tables, code frames, links and admonitions look the same whatever the source. HTML drops `script`, `style`, `head` and similar non-content, and decodes entities. reStructuredText and AsciiDoc admonitions and Org `#+BEGIN_NOTE`-style blocks render like GitHub alerts.

**Data** is colored by token: keys, strings, numbers, booleans and null, comments, punctuation, YAML anchors and tags, and TOML table headers. Valid JSON (and NDJSON) is re-indented with two spaces; invalid JSON, YAML and TOML are shown exactly as written. Long lines hard-wrap behind a `┆` bar.

**XML** colors element names, attributes, values, entities, comments, declarations and CDATA. It is re-indented one node per line when tags balance and no element mixes text with child elements; otherwise, for example XHTML-like prose, it is shown exactly as written.

**CSV and TSV** become box-drawn tables like Markdown tables: the first row is the header, numeric columns are right-aligned, quoting follows RFC 4180, and a CSV's delimiter (`,` `;` tab or `|`) is detected. Tables use the full terminal width rather than the 100-column reading cap. Columns shrink to fit but never below 10 characters; a table that still does not fit runs past the edge, and the pager scrolls it sideways with `h` and `l`.

## Images

An image that stands alone in its paragraph or block is drawn as a picture, with its alt text as a dim caption: HTML `<img>` and `<figure>`, Markdown `![alt](src)` on its own line, RST `.. image::`, AsciiDoc `image::` and Org `[[file.png]]`. An image inside running text stays `[image: alt]`, since a picture cannot sit inside a wrapped line.

- **Aspect ratio:** mu reads the terminal's cell size in pixels and scales the image by one factor so it fits the text width and the screen height. Width and height shrink together, so the aspect ratio is exact to within half a cell. Images are never enlarged past their natural size. When a terminal does not report pixel sizes, mu assumes the common 1:2 cell.
- **Drawing:** printed straight to kitty or Ghostty, the Kitty graphics protocol is used; in iTerm2 or WezTerm, iTerm2 inline images. Everywhere else, inside the pager and under tmux, images are drawn with 24-bit-color `▀` half blocks.
- **Formats:** PNG, JPEG, GIF (first frame), WebP and SVG. SVG text is not rendered, and SVGs cannot load external files or URLs.
- **Sources:** local paths, relative to the input file (or `--base`, which piped input needs), and `data:` URIs. Remote `http(s)` images load only with `--images fetch`, which is the default when the input itself is a URL, through the system `curl` with a 10 s timeout and a 20 MB cap. A local file cannot make mu touch the network unless you ask.
- **Fallback:** with `--images never`, color off, or anything that fails to load or decode, the `[image: alt]` text is shown as before.
- **Limits:** sources over 20 MB, images over 64 megapixels or 16384 pixels on a side are refused before decoding.

## What renders

The Markdown mapping below is the reference; the other document formats map their equivalents onto it.

| Markdown | Terminal output |
|---|---|
| `#` / `##` headings | Bold, colored, underlined with `═` / `─` |
| `###`–`######` | Bold, a distinct color per level |
| `**bold**` `*italic*` `~~strike~~` | SGR bold / italic / strikethrough |
| `` `code` `` | Yellow |
| `[text](url)` | Underlined blue text, then the URL dimmed in parentheses (autolinks are not repeated) |
| `![alt](src)` | `[image: alt]` |
| Lists | `•` `◦` `▪` by depth, numbers for ordered lists; wrapped lines align under the text; tight lists stay tight |
| `- [ ]` / `- [x]` | `☐` / `☑` in place of the bullet |
| `> quote` | `│` bar, kept through blank lines and nesting |
| `> [!NOTE]` and friends | GitHub alerts: colored bar plus a Note / Tip / Important / Warning / Caution label |
| Fenced / indented code | Frame with the language on the top edge; tabs expanded; long lines hard-wrap with a `┆` continuation bar |
| GFM tables | Box-drawn, column alignment honoured; columns shrink widest-first and cells wrap to fit |
| Footnotes | `[label]` reference and a `[label]` definition block |
| `---` | Dim rule across the content width |
| YAML front matter | Dim framed block, recognised only at the very top of the file |
| Raw HTML | Dimmed as-is; comments hidden; `<br>` becomes a line break |

## Design

- **Images:** `png`, `zune-jpeg`, `gif` and `image-webp` decode raster images; `resvg` (default features off: no text, no system fonts) renders SVG. Fitting, resampling and the half-block, Kitty and iTerm2 encoders are hand-written. Decoded images are cached for the run, so a pager resize redraws without reloading.
- **Parser:** [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) with default features off and `simd` on (CommonMark plus GFM tables, strikethrough, task lists, footnotes, alerts). HTML, reStructuredText, Org and AsciiDoc have small hand-written, tolerant parsers that emit the same pulldown-cmark events, so they reuse the whole layout engine. JSON, YAML and TOML have hand-written tokenizers feeding a shared printer. The other dependencies are `unicode-width` (CJK and emoji columns) and `libc` (one ioctl).
- **Theme:** text uses only the 16 base ANSI colors plus bold, dim, italic, underline and strike, so output follows your terminal's palette instead of fighting it.
- **Untrusted input is safe to view:** control characters in the document (including ESC, BEL and C1 codes) are replaced with `�` before output, so a hostile file cannot set your title, write to the clipboard via OSC 52, or clear the screen. The only escapes mu emits are its own: SGR styling and, for images, 24-bit colors and Kitty or iTerm2 image sequences whose payload mu encodes itself.
- **Pipeline:** events → per-block buffer of styled word atoms in one text arena → word wrap → container gutters (quote bars, list indents) prepended per line → one buffered write, or the pager. Styles are emitted lazily and every line closes its own style, so the pager can draw any window of lines on its own.
- **Pager terminal handling:** raw mode via `termios`, a self-pipe for `SIGWINCH` so a resize can never be missed between a redraw and the next `select()` (not `poll()`, which macOS rejects for `/dev/tty`), synchronized-output frames to avoid flicker, and the terminal restored on exit, on error and from a panic hook. Image decoding runs under `catch_unwind`, so a bug in a third-party decoder shows the alt text instead of crashing.

```text
src/
  main.rs    CLI: args, input, width/color/paging decisions, single write
  pager/     built-in pager: raw mode, keys, scrolling, search, outline, resize, status bar
  term.rs    terminal size via TIOCGWINSZ
  lib.rs     public API: render, present, present_outlined, outline, detect, sniff, Format, Options
  format/    Format enum, detection, dispatch; html, rst, org, asciidoc, man, csv → events; jupyter → Markdown
  data/      styled-line printer; json, yaml, toml, xml highlighters
  image/     sources, codecs, svg, fit and encoders (half block, Kitty, iTerm2)
  render.rs  event walker: blocks, containers, lists, code frames
  inline.rs  styled word atoms, control-char scrubbing, word wrap
  table.rs   column fitting and box drawing
  style.rs   SGR styles and the color theme
```

## Performance

Measured 2026-11-09 on a 12-core Linux box under background load, so treat these as rough:

| Input | Time |
|---|---|
| 7 KB README | ≈2 ms per run over a `cat` baseline (process start dominates); `bat` on the same file ≈120 ms |
| 9 MB corpus (909 real crate READMEs and CHANGELOGs) | ≈0.25–0.35 s, ≈26–40 MB/s |
| Parser alone on the same corpus | ≈60–100 MB/s |
| 5.7 MB minified JSON, re-indented and colored (Apple Silicon Mac) | ≈0.3 s |

Layout roughly doubles the parse cost. There is headroom there if throughput on multi-megabyte files ever matters; for normal documents the run time is process start.

## Development

```sh
cargo test --release
cargo clippy --all-targets
cargo run --release -- -w 60 tests/fixtures/kitchen-sink.md
```

`tests/render.rs` and one test file per format (`tests/html.rs`, `tests/json.rs`, …) have exact-output cases for each construct plus invariants swept across widths 30–120: no line exceeds the width, color output with SGR stripped equals plain output, every styled line resets before its newline, and no control character from the input reaches the output. `src/pager/` unit-tests key decoding, scrolling, search, the outline, resize behaviour and the status bar, and `tests/pager_pty.rs` drives the real pager in a pseudo-terminal: resizing, search, the outline and sideways scrolling. `tests/sniff.rs` sniffs every fixture against every format so one format's rule cannot quietly claim another's files.

## Versioning

Semantic Versioning, with every change recorded in `CHANGELOG.md`. Release and commit rules are in `CONTRIBUTING.md`.

The user guide lives in `docs/wiki/`, one Markdown page per wiki page, and changes in the same commit as the feature it describes. `scripts/check-docs.sh` checks that before a release.

## Known limits

- No syntax highlighting in code blocks (deliberate: it would cost startup time and binary size). Whole JSON, YAML and TOML files are colored; code blocks inside documents are not.
- Non-Markdown parsers cover the subset real READMEs and pages use, not the full specs: no table cell spans (HTML, RST, AsciiDoc), no RST line blocks or letter/roman enumerators, no AsciiDoc footnotes, one header row per Org table.
- YAML and TOML are colored, never reformatted. JSON nested deeper than 256 levels is colored as written rather than re-indented.
- Content detection reads the first 8 KB. Short or unusual inputs may be shown as Markdown; use `-f`.
- Images: no EXIF orientation for JPEGs, no animated GIFs, no SVG text, and no images inline within a line of text.
- URLs: no caching between runs, and charsets other than UTF-8 are shown with replacement characters. A repository's front page is fetched as HTML; point at the README's blob URL instead.
- Links are not clickable (no OSC 8 hyperlinks); the URL is printed instead.
- In ordered lists that cross into two-digit numbers, items 10+ indent their wrapped lines one column further than items 1–9.
- A table with too many columns to fit at 3 columns each (10 for CSV and TSV) overflows the width rather than becoming unreadable; the pager scrolls it sideways.
- Binary plists (`bplist00`) are not decoded.
- Very deep nesting stops shrinking content below 16 columns and overflows instead.
- Definition lists, math and wiki links are not enabled; they render as plain text.

## License

MIT, see `LICENSE`.
