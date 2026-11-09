# mu

A small, fast Markdown renderer for the terminal. It reads a file or stdin, wraps text to the terminal width, and prints styled output: headings, lists, quotes, tables, framed code blocks, links and footnotes. Documents taller than the screen open in a built-in pager. It is a single ~750 KB binary with three dependencies and no config file.

```sh
mu README.md
curl -s https://example.com/doc.md | mu
mu -w 72 notes.md
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

  -w, --width <N>      Wrap at N columns (default: terminal width, capped at 100)
      --color <WHEN>   auto, always or never (default: auto, honours NO_COLOR)
      --paging <WHEN>  auto, always or never (default: auto, pages when taller than the screen)
  -h, --help
  -V, --version
```

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

## What renders

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

- **Parser:** [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) with default features off and `simd` on (CommonMark plus GFM tables, strikethrough, task lists, footnotes, alerts). The other dependencies are `unicode-width` (CJK and emoji columns) and `libc` (one ioctl).
- **Theme:** only the 16 base ANSI colors plus bold, dim, italic, underline and strike, so output follows your terminal's palette instead of fighting it.
- **Untrusted input is safe to view:** control characters in the document (including ESC, BEL and C1 codes) are replaced with `�` before output, so a hostile file cannot set your title, write to the clipboard via OSC 52, or clear the screen. The only escapes mu emits are its own SGR codes.
- **Pipeline:** events → per-block buffer of styled word atoms in one text arena → word wrap → container gutters (quote bars, list indents) prepended per line → one buffered write, or the pager. Styles are emitted lazily and every line closes its own style, so the pager can draw any window of lines on its own.
- **Pager terminal handling:** raw mode via `termios`, a self-pipe for `SIGWINCH` so a resize can never be missed between a redraw and the next `poll()`, synchronized-output frames to avoid flicker, and the terminal restored on exit, on error and from a panic hook (release builds abort on panic, which skips destructors).

```text
src/
  main.rs    CLI: args, input, width/color/paging decisions, single write
  pager.rs   built-in pager: raw mode, keys, scrolling, resize, status bar
  term.rs    terminal size via TIOCGWINSZ
  lib.rs     public API: render(&str, &Options) -> String
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

Layout roughly doubles the parse cost. There is headroom there if throughput on multi-megabyte files ever matters; for normal documents the run time is process start.

## Development

```sh
cargo test --release
cargo clippy --all-targets
cargo run --release -- -w 60 tests/fixtures/kitchen-sink.md
```

`tests/render.rs` has exact-output cases for each construct plus invariants swept across widths 30–120: no line exceeds the width, color output with SGR stripped equals plain output, every styled line resets before its newline, and no control character from the input reaches the output. `src/pager.rs` unit-tests key decoding, scroll clamping, resize behaviour and the status bar; the pager's terminal I/O was checked by hand in a pseudo-terminal.

## Known limits

- No syntax highlighting in code blocks (deliberate: it would cost startup time and binary size).
- The pager has no search (`/`) yet.
- Links are not clickable (no OSC 8 hyperlinks); the URL is printed instead.
- In ordered lists that cross into two-digit numbers, items 10+ indent their wrapped lines one column further than items 1–9.
- A table with too many columns to fit at 3 columns each overflows the width rather than becoming unreadable.
- Very deep nesting stops shrinking content below 16 columns and overflows instead.
- Definition lists, math and wiki links are not enabled; they render as plain text.
