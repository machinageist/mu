mu · markup/markdown utility · v0.2.0 + images in progress

# mu logic maps════════════════

How input becomes terminal output, traced through the real functions. Each map lists the code it follows, so you can open the file and walk it step by step. Diamonds are decisions; rounded ends are where the program exits.

[1 Overview](#overview) [2 CLI entry](#cli) [3 Format detection](#detect) [4 Document layout](#layout) [5 Data printing](#data) [6 Pager loop](#pager) [7 Images wip](#images)

## 1 · End to end

The two paths through mu. Documents become one event stream that a single layout engine lays out. Data formats become styled lines for a simpler printer. Both produce plain lines with their own SGR styling, which either print once or open in the pager.

```mermaid
flowchart LR
  IN(["File or stdin"]) --> RD["read_inputbytes to UTF-8, invalid bytes replaced"]
  RD --> DET{"Which format?"}
  DET -->|"-f flag, extension, or sniff"| P["present"]
  P -->|"Markdown"| PC["pulldown-cmark Parser"]
  P -->|"HTML, RST, Org, AsciiDoc"| FP["format::*::eventshand-written parsers"]
  PC --> EV["pulldown-cmark Event stream"]
  FP --> EV
  EV --> RE["render_eventsshared layout engine"]
  P -->|"JSON, YAML, TOML"| HL["data::*::highlight"]
  HL --> SL["Styled lines"]
  SL --> DP["data::print"]
  RE --> OUT["Terminal lineseach closes its own style"]
  DP --> OUT
  OUT --> PG{"Taller than the screen?"}
  PG -->|"yes"| PAGER["Built-in pager"]
  PG -->|"no"| W(["One write to stdout"])
    
```

- `src/main.rs:main` drives everything
- `src/format/mod.rs:present` picks the path
- `src/render.rs:render_events` documents
- `src/data/mod.rs:print` data

## 2 · CLI entry

What `main` does from arguments to exit code. A closed pipe such as `mu x.md | head` counts as success.

```mermaid
flowchart TD
  S(["mu args"]) --> PA{"parse_args"}
  PA -->|"-h"| H(["print usage, exit 0"])
  PA -->|"-V"| V(["print version, exit 0"])
  PA -->|"unknown flag or bad value"| E2(["error and usage, exit 2"])
  PA -->|"ok"| T{"No FILE and stdinis a terminal?"}
  T -->|"yes"| U(["print usage, exit 2"])
  T -->|"no"| R["read_input FILE or stdin"]
  R -->|"io error"| E1(["error, exit 1"])
  R --> O["Options::newresolve_width, resolve_color"]
  O --> F{"--format given?"}
  F -->|"yes"| FMT["use that format"]
  F -->|"no"| D["detect path, input"]
  FMT --> P["present input, format, options"]
  D --> P
  P --> SP{"should_page?"}
  SP -->|"yes"| SO{"pager::Session::open/dev/tty usable?"}
  SO -->|"yes"| RUN["session.run with relayout closure"]
  RUN --> X0(["exit 0, or 1 on a tty error"])
  SO -->|"no"| WR
  SP -->|"no"| WR["write_all and flush"]
  WR -->|"ok or BrokenPipe"| X0b(["exit 0"])
  WR -->|"other error"| X1(["exit 1"])
    
```

| Decision | Rule |
| --- | --- |
| resolve_width | `-w N` wins, at least 20. Otherwise the terminal width, clamped to 20–100. With no terminal, 80. The width is probed on stdout, then stderr, then stdin, so `mu x \| less -R` still sees the real width. |
| resolve_color | `always` / `never` as given. With `auto`, color is on when stdout is a terminal and `NO_COLOR` is unset or empty. |
| should_page | Never when stdout is not a terminal. `never` → no, `always` → yes. With `auto`, it pages only when the output has more lines than the screen minus one, like `less -F`. |

- `src/main.rs:parse_args`
- `src/main.rs:read_input`
- `src/main.rs:resolve_width` · `auto_width`
- `src/main.rs:should_page`
- `src/term.rs:size` TIOCGWINSZ

## 3 · Format detection

An extension decides immediately. Without one, as with stdin, `.txt` or no extension, the content is sniffed in a fixed order, most specific rule first. Every rule is strict, so uncertain input falls through to Markdown, which shows plain text correctly anyway.

```mermaid
flowchart TD
  A["detect path, input"] --> X{"Known extension?md html rst org adoc json yaml toml"}
  X -->|"yes"| XF(["Format from extension"])
  X -->|"no"| H["head = first 8 KBcut back to a char boundary"]
  H --> J{"JSON?starts with brace or bracketand tokenizes as JSON so far"}
  J -->|"yes"| FJ(["JSON"])
  J -->|"no"| HT{"HTML?doctype, html, head or body tag first"}
  HT -->|"yes"| FH(["HTML"])
  HT -->|"no"| OR{"Org?#35;+KEYWORD or #35;+BEGIN_ line, PROPERTIES drawer,or star heading plus a described or URL link"}
  OR -->|"yes"| FO(["Org"])
  OR -->|"no"| RS{"RST?.. directive:: or .. _target: line,over-and-underlined title after a blank line,or link/role syntax with no Markdown headings or fences"}
  RS -->|"yes"| FR(["RST"])
  RS -->|"no"| AD{"AsciiDoc?= Title first line, :attr: header,source block attribute plus delimiter, table fence"}
  AD -->|"yes"| FA(["AsciiDoc"])
  AD -->|"no"| TO{"TOML?2+ lines, 80% table or key = value,at least one key = value"}
  TO -->|"yes"| FT(["TOML"])
  TO -->|"no"| YA{"YAML?3+ lines, 80% shaped, a key,top-level key or --- or item mapping,no fences, quotes or tables"}
  YA -->|"yes"| FY(["YAML"])
  YA -->|"no"| MD(["Markdown"])
    
```

- `src/format/mod.rs:detect` · `sniff`
- `src/data/json.rs:sniff`
- `src/format/html.rs:sniff`
- `src/format/org.rs:sniff` · `has_sniff_link`
- `src/format/rst.rs:sniff` · `has_markdown_structure`
- `src/format/asciidoc.rs:sniff`
- `src/data/toml.rs:sniff`
- `src/data/yaml.rs:sniff` · `is_prose_value`
- `tests/sniff.rs` pins every fixture to its format

## 4 · Document layout

One `Renderer` consumes events from any document format. Inline text collects as styled word atoms. When its block ends, the atoms are wrapped to the width left after container gutters, such as quote bars and list indents, and then written behind those gutters.

```mermaid
flowchart TD
  EV["Next event"] --> K{"Event kind"}
  K -->|"Start tag"| ST{"Which tag"}
  ST -->|"Paragraph or Heading"| IB["begin_block, open inline bufferheading picks its level style"]
  ST -->|"BlockQuote, Item, Footnote"| CT["push Containergutter: quote bar, bullet, number, label"]
  ST -->|"CodeBlock, Metadata, HtmlBlock"| RAW["start Raw buffer"]
  ST -->|"Table"| TB["start Tablecells collect as atoms"]
  ST -->|"Emphasis, Strong, Link, Image"| STY["push inline stylelink remembers its URL"]
  K -->|"Text, Code, breaks"| TX{"Raw block open?"}
  TX -->|"yes"| RAWT["append verbatim"]
  TX -->|"no"| AT["Inline atoms in one arenawords, spaces, stylescontrol chars become U+FFFD"]
  K -->|"End tag"| EN{"Which tag"}
  EN -->|"Paragraph or Heading"| WRAP["wrap atoms to avail= width minus gutters, min 16"]
  WRAP --> EMIT["emit lines behind guttersH1 and H2 get a rule underneath"]
  EN -->|"CodeBlock or Metadata"| FR["frame: top edge with language,bar per line, dashed bar on wrap"]
  EN -->|"HtmlBlock"| HB["dim raw HTML, drop comments"]
  EN -->|"Table"| TR["table::rendershrink widest columns, box-draw"]
  EN -->|"Quote, Item, Footnote"| POP["pop Container"]
  K -->|"Rule"| RU["dim rule across the width"]
  K -->|"TaskListMarker"| TM["swap bullet for a checkbox"]
  EMIT --> OUT["output String"]
  FR --> OUT
  HB --> OUT
  TR --> OUT
  RU --> OUT
    
```

- `src/render.rs:Renderer::event` · `start` · `end`
- `src/render.rs:flush_inline` · `emit_lines` · `line_prefix`
- `src/render.rs:end_framed` · `verbatim_chunks`
- `src/inline.rs:Inline` · `wrap` · `push_clean`
- `src/table.rs:render`
- `src/format/{html,rst,org,asciidoc}.rs:events` feed this

## 5 · Data printing

Highlighters only classify text into styled spans. The shared printer owns everything that touches the terminal: tabs, control-character scrubbing, width and SGR. Every data format therefore keeps the same safety and layout guarantees as documents.

```mermaid
flowchart TD
  IN["input text"] --> F{"Format"}
  F -->|"JSON"| JV{"Valid JSON or NDJSONand nested 256 deep or less?"}
  JV -->|"yes"| JP["re-indent, 2 spacesstrings and numbers kept as written"]
  JV -->|"no"| JR["color line by line as written"]
  F -->|"YAML"| Y["line tokenizer with stateblock scalars, quotes, flow collections"]
  F -->|"TOML"| T["line lexer with statemulti-line strings, open arrays"]
  JP --> L["StyledLine = list of style and text spans"]
  JR --> L
  Y --> L
  T --> L
  L --> C["next char"]
  C --> Q{"char"}
  Q -->|"CR or LF"| C
  Q -->|"tab"| TAB["spaces to the next 4-column stop"]
  Q -->|"control"| REP["U+FFFD"]
  Q -->|"other"| G["glyph with its display width"]
  TAB --> G
  REP --> G
  G --> OV{"column + width over the limit?"}
  OV -->|"yes"| BR["reset style, newline,dim dashed continuation bar"]
  OV -->|"no"| SC{"style changed?"}
  BR --> SC
  SC -->|"yes"| SG["write SGR, or reset if plain"]
  SC -->|"no"| PUSH["write glyph"]
  SG --> PUSH
  PUSH --> C
  C -->|"end of line"| EL["reset if styled, newline"]
    
```

- `src/data/json.rs:highlight` · `sniff`
- `src/data/yaml.rs:highlight` · `State::line`
- `src/data/toml.rs:highlight`
- `src/data/mod.rs:print` · `Printer::glyph` · `end_line`
- `src/style.rs` STYLE_DATA\_\* theme

## 6 · Pager loop

The built-in pager reads keys from `/dev/tty`, which is why `curl … | mu` can page. A resize arrives through a self-pipe, so `poll` can never miss it. When the width is automatic, a resize re-renders at the new width and keeps the reader's place proportionally.

```mermaid
flowchart TD
  O["Session::open/dev/tty, raw mode, alternate screen,panic hook restores the terminal"] --> R["run: route SIGWINCH into a self-pipe"]
  R --> D["draw_framevisible lines plus status bar,one synchronized write"]
  D --> W["poll tty and resize pipe"]
  W -->|"resize"| RS["drain pipe, read the new size"]
  RS --> RL{"relayout columns:width is automatic and changed?"}
  RL -->|"yes"| RR["present again at the new width"]
  RL -->|"no"| KEEP["keep the document"]
  RR --> VR["view.resizetop scaled to the new length"]
  KEEP --> VR
  VR --> D
  W -->|"input"| KEY["read bytes, parse_keysletters, control keys, CSI escapes"]
  KEY --> A{"Action"}
  A -->|"Quit: q or Ctrl-C"| Q(["drop: leave alternate screen,restore termios"])
  A -->|"line, page, half page, top, bottom"| CL["view.applyclamp to 0 .. max_top"]
  CL --> D
  KEY -->|"tty closed"| Q
    
```

- `src/pager.rs:Session::open` · `run` · `drop`
- `src/pager.rs:ResizeSignal` · `on_resize`
- `src/pager.rs:wait_for_input` poll
- `src/pager.rs:parse_keys` · `key_action` · `escape_action`
- `src/pager.rs:View::apply` · `View::resize`
- `src/main.rs` relayout closure

## 7 · Images in progress · 0.3.0

This is the planned flow. The skeleton and entry points are committed, and the four parts are being built in parallel: sources, codecs, SVG and encoders. Renderer and CLI wiring come next. Any failure along the way falls back to the alt text that mu shows today.

```mermaid
flowchart TD
  IMG["Image alone in its paragraph or block"] --> MODE{"--images mode, color on?"}
  MODE -->|"never, or color off"| ALT(["show alt text as today"])
  MODE -->|"auto or fetch"| SRC{"source::fetch src"}
  SRC -->|"data: URI"| B64["base64 or percent decode"]
  SRC -->|"http or https"| NET{"mode is fetch?"}
  NET -->|"yes"| CURL["system curl, 10 s timeout, 20 MB cap"]
  NET -->|"no"| ALT
  SRC -->|"relative or file path"| PATH["resolve against --baseor the input file's folder"]
  B64 --> DEC{"codec::decode by magic bytes"}
  CURL --> DEC
  PATH --> DEC
  DEC -->|"PNG, JPEG, GIF, WebP"| RAS["RGBA pixels, size limits checked"]
  DEC -->|"other"| SVG{"svg::parse"}
  SVG -->|"yes"| VEC["SVG tree, external refs blocked"]
  SVG -->|"no"| ALT
  RAS --> FIT["encode::fitone scale factor, at most 1,bounded by width and screen height"]
  VEC --> FIT
  FIT --> PR{"Output protocol"}
  PR -->|"half-block: pager, most terminals"| HB["upper-half-block cellstop pixel as fg, bottom as bg"]
  PR -->|"Kitty, Ghostty, WezTerm"| KI["resize, send raw RGBA,cursor stays put"]
  PR -->|"iTerm2"| IT["resize, encode PNG,inline image escape"]
  HB --> CAP["dim alt caption below"]
  KI --> CAP
  IT --> CAP
    
```

- `src/image/mod.rs:load` · `draw` · `reserve`
- `src/image/source.rs:fetch` *(agent in progress)*
- `src/image/codec.rs:decode` · `encode_png` *(agent in progress)*
- `src/image/svg.rs:Svg` *(agent in progress)*
- `src/image/encode.rs:fit` · `half_blocks` · `kitty` · `iterm` *(agent in progress)*
- `src/image/base64.rs` done

Drawn from the mu source at commit 6697424 (v0.2.0 plus the image skeleton). The aspect math in map 7: the scale factor is the smaller of 1, max width in pixels over image width, and max height in pixels over image height. Cells are counted from that one factor, so width and height shrink together.