# Plan: highlighting and HTML rendering fixes

1. Inspect existing adapters, highlighting, heading rendering and tests; run the baseline release suite.
2. Reproduce concrete gaps with focused regressions, including shared call paths.
3. Apply bounded fixes, preserving plain text and terminal safety; expand adapter/extension coverage.
4. Update `FEATURE/FEATURE_SPEC.md`, `CHANGELOG.md` and the affected wiki pages.
5. Run `cargo fmt --check && cargo test --release && cargo clippy --all-targets -- -D warnings` and `scripts/check-docs.sh`; exercise CLI cases and obtain an independent diff review.

The user approved a local fix commit followed by a dedicated `Release 0.8.1` commit. Push, tagging, publication and installation remain out of scope.

## Result

Implementation and execution verification are complete. No format, dependency or flag was added; the patch version is recorded separately in the release commit.

- Fixed existing lexer state/constant bugs, source URL and shebang language selection, document language-hint propagation, and chapter-scoped EPUB metadata preservation.
- Cleaned HTML heading permalinks without dropping meaningful symbol/code titles, repaired hidden/malformed heading recovery, and made outline text independent of wrapping.
- Removed the second heading-wrapping pass and skipped language lookup for plain source output.
- Updated the feature contract, changelog and affected wiki pages.

## Verification

- `cargo fmt --check`: passed.
- `cargo test --release`: 468 passed, 0 failed, 0 ignored.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `scripts/check-docs.sh` and `git diff --check`: passed.
- Release CLI sweep: all 81 supported source extensions, 162 color/plain pairs across widths 30 and 80, including control scrubbing and per-line style resets: passed.
- Supported source media types: all 20 covered by lexer lookup and local HTTP integration tests.
- Final release CLI follow-up: 52 cases covering heading/outline output at widths 30/60/80 with both color modes, the last hidden-descendant regression, and normal title/code/link behavior: passed.
- Actual zipped EPUB tests exercise extraction, root/body language hints, unknown overrides, chapter isolation, image/link preservation and metadata escaping.

Independent standards review passed. Independent correctness reviews identified regressions that were fixed with observed failing-then-passing tests. The last independent follow-up was interrupted by a provider rate limit, so there is no final independent PASS claim; the parent verified its remaining case with a fresh release-CLI sweep, and the full execution gate passed on the same code.

The fix commit retains 0.8.0; the following `Release 0.8.1` commit increments the patch version and moves these notes out of Unreleased. No installed binary is replaced.
