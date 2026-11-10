# mu — feature spec

mu is short for *markup/markdown utility*.

Long-lived behaviour and invariants. Update this when behaviour ships.

## Purpose

A terminal presenter that turns markup and data into readable, styled, wrapped text so the reader never has to leave the terminal or parse raw syntax by eye. Small, fast, few dependencies, no config file.

## Inputs

- A file path, or stdin when the path is `-` or absent (`cat x | mu`, `curl … | mu`, `mu < x`)
- An `http://` or `https://` URL, fetched with the system `curl` (see URLs)
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
2. For a URL, the response `Content-Type`. `text/plain` and `application/octet-stream` are skipped
3. File extension (case-insensitive), or for a URL the extension of its path, never its host
4. Content sniffing on the first 8 KB: used for stdin and for unknown or missing extensions. The order is JSON, HTML, Org, RST, AsciiDoc, TOML, YAML, and each rule is conservative
5. Fallback: Markdown, which shows plain text correctly too

`tests/sniff.rs` pins every fixture to its format. A sniffing change must keep it green.

## Invariants, all formats

- No output line is wider than the wrap width, except the documented overflow cases
- Stripping SGR from color output gives exactly the plain output
- Every styled line resets its style before its newline, so the pager can draw any window of lines
- No control character from the input reaches the output. The only escapes mu writes are its own: SGR codes, 24-bit colors for half-block images, and Kitty or iTerm2 image sequences with payloads mu encodes
- Malformed input never panics or errors. It renders on a best-effort basis
- A closed pipe is a success. Exit codes: 0 ok, 1 read/write error, fetch failure or HTTP status of 400 or more, 2 usage error

## Pager

- Pages only when stdout is a terminal. It reads keys from `/dev/tty` and waits on it with `select()`, because macOS `poll()` rejects `/dev/tty`
- A resize re-renders at the new width when the width is automatic, keeping the reader's place proportionally. Any search and the outline follow the new layout
- **Search** (`/` `?` `n` `N` `Esc`):
  - Literal substrings with smart case, matched against each line with SGR removed. A match never spans two lines
  - `n` and `N` wrap round the ends. A match already on screen does not scroll the view
  - Highlighting only inserts SGR. With styling removed, a highlighted line equals the original, and it still resets before its newline
- **Outline** (`o`):
  - The renderer records each heading's level, plain text and first output line
  - The list opens on the section holding the top line, and `Enter` puts the heading's line at the top, clamped at the end of the document
  - `--toc` prints the same outline, indented two spaces per level below the shallowest. Data formats have no headings


- An image alone in its paragraph or block is drawn. Inline images, failures, color off and `--images never` all show `[image: alt]`
- The aspect ratio is exact to within half a cell, using the terminal's reported cell pixels or an assumed 1:2 cell. Images are never enlarged, never wider than the content column and never taller than the screen minus 2 rows
- No image network access unless `--images fetch`, which a URL input implies unless `--images` is given. Remote loads go through the system `curl`, limited to http(s), including on redirect
- Decode limits: 20 MB per source, 64 megapixels, 16384 px per side
- The pager always uses half blocks. Native protocols are used only when writing straight to a supporting terminal outside tmux

## URLs

- An argument starting with `http://` or `https://` (any case) is a URL. Anything else is a path
- Fetched with the system `curl`, no shell, the URL one argument after `--`. Protocols are limited to http(s), including on redirect. Redirects are followed. Connect timeout 10 s
- URL options (`-H -X -d -A -u -k -i -I --timeout --curl --no-rewrite`) are a usage error without a URL. `--curl` arguments pass to curl verbatim
- GitHub `blob`, GitLab `-/blob` and Gist page URLs are fetched as raw files unless `--no-rewrite`. The query and fragment are dropped
- `-i` shows the final response's status and headers as a table before the body. `-I` shows only that table. Redirect, 1xx and proxy-tunnel header blocks are never shown
- Status 400 or more: the body is rendered, then `mu: <url>: HTTP <code>` on stderr, exit 1. Transport failure: curl's message, exit 1
- The pager title is the URL as typed. A resize re-renders without fetching again
