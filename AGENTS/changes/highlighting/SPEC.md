# Change: syntax highlighting for code

Ships in **0.8.0**, pulled forward from the roadmap's "Code and links" release at machinageist's request on 2026-11-10. OSC 8 links stay behind as 0.9.0.

## Outcome

- Source files (`mu main.c`) are highlighted.
- Fenced code blocks in every document format (Markdown, RST, Org, AsciiDoc, HTML `<pre>` with a language, Jupyter code cells) are highlighted when their language is known.
- Every extension mu shows as source code has a lexer.

## Lexers

- **One table-driven lexer:** each language is a row in `src/highlight/languages.rs` giving:
  - its names and extensions
  - line and block comment markers, which may nest (`/*` in Rust and Swift, `{-` in Haskell)
  - its quotes, each with an escape flag and whether it may span lines
  - keywords, built-in types and constants
  - flags: case-insensitive keywords (SQL), preprocessor lines (C), `@decorators`, `#[attributes]`, `'lifetimes`, `$variables`, and capitalised identifiers as types
- **Styles:** keyword, type, function (an identifier before `(`, or a Rust `name!(`), string, number, constant, comment, meta (preprocessor, decorators, attributes), and variable.
- **Special kinds:**
  - Diffs are colored by line: headers, hunks, insertions and deletions.
  - INI-style config (`.ini` `.cfg` `.conf` `.properties`) shows sections, keys and values.
  - JSON, YAML, TOML and XML/HTML reuse mu's data highlighters, as written. Code blocks are never re-indented.
- **Languages:**
  - C family: C, C++, Objective-C, C#
  - Systems: Rust, Go, Zig, Nim, assembly
  - Scripting: Python, Ruby, Perl, PHP, Lua, R, Julia
  - Web: JavaScript, TypeScript, CSS/SCSS/Sass/Less
  - JVM: Java, Kotlin, Scala, Groovy/Gradle
  - Functional: F#, Haskell, OCaml, Elixir, Erlang, the Lisps (Clojure, Emacs Lisp, Scheme, Common Lisp, Racket), Dart, Swift
  - Shells: sh/bash/zsh/ksh, fish, PowerShell, batch
  - Other: SQL, Vim script, Make, CMake, Dockerfile, protobuf, GraphQL, HCL/Terraform, Nix, diff, INI, JSON, YAML, TOML, XML/HTML/SVG/Vue/Svelte
- **Names:** info strings and extensions match case-insensitively. Common aliases (`py`, `js`, `sh`, `bash`, `console`, `c++`, `golang`…) are listed. Unknown languages stay plain.

## Choosing the language

- **Fenced blocks:** the info string's first word.
- **Source files:**
  - the file's extension, or a URL path's extension
  - otherwise the `#!` interpreter (`sh`, `bash`, `zsh`, `python3`, `node`, `ruby`, `perl`, `php`, `lua`, `fish`, through `env` and `env -S`)
  - otherwise plain
- **Library:** `Options` gains `language: Option<String>`, which names the language for `Format::Code`. Adding a public field breaks struct literals, which is acceptable before 1.0.

## Invariants

- Highlighting only applies with color on. With color off, output is byte-identical to 0.7.0.
- With styling removed, color output equals plain output at every width. Highlighted lines wrap by the same rules (tabs, controls, wide characters) as plain ones.
- The binary grows by at most about 100 KB. It measured 66 KB.
- There is no noticeable cost: 20,000 lines of Rust render in about 30 ms.

## Acceptance criteria

- **Unit tests:**
  - per-language tokens for C, Rust, Python, shell, SQL, JavaScript and diff
  - nested comments, shebang detection, unique names
  - every source extension has a lexer
  - every lexer keeps every character
- **Integration tests:**
  - a C file and a shell script are highlighted
  - styling removed equals plain output at several widths
  - fenced blocks in Markdown are highlighted, and still equal plain output with styling removed
  - unknown languages stay plain

## Out of scope

- Grammar-based highlighting, as before
- Heredocs, string interpolation, regex literals, and context such as distinguishing a type from a variable
- Theme overrides for the new styles, which come with the config file in 0.10.0
