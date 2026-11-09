# Changelog

All notable changes to mu are recorded here. The format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org). See `CONTRIBUTING.md` for how versions are chosen and releases are cut.

## Unreleased

## 0.2.0 - 2026-11-09

### Added
- Detects the input format from the file extension, else from the content (including piped stdin), else falls back to Markdown
- `-f`/`--format <NAME>` forces a format; an unknown name is a usage error (exit 2)
- JSON: valid JSON and NDJSON are re-indented and colored; invalid or truncated JSON is colored as written
- AsciiDoc: title and attributes, sections, inline markup and links, lists and checklists, source/listing/quote/example/sidebar blocks, admonitions, tables, images
- HTML: pages from a file or `curl` are shown as documents (headings, paragraphs, lists, quotes, `pre` code, tables, links, images); scripts, styles and the head are hidden and entities decoded
- Org-mode: headings (with `#+TITLE`, TODO keywords and tags), emphasis, links, lists and checkboxes, src/quote/example blocks, admonition blocks, tables and footnotes; drawers and planning lines are hidden
- reStructuredText: titles, inline markup and links, lists, definition and field lists, literal and `code-block` blocks, admonitions, simple and grid tables, images
- YAML: syntax coloring for keys, scalars, comments, anchors, tags and block scalars, with the text left exactly as written
- TOML: syntax coloring for tables, keys, strings (including multi-line), numbers, dates, booleans and comments

### Changed
- Licensed under MIT
- The project and binary are renamed from `mdread` to `mu`

## 0.1.0 - 2026-11-09

First version, released as `mdread`.

### Added
- Markdown renderer for the terminal: CommonMark plus GFM tables, task lists, strikethrough, footnotes and alerts; front matter; word wrap to the terminal width
- `-w`/`--width`, `--color` and `--paging` options; `NO_COLOR` support
- Built-in pager with vi-style and arrow keys, mouse wheel, a status bar, and re-layout on resize
- Control characters in the input are scrubbed so a document cannot drive the terminal
