# Change: source files, man pages and Jupyter notebooks

Ships as **0.7.0**, the third release on `FEATURE/ROADMAP.md`.

## Outcome

- **Source files:** `mu main.c` shows the file as written. Today it is read as Markdown, which joins lines, eats `*` and `_` as emphasis, and turns `# define` into a heading.
- **Man pages:** `mu ls.1` shows a man page in its roff source as a formatted document with an outline of its sections, and so does `man ls | mu`.
- **Notebooks:** `mu analysis.ipynb` shows a notebook's prose, code and outputs, including plots.

## Formats

| Format | Names for `-f` | Extensions | Media types | Sniffing |
|---|---|---|---|---|
| Code | `code`, `text` | see below | see below | the head starts with `#!` |
| Man | `man`, `roff`, `mdoc` | `.1` to `.9`, optionally followed by letters (`.3p`, `.1ssl`), plus `.man` `.mdoc` | `text/troff`, `application/x-troff-man` | see Man pages |
| Jupyter | `ipynb`, `jupyter`, `notebook` | `.ipynb` | `application/x-ipynb+json`, matched before the `+json` rule | before JSON: a JSON object whose head has both `"cells"` and `"nbformat"` |

## Source files (Code)

- **Extensions:**
  - C family: `c h cc cpp cxx c++ hh hpp hxx m mm`
  - Systems: `rs go zig nim asm s`
  - Scripting: `py pyi rb pl pm php lua r jl`
  - Web: `js mjs cjs jsx ts tsx vue svelte css scss sass less`
  - JVM and .NET: `java kt kts scala groovy gradle cs fs`
  - Functional: `hs ml mli ex exs erl clj el lisp scm dart swift`
  - Shell: `sh bash zsh fish ksh ps1 bat cmd`
  - Other: `sql diff patch vim mk cmake proto graphql tf hcl nix ini cfg conf properties`
- **Media types:**
  - C and C++: `text/x-c`, `text/x-csrc`, `text/x-chdr`, `text/x-c++src`, `text/x-c++hdr`
  - Other languages: `text/x-python`, `text/x-rust`, `text/x-go`, `text/x-java`, `text/x-java-source`
  - JavaScript: `text/javascript`, `application/javascript`, `application/x-javascript`
  - Shell: `text/x-sh`, `application/x-sh`, `text/x-shellscript`
  - Other: `text/x-diff`, `text/x-patch`, `text/css`, `application/sql`
- **Output:**
  - Lines are shown exactly as written, through the data printer, the same path JSON takes.
  - Tabs expand to the usual tab stops, control characters are scrubbed, and a leading byte order mark is dropped.
  - Long lines hard-wrap behind the dashed continuation bar.
  - There is no highlighting yet: 0.9.0 adds it by extension.
