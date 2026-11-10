# Plan: syntax highlighting (in 0.8.0)

Gate for every commit:

```sh
cargo fmt --check && cargo test --release --no-fail-fast && cargo clippy --all-targets -- -D warnings
```

1. Add `src/highlight/`: the generic lexer, diff, INI, and delegation to the data highlighters as written. Add the language table and the code styles.
   → verify: unit tests, including that every source extension has a lexer.
2. Highlight code frames in the renderer, using styled chunking with the same rules as plain chunking.
   → verify: fenced-block test, color output with styling removed equals plain.
3. Add `Options.language`, set by the CLI from the file or URL extension, with the shebang as fallback.
   → verify: `tests/code.rs`.
4. Docs: CHANGELOG, README, `FEATURE_SPEC.md`, the wiki, and ROADMAP's reordering.
