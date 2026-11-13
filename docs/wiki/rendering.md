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
| `[text](url)` | Underlined blue text when color is on; an OSC 8 label when enabled and valid, otherwise the URL dimmed in parentheses. Autolinks are not repeated |
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
- **Links**: eligible external links use OSC 8 when enabled, otherwise retain printed URLs. Internal EPUB links use [keyboard chapter navigation](/wiki/pager#chapter-links), not OSC 8.

## Links {#links}

The shared document renderer emits OSC 8 for semantic links to validated absolute `http`, `https` or `mailto` destinations when [the link policy](/wiki/usage#links) enables it. The label becomes the hyperlink, without a redundant `(URL)` suffix. For example, `[the guide](https://example.com/guide)` shows `the guide` with OSC 8, or `the guide (https://example.com/guide)` without it. Autolinks are not repeated; an empty external label outside HTML/EPUB headings still exposes its destination. Empty heading anchors remain invisible rather than creating outline noise.

This applies to link markup from Markdown, HTML, reStructuredText, Org, AsciiDoc, man-page source, Jupyter Markdown cells/outputs and EPUB chapters. URLs merely appearing in source files, code blocks or data are not auto-linkified. Relative paths, local files, fragments outside EPUB and unsupported or unsafe targets stay non-actionable; image `--base` settings do not resolve them. Existing HTML heading-fragment cleanup still omits decorative permalinks and fragment suffixes.

- The emulator decides how external links are activated, for example with Ctrl-click; mu does not open or fetch them. Auto detection is conservative and does not prove a live emulator's click behavior.
- `--links always` enables OSC 8 even with `--color never` or `NO_COLOR`. SGR styling affects appearance, not the resolved hyperlink destination. At a fixed hyperlink policy, turning SGR off changes neither visible text nor layout; changing link policy can restore or remove the URL suffix.
- Payloads are ASCII URIs, percent-encoding Unicode and unsafe URI bytes, bounded to 8192 encoded bytes. Controls and unsupported targets are rejected before encoding; see [Safety](/wiki/safety#hyperlinks).
- Wrapped and clipped labels keep their destination. Every line closes OSC 8 before its newline, and link state excludes padding, gutters, table borders and status bars. Mixed inline styles and search highlighting do not expose hidden URI bytes as text.
- Images inside active external or internal links keep their actionable `[image: alt]` text rather than losing their destination in a standalone picture.

EPUB internal links show their labels without ZIP paths or generated anchor IDs. The pager follows resolved chapter paths/fragments with `Tab` / `Shift-Tab`, `Enter` and `Backspace`, including with `--links never`. Unresolved targets stay readable but non-jumpable. These are not emulator mouse clicks, and require no external opener, network access or invented ZIP-member file URIs.

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

Documentation permalinks such as `¶`, `§` and link icons are omitted from headings and outlines. Fragment links wrapping real heading text keep the text and its inline styles without appending `(#fragment)`. External heading links, and links in ordinary paragraphs or tables, follow [the link policy](#links), retaining printed destinations when OSC 8 is disabled or ineligible. Missing or mismatched heading tags recover without consuming later sections, and headings inside hidden templates do not suppress the page-title fallback.

Only recognizable decorative-link classes or a heading's own permalink identify a symbol as decoration. Linked code symbols such as `#`, links to other sections, and all-symbol titles keep their text.

## Theme {#theme}

Text uses only the 16 base ANSI colors plus bold, dim, italic, underline and strike, so output follows your terminal's palette instead of fighting it. The only 24-bit colors mu writes are for half-block image pixels.

Styles are emitted lazily and every line closes its own style and hyperlink. That's what lets the pager draw any window of lines on its own.
