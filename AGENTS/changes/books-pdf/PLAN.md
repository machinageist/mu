# Plan: EPUB and PDF (0.8.0)

Gate: `cargo fmt --check && cargo test --release --no-fail-fast && cargo clippy --all-targets -- -D warnings`, plus `scripts/check-docs.sh` for user-visible commits.

1. **Bytes first:** the CLI reads bytes, adds `Format::Epub`/`Format::Pdf`, `is_binary`, `sniff_bytes` and `extract`, and handles URL bodies.
   → verify: the detection tests.
2. **Zip and EPUB:** add `src/book/zip.rs` and `src/book/epub.rs`, with the `miniz_oxide` dependency.
   → verify: the unit tests and `tests/epub.rs`.
3. **PDF:** add `src/book/pdf.rs` (`pdftotext`, page printer, outline).
   → verify: the unit tests and `tests/pdf.rs`, gated on the tool.
4. **Docs and release:** README, `FEATURE_SPEC`, wiki, `--help` and CHANGELOG. Release 0.8.0.
