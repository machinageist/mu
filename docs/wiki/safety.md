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
- replay its own terminal hyperlink controls or other escape sequences.

The only escapes mu writes are its own: SGR styling, validated and encoded OSC 8 hyperlinks, and for images, 24-bit colors plus Kitty or iTerm2 image sequences whose payload mu encodes itself. Source escape sequences are scrubbed, never replayed.

## Hyperlink safety {#hyperlinks}

- Only semantic document links with validated absolute HTTP(S)/mailto targets can become external OSC 8 hyperlinks. Local files, relative paths, fragments outside EPUB and other schemes, including `file:`, `data:` and `javascript:`, are not actionable. URLs in source files, code blocks and data are not auto-linkified.
- Validation precedes URI encoding. Literal controls, including ESC, BEL, C1 and line breaks, backslashes, unsafe directional characters, malformed percent escapes and invalid/unsupported destinations are rejected, not removed to manufacture a different actionable URI. Rejected links keep readable, scrubbed fallback text rather than failing the document.
- Format adapters retain a semantic link's original scheme. Local-file prefixes are not stripped into web/mail actions. A replacement character in a destination also forces fallback, because parser or UTF-8 repair may have lost its original spelling.
- In EPUB chapters and package metadata, CDATA stays literal text and complete XML instructions/declarations cannot become links. Missing-image alt text also stays literal rather than creating markup actions. Hidden elements expose neither text nor IDs. Empty chapters and trailing anchors cannot borrow a following chapter's destination; ambiguous internal targets also stay inactive.
- Recovered chapter markup is closed before chapters are joined, so an unclosed heading, code block or table cell cannot swallow a later chapter's navigation boundary.
- Unicode and unsafe URI bytes are percent-encoded into a portable ASCII URI, bounded to 8192 encoded bytes. OSC 8 is terminated with ESC-backslash, not BEL.
- Hyperlinks close before line endings, padding, gutters, table borders, status bars and pager exit. Wrapped or clipped labels keep their destination without leaking it to neighboring text.
- `--links auto` uses a [conservative environment whitelist](/wiki/usage#links), not terminal queries or a capability handshake. `--links always` is explicit and independent of SGR color and `NO_COLOR`; `--links never` emits no OSC 8, including cleanup sequences.
- mu does not open or fetch external links when rendering them. Activation belongs to the terminal emulator. EPUB chapter links are separate in-pager keyboard jumps: relative paths/fragments are scoped within readable book chapters, and unresolved or hidden targets cannot jump elsewhere. No external opener, network request or invented ZIP-member file URI is involved.

## Network access {#network}

- A local file never makes mu touch the network unless you pass `--images fetch`.
- All network access goes through the system `curl`, run without a shell. The URL is a single argument after `--`, so it can't be read as a curl option.
- Protocols are limited to http(s), including on redirect, so a redirect to `file://` or another scheme is refused.
- SVG images can't load external files or URLs.
- Remote images: 10 s timeout and a 20 MB cap.
- Rendering external hyperlinks and following internal EPUB chapter links do not initiate network access. `--base` remains an image setting, not a resolver for document links.

## Output invariants {#invariants}

These come from `FEATURE/FEATURE_SPEC.md`. Tests sweep them across widths 30–120 for every format.

1. No output line is wider than the wrap width, except in the documented overflow cases: very deep nesting, tables with too many columns, and CSV or TSV tables whose columns have shrunk to 10.
2. At a fixed hyperlink policy, stripping SGR from color output gives exactly the plain output. Switching hyperlink policy may add or remove printed URL suffixes and therefore change layout.
3. Every styled line resets its style and closes its hyperlink before its newline, so the pager can draw any window of lines.
4. No control character from the input reaches the output.
5. Malformed input never panics or errors. It renders on a best-effort basis.
6. A closed pipe is a success. Exit codes: 0 ok, 1 read/write error, fetch failure or HTTP status of 400 or more, 2 usage error.

The pager keeps the same guarantees while it searches and scrolls:

- Search highlighting only inserts SGR. With styling removed, a highlighted line is exactly the original line, and it still resets before its newline.
- Search matches visible text with SGR and OSC 8 controls removed, never hidden destination bytes.
- Scrolling sideways cuts lines by display column and carries their SGR and OSC 8 state across the cut, so colors and destinations stay right. A wide character cut in half becomes spaces.
- The window title in the status bar has control characters removed.

## Robustness {#robustness}

- **Image decoders** run under `catch_unwind`. A malformed image that makes a third-party decoder panic shows its alt text instead.
- **Decode limits**: sources over 20 MB, images over 64 megapixels or 16384 px on a side are refused before decoding.
- **The terminal is always restored** after the pager: on normal exit, on error, and from a panic hook.
- **Deep JSON and XML**: either nested deeper than 256 levels is colored as written rather than re-indented.
- **Broken XML** is never re-indented. It is colored exactly as written, so nothing is lost.
- **Ragged CSV**: rows with fewer fields are padded with empty cells, and an unclosed quote runs to the end of the input rather than failing.
- **Invalid UTF-8** bytes are replaced while reading, so mu never refuses a file.
