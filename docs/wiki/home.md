---
title: Read docs and data without leaving the terminal
group: Guide
order: 0
summary: mu is the markup/markdown utility. It reads a file, stdin or a URL, works out the format, and prints styled, wrapped output for Markdown, HTML, reStructuredText, Org, AsciiDoc, man pages, Jupyter notebooks, JSON, YAML, TOML, XML, CSV and TSV, and shows source code as written.
---

```term notes.md — mu
<m>Release notes</m>
<m>═════════════</m>

mu draws <y>headings</y>, lists, quotes, tables and code
frames straight in your terminal.

<bl>Highlights</bl>
<bl>──────────</bl>

• Eleven formats, from Markdown to CSV
• Built-in pager with search, no <y>less</y> needed
☑ Images drawn in the terminal

<g>│</g> <g>Tip</g>
<g>│</g> Pipe anything in: <y>curl -s … | mu</y>

See <b>the docs</b> <d>(https://github.com/machinageist/mu)</d>.
```

## What it does {#features}

- **Documents** in Markdown, HTML, reStructuredText, Org and AsciiDoc get headings, lists, quotes, tables, framed code blocks and links, all from one shared layout engine.
- **Data** in JSON, YAML, TOML and XML gets token coloring. Valid JSON is re-indented as well, and so is XML whose structure allows it, including plists, SVG source and RSS or Atom feeds.
- **Tables** in CSV and TSV are drawn with box borders, a header row and right-aligned numbers. A CSV's delimiter is detected.
- **Source code** is shown verbatim, line for line, rather than read as Markdown.
- **URLs** are fetched with the system `curl` and presented like files. GitHub, GitLab and Gist page URLs become their raw files.
- **Images** standing alone in a document are drawn in the terminal with an exact aspect ratio.
- **A pager is built in.** Documents taller than the screen open in it automatically. It searches with `/` and `?`, lists headings with `o`, and scrolls wide tables sideways with `h` and `l`.
- **Outlines.** `mu --toc FILE` prints a document's headings, indented by level.
- **It's small.** A single ~2.4 MB binary with no config file. A typical README renders in about 2 ms over process start.
- **Untrusted input is safe to view.** Control characters from the document never reach your terminal.

## Install {#install}

```sh
cargo install --git https://github.com/machinageist/mu
# or, from a checkout
cargo install --path .
```

Requires Rust 1.85+ (edition 2024). Runs on Linux and macOS, where the terminal width comes from the `TIOCGWINSZ` ioctl. Opening URLs and fetching remote images needs `curl` on the `PATH`.

## Quick start {#quick-start}

```sh
mu README.md
mu https://example.com/                    # HTML, with its images
mu https://api.github.com/repos/rust-lang/rust   # JSON, re-indented
mu https://github.com/machinageist/mu/blob/main/README.md   # the raw file, not the page
mu -I https://example.com/                 # response headers as a table
curl -s https://example.com/doc.md | mu    # pipes still work
mu Cargo.toml
mu data.csv                                # a box-drawn table
mu Info.plist                              # XML, re-indented
mu --toc README.md                         # just the headings
mu -f rst < docs/index.txt
mu -w 72 notes.org
mu --images fetch page.html                # also download remote images
mu --paging never CHANGELOG.md | grep -i fix
```
