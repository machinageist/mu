---
title: Logic maps
group: Internals
order: 2
summary: Eight flowcharts traced through the real code
---

How input becomes terminal output, traced through the real functions. Each map lists the code it follows, so you can open the file and walk it step by step. Diamonds are decisions; rounded ends are where the program exits.


## 1 · End to end {#map-1}

The two paths through mu. Input is a file, stdin or a URL fetched with curl. Documents become one event stream that a single layout engine lays out. Data formats become styled lines for a simpler printer. Both produce plain lines with their own SGR styling, which either print once or open in the pager.

```mermaid
flowchart LR
  IN(["File or stdin"]) --> RD["read_input<br/>bytes to UTF-8, invalid bytes replaced"]
  URL(["http or https URL"]) --> FE["load_url<br/>system curl, see map 8"]
  FE --> DET
  RD --> DET{"Which format?"}
  DET -->|"-f, Content-Type, extension, or sniff"| P["present"]
  P -->|"Markdown"| PC["pulldown-cmark Parser"]
  P -->|"HTML, RST, Org, AsciiDoc"| FP["format::*::events<br/>hand-written parsers"]
  PC --> EV["pulldown-cmark Event stream"]
  FP --> EV
  EV --> RE["render_events<br/>shared layout engine"]
  P -->|"JSON, YAML, TOML"| HL["data::*::highlight"]
  HL --> SL["Styled lines"]
  SL --> DP["data::print"]
  RE --> OUT["Terminal lines<br/>each closes its own style"]
  DP --> OUT
  OUT --> PG{"Taller than the screen?"}
  PG -->|"yes"| PAGER["Built-in pager"]
  PG -->|"no"| W(["One write to stdout"])
```

- `src/main.rs:main` drives everything
- `src/fetch.rs` URL input
- `src/format/mod.rs:present` picks the path
- `src/render.rs:render_events` documents
- `src/data/mod.rs:print` data

## 2 · CLI entry {#map-2}

What `main` does from arguments to exit code. Files and URLs both load into a `Source`, which `show` renders the same way. A closed pipe such as `mu x.md | head` counts as success.

```mermaid
flowchart TD
  S(["mu args"]) --> PA{"parse_args"}
  PA -->|"-h"| H(["print usage, exit 0"])
  PA -->|"-V"| V(["print version, exit 0"])
  PA -->|"unknown flag, bad value,<br/>or a URL flag without a URL"| E2(["error and usage, exit 2"])
  PA -->|"ok"| T{"No FILE and stdin<br/>is a terminal?"}
  T -->|"yes"| U(["print usage, exit 2"])
  T -->|"no"| ISU{"FILE starts with<br/>http:// or https://?"}
  ISU -->|"yes"| LU["load_url<br/>see map 8"]
  ISU -->|"no"| LF["load_file<br/>read_input, then -f or detect"]
  LU -->|"curl missing or failed"| E1(["error, exit 1"])
  LF -->|"io error"| E1
  LU --> SRC["Source: parts, title, base,<br/>image mode, HTTP failure"]
  LF --> SRC
  SRC --> SH["show<br/>Options: resolve_width, resolve_color,<br/>resolve_images"]
  SH --> P["present_all<br/>each part in its own format, half-blocks"]
  P --> SP{"should_page?"}
  SP -->|"yes"| SO{"pager::Session::open<br/>/dev/tty usable?"}
  SO -->|"yes"| RUN["session.run titled with the file or URL,<br/>relayout closure"]
  SO -->|"no"| NP
  SP -->|"no"| NP{"native_protocol:<br/>kitty or iTerm2, not tmux?"}
  NP -->|"yes"| RN["present_all again with that protocol"]
  NP -->|"no"| WR["write_all and flush"]
  RN --> WR
  RUN --> HF
  WR -->|"ok or BrokenPipe"| HF{"HTTP status 400 or more?"}
  WR -->|"other error"| X1(["exit 1"])
  HF -->|"yes"| X1h(["mu: url: HTTP code, exit 1"])
  HF -->|"no"| X0(["exit 0"])
```

| Decision | Rule |
| --- | --- |
| resolve_width | `-w N` wins, at least 20. Otherwise the terminal width, clamped to 20–100. With no terminal, 80. The width is probed on stdout, then stderr, then stdin, so `mu x \| less -R` still sees the real width. |
| resolve_color | `always` / `never` as given. With `auto`, color is on when stdout is a terminal and `NO_COLOR` is unset or empty. |
| resolve_images | Mode from `--images`, else `fetch` for a URL and `auto` for a file. Base from `--base`, else the file path or the final URL. Cell pixels from TIOCGWINSZ, else 8×16. At most the screen height minus 2 rows. |
| should_page | Never when stdout is not a terminal. `never` → no, `always` → yes. With `auto`, it pages only when the output has more lines than the screen minus one, like `less -F`. |

