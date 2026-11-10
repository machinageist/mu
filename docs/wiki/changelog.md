---
title: Changelog
group: Project
order: 2
summary: Every release from 0.1.0 to 0.6.0, and what's unreleased
---

Every notable change, following [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/). Mirrors `CHANGELOG.md`.

## Unreleased {#unreleased}

Changes merged since 0.6.0, not yet in a release.

### Added {#unreleased-added}

- Man pages in roff source are shown as documents: `mu ls.1`, or any `.1` to `.9` page (`.3p`, `.1ssl` too), `.man` or `.mdoc` file. Both the `man` macros (Linux, GNU) and the `mdoc` macros (BSD, macOS) are understood: sections become headings and fill the outline, option lists become terms with indented descriptions, and fonts, escapes, links and examples carry over. Recognised by `text/troff`, `-f man`/`roff`/`mdoc`, and a `.TH` or `.Dd` first request in piped input
- `man ls | mu`: formatted man output, with its backspace overstrikes, shows bold and underline as styles instead of stray characters. Sections and subsections fill the outline and take the heading colors. SGR bold and underline from `groff -T utf8` or `MAN_KEEP_FORMATTING` work too
- Jupyter notebooks (`.ipynb`, nbformat 4) are shown as one document: Markdown cells as prose, code cells as code blocks in the notebook's language with their `In [n]:` count, and outputs below them. Text, errors (with colors removed) and Markdown outputs are shown, and PNG, JPEG and SVG plots are drawn like any other image. Headings fill the outline. Recognised by `application/x-ipynb+json`, `-f ipynb`/`jupyter`/`notebook`, and in piped input. Other notebook versions are shown as JSON

### Fixed {#unreleased-fixed}

- Source files (`.c`, `.h`, `.cpp`, `.rs`, `.py`, `.js`, `.go`, `.sh` and about 80 other extensions) are shown verbatim instead of being read as Markdown, which joined lines and ate `*` and `_`. So are piped scripts that start with `#!`. `-f code` (or `-f text`) shows any input verbatim

## 0.6.0 · 2026-11-09 {#v0-6-0}

### Added {#v0-6-0-added}

- Pager: `h`/`l` and `←`/`→` scroll sideways by half a window when lines are wider than the screen, such as wide tables. The status bar shows `col N` while scrolled, and a search scrolls sideways to reach its match
- XML (including plist, SVG source, RSS and Atom) is highlighted, and re-indented two spaces per level when tags balance and no element mixes text with child elements; anything else is shown as written. Recognised by `.xml`, `.plist`, `.svg`, `.rss`, `.atom` and similar, `application/xml`, `text/xml` and `+xml` media types, `-f xml`/`plist`, and an `<?xml` declaration or well-known root in piped input
- CSV and TSV are shown as box-drawn tables: the first row is the header, numeric columns are right-aligned, quoting follows RFC 4180, and a CSV's delimiter (`,` `;` tab `|`) is detected. Tables use the full terminal width, and columns stop shrinking at 10 so wide tables overflow rather than crush. Recognised by `.csv`, `.tsv`, `.tab`, `text/csv`, `text/tab-separated-values`, `-f csv`/`tsv`, and in piped input

## 0.5.0 · 2026-11-09 {#v0-5-0}

### Added {#v0-5-0-added}

- Pager search: `/` and `?` search forward and back, `n` and `N` move between matches with wrap-around, matches are highlighted, and the status bar shows the match count. Lowercase queries ignore case. `Esc` clears the highlighting
- Pager outline: `o` lists the document's headings, starting on the section being read; `Enter` jumps to one and `o`, `q` or `Esc` closes the list. Works for every document format
- `--toc` prints a document's outline (its headings, indented by level) and exits. Works for files, stdin and URLs; data formats print nothing

### Fixed {#v0-5-0-fixed}

- On macOS, resizing the window while paging re-renders at once; it used to wait for the next key press

