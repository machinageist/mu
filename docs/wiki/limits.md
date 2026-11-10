---
title: Limits & performance
group: Project
order: 3
summary: What mu doesn't do yet, and how fast it is
---

What mu deliberately leaves out, what it doesn't do yet, and how fast it is.

## Known limits {#known-limits}

### Rendering {#limits-rendering}

- No syntax highlighting in code blocks. This is deliberate, because it would cost startup time and binary size. Whole JSON, YAML and TOML files are colored; code blocks inside documents aren't.
- Links aren't clickable (no OSC 8 hyperlinks). The URL is printed instead.
- Definition lists, math and wiki links aren't enabled in Markdown; they render as plain text.
- In ordered lists that cross into two-digit numbers, items 10+ indent their wrapped lines one column further than items 1–9.
- A table with too many columns to fit at 3 columns each overflows the width rather than becoming unreadable.
- Very deep nesting stops shrinking content below 16 columns and overflows instead.

### Formats {#limits-formats}

- The non-Markdown parsers cover the subset real READMEs and pages use, not the full specs: no table cell spans (HTML, RST, AsciiDoc), no RST line blocks or letter/roman enumerators, no AsciiDoc footnotes, and one header row per Org table.
- YAML and TOML are colored, never reformatted. JSON nested deeper than 256 levels is colored as written rather than re-indented.
- Content detection reads the first 8 KB. Short or unusual inputs may be shown as Markdown; use `-f`.

### Pager, images and URLs {#limits-other}

- The pager has no search (`/`) yet.
- Images: no EXIF orientation for JPEGs, no animated GIFs, no SVG text, and no images inline within a line of text.
- URLs: no caching between runs, and charsets other than UTF-8 are shown with replacement characters. A repository's front page is fetched as HTML; point at the README's blob URL instead.

## Performance {#performance}

Measured 2026-11-09 on a 12-core Linux box under background load, so treat these as rough:

| Input | Time |
|---|---|
| 7 KB README | ≈2 ms per run over a `cat` baseline (process start dominates). `bat` on the same file takes ≈120 ms |
| 9 MB corpus (909 real crate READMEs and CHANGELOGs) | ≈0.25–0.35 s, ≈26–40 MB/s |
| Parser alone on the same corpus | ≈60–100 MB/s |
| 5.7 MB minified JSON, re-indented and colored (Apple Silicon Mac) | ≈0.3 s |

Layout roughly doubles the parse cost. There's headroom there if throughput on multi-megabyte files ever matters. For normal documents, the run time is process start.
