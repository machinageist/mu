# Plan: pager search and outline (0.5.0)

Gate for every commit:

```sh
cargo fmt --check && cargo test --release --no-fail-fast && cargo clippy --all-targets -- -D warnings
```

## 1. Headings from the renderer
`Renderer` records `Heading { level, text, line }` in `end_heading`, keeping a running count of newlines rather than counting them again each time. Add `Rendered` and `present_outlined` to the library. `present` stays as a thin wrapper, so existing callers and output don't change.
→ verify: a fixture sweep in `tests/outline.rs` covers every document format at widths 30–120. Existing suites stay byte-identical.

## 2. `--toc`
Add the CLI flag. It prints the outline through a small `outline::print`, with indentation, heading styles and the width cut, then exits without paging.
→ verify: CLI tests in `tests/outline.rs` for Markdown (exact output), HTML, JSON (empty) and a URL from the local server.

## 3. Search engine
In `src/pager/search.rs`, which turns `pager.rs` into a `pager/` module:
- a per-line cache of stripped text
- smart-case matching
- `find(from, direction)` with wrap-around
- the current match and the match count
- `highlight(line, ranges, current)`, which re-inserts reverse video and bold around matches and restores the line's own SGR state after each match
→ verify: unit tests for matching, wrap-around, counts, and the highlight invariants: styling removed gives the same text, reset before the newline, and wide characters.

## 4. Prompt and keys
- Add a prompt mode to the pager loop (`/`, `?`, editing keys, Enter, Esc).
- Add `n`, `N` and `Esc` to clear highlighting.
- Show the status bar variants (query and count, `wrapped`, `not found`), and the new hint.
- Parse keys by mode: a lone `Esc` byte only counts as Esc when no `[` or `O` follows in the same read.
- Re-run the search after a re-layout.

→ verify: unit tests for prompt editing, key parsing per mode, and the status bar text at fixed widths.

## 5. Outline overlay
- Add `src/pager/outline.rs`: list state, starting selection, scrolling, and drawing at the window size.
- `o` opens it, `Enter` jumps there, and the close keys close it.
- `relayout` returns `Rendered`.

→ verify: unit tests for the starting selection, movement and scrolling, the jump target, and no headings.

## 6. Manual pty check
Drive `mu` in a pseudo-terminal by hand:
- search forward and back, `n`/`N` with wrap-around
- not found
- the outline open and jump
- a resize during a search, and one with the outline open

Record the results here.

**Result, 2026-11-09:** scripted in a real pseudo-terminal, with resizes sent through the kernel's own SIGWINCH.
- **Search:** `/` and `?` work, `n` and `N` wrap, not-found shows and clears on the next key, `Esc` clears, and prompt editing works.
- **Resize during a search:** the page re-lays out (81 lines become 83 at 60 columns) and the search is kept.
- **Outline:** it opens on the section being read, and a resize with it open redraws at the new width. `Enter` jumps, clamped at the end, and `q` exits 0.
- **Bug found:** a resize waited for the next key on macOS. `poll()` returns POLLNVAL for `/dev/tty`, so the loop sat in a blocking read. The bug was already in 0.4.0. It is fixed by waiting with `select()`.
- **Regression test:** `tests/pager_pty.rs` now drives the real pager in a pty as part of the test suite, and its resize test fails on the old code.

## 7. Docs and release
README (pager table, Usage, `--toc`), `FEATURE_SPEC.md` (a Pager section), and `--help`. CHANGELOG `Added` entries. Release 0.5.0 per `CONTRIBUTING.md`, push, and create the GitHub release if that's allowed.