## 0.4.0 · 2026-11-09 {#v0-4-0}

### Added {#v0-4-0-added}

- `mu <url>` fetches an http(s) URL with the system `curl` and presents it. The format comes from `-f`, then the response `Content-Type`, then the URL path's extension, then the content
- curl-like URL options: `-H`, `-X`, `-d`, `-A`, `-u`, `-k`, `-i` (status and headers as a table, then the body), `-I` (status and headers only), `--timeout`, and `--curl <ARG>` passed straight to curl
- GitHub, GitLab and Gist page URLs are fetched as their raw file; `--no-rewrite` fetches the URL as typed
- Images in a fetched page download by default and resolve against the final URL; `--images` overrides
- An HTTP status of 400 or above still shows the body, then reports `HTTP <code>` and exits 1

## 0.3.0 · 2026-11-09 {#v0-3-0}

### Added {#v0-3-0-added}

- Images: a picture standing alone in a document (HTML `<img>`/`<figure>`, Markdown `![]()` on its own line, RST/AsciiDoc image blocks, Org image links) is drawn in the terminal with its exact aspect ratio and the alt text as a caption. PNG, JPEG, GIF (first frame), WebP and SVG
- Images draw with the Kitty graphics protocol (kitty, Ghostty) or iTerm2 inline images (iTerm2, WezTerm) when printing straight to those terminals, and as 24-bit-color half blocks in the pager and everywhere else
- `--images <auto|fetch|never>`: `auto` loads local files and `data:` URIs, `fetch` also downloads http(s) images with the system `curl`, `never` keeps alt text
- `--base <DIR|URL>` sets where relative image paths resolve; defaults to the input file's folder
- A malformed image that makes a decoder panic shows its alt text instead of ending mu

### Changed {#v0-3-0-changed}

- Library: `Options` gains an `images` field (`ImageOptions`, images off by default) and an `Options::new(width, color)` constructor

## 0.2.0 · 2026-11-09 {#v0-2-0}

### Added {#v0-2-0-added}

- Detects the input format from the file extension, else from the content (including piped stdin), else falls back to Markdown
- `-f`/`--format <NAME>` forces a format; an unknown name is a usage error (exit 2)
- JSON: valid JSON and NDJSON are re-indented and colored; invalid or truncated JSON is colored as written
- AsciiDoc: title and attributes, sections, inline markup and links, lists and checklists, source/listing/quote/example/sidebar blocks, admonitions, tables, images
- HTML: pages from a file or `curl` are shown as documents (headings, paragraphs, lists, quotes, `pre` code, tables, links, images); scripts, styles and the head are hidden and entities decoded
- Org-mode: headings (with `#+TITLE`, TODO keywords and tags), emphasis, links, lists and checkboxes, src/quote/example blocks, admonition blocks, tables and footnotes; drawers and planning lines are hidden
- reStructuredText: titles, inline markup and links, lists, definition and field lists, literal and `code-block` blocks, admonitions, simple and grid tables, images
- YAML: syntax coloring for keys, scalars, comments, anchors, tags and block scalars, with the text left exactly as written
- TOML: syntax coloring for tables, keys, strings (including multi-line), numbers, dates, booleans and comments

### Changed {#v0-2-0-changed}

- Licensed under MIT
- The project and binary are renamed from `mdread` to `mu`

## 0.1.0 · 2026-11-09 {#v0-1-0}

First version, released as `mdread`.

### Added {#v0-1-0-added}

- Markdown renderer for the terminal: CommonMark plus GFM tables, task lists, strikethrough, footnotes and alerts; front matter; word wrap to the terminal width
- `-w`/`--width`, `--color` and `--paging` options; `NO_COLOR` support
- Built-in pager with vi-style and arrow keys, mouse wheel, a status bar, and re-layout on resize
- Control characters in the input are scrubbed so a document cannot drive the terminal
