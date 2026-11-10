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

## 7. Docs and release
README (pager table, Usage, `--toc`), `FEATURE_SPEC.md` (a Pager section), and `--help`. CHANGELOG `Added` entries. Release 0.5.0 per `CONTRIBUTING.md`, push, and create the GitHub release if that's allowed.
