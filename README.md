# mu

**mu** is the *markup/markdown utility*: a small, fast terminal presenter for Markdown, HTML, reStructuredText, Org-mode, AsciiDoc, JSON, YAML and TOML. It reads a file or stdin, works out the format, wraps text to the terminal width, and prints styled output: headings, lists, quotes, tables, framed code blocks and links for documents, and syntax coloring for data. Documents taller than the screen open in a built-in pager. Pictures in documents are drawn in the terminal. It is a single ~2.0 MB binary with no config file.

```sh
mu README.md
curl -s https://example.com/doc.md | mu
curl -s https://example.com/ | mu          # HTML, detected from the content
curl -s https://api.github.com/repos/rust-lang/rust | mu   # JSON, re-indented
mu Cargo.toml
mu -f rst < docs/index.txt
mu -w 72 notes.org
mu --images fetch page.html                # also download remote images
mu --paging never CHANGELOG.md | grep -i fix
```

## Install

```sh
cargo install --path .
```

Requires Rust 1.85+ (edition 2024). Linux and macOS (terminal width comes from the `TIOCGWINSZ` ioctl).

## Usage

```text
mu [OPTIONS] [FILE]

Reads FILE, or stdin when FILE is - or absent.

  -f, --format <NAME>  Force a format: markdown, html, rst, org, asciidoc, json, yaml, toml
      --images <MODE>  auto, fetch or never (default: auto; fetch also downloads http(s) images)
      --base <DIR|URL> Resolve relative image paths here (default: the file's folder)
  -w, --width <N>      Wrap at N columns (default: terminal width, capped at 100)
      --color <WHEN>   auto, always or never (default: auto, honours NO_COLOR)
      --paging <WHEN>  auto, always or never (default: auto, pages when taller than the screen)
  -h, --help
  -V, --version
```

