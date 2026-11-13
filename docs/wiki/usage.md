---
title: Usage
group: Guide
order: 1
summary: Flags, width, color, paging, outlines and exit codes
---

One command, one optional argument. Everything else has a sensible default.

## Synopsis {#synopsis}

```text
mu [OPTIONS] [FILE | URL]

Reads FILE, or stdin when FILE is - or absent, or fetches an http(s) URL.
```

- `mu file.md` reads a file.
- `cat file.md | mu`, `mu - < file.md` and `mu < file.md` read stdin.
- An argument starting with `http://` or `https://` (any case) is a URL. See [URLs](/wiki/urls).
- Bare `mu` at an interactive prompt prints usage instead of waiting on stdin.

## Options {#options}

| Flag | Meaning | Default |
|---|---|---|
| `-f`, `--format <NAME>` | Force a format: `markdown`, `html`, `rst`, `org`, `asciidoc`, `json`, `yaml`, `toml`, `csv`, `tsv`, `xml`, `man`, `ipynb`, `epub`, `pdf`, `code`. The aliases `md`, `htm`, `restructuredtext`, `adoc`, `yml`, `plist`, `roff`, `mdoc`, `jupyter`, `notebook` and `text` work too, in any case. `code` shows the input verbatim. An unknown name is a usage error. | detected (see [Formats](/wiki/formats#selection)) |
| `--images <MODE>` | `auto`, `fetch` or `never`. `fetch` also downloads http(s) images. | `auto`, or `fetch` for a URL |
| `--base <DIR\|URL>` | Resolve relative image paths here. | the input file's folder, or the final URL |
| `--toc` | Print the document's outline and exit. See [Outline](#toc). |  |
| `-w`, `--width <N>` | Wrap at N columns, minimum 20. | terminal width, capped at 100 except for CSV and TSV |
| `--color <WHEN>` | `auto`, `always` or `never`. | `auto`, honours `NO_COLOR` |
| `--links <WHEN>` | `auto`, `always` or `never`; controls external OSC 8 hyperlinks, not EPUB chapter jumps. | `auto`, recognized styled terminals only |
| `--paging <WHEN>` | `auto`, `always` or `never`. | `auto` |
| `-h`, `--help` | Print usage. |  |
| `-V`, `--version` | Print `mu X.Y.Z`. |  |

The curl-like options (`-H -X -d -A -u -k -i -I --timeout --curl --no-rewrite`) only apply to URLs and are listed on the [URLs](/wiki/urls#url-options) page. Using them without a URL is a usage error.

## Width {#width}

- `-w N` wins, with a minimum of 20.
- Otherwise mu uses the terminal width, clamped to 20–100 columns so lines stay readable.
- CSV and TSV tables and PDF pages are the exception: they use the full terminal width, with no 100-column cap. See [Tables](/wiki/formats#tables) and [PDF](/wiki/formats#pdf).
- The width is probed on stdout, then stderr, then stdin, so `mu x.md | less -R` still sees the real width.
- With no terminal attached at all, the width is 80.

## Color {#color}

- `auto`: color is on when stdout is a terminal and `NO_COLOR` is unset or empty.
- `always` keeps styling when piping elsewhere, for example into `less -R`.
- `never` disables SGR styling. Turning color off also turns images off, so each image shows its `[image: alt]` text. Explicit `--links always` still emits OSC 8 hyperlinks; use `--links never` as well for escape-free document text.

## Links {#links}

`--links auto|always|never` controls terminal OSC 8 hyperlinks for semantic document links:

- `auto` needs terminal stdout, color enabled, and a recognized environment. Unknown terminals and output to a pipe or file retain printed URLs.
- `always` is an explicit override, independent of `--color never`, `NO_COLOR` and terminal detection. It also emits OSC 8 when output is piped; only use it where the destination understands that protocol.
- `never` emits no OSC 8. It does **not** disable the pager's [EPUB chapter navigation](/wiki/pager#chapter-links).

Auto recognizes these exact environment markers, rather than probing the terminal:

| Marker | Accepted value |
|---|---|
| `TERM` | `xterm-kitty` or `xterm-ghostty` |
| `TERM_PROGRAM` | `iTerm.app`, `WezTerm` or `ghostty` |
| `VTE_VERSION` | Nonempty digits only, parsing as `u32` and at least `5000` |

Any one marker is sufficient, but a set `TMUX` or `STY`, `TERM=dumb`, or a `TERM` starting with `screen` or `tmux` disables auto regardless of the others. This is a conservative heuristic, not a capability handshake or a guarantee about an emulator's mouse gesture. mu sends no capability query and starts no detector subprocess.

A usable absolute HTTP(S)/mailto link shows its label without a redundant `(URL)` suffix; otherwise the existing printed-URL fallback remains, with no repeated autolinks. Literal control characters and unsupported or unsafe destinations are rejected before URI encoding. The ASCII URI payload is bounded at 8192 encoded bytes. Relative paths and fragments outside EPUB are not resolved, and `--base` remains image-only. URLs in source files, code blocks and data are not auto-linkified. See [Rendering](/wiki/rendering#links) and [Safety](/wiki/safety#hyperlinks).

## Paging {#paging}

- `auto`: output that would push the prompt off screen opens in the [pager](/wiki/pager). Shorter output prints straight to the terminal, like `less -F`.
- `always` opens the pager even for short output.
- Output to a pipe or file is never paged, whatever the setting.

## Outline {#toc}

`--toc` prints the document's headings and exits, without paging and without loading images. The first lines for mu's own README:

```term mu --toc README.md
<m>mu</m>
  <bl>Install</bl>
  <bl>Usage</bl>
  <bl>URLs</bl>
  <bl>Links</bl>
  <bl>Pager</bl>
  <bl>Formats</bl>
```

- Each heading is indented two spaces per level below the shallowest one present, so a document without a level-1 heading still starts at the margin.
- With color on, each heading takes its level's color. A heading wider than the width is cut with `…`.
- It works for every document format, and for files, stdin and URLs. Data formats and CSV/TSV tables have no headings, so they print nothing.
- The pager shows the same list on `o`. See [Outline](/wiki/pager#outline).

## Exit codes {#exit-codes}

| Code | Meaning |
|---|---|
| `0` | Success, including a closed pipe such as `mu x.md \| head` |
| `1` | Read or write error, fetch failure, an HTTP status of 400 or more, an unreadable EPUB, or a PDF without `pdftotext` |
| `2` | Usage error: unknown flag, bad value, a URL flag without a URL, or bare `mu` at a prompt |

## Recipes {#recipes}

```sh
# Read a project's changelog and grep it, no pager
mu --paging never CHANGELOG.md | grep -i fix

# Keep colors through another pager
mu --color always README.md | less -R

# Force external hyperlinks without color, for an OSC 8-capable terminal
mu --links always --color never notes.md

# EPUB chapter jumps still work: Tab / Shift-Tab, Enter, Backspace
mu --paging always --links never --color never book.epub

# A .txt file that is really reStructuredText
mu -f rst docs/index.txt

# Narrow column for side-by-side reading
mu -w 60 notes.org

# The section list of a long document
mu --toc docs/guide.rst

# A semicolon-separated export as a table
mu export.csv

# Any file verbatim, with no Markdown interpretation
mu -f text notes.txt

# A macOS property list, re-indented
mu Info.plist

# Piped HTML with images relative to a site
curl -s https://example.com/ | mu --base https://example.com/ --images fetch
```
