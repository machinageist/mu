---
title: Safety & invariants
group: Internals
order: 3
summary: Guarantees every format keeps
---

mu is meant for reading files and pages you didn't write. These guarantees hold for every format and are checked by the test suite.

## Untrusted input {#untrusted-input}

Control characters in the document, including ESC, BEL and C1 codes, are replaced with `�` (U+FFFD) before output. A hostile file therefore can't:

- set your terminal title,
- write to your clipboard via OSC 52,
- clear the screen or move the cursor, or
- inject hyperlinks or other escape sequences.

The only escapes mu writes are its own: SGR styling, and for images, 24-bit colors plus Kitty or iTerm2 image sequences whose payload mu encodes itself.

## Network access {#network}

- A local file never makes mu touch the network unless you pass `--images fetch`.
- All network access goes through the system `curl`, run without a shell. The URL is a single argument after `--`, so it can't be read as a curl option.
- Protocols are limited to http(s), including on redirect, so a redirect to `file://` or another scheme is refused.
- SVG images can't load external files or URLs.
- Remote images: 10 s timeout and a 20 MB cap.

## Output invariants {#invariants}

These come from `FEATURE/FEATURE_SPEC.md`. Tests sweep them across widths 30–120 for every format.

1. No output line is wider than the wrap width, except in the documented overflow cases: very deep nesting, tables with too many columns, and CSV or TSV tables whose columns have shrunk to 10.
2. Stripping SGR from color output gives exactly the plain output.
3. Every styled line resets its style before its newline, so the pager can draw any window of lines.
4. No control character from the input reaches the output.
5. Malformed input never panics or errors. It renders on a best-effort basis.
6. A closed pipe is a success. Exit codes: 0 ok, 1 read/write error, fetch failure or HTTP status of 400 or more, 2 usage error.

The pager keeps the same guarantees while it searches and scrolls:

- Search highlighting only inserts SGR. With styling removed, a highlighted line is exactly the original line, and it still resets before its newline.
- Scrolling sideways cuts lines by display column and carries their SGR state across the cut, so colors stay right. A wide character cut in half becomes spaces.
- The window title in the status bar has control characters removed.

## Robustness {#robustness}

- **Image decoders** run under `catch_unwind`. A malformed image that makes a third-party decoder panic shows its alt text instead.
- **Decode limits**: sources over 20 MB, images over 64 megapixels or 16384 px on a side are refused before decoding.
- **The terminal is always restored** after the pager: on normal exit, on error, and from a panic hook.
- **Deep JSON and XML**: either nested deeper than 256 levels is colored as written rather than re-indented.
- **Broken XML** is never re-indented. It is colored exactly as written, so nothing is lost.
- **Ragged CSV**: rows with fewer fields are padded with empty cells, and an unclosed quote runs to the end of the input rather than failing.
- **Invalid UTF-8** bytes are replaced while reading, so mu never refuses a file.
