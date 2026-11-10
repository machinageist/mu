# mu — road to 1.0

Agreed with machinageist on 2026-11-09. Each row below ships as its own minor release, in this order. Each release gets an `AGENTS/changes/<id>/SPEC.md` and `PLAN.md` before work starts, and updates `FEATURE_SPEC.md` when it ships.

## What 1.0 promises

- **Stable at 1.0:** the command line. That covers flags and their meanings, exit codes, config file keys, format detection order, and the output invariants in `FEATURE_SPEC.md`. Breaking any of these after 1.0 needs 2.0.
- **Not stable at 1.0:** the Rust library (`mu::present`, `Options`, `Format`, …). It stays public but is documented as unstable, so internals can keep changing in minor releases.
- **Later:** a stable library API for embedding mu in other tools, designed once the CLI has settled.

## Releases

| Version | Theme | Scope |
|---|---|---|
| 0.5.0 | Navigation | Pager search: `/` and `?` to search, `n` and `N` to move between matches, matches highlighted, matching on text with styling stripped. Outline: `o` in the pager lists headings and jumps to one. `--toc` prints just the outline and exits |
| 0.6.0 | Data inputs | CSV and TSV become a box-drawn table that fits the width, through the existing table engine, with quoting per RFC 4180. Wider tables scroll horizontally in the pager rather than wrapping. XML and plist are highlighted and re-indented like JSON. Each new format gets detection by extension and media type, a sniff rule, and a row in `tests/sniff.rs` |
| 0.7.0 | Document inputs | Man pages: roff with `man` macros, plus common `mdoc`, mapped to document events. `man ls \| mu`, which arrives as overstruck text, is cleaned up too. Jupyter `.ipynb`: Markdown cells, code cells with their language, text outputs, and PNG/SVG outputs drawn by the image pipeline |
| 0.8.0 | Code and links | Highlighting in code blocks from small built-in lexers, with no grammar engine. First set: sh, rust, python, js/ts, go, c/c++, json, yaml, toml, html, css, sql, diff. The binary may grow by about 100 KB at most, with no measurable startup cost. Unknown languages stay plain. OSC 8 hyperlinks where the terminal supports them, falling back to the printed URL |
| 0.9.0 | Config and shipping | Config file at `~/.config/mu/config.toml` (or `$XDG_CONFIG_HOME`) holding default flags and a theme: built-in `dark` and `light`, or custom colors. Flags always override the file, and `--no-config` ignores it. This retires the "no config file" promise in the README and `FEATURE_SPEC.md`. The library is documented as unstable. CI runs the gate on every push, tags build macOS (arm64, x86_64) and Linux (x86_64, aarch64) binaries, and the crate is published to crates.io as **`markup-utility`** with the binary still named `mu`. A Homebrew tap, `machinageist/homebrew-tap`, is updated from the release binaries |
| 1.0.0 | Freeze | No new features. Release candidates (`1.0.0-rc.N`) to review every flag name, default and config key. Close or document each item under Known limits. The CLI promise above is written into `CONTRIBUTING.md` |

## Not planned before 1.0

- Watch mode (`--watch`)
- Windows support
- Grammar-based highlighting (syntect or tree-sitter)
- Per-format or per-URL config settings, which would be more surface to freeze

## Small debts to fold in along the way

- In ordered lists that reach 10 items, items 10 and above indent their wrapped lines one column further than items 1–9
- The README gives the binary size as about 2.0 MB, which is out of date. Measure it again each release, and set a size budget in 0.9.0 once release binaries exist
