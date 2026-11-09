# Plan: multi-format presenter

One task per commit, or one `--no-ff` merge per agent-built format. Each merge adds its `CHANGELOG.md` line under Unreleased (see `CONTRIBUTING.md`). This change ships as **0.2.0**: it is MINOR because it adds features and renames the binary, which is breaking below 1.0. Verification gate for every task:

```sh
cargo test --release && cargo clippy --all-targets -- -D warnings
```

## 0. Housekeeping
Commit the pending `mdread` → `mu` rename. Restore `.gitignore` (`/target`). ✅ done
→ verify: `git status` is clean apart from the spec docs

## 1. Format plumbing (no behaviour change)
`Format` enum, `present()`, `render_events()` in render.rs fed by any `Event` iterator. Add the `-f/--format` flag and extension detection. Every non-Markdown format errors "not yet supported" for now.
→ verify: all existing tests pass; unit tests for extension mapping and flag parsing

## 2. Content sniffing
`detect()` for stdin and unknown extensions. Only the Markdown and JSON rules return a result until their formats exist. Each later task adds its own rule.
→ verify: unit tests, one per sniff rule plus "ambiguous → Markdown"

## 3. Styled data lines + JSON
Data path: tokenizer → styled lines → hard wrap with `┆`. JSON pretty-print (2-space indent, keys/strings/numbers/literals colored). Invalid JSON is highlighted as written.
→ verify: exact-output tests; invariants over `tests/fixtures/data.json`; invalid JSON does not panic

## 4. YAML and TOML highlighting
Comments, keys, strings, numbers, booleans, anchors/tags (YAML), and table headers (TOML).
→ verify: exact-output tests plus invariants over fixtures

## 5. HTML
Tolerant tokenizer, entity decoding, and block/inline mapping to events. `script`/`style`/`head`/`svg`/`template` are dropped. `pre` → code frame, `table` → GFM table, `blockquote`, lists, `a`/`img`.
→ verify: exact-output tests plus invariants; a saved real-world page fixture renders without markup soup

## 6. Org-mode
Headings, emphasis, lists and checkboxes, `#+BEGIN_SRC/QUOTE/EXAMPLE`, tables, `[[link][text]]`, `#+TITLE`.
→ verify: as above

## 7. reStructuredText
Adornment headings, inline markup, lists, `::` literal blocks, `code-block`/`note`/`warning` directives (admonitions → alert quotes), simple and grid tables, hyperlinks.
→ verify: as above

## 8. AsciiDoc
`=` headings, inline markup, lists, `----`/`....`/`____` blocks, `[source,lang]`, `NOTE:`-style admonitions, `|===` tables, links.
→ verify: as above

## 9. Docs
README (usage, formats table, detection), FEATURE_SPEC updated to the shipped reality, and the binary size checked. ✅ done: 816 KB
→ verify: `cargo build --release && ls -l target/release/mu`

## 10. Release 0.2.0
Follow the release steps in `CONTRIBUTING.md`: bump to 0.2.0, date the changelog section, commit `Release 0.2.0`, tag `v0.2.0`. ✅ done
→ verify: `cargo run --release -- --version` prints `mu 0.2.0`; `git describe` prints `v0.2.0`
