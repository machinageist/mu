# Change: multi-format presenter

## Outcome

mu stops being Markdown-only. It detects HTML, reStructuredText, Org-mode, AsciiDoc, JSON, YAML and TOML by extension or by content, including on stdin, and presents each one pretty-printed. A user can override detection with `--format`.

## Decisions (machinageist, 2026-11-09)

- Formats: HTML, RST, Org, AsciiDoc, JSON, YAML, TOML
- Unclassifiable input falls back to Markdown, and `-f/--format` forces a parser
- Data formats are in scope. Goal: less cognitive strain without leaving the terminal

## Design

- **Document formats → pulldown-cmark events.** Each format has a hand-written, tolerant parser that emits `pulldown_cmark::Event`s. `render.rs` consumes the events unchanged, so layout, wrapping, tables, scrubbing and the pager are shared. No new dependencies.
- **Data formats → highlighter.** A tokenizer turns the input into styled lines (keys, strings, numbers, booleans/null, comments, punctuation). The lines go through the existing hard-wrap with the `┆` continuation bar. JSON that parses is re-indented with 2 spaces. Invalid JSON, YAML and TOML are highlighted as written.
- **Library API.** `render(&str, &Options)` stays Markdown-only, so existing tests keep their meaning. Add a `Format` enum, `detect(path, input) -> Format`, and `present(&str, Format, &Options) -> String`.
- **Sniffing** runs on the first 8 KB. It is conservative: when unsure, the answer is Markdown.
  - JSON: the first non-space char is `{` or `[` and the document tokenizes as JSON
  - HTML: starts with `<!doctype html`, `<html`, `<head`, `<body`, or `<?xml` followed by `<html`
  - Org: a `#+TITLE:`/`#+BEGIN_` line, or `* ` headings with no Markdown signals
  - RST: an `.. directive::` line, or a title with over- and underline adornment
  - AsciiDoc: first line `= Title`, a `:attr:` header, or `[source,…]`/`----` delimited blocks
  - TOML: `[table]` headers plus `key = value` lines
  - YAML: `key: value` / `- item` lines dominate and there are no Markdown block markers

## Acceptance criteria

- `mu file.<ext>` picks the right renderer for every extension in FEATURE_SPEC
- `cat file.<ext> | mu` picks the same renderer for each fixture, by sniffing
- `--format md|markdown|html|rst|org|adoc|asciidoc|json|yaml|toml` overrides detection. An unknown name is a usage error (exit 2)
- The existing Markdown tests pass unchanged
- Every new fixture passes the four invariants in FEATURE_SPEC across widths 30–120
- Each format has exact-output tests for its main constructs
- No new crate dependencies, and a release binary under 1 MB

## Out of scope for this change

- Syntax highlighting inside code blocks of document formats
- CSV/TSV, XML (other than XHTML), man/roff, LaTeX
- Fetching URLs: mu reads what curl gives it
- Full spec conformance for RST/AsciiDoc/Org. The common subset that real READMEs use is the target
