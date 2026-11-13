---
title: Architecture
group: Internals
order: 1
summary: Pipeline, source layout and library API
---

About 14,400 lines of Rust in one crate: a thin CLI around a library that turns text into styled terminal lines.

## Pipeline {#pipeline}

There are two paths through mu. **Documents** become one event stream that a single layout engine lays out. CSV and TSV **tables** join that path as table events. **Data** becomes styled lines for a simpler printer. Both paths produce terminal lines carrying their own SGR styling; semantic document links can also carry validated OSC 8 controls. Text prints in one write or opens in the pager, alongside heading and internal EPUB navigation metadata.

```mermaid
flowchart LR
  IN(["File or stdin"]) --> DET{"Which format?"}
  URL(["http(s) URL"]) --> FE["curl"] --> DET
  DET -->|"Markdown"| PC["pulldown-cmark"]
  DET -->|"HTML, RST, Org, AsciiDoc"| FP["hand-written parsers"]
  DET -->|"CSV, TSV"| CS["csv::events<br/>header row and alignment"]
  PC --> EV["Event stream"]
  FP --> EV
  CS --> EV
  EV --> RE["render_events / render_annotated_events<br/>shared layout engine"]
  DET -->|"EPUB, after extract"| EP["HTML annotated events<br/>chapter-scoped anchors and links"]
  EP --> RE
  DET -->|"JSON, YAML, TOML, XML,<br/>source code"| HL["highlighters"]
  HL --> DP["data::print"]
  RE --> OUT["Terminal lines, headings<br/>and EPUB navigation metadata"]
  DP --> OUT
  OUT --> PG{"Taller than<br/>the screen?"}
  PG -->|"yes"| PAGER["Pager"]
  PG -->|"no"| W(["One write"])
```

Inside the document path the stages are: events → a per-block buffer of styled word atoms in one text arena → word wrap → container gutters (quote bars, list indents) prepended per line → one buffered write, or the pager. Styles are emitted lazily and every line closes its own style and hyperlink, so the pager can draw any window of lines on its own. The renderer also records each heading's level, plain text and first output line, which the pager's outline and `--toc` use.

The CLI resolves external hyperlink policy separately from SGR color and images. The library receives only explicit `Options.links`; it does not inspect the terminal environment. `hyperlink::destination` validates absolute HTTP(S)/mailto targets and encodes an ASCII URI bounded to 8192 bytes before inline atoms can emit OSC 8. A usable label omits its URL suffix; disabled or rejected targets retain the printed fallback. Source/code/data URLs are not auto-linkified.

EPUB extraction rewrites relative chapter paths, fragments and IDs into chapter-scoped anchor identities within the book. `Format::Epub` uses annotated HTML events; the renderer maps anchors and internal link labels to actual output lines, separately from terminal controls. The pager refreshes that metadata on reflow, selects links with `Tab` / `Shift-Tab`, follows with `Enter` and returns with `Backspace`. Only resolved rendered targets are selectable. This path works with OSC 8 and color disabled and uses no external opener, network request or ZIP-member file URI.

CSV and TSV records become the same table events a Markdown table produces, so they share the table engine. Two settings differ: the width isn't capped at 100, and columns stop shrinking at 10 instead of 3.

The data path has highlighters that only classify text into styled spans. A shared printer owns everything that touches the terminal: tab expansion, control-character scrubbing, width and SGR. Every data format therefore keeps the same safety and layout guarantees as documents.

For the detailed flow of each stage, see the [logic maps](/wiki/logic-maps).

## Source layout {#source-layout}

```text
src/
  main.rs     CLI: args, input, width/color/links/paging decisions, --toc, single write
  fetch.rs    URL input: curl arguments, raw-file rewrites, response parsing
  pager/      built-in pager: raw mode, keys, scrolling, search, outline, chapter jumps, sideways, resize, status bar
  term.rs     terminal size via TIOCGWINSZ
  lib.rs      public API: render, present, present_outlined, outline, detect, sniff, sniff_bytes, extract, Format, Options
  outline.rs  heading list shared by --toc and the pager
  format/     Format enum, detection, dispatch; html, rst, org, asciidoc, man, csv → events; jupyter → Markdown
  book/       binary formats: zip reader, EPUB → HTML, PDF → page text through pdftotext
  data/       styled-line printer; json, yaml, toml, xml highlighters; verbatim code lines
  image/      sources, codecs, svg, fit and encoders (half block, Kitty, iTerm2)
  render.rs   event walker: blocks, containers, lists, code frames, headings
  inline.rs   styled word atoms, control-char scrubbing, word wrap
  hyperlink.rs  external OSC 8 target validation and bounded ASCII URI encoding
  table.rs    column fitting and box drawing
  style.rs    SGR styles and the color theme
```

