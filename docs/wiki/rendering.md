---
title: Rendering
group: Guide
order: 3
summary: What each construct looks like in the terminal
---

The Markdown mapping is the reference. The other document formats map their equivalents onto it.

## Markdown mapping {#mapping}

| Markdown | Terminal output |
|---|---|
| `#` / `##` headings | Bold, colored, underlined with `═` / `─` |
| `###`–`######` | Bold, a distinct color per level |
| `**bold**` `*italic*` `~~strike~~` | SGR bold / italic / strikethrough |
| `` `code` `` | Yellow |
| `[text](url)` | Underlined blue text, then the URL dimmed in parentheses. Autolinks are not repeated |
| `![alt](src)` | A drawn picture when it stands alone, else `[image: alt]`. See [Images](/wiki/images) |
| Lists | `•` `◦` `▪` by depth, numbers for ordered lists. Wrapped lines align under the text, and tight lists stay tight |
| `- [ ]` / `- [x]` | `☐` / `☑` in place of the bullet |
| `> quote` | `│` bar, kept through blank lines and nesting |
| `> [!NOTE]` and friends | GitHub alerts: colored bar plus a Note / Tip / Important / Warning / Caution label |
| Fenced / indented code | Frame with the language on the top edge. Tabs are expanded, and long lines hard-wrap with a `┆` continuation bar |
| GFM tables | Box-drawn, with column alignment honoured. Columns shrink widest-first and cells wrap to fit |
| Footnotes | `[label]` reference and a `[label]` definition block |
| `---` | Dim rule across the content width |
| YAML front matter | Dim framed block, recognised only at the very top of the file |
| Raw HTML | Dimmed as-is. Comments are hidden and `<br>` becomes a line break |

### Example {#example}

Real output of `mu -w 52` on a short document. Note the `┆` where the long code line wraps:

```term
<m>Install</m>
<m>═══════</m>

1. Clone the repository
2. Run <y>cargo install --path .</y> and wait for the
   release build

<d>╭─ sh ──────────────────────────────────────────────</d>
<d>│</d> cargo install --git https://github.com/machinageis
<d>┆</d> t/mu
<d>╰───────────────────────────────────────────────────</d>

┌────────┬──────────┐
│ <bold>Format</bold> │ <bold>Kind</bold>     │
├────────┼──────────┤
│ JSON   │ data     │
│ Org    │ document │
└────────┴──────────┘

<g>│</g> <g>Tip</g>
<g>│</g> Pipe anything in
```

## Layout rules {#layout-rules}

- **Wrapping**: inline text is collected as styled word atoms, then wrapped to the width left after container gutters such as quote bars and list indents.
- **Minimum content width**: very deep nesting stops shrinking content below 16 columns and overflows instead.
- **Tables**: columns shrink widest-first and cells wrap. A table with too many columns to fit at 3 columns each overflows the width rather than becoming unreadable.
- **CSV and TSV tables** use the same engine, but take the full terminal width and stop shrinking columns at 10. A wider table overflows, and the pager [scrolls it sideways](/wiki/pager#sideways). See [Tables](/wiki/formats#tables).
- **Wide glyphs**: CJK and emoji are measured by display width (`unicode-width`), not byte or char count.
- **Code blocks**: highlighted by the language in the info string, as below. A block in an unknown language stays plain.
- **Headings** are recorded with their level, text and first output line, for the pager's [outline](/wiki/pager#outline) and `--toc`. Outline labels are independent of wrapping, so resizing never inserts spaces inside long words.
- **Links**: the URL is printed after the text. Links are not clickable, since mu doesn't emit OSC 8 hyperlinks.

## Syntax highlighting {#highlighting}

Source files and fenced code blocks are highlighted when color is on. Keywords, types, function calls, strings, numbers, constants, comments, and meta such as `#include`, `@decorators` and `#[attributes]` each get a style. Highlighting only changes styles, so with color off the output is exactly what it was without it.

- **Languages**:
  - C, C++, Objective-C and C#
  - Rust, Go, Zig, Nim and assembly
  - Python, Ruby, Perl, PHP, Lua, R and Julia
  - JavaScript, TypeScript and CSS/SCSS/Sass/Less
  - Java, Kotlin, Scala and Groovy/Gradle
  - F#, Haskell, OCaml, Elixir, Erlang, the Lisps, Dart and Swift
  - sh/bash/zsh, fish, PowerShell and batch
  - SQL, Vim script, Make, CMake, Dockerfile, protobuf, GraphQL, HCL/Terraform and Nix
  - diffs, INI-style config, and JSON, YAML, TOML and XML/HTML, which use the data highlighters but are never re-indented inside a code block
- **Names**: a code block's info string or a file's extension, in any case, with common aliases such as `py`, `js`, `bash`, `console`, `c++` and `golang`. A source URL first uses its supported source `Content-Type`, so extensionless code URLs are highlighted too.
- **Scripts**: the `#!` interpreter supplies a missing language, including in unlabelled document code blocks and man-page examples. A leading UTF-8 BOM and `env` options or assignments are accepted. An explicitly unknown block language stays plain.
- **HTML and EPUB**: language hints can come from `language-`/`lang-` classes or `data-language`/`data-lang` attributes on wrappers, `pre`, or nested `code`. Bare known language classes such as `python` work on `pre` and `code`. The nested code's hint wins over surrounding blocks.
- **AsciiDoc**: `[source,"python"]` and attribute references such as `[source,{lang}]` use the same highlighting as `[source,python]`.
- **How**: one table-driven lexer reads each language's comment markers, quotes, keywords and a few flags, without runtime grammar loading or an external highlighter. There's no grammar, so it can't tell a type from a variable by context.
- **Comments and constants**: block-comment nesting and hash-comment boundaries follow the selected language, and comment markers inside a preprocessor directive's quoted strings do not open comments. PowerShell `$true`/`$false`/`$null` and Scheme `#t`/`#f` use the constant style rather than being mistaken for variables or plain text.

### HTML heading cleanup {#html-headings}

Documentation permalinks such as `¶`, `§` and link icons are omitted from headings and outlines. Fragment links wrapping real heading text keep the text and its inline styles without appending `(#fragment)`. External heading links, and links in ordinary paragraphs or tables, still print their destinations. Missing or mismatched heading tags recover without consuming later sections, and headings inside hidden templates do not suppress the page-title fallback.

Only recognizable decorative-link classes or a heading's own permalink identify a symbol as decoration. Linked code symbols such as `#`, links to other sections, and all-symbol titles keep their text.

## Theme {#theme}

Text uses only the 16 base ANSI colors plus bold, dim, italic, underline and strike, so output follows your terminal's palette instead of fighting it. The only 24-bit colors mu writes are for half-block image pixels.

Styles are emitted lazily and every line closes its own style. That's what lets the pager draw any window of lines on its own.
