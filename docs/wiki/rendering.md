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
- **Wide glyphs**: CJK and emoji are measured by display width (`unicode-width`), not byte or char count.
- **Code blocks**: no syntax highlighting, on purpose, because it would cost startup time and binary size.
- **Links**: the URL is printed after the text. Links are not clickable, since mu doesn't emit OSC 8 hyperlinks.

## Theme {#theme}

Text uses only the 16 base ANSI colors plus bold, dim, italic, underline and strike, so output follows your terminal's palette instead of fighting it. The only 24-bit colors mu writes are for half-block image pixels.

Styles are emitted lazily and every line closes its own style. That's what lets the pager draw any window of lines on its own.
