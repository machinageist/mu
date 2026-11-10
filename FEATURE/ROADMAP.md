# mu — road to 1.0

Agreed with machinageist on 2026-11-09, with EPUB and PDF added the same day as 0.8.0. Each row below ships as its own minor release, in this order. Each release gets an `AGENTS/changes/<id>/SPEC.md` and `PLAN.md` before work starts, and updates `FEATURE_SPEC.md` when it ships.

## What 1.0 promises

- **Stable at 1.0:** the command line. That covers flags and their meanings, exit codes, config file keys, format detection order, and the output invariants in `FEATURE_SPEC.md`. Breaking any of these after 1.0 needs 2.0.
- **Not stable at 1.0:** the Rust library (`mu::present`, `Options`, `Format`, …). It stays public but is documented as unstable, so internals can keep changing in minor releases.
- **Later:** a stable library API for embedding mu in other tools, designed once the CLI has settled.

## Releases

| Version | Theme | Scope |
|---|---|---|
| 0.5.0 | Navigation | Pager search: `/` and `?` to search, `n` and `N` to move between matches, matches highlighted, matching on text with styling stripped. Outline: `o` in the pager lists headings and jumps to one. `--toc` prints just the outline and exits |
| 0.6.0 | Data inputs | CSV and TSV become a box-drawn table that fits the width, through the existing table engine, with quoting per RFC 4180. Wider tables scroll horizontally in the pager rather than wrapping. XML and plist are highlighted and re-indented like JSON. Each new format gets detection by extension and media type, a sniff rule, and a row in `tests/sniff.rs` |
| 0.7.0 | Document inputs | Source files (`.c`, `.h`, `.rs`, `.py`, `.sh`, …) are shown verbatim instead of being read as Markdown. Man pages: roff with `man` macros, plus common `mdoc`, mapped to document events. `man ls \| mu`, which arrives as overstruck text, is cleaned up too. Jupyter `.ipynb`: Markdown cells, code cells with their language, text outputs, and PNG/SVG outputs drawn by the image pipeline |
| 0.8.0 | Books and PDF | EPUB: the zip is read in-process with `miniz_oxide`, already in the dependency tree. Chapters go through the HTML renderer in spine order, images come from inside the book, and headings fill the outline. PDF: the text comes from poppler's `pdftotext -layout` when it is installed, the same way URLs use the system curl. Without it, mu exits with an error naming the tool and how to install it. Pages are separated by a rule. Both formats are detected by extension, media type and magic bytes (`%PDF-`, and a zip whose `mimetype` is `application/epub+zip`). That requires reading input as bytes before decoding it as text |
| 0.9.0 | Code and links | Highlighting in code blocks and in source files from small built-in lexers, with no grammar engine. First set: sh, rust, python, js/ts, go, c/c++, json, yaml, toml, html, css, sql, diff. The binary may grow by about 100 KB at most, with no measurable startup cost. Unknown languages stay plain. OSC 8 hyperlinks where the terminal supports them, falling back to the printed URL |
| 0.10.0 | Config and themes | The config file and `MU_OPTS`, as set out under Config below. A `--theme` flag, built-in `dark` and `light` themes, per-element colors, and a light-or-dark choice made automatically. This retires the "no config file" promise in the README and `FEATURE_SPEC.md` |
| 0.11.0 | Shipping | The library is documented as unstable. CI runs the gate on every push, tags build macOS (arm64, x86_64) and Linux (x86_64, aarch64) binaries, and the crate is published to crates.io as **`markup-utility`** with the binary still named `mu`. A Homebrew tap, `machinageist/homebrew-tap`, is updated from the release binaries |
| 1.0.0 | Freeze | No new features. Release candidates (`1.0.0-rc.N`) to review every flag name, default and config key. Close or document each item under Known limits. The CLI promise above is written into `CONTRIBUTING.md` |

## Config (0.10.0)

Decided with machinageist on 2026-11-09.

- **Location:** `$XDG_CONFIG_HOME/mu/config.toml`, else `~/.config/mu/config.toml`. TOML, read with the TOML support mu already has, without a new dependency where possible.
- **Precedence, lowest first:**
  1. built-in defaults
  2. the config file
  3. `MU_OPTS`, flags in an environment variable such as `MU_OPTS='--width 90 --theme light'`, split like a shell would but never run through one
  4. flags on the command line

  `--no-config` ignores both the file and `MU_OPTS`.
- **Errors:** an unknown key or a bad value is a warning on stderr naming the file and line, and that key is ignored. A config file never stops mu from showing a document.
- **What it covers:**

  | Area | Keys | Notes |
  |---|---|---|
  | Layout | `width`, `max_width` | `max_width` replaces the fixed 100-column cap on the auto width |
  | Output | `color`, `paging` | Same values as the flags |
  | Images | `images`, `protocol` | `protocol` forces `halfblock`, `kitty` or `iterm` when detection guesses wrong |
  | URL fetching | `[url]`: `user_agent`, `timeout`, `headers`, `rewrite`, `images` | `headers` is a list sent with every request. `images` is the mode for URL input, default `fetch` |
  | Theme | `theme`, `[colors]` | `theme` is `auto` (the default), `dark` or `light`. `[colors]` overrides named elements (`heading1`…`heading6`, `link`, `code`, `quote`, `rule`, `table`, `caption`, `data.key`, `data.string`, `data.number`, `data.bool`, `data.null`, `data.comment`, …) with a 16-color name or `#rrggbb` |
  | Formats | `[formats]` | Map extensions to formats, e.g. `txt = "markdown"`, `conf = "toml"`. Checked before the built-in extensions |
  | Pager | `[pager.keys]` | Rebind actions (`down`, `up`, `page_down`, `page_up`, `half_down`, `half_up`, `top`, `bottom`, `search`, `search_back`, `next_match`, `prev_match`, `outline`, `quit`) to one or more keys |
  | Per format | `[json]` `indent`; `[csv]` `delimiter` | More keys only as formats need them |

- **Auto theme:** with `theme = "auto"` and stdout a terminal, mu asks the terminal for its background color (OSC 11) through `/dev/tty`, waiting at most about 100 ms. A light background picks `light`. Anything else, including no reply, picks `dark`. There is no query when output is not a terminal.
- **Example:**

  ```toml
  max_width = 110
  paging = "auto"
  theme = "auto"

  [colors]
  heading1 = "#e5c07b"
  link = "cyan"

  [url]
  user_agent = "mu"
  timeout = 20

  [formats]
  txt = "markdown"

  [pager.keys]
  down = ["j", "Down", "Ctrl-N"]
  up = ["k", "Up", "Ctrl-P"]
  ```

Every key above becomes part of the 1.0 promise, so the 1.0 release candidates review them as closely as the flags.

## Not planned before 1.0

- Watch mode (`--watch`)
- Windows support
- Grammar-based highlighting (syntect or tree-sitter)

## Small debts to fold in along the way

- In ordered lists that reach 10 items, items 10 and above indent their wrapped lines one column further than items 1–9
- The README gives the binary size as about 2.0 MB, which is out of date. Measure it again each release, and set a size budget in 0.11.0 once release binaries exist