- `src/main.rs:parse_args`
- `src/main.rs:load_file` · `load_url` · `read_input`
- `src/main.rs:show` · `present_all` · `native_protocol`
- `src/main.rs:resolve_images`
- `src/main.rs:resolve_width` · `auto_width`
- `src/main.rs:should_page`
- `src/term.rs:size` TIOCGWINSZ

## 3 · Format detection {#map-3}

For a URL, the response `Content-Type` is tried first and then the URL path's extension (map 8). An extension decides immediately. Without one, as with stdin, `.txt` or no extension, the content is sniffed in a fixed order, most specific rule first. Every rule is strict, so uncertain input falls through to Markdown, which shows plain text correctly anyway.

```mermaid
flowchart TD
  A["detect path, input"] --> X{"Known extension?<br/>md html rst org adoc json yaml toml"}
  X -->|"yes"| XF(["Format from extension"])
  X -->|"no"| H["head = first 8 KB<br/>cut back to a char boundary"]
  H --> J{"JSON?<br/>starts with brace or bracket<br/>and tokenizes as JSON so far"}
  J -->|"yes"| FJ(["JSON"])
  J -->|"no"| HT{"HTML?<br/>doctype, html, head or body tag first"}
  HT -->|"yes"| FH(["HTML"])
  HT -->|"no"| OR{"Org?<br/>#35;+KEYWORD or #35;+BEGIN_ line, PROPERTIES drawer,<br/>or star heading plus a described or URL link"}
  OR -->|"yes"| FO(["Org"])
  OR -->|"no"| RS{"RST?<br/>.. directive:: or .. _target: line,<br/>over-and-underlined title after a blank line,<br/>or link/role syntax with no Markdown headings or fences"}
  RS -->|"yes"| FR(["RST"])
  RS -->|"no"| AD{"AsciiDoc?<br/>= Title first line, :attr: header,<br/>source block attribute plus delimiter, table fence"}
  AD -->|"yes"| FA(["AsciiDoc"])
  AD -->|"no"| TO{"TOML?<br/>2+ lines, 80% table or key = value,<br/>at least one key = value"}
  TO -->|"yes"| FT(["TOML"])
  TO -->|"no"| YA{"YAML?<br/>3+ lines, 80% shaped, a key,<br/>top-level key or --- or item mapping,<br/>no fences, quotes or tables"}
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

## 4 · Document layout {#map-4}

One `Renderer` consumes events from any document format. Inline text collects as styled word atoms. When its block ends, the atoms are wrapped to the width left after container gutters, such as quote bars and list indents, and then written behind those gutters.

```mermaid
flowchart TD
  EV["Next event"] --> K{"Event kind"}
  K -->|"Start tag"| ST{"Which tag"}
  ST -->|"Paragraph or Heading"| IB["begin_block, open inline buffer<br/>heading picks its level style"]
  ST -->|"BlockQuote, Item, Footnote"| CT["push Container<br/>gutter: quote bar, bullet, number, label"]
  ST -->|"CodeBlock, Metadata, HtmlBlock"| RAW["start Raw buffer"]
  ST -->|"Table"| TB["start Table<br/>cells collect as atoms"]
  ST -->|"Emphasis, Strong, Link, Image"| STY["push inline style<br/>link remembers its URL"]
  K -->|"Text, Code, breaks"| TX{"Raw block open?"}
  TX -->|"yes"| RAWT["append verbatim"]
  TX -->|"no"| AT["Inline atoms in one arena<br/>words, spaces, styles<br/>control chars become U+FFFD"]
  K -->|"End tag"| EN{"Which tag"}
  EN -->|"Paragraph or Heading"| WRAP["wrap atoms to avail<br/>= width minus gutters, min 16"]
  WRAP --> EMIT["emit lines behind gutters<br/>H1 and H2 get a rule underneath"]
  EN -->|"CodeBlock or Metadata"| FR["frame: top edge with language,<br/>bar per line, dashed bar on wrap"]
  EN -->|"HtmlBlock"| HB["dim raw HTML, drop comments"]
  EN -->|"Table"| TR["table::render<br/>shrink widest columns, box-draw"]
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

