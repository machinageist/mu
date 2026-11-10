# Plan: source files, man pages and Jupyter (0.7.0)

Gate for every commit:

```sh
cargo fmt --check && cargo test --release --no-fail-fast && cargo clippy --all-targets -- -D warnings
```

## 1. Source files verbatim (the C fix)
- Add a `Format::Code` variant with its names, extensions and media types.
- Sniff a leading `#!` first in the sniff order.
- Add `src/data/code.rs`, which turns the input into plain lines for the data printer.

→ verify: `tests/code.rs` with a C fixture, a shebang row in `tests/sniff.rs`, and the detection unit tests.

## 2. Man pages from roff source
- Add `src/format/man.rs`: a request and escape parser that emits events, covering the `man` macros and then the `mdoc` macros.
- Add `Format::Man`, its detection rules, and its sniff after JSON.

→ verify: unit tests, `tests/man.rs` with man and mdoc fixtures, `--toc`, and the sniff rows.

## 3. Formatted man output
- Detect overstrikes and SGR, convert them to styled lines, and collect headings.
- Add the sniff first in the order.

→ verify: unit tests, a fixture captured from `man`, and the styling-removed invariant.

## 4. Jupyter
- Add a JSON value parser in `src/data/json.rs`.
- Add `src/format/jupyter.rs`, which turns the notebook into Markdown.
- Add `Format::Jupyter`, its detection, and the `+json` exception.

→ verify: unit tests, `tests/jupyter.rs` with a fixture notebook, the nbformat 3 fallback, and the sniff row.

## 5. Docs and release
- Update the README (Formats, Known limits), `FEATURE_SPEC.md`, `--help` and the CHANGELOG.
- Release 0.7.0, push, and write the release notes file.
