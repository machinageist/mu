---
title: Architecture
group: Internals
order: 1
summary: Pipeline, source layout and library API
---

About 11,600 lines of Rust in one crate: a thin CLI around a library that turns text into styled terminal lines.

## Pipeline {#pipeline}

There are two paths through mu. **Documents** become one event stream that a single layout engine lays out. **Data** becomes styled lines for a simpler printer. Both paths produce plain lines that carry their own SGR styling, which then either print in one write or open in the pager.

```mermaid
flowchart LR
  IN(["File or stdin"]) --> DET{"Which format?"}
  URL(["http(s) URL"]) --> FE["curl"] --> DET
  DET -->|"Markdown"| PC["pulldown-cmark"]
  DET -->|"HTML, RST, Org, AsciiDoc"| FP["hand-written parsers"]
  PC --> EV["Event stream"]
  FP --> EV
  EV --> RE["render_eventsshared layout engine"]
  DET -->|"JSON, YAML, TOML"| HL["highlighters"]
  HL --> DP["data::print"]
  RE --> OUT["Terminal lines"]
  DP --> OUT
  OUT --> PG{"Taller thanthe screen?"}
  PG -->|"yes"| PAGER["Pager"]
  PG -->|"no"| W(["One write"])
```

Inside the document path the stages are: events → a per-block buffer of styled word atoms in one text arena → word wrap → container gutters (quote bars, list indents) prepended per line → one buffered write, or the pager. Styles are emitted lazily and every line closes its own style, so the pager can draw any window of lines on its own.

The data path has highlighters that only classify text into styled spans. A shared printer owns everything that touches the terminal: tab expansion, control-character scrubbing, width and SGR. Every data format therefore keeps the same safety and layout guarantees as documents.

For the detailed flow of each stage, see the [logic maps](/wiki/logic-maps).

## Source layout {#source-layout}

```text
src/
  main.rs    CLI: args, input, width/color/paging decisions, single write
  fetch.rs   URL input: curl arguments, raw-file rewrites, response parsing
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

| File | Lines | File | Lines |
|---|---|---|---|
| `format/asciidoc.rs` | 1398 | `image/codec.rs` | 545 |
| `format/html.rs` | 1055 | `image/encode.rs` | 532 |
| `format/rst.rs` | 1037 | `main.rs` | 519 |
| `render.rs` | 987 | `data/yaml.rs` | 514 |
| `format/org.rs` | 892 | `fetch.rs` | 471 |
| `pager.rs` | 627 | `data/toml.rs` | 454 |

## Dependencies {#dependencies}

| Crate | Used for |
|---|---|
| `pulldown-cmark` 0.13 | Markdown parsing. Default features off, `simd` on: CommonMark plus GFM tables, strikethrough, task lists, footnotes, alerts |
| `unicode-width` | Display width of CJK and emoji |
| `libc` | The `TIOCGWINSZ` ioctl, `termios` and `poll` |
| `png`, `zune-jpeg`, `gif`, `image-webp` | Raster image decoding |
| `resvg` | SVG rendering, with default features off: no text, no system fonts |

Everything else is hand-written: the HTML, RST, Org and AsciiDoc parsers, the JSON, YAML and TOML tokenizers, image fitting and resampling, the half-block, Kitty and iTerm2 encoders, base64, and the pager. There's no HTTP or TLS code; URLs go through the system `curl`.

The release profile uses `opt-level = 3`, fat LTO, one codegen unit and `strip`, giving a ~2.0 MB binary. It keeps `panic = "unwind"` on purpose, so a panic inside a third-party image decoder can be caught and shown as alt text.

## Library API {#library-api}

The binary is a thin CLI around the `mu` library crate, which exports:

```rust
pub use format::{Format, detect, present, sniff};
pub use image::{CellSize, ImageMode, ImageOptions, Protocol};
pub use render::{Options, render};
```

| Item | Purpose |
|---|---|
| `render(input, &Options) -> String` | Render Markdown |
| `present(input, Format, &Options) -> String` | Render any format |
| `detect(path: Option<&str>, input) -> Format` | Extension, then sniff, then Markdown |
| `sniff(input) -> Format` | Content sniffing alone |
| `Format` | Enum with `from_name`, `from_path`, `from_media_type`, `from_url_path`, `name` |
| `Options { width, color, images }` | `Options::new(width, color)` builds text-only options with images off |
| `ImageOptions { mode, base, cell, protocol, max_rows }` | Image loading and drawing settings |

```rust
let options = mu::Options::new(80, true);
let format = mu::detect(Some("notes.org"), &text);
print!("{}", mu::present(&text, format, &options));
```

> [!NOTE]
> At 1.0, the CLI flags, exit codes and `mu::{render, present, Format, Options}` become stable. Until then, a breaking API change bumps the minor version.