## 5 · Data printing {#map-5}

Highlighters only classify text into styled spans. The shared printer owns everything that touches the terminal: tabs, control-character scrubbing, width and SGR. Every data format therefore keeps the same safety and layout guarantees as documents.

```mermaid
flowchart TD
  IN["input text"] --> F{"Format"}
  F -->|"JSON"| JV{"Valid JSON or NDJSON<br/>and nested 256 deep or less?"}
  JV -->|"yes"| JP["re-indent, 2 spaces<br/>strings and numbers kept as written"]
  JV -->|"no"| JR["color line by line as written"]
  F -->|"YAML"| Y["line tokenizer with state<br/>block scalars, quotes, flow collections"]
  F -->|"TOML"| T["line lexer with state<br/>multi-line strings, open arrays"]
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
  OV -->|"yes"| BR["reset style, newline,<br/>dim dashed continuation bar"]
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
- `src/style.rs` STYLE_DATA_* theme

## 6 · Pager loop {#map-6}

The built-in pager reads keys from `/dev/tty`, which is why `curl … | mu` can page. A resize arrives through a self-pipe, so `poll` can never miss it. When the width is automatic, a resize re-renders at the new width and keeps the reader's place proportionally.

```mermaid
flowchart TD
  O["Session::open<br/>/dev/tty, raw mode, alternate screen,<br/>panic hook restores the terminal"] --> R["run: route SIGWINCH into a self-pipe"]
  R --> D["draw_frame<br/>visible lines plus status bar,<br/>one synchronized write"]
  D --> W["poll tty and resize pipe"]
  W -->|"resize"| RS["drain pipe, read the new size"]
  RS --> RL{"relayout columns:<br/>width is automatic and changed?"}
  RL -->|"yes"| RR["present again at the new width"]
  RL -->|"no"| KEEP["keep the document"]
  RR --> VR["view.resize<br/>top scaled to the new length"]
  KEEP --> VR
  VR --> D
  W -->|"input"| KEY["read bytes, parse_keys<br/>letters, control keys, CSI escapes"]
  KEY --> A{"Action"}
  A -->|"Quit: q or Ctrl-C"| Q(["drop: leave alternate screen,<br/>restore termios"])
  A -->|"line, page, half page, top, bottom"| CL["view.apply<br/>clamp to 0 .. max_top"]
  CL --> D
  KEY -->|"tty closed"| Q
```

- `src/pager.rs:Session::open` · `run` · `drop`
- `src/pager.rs:ResizeSignal` · `on_resize`
- `src/pager.rs:wait_for_input` poll
- `src/pager.rs:parse_keys` · `key_action` · `escape_action`
- `src/pager.rs:View::apply` · `View::resize`
- `src/main.rs` relayout closure

## 7 · Images {#map-7}

How an image in a document becomes a picture. Only an image standing alone in its paragraph or block is drawn; inside running text it stays `[image: alt]`. Every failure, including a decoder panic, falls back to that alt text, never to an error.

```mermaid
flowchart TD
  IMG["Block ends with one image<br/>and no other text"] --> MODE{"color on and<br/>--images not never?"}
  MODE -->|"no"| ALT(["show [image: alt]"])
  MODE -->|"yes"| CACHE{"load_cached:<br/>seen this src and base?"}
  CACHE -->|"yes"| PIC
  CACHE -->|"no"| SRC{"source::fetch src"}
  SRC -->|"data: URI"| B64["base64 or percent decode"]
  SRC -->|"http or https,<br/>or a relative path on a URL base"| NET{"mode is fetch?"}
  NET -->|"yes"| CURL["system curl -fsSL, http(s) only,<br/>10 s timeout, 20 MB cap"]
  NET -->|"no"| ALT
  SRC -->|"relative or file path"| PATH["resolve against --base<br/>or the input file's folder"]
  B64 --> DEC{"codec::decode by magic bytes<br/>inside guarded: panics caught"}
  CURL --> DEC
  PATH --> DEC
  DEC -->|"PNG, JPEG, GIF, WebP"| RAS["RGBA pixels,<br/>64 MP and 16384 px limits"]
  DEC -->|"other"| SVG{"Svg::parse"}
  SVG -->|"yes"| VEC["SVG tree, external refs blocked"]
  SVG -->|"no"| ALT
  RAS --> PIC["Picture, cached per thread"]
  VEC --> PIC
  PIC --> FIT["encode::fit<br/>one scale factor, at most 1,<br/>bounded by column width and screen height"]
  FIT --> PR{"Output protocol"}
  PR -->|"half-block: pager, tmux, pipes, most terminals"| HB["encode::half_blocks<br/>top pixel as fg, bottom as bg"]
  PR -->|"Kitty: kitty, Ghostty"| KI["encode::resize then kitty<br/>raw RGBA, rows reserved"]
  PR -->|"iTerm2: iTerm2, WezTerm"| IT["encode::resize, encode_png,<br/>iterm inline image escape"]
  HB --> CAP["dim alt caption below"]
  KI --> CAP
  IT --> CAP