| File | Lines | File | Lines |
|---|---|---|---|
| `format/asciidoc.rs` | 1398 | `data/xml.rs` | 625 |
| `pager/mod.rs` | 1189 | `main.rs` | 567 |
| `render.rs` | 1069 | `image/codec.rs` | 545 |
| `format/html.rs` | 1055 | `image/encode.rs` | 532 |
| `format/rst.rs` | 1037 | `data/yaml.rs` | 514 |
| `format/org.rs` | 892 | `fetch.rs` | 471 |

## Dependencies {#dependencies}

| Crate | Used for |
|---|---|
| `pulldown-cmark` 0.13 | Markdown parsing. Default features off, `simd` on: CommonMark plus GFM tables, strikethrough, task lists, footnotes, alerts |
| `unicode-width` | Display width of CJK and emoji |
| `libc` | The `TIOCGWINSZ` ioctl, `termios`, `select` and the resize self-pipe |
| `png`, `zune-jpeg`, `gif`, `image-webp` | Raster image decoding |
| `resvg` | SVG rendering, with default features off: no text, no system fonts |
| `miniz_oxide` | Inflating EPUB zip entries. `png` already depends on it |

Everything else is hand-written: the HTML, RST, Org and AsciiDoc parsers, the CSV and TSV reader, the JSON, YAML, TOML and XML tokenizers, image fitting and resampling, the half-block, Kitty and iTerm2 encoders, base64, the zip reader, and the pager with its search. There's no HTTP or TLS code; URLs go through the system `curl`. PDF text comes from poppler's `pdftotext` when it's installed.

The release profile uses `opt-level = 3`, fat LTO, one codegen unit and `strip`, giving a ~2.6 MB binary. It keeps `panic = "unwind"` on purpose, so a panic inside a third-party image decoder can be caught and shown as alt text.

## Library API {#library-api}

The binary is a thin CLI around the `mu` library crate, which exports:

```rust
pub use book::{extract, sniff_bytes};
pub use format::{Format, detect, present, present_outlined, sniff};
pub use image::{CellSize, ImageMode, ImageOptions, Protocol};
pub use outline::{outline, outline_entries};
pub use render::{Anchor, Heading, NavigationLink, Options, Rendered, render};
```

| Item | Purpose |
|---|---|
| `render(input, &Options) -> String` | Render Markdown |
| `present(input, Format, &Options) -> String` | Render any format |
| `present_outlined(input, Format, &Options) -> Rendered` | Render any format and keep headings and internal EPUB navigation metadata |
| `Rendered { text, headings, anchors, links }` | Rendered text plus `Heading { level, text, line }`, `Anchor` and `NavigationLink` lists; data/source formats have no headings or navigation metadata |
| `Anchor { id, line, column }` | Internal destination identity and zero-based rendered output line/display column |
| `NavigationLink { target, label, line, column }` | Internal link's target anchor ID, semantic label and zero-based first rendered output line/display column; not an external OSC 8 destination |
| `outline(&[Heading], &Options) -> String` | The `--toc` text: headings indented by level, colored when `color` is on |
| `outline_entries(&[Heading], width) -> Vec<String>` | The same list as plain lines cut to a width |
| `detect(path: Option<&str>, input) -> Format` | Extension, then sniff, then Markdown |
| `sniff(input) -> Format` | Content sniffing alone |
| `sniff_bytes(bytes) -> Option<Format>` | Magic bytes for EPUB and PDF |
| `extract(bytes, Format) -> Result<String, String>` | An EPUB's HTML or a PDF's page text, ready for `present`; other formats are decoded as UTF-8 |
| `Format` | Enum of every format, with `from_name`, `from_path`, `from_media_type`, `from_url_path`, `is_wide`, `is_binary`, `name` |
| `Options { width, color, links, images, language }` | `Options::new(width, color)` builds text-only options with images off, `links: false` and no source language override; the library does not detect hyperlink capabilities |
| `ImageOptions { mode, base, cell, protocol, max_rows }` | Image loading and drawing settings |

```rust
let options = mu::Options::new(80, true);
let format = mu::detect(Some("notes.org"), &text);
print!("{}", mu::present(&text, format, &options));
```

Set `options.links = true` to explicitly enable eligible external OSC 8 links, independently of `options.color`. For a binary EPUB, first call `extract(bytes, Format::Epub)`, then `present_outlined` with `Format::Epub` on the returned HTML. Keep its `anchors` and `links` to implement internal navigation; `present` returns only the text. Re-render at a new width to refresh output-line metadata.

> [!NOTE]
> At 1.0, the CLI flags, exit codes and `mu::{render, present, Format, Options}` become stable. Until then, a breaking API change bumps the minor version.