- **Format:** `-f` if given, else the file extension, else a look at the content (this is how piped input such as `curl … | mu` is recognised), else Markdown. See [Formats](#formats).
- **Width:** the terminal width (probed on stdout, then stderr, then stdin, so piping into a pager still sees the real width), capped at 100 columns for readable line lengths. Falls back to 80 when no terminal is attached. `-w` overrides both, minimum 20.
- **Color:** on when stdout is a terminal and `NO_COLOR` is unset. Use `--color always` to keep styling when piping elsewhere.
- **Paging:** with `auto`, output that would push the prompt off screen opens in the pager; shorter output prints straight to the terminal (like `less -F`). Output to a pipe or file is never paged, whatever the setting.
- **Exit codes:** 0 success (including a closed pipe, e.g. `| head`), 1 read/write error, 2 usage error. Run with no arguments at an interactive prompt, it prints usage instead of waiting on stdin.

## Pager

Built in, no `less` needed. It takes over the screen (alternate screen, so your scrollback is untouched on exit) and reads keys from `/dev/tty`, so `curl … | mu` pages too.

| Keys | Action |
|---|---|
| `j` `↓` `Enter` / `k` `↑` | Scroll one line down / up |
| `space` `f` `PgDn` `Ctrl-F` / `b` `PgUp` `Ctrl-B` | Scroll one page down / up |
| `d` `Ctrl-D` / `u` `Ctrl-U` | Scroll half a page down / up |
| `g` `Home` `<` / `G` `End` `>` | Jump to top / bottom |
| Mouse wheel | Scroll (sent as arrow keys, so text selection still works) |
| `q` `Ctrl-C` | Quit |

The bottom row shows the file name, the visible line range, how far through you are, and a quit hint. Resizing the window re-renders the document at the new width and keeps your place proportionally; with `-w` the layout stays fixed and only the window changes. Lines wider than the window are clipped rather than wrapped.

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

Detection from content is deliberately conservative: when in doubt it picks Markdown, which shows plain text correctly too. Use `-f` when it guesses wrong.

**Documents** share one layout engine: each format is translated into the same block and inline structure as Markdown, so headings, lists, quotes, tables, code frames, links and admonitions look the same whatever the source. HTML drops `script`, `style`, `head` and similar non-content, and decodes entities. reStructuredText and AsciiDoc admonitions and Org `#+BEGIN_NOTE`-style blocks render like GitHub alerts.

**Data** is colored by token: keys, strings, numbers, booleans and null, comments, punctuation, YAML anchors and tags, and TOML table headers. Valid JSON (and NDJSON) is re-indented with two spaces; invalid JSON, YAML and TOML are shown exactly as written. Long lines hard-wrap behind a `┆` bar.

## Images

An image that stands alone in its paragraph or block is drawn as a picture, with its alt text as a dim caption: HTML `<img>` and `<figure>`, Markdown `![alt](src)` on its own line, RST `.. image::`, AsciiDoc `image::` and Org `[[file.png]]`. An image inside running text stays `[image: alt]`, since a picture cannot sit inside a wrapped line.

- **Aspect ratio:** mu reads the terminal's cell size in pixels and scales the image by one factor so it fits the text width and the screen height. Width and height shrink together, so the aspect ratio is exact to within half a cell. Images are never enlarged past their natural size. When a terminal does not report pixel sizes, mu assumes the common 1:2 cell.
- **Drawing:** printed straight to kitty or Ghostty, the Kitty graphics protocol is used; in iTerm2 or WezTerm, iTerm2 inline images. Everywhere else, inside the pager and under tmux, images are drawn with 24-bit-color `▀` half blocks.
- **Formats:** PNG, JPEG, GIF (first frame), WebP and SVG. SVG text is not rendered, and SVGs cannot load external files or URLs.
- **Sources:** local paths, relative to the input file (or `--base`, which piped input needs), and `data:` URIs. Remote `http(s)` images load only with `--images fetch`, through the system `curl` with a 10 s timeout and a 20 MB cap. A page cannot make mu touch the network unless you ask.
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
- **Pager terminal handling:** raw mode via `termios`, a self-pipe for `SIGWINCH` so a resize can never be missed between a redraw and the next `poll()`, synchronized-output frames to avoid flicker, and the terminal restored on exit, on error and from a panic hook. Image decoding runs under `catch_unwind`, so a bug in a third-party decoder shows the alt text instead of crashing.

```text
src/
  main.rs    CLI: args, input, width/color/paging decisions, single write
  pager.rs   built-in pager: raw mode, keys, scrolling, resize, status bar
  term.rs    terminal size via TIOCGWINSZ
  lib.rs     public API: render, present, detect, sniff, Format, Options
  format/    Format enum, detection, dispatch; html, rst, org, asciidoc → events
  data/      styled-line printer; json, yaml, toml highlighters
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

`tests/render.rs` and one test file per format (`tests/html.rs`, `tests/json.rs`, …) have exact-output cases for each construct plus invariants swept across widths 30–120: no line exceeds the width, color output with SGR stripped equals plain output, every styled line resets before its newline, and no control character from the input reaches the output. `src/pager.rs` unit-tests key decoding, scroll clamping, resize behaviour and the status bar; the pager's terminal I/O was checked by hand in a pseudo-terminal. `tests/sniff.rs` sniffs every fixture against every format so one format's rule cannot quietly claim another's files.

## Versioning

Semantic Versioning, with every change recorded in `CHANGELOG.md`. Release and commit rules are in `CONTRIBUTING.md`.

## Known limits

- No syntax highlighting in code blocks (deliberate: it would cost startup time and binary size). Whole JSON, YAML and TOML files are colored; code blocks inside documents are not.
- Non-Markdown parsers cover the subset real READMEs and pages use, not the full specs: no table cell spans (HTML, RST, AsciiDoc), no RST line blocks or letter/roman enumerators, no AsciiDoc footnotes, one header row per Org table.
- YAML and TOML are colored, never reformatted. JSON nested deeper than 256 levels is colored as written rather than re-indented.
- Content detection reads the first 8 KB. Short or unusual inputs may be shown as Markdown; use `-f`.
- The pager has no search (`/`) yet.
- Images: no EXIF orientation for JPEGs, no animated GIFs, no SVG text, and no images inline within a line of text.
- Links are not clickable (no OSC 8 hyperlinks); the URL is printed instead.
- In ordered lists that cross into two-digit numbers, items 10+ indent their wrapped lines one column further than items 1–9.
- A table with too many columns to fit at 3 columns each overflows the width rather than becoming unreadable.
- Very deep nesting stops shrinking content below 16 columns and overflows instead.
- Definition lists, math and wiki links are not enabled; they render as plain text.

## License

MIT, see `LICENSE`.