```

- `src/render.rs:draw_standalone_image`
- `src/image/mod.rs:load_cached` · `load` · `guarded`
- `src/image/mod.rs:draw_guarded` · `draw` · `reserve`
- `src/image/source.rs:fetch` · `join_url` · `remote`
- `src/image/codec.rs:decode` · `encode_png`
- `src/image/svg.rs:Svg::parse` · `rasterize`
- `src/image/encode.rs:fit` · `half_blocks` · `kitty` · `iterm`
- `src/main.rs:native_protocol` picks the protocol

## 8 · URL fetch {#map-8}

What `mu <url>` does before the shared rendering takes over. mu has no HTTP or TLS code; it runs the system `curl` without a shell, with the URL as one argument after `--`. Response headers come back on stdout ahead of the body, and the final URL comes back on stderr.

```mermaid
flowchart TD
  U["load_url url"] --> RW{"--no-rewrite given?"}
  RW -->|"no"| RAW{"raw_url:<br/>GitHub blob, GitLab -/blob,<br/>or Gist page?"}
  RAW -->|"yes"| RT["raw file URL,<br/>query and fragment dropped"]
  RAW -->|"no"| AS["URL as typed"]
  RW -->|"yes"| AS
  RT --> ARGS["curl_args<br/>-sS -L, http(s) only incl. redirects,<br/>connect timeout 10, --compressed, -D -,<br/>user flags, --curl extras, then -- URL"]
  AS --> ARGS
  ARGS --> RUN{"run curl"}
  RUN -->|"not installed"| MISS(["opening URLs needs curl, exit 1"])
  RUN -->|"non-zero exit"| FAIL(["url: curl's message, exit 1"])
  RUN -->|"ok"| PARSE["parse_response<br/>skip redirect, 1xx and proxy-tunnel blocks,<br/>keep the final status and headers"]
  PARSE --> INC{"-i or -I?"}
  INC -->|"yes"| TAB["header_table<br/>status line and headers as escaped Markdown"]
  INC -->|"no"| BODY
  TAB --> HEAD{"-I?"}
  HEAD -->|"yes"| SRC
  HEAD -->|"no"| BODY{"Body format"}
  BODY -->|"-f given"| F1["that format"]
  BODY -->|"Content-Type known"| F2["from_media_type<br/>text/plain and octet-stream skipped"]
  BODY -->|"path extension known"| F3["from_url_path<br/>path only, never the host"]
  BODY -->|"otherwise"| F4["sniff, else Markdown"]
  F1 --> SRC
  F2 --> SRC
  F3 --> SRC
  F4 --> SRC["Source<br/>title = URL as typed,<br/>base = final URL, images = fetch<br/>unless --images is given,<br/>failure set for HTTP 400 and above"]
```

| Flag | Sent to curl as |
| --- | --- |
| -H -X -u -k | the same flag. `-H` repeats. |
| -d, --data | `--data`, repeatable. |
| -A | `-A`, default `mu/VERSION`. |
| -I | `-I -o /dev/null`, so the headers come only through `-D -` and are not repeated. |
| --timeout | `--max-time`. |
| --curl ARG | ARG verbatim, before `--`. |
| -i, --no-rewrite | nothing: handled by mu. |

- `src/main.rs:load_url`
- `src/fetch.rs:is_url` · `raw_url`
- `src/fetch.rs:curl_args` · `fetch`
- `src/fetch.rs:parse_response` · `split_block` · `is_interim`
- `src/fetch.rs:header_table` · `escape_markdown`
- `src/format/mod.rs:from_media_type` · `from_url_path`
- `tests/url.rs` runs against a local server

Drawn from the mu source at v0.4.0. The aspect math in map 7: the scale factor is the smaller of 1, max width in pixels over image width, and max height in pixels over image height. Cells are counted from that one factor, so width and height shrink together.

Source: `docs/muLogicMaps.md` in the mu repository. Diagrams render with Mermaid from a CDN; offline, their source text is shown instead.