- **Width:** the same as JSON (the auto width is capped at 100).
- **Sniffing:** only a leading `#!` (a script's shebang) is sniffed, and it is checked first. Other source code on stdin is still read as Markdown, so `-f code` is the way to show it verbatim.
- **Outline:** none.
- **Plain text:** `.txt` stays Markdown, since many text files are Markdown in all but name. `-f text` shows any input verbatim.

## Man pages (roff source)

- **Sniffing:** after JSON and before HTML. Skipping comment lines (`.\"`, `'\"`) and blank lines, the first request is `.TH` or `.Dd`.
- **Requests mapped to document events:**

  | Requests | Becomes |
  |---|---|
  | `.TH name section …`, `.Dt` | H1 `NAME(SECTION)` |
  | `.SH`, `.Sh` | H2 |
  | `.SS`, `.Ss` | H3 |
  | `.PP` `.LP` `.P` `.Pp` | a paragraph break |
  | `.TP` | a definition list: the next line is the term, and the text after it is the definition |
  | `.IP tag` | a definition with that tag, or an indented paragraph with no tag |
  | `.RS`/`.RE` | a block quote, nested |
  | `.nf`/`.fi`, `.EX`/`.EE`, `.Bd -literal`/`.Ed`, `.Dl`, `.D1` | a code block, with lines kept as written |
  | `.br` | a line break |
  | `.sp` | a paragraph break |
  | `.B` `.I` `.SM` `.SB` | bold or italic text |
  | `.BR` `.BI` `.IB` `.IR` `.RB` `.RI` | alternating styles, joined without spaces |
  | `.UR url`/`.UE`, `.MT addr`/`.ME` | links |
  | `.SY`/`.OP`/`.YS` | synopsis lines: the command in bold, options in brackets |

- **mdoc macros:**
  - `.Nm` `.Nd` `.Fl` `.Ar` `.Op`/`.Oo`/`.Oc` `.Cm` `.Ev` `.Pa` `.Xr` `.Em` `.Sy` `.Li` `.Ql` `.Dq` `.Sq` `.Pq` `.Ns` `.No` `.Va` `.Fn` `.Fa` `.Ft` `.In` `.Ic` `.Ex -std` `.Lk` `.Mt`
  - Lists: `.Bl -tag`/`-bullet`/`-dash`/`-enum`/`-item`/`-column` with `.It`, and `.El`
  - `.Dd`, `.Os`, `.Ux`, `.Bx` and `.At` print their usual short text, or nothing
- **Inline escapes:**
  - Fonts: `\fB` `\fI` `\fR` `\fP` `\f(CW` `\f[…]`
  - Characters: `\-` `\e` `\&` `\ ` `\~` `\0` `\|` `\^` `\c`
  - Named characters: `\(xx` and `\[name]` for the common set (`em en bu aq dq lq rq oq cq co rg tm hy` and the arrows), and `\*(lq` style strings
  - Anything unknown is dropped
- **Comments:** `.\"`, `\"` to the end of the line, and `\#` are dropped.
- **Other requests:** unknown requests (`.ad`, `.na`, `.hy`, `.ne`, `.ds`, `.de`, `.so`, `.TS`…) are skipped, and the text lines after them still show. A `.TS`/`.TE` table shows as a code block of its raw lines.
- **Outline:** sections and subsections.

## Man pages (formatted output)

`man ls | mu` arrives as formatted text with overstrikes: `X\bX` for bold and `_\bX` for underline.

- **Sniffing:** first in the sniff order. It fires when the head holds at least 8 overstrikes.
- **Output:**
  - Overstrikes become bold or underlined text, and the other backspaces are removed.
  - Lines are kept as written, through the data printer.
  - SGR sequences already in the input (`MAN_KEEP_FORMATTING`, `groff -T utf8`) are kept as styles: only bold, underline and reset are understood, and anything else is scrubbed.
- **Outline:** each line that starts at column 0 and is entirely bold, except the first and last lines, which are the page header and footer.
- **Format:** the same `Man` format as roff source. The presenter checks for overstrikes or SGR before parsing roff.

## Jupyter notebooks

- **Supported versions:** nbformat 4. Any other version, or a JSON document without `cells`, is shown as JSON with a note on stderr.
- **Parser:** a small JSON value parser, new in `src/data/json.rs`, with a depth limit of 256 and no dependency.
- **Cells:**
  - **Markdown cells** are Markdown, and their headings fill the outline. `attachment:name` images become `data:` URIs from the cell's attachments.
  - **Code cells** become a fenced code block whose info string is the notebook's language: `metadata.kernelspec.language`, else `language_info.name`, else none.
    - Above the block, a dim line `In [n]:` shows the execution count, or `In [ ]:` when the cell was not run.
  - **Raw cells** become a code block with no language.
- **Outputs, below the code block:**
  - **Stream outputs** (`stdout`, `stderr`) become a code block with no language. `stderr` is labelled.
  - **`execute_result` and `display_data`** pick the first type available from `image/png`, `image/jpeg`, `image/svg+xml`, `text/markdown`, `text/plain`.
    - Images become `data:` URI images in the image pipeline, so they draw as pictures or fall back to alt text as usual.
    - `text/markdown` is rendered.
    - `text/plain` becomes a code block.
    - `text/html` alone is converted through the HTML presenter.
  - **`error` outputs** show `ename: evalue` and the traceback with its ANSI codes removed, as a code block.
- **Code blocks:** every block uses a fence longer than any backtick run inside it, so cell content can never close it early.
- **How it renders:**
  - The notebook becomes one Markdown string, rendered by `render_markdown`, so wrapping, images, search and the outline all work unchanged.
  - Images resolve against the notebook's folder.

## Library

- **New variants:** `Format` gains `Code`, `Man` and `Jupyter`. This breaks exhaustive matches, which is acceptable before 1.0.

## Acceptance criteria

- **Code:**
  - `mu x.c` with a fixture containing `*`, `_`, `#` and leading spaces prints each line unchanged.
  - Every listed extension resolves, `#!` sniffs as Code, and the existing sniff fixtures are unchanged.
- **Man source:**
  - Fixture tests for a `man`-macro page and an `mdoc` page check exact output at one width.
  - Unit tests cover fonts, alternating macros, escapes, `.TP`, `.IP`, `.RS`/`.RE`, `.nf`, comments and unknown requests.
  - `--toc` lists the sections.
- **Man output:**
  - Unit tests cover bold, underline and stray backspaces, plus a fixture captured from `man`.
  - With styling removed, the output equals the input with the overstrikes removed.
- **Jupyter:**
  - A fixture notebook has Markdown, code, stream, error, PNG and `text/plain` outputs, and is checked for exact output with images turned off.
  - Unit tests cover the JSON parser, including its depth limit and bad input, and the fence length.
  - nbformat 3 falls back to JSON.
- **Sniffing:** `tests/sniff.rs` gains a man page, man output, a notebook and a shebang script, and every fixture still detects only as its own format.
- **Detection:** names, extensions and media types resolve, with tests. `application/x-ipynb+json` is Jupyter, not JSON.
- **Unchanged:** existing fixtures render byte-identical.

## Out of scope

- Highlighting source files and notebook code, which comes in 0.9.0
- Gzipped man pages (`ls.1.gz`), until 0.8.0 brings inflate. Use `gunzip -c ls.1.gz | mu`
- `.so` includes, macro definitions (`.de`), tbl layout, eqn, pic
- Notebook widgets, JavaScript outputs and LaTeX rendering (`$…$` stays as written)
- Sniffing source code other than shebang scripts
