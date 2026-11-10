# Plan: CSV, TSV and XML inputs (0.6.0)

Gate for every commit:

```sh
cargo fmt --check && cargo test --release --no-fail-fast && cargo clippy --all-targets -- -D warnings
```

## 1. CSV and TSV
- Add `src/format/csv.rs`: the parser, delimiter detection, numeric columns, sniffing, and the events for a table.
- The renderer takes a minimum column width for this table.
- Add the `Format` variants, names, extensions and media types, plus `is_wide`.
- The CLI lifts the width cap for wide formats.

→ verify: unit tests, `tests/csv.rs` with fixtures, and the `tests/sniff.rs` rows.

## 2. XML
- Add `src/data/xml.rs`: the lossless tokenizer, tag highlighting, the tree check, re-indenting, and sniffing.
- Add the variant, names, extensions, media types (`+xml`), and sniff order after HTML.

→ verify: unit tests, `tests/xml.rs` with plist and RSS fixtures, and the sniff rows.

## 3. Horizontal scrolling
- Track `left` and the widest line in the pager.
- Add the `h`/`l`/arrow keys, the column cut, the `col N` status, and sideways jumps for search.
- An outline jump resets the column.

→ verify: unit tests, and a pty test on a wide CSV.

## 4. Docs and release
README (Formats, Pager keys, Known limits), `FEATURE_SPEC.md`, `--help`, CHANGELOG. Release 0.6.0, push, and the release notes file.
