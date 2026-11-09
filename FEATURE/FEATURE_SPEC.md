# mu — feature spec

mu is short for *markup/markdown utility*.

Long-lived behaviour and invariants. Update this when behaviour ships.

## Purpose

A terminal presenter that turns markup and data into readable, styled, wrapped text so the reader never has to leave the terminal or parse raw syntax by eye. Small, fast, few dependencies, no config file.

## Inputs

- A file path, or stdin when the path is `-` or absent (`cat x | mu`, `curl … | mu`, `mu < x`)
- Bare `mu` at an interactive prompt prints usage instead of waiting on stdin

## Formats

| Format | Kind | Extensions |
|---|---|---|
| Markdown (CommonMark + GFM) | document | `.md` `.markdown` `.mdown` `.mkd` |
| HTML | document | `.html` `.htm` `.xhtml` |
| reStructuredText | document | `.rst` `.rest` |
| Org-mode | document | `.org` |
| AsciiDoc | document | `.adoc` `.asciidoc` `.asc` |
| JSON | data | `.json` `.jsonl` `.geojson` |
| YAML | data | `.yaml` `.yml` |
| TOML | data | `.toml` |

Document formats share one layout engine. Each one gets the same headings, lists, quotes, tables, code frames, links and pager. Data formats are syntax-highlighted. Valid JSON is also re-indented.

## Format selection, first match wins

1. `-f/--format <name>` flag
2. File extension (case-insensitive)
3. Content sniffing on the first 8 KB: used for stdin and for unknown or missing extensions. The order is JSON, HTML, Org, RST, AsciiDoc, TOML, YAML, and each rule is conservative
4. Fallback: Markdown, which shows plain text correctly too

`tests/sniff.rs` pins every fixture to its format. A sniffing change must keep it green.

## Invariants, all formats

- No output line is wider than the wrap width, except the documented overflow cases
- Stripping SGR from color output gives exactly the plain output
- Every styled line resets its style before its newline, so the pager can draw any window of lines
- No control character from the input reaches the output. The only escapes mu writes are its own: SGR codes, 24-bit colors for half-block images, and Kitty or iTerm2 image sequences with payloads mu encodes
- Malformed input never panics or errors. It renders on a best-effort basis
- A closed pipe is a success. Exit codes: 0 ok, 1 read/write error, 2 usage error

## Images

- An image alone in its paragraph or block is drawn. Inline images, failures, color off and `--images never` all show `[image: alt]`
- The aspect ratio is exact to within half a cell, using the terminal's reported cell pixels or an assumed 1:2 cell. Images are never enlarged, never wider than the content column and never taller than the screen minus 2 rows
- No network access unless `--images fetch`. Remote loads go through the system `curl`, limited to http(s), including on redirect
- Decode limits: 20 MB per source, 64 megapixels, 16384 px per side
- The pager always uses half blocks. Native protocols are used only when writing straight to a supporting terminal outside tmux
