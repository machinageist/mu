# Plan: OSC 8 hyperlinks (0.9.0)

## Established baseline

- The repository is on `main` at `a7e51c9` (`Release 0.8.1`); it was clean before this change.
- `FEATURE/ROADMAP.md` assigns only OSC 8 hyperlinks and printed-URL fallback to 0.9.0. Configuration/themes remain 0.10.0.
- Fresh baseline: `cargo fmt --check` and `cargo test --release` pass, with 468 passed, 0 failed, 0 ignored. The execution log is in `$TMPDIR/mu-links-baseline.log` outside the repository.
- `src/render.rs::start_link` / `end_link` currently collect a URL suffix. `src/inline.rs` wraps styled atoms and `src/table.rs` composes already-wrapped cell lines.
- Pager clipping (`src/pager/mod.rs::cut_left`) and search (`src/pager/search.rs`) currently understand SGR, not OSC 8. They must not count hidden target bytes as columns/searchable text.
- The original protocol's framing, ASCII URI encoding requirement, and capability-detection limitation were checked against the author's raw source.

## Work sequence

- [x] Read the release roadmap and contribution rules; establish repository state and rerun the baseline.
- [x] Trace link rendering, wrapping/table composition, CLI output policy, and pager control handling.
- [x] Create this plan and the draft change contract before implementation.
- [x] The user instructed implementation after the defaults/test boundaries were presented, adding working EPUB chapter links. Record the accepted defaults and chapter-navigation extension in the contract.
- [x] First vertical slice: one public render regression for a safe absolute link, then the smallest validated OSC 8 representation and rendering implementation that makes it pass. Keep disabled/default-library output unchanged.
- [x] Extend one behavior at a time: inline styles and autolinks; unsupported/malicious targets and portable URI encoding; multiline/wrapped labels, containers, tables, headings, and outline metadata.
- [x] Teach pager consumers to distinguish generated hyperlink controls from visible text; verify through real sessions rather than tests coupled to private helpers. Cover searching, clipping, state reset, resize, and exit.
- [x] Wire the agreed CLI policy and document conservative auto-detection. Exercise real subprocess/PTY output for supported/unknown terminals, multiplexer fallback, pipes, and explicit overrides.
- [x] Verify supported document adapters and actual EPUB extraction, not only direct adapter calls.
- [x] Update the feature contract, Unreleased notes, CLI documentation, and affected local wiki pages together with implemented behavior.
- [x] Review the full diff independently, remediate findings, and obtain corrective standards/spec PASS verdicts with focused public and real CLI/PTY regressions.
- [x] Finish the complete release/style/lint/documentation gates and rerun CLI/PTY smoke checks against the final release binary.

## Final verification

- `cargo fmt --check`, `cargo test --release`, `cargo clippy --all-targets -- -D warnings`, `scripts/check-docs.sh` and whitespace checks pass. The final release suite has 524 passed, 0 failed and 0 ignored.
- The final release binary passes 120 additional actual-EPUB PTY jump/return/terminal-restoration cases across prose, code, tables, CJK text, wrap edges, color modes and unprefixed/prefixed XHTML. Twenty corrective CLI/PTY cases also pass, including malformed chapter recovery and literal image alternatives.
- Independent standards and spec reviews pass after corrective review. Saved counterexamples were converted to public extraction/rendering regressions rather than treating interrupted review as acceptance.
- The 13 canonical wiki pages retain their front matter and local links; their four Unreleased changelog entries match the root changelog. The public line/display-column metadata is documented.

## Correctness coverage

- Validated external destinations retain original URI provenance; malformed authorities, controls and parser-repaired replacement characters fall back. Empty standalone external labels retain their destinations.
- XML literals and image alternatives remain data in chapter and package extraction. Hidden elements expose neither text nor IDs. Scoped namespace resolution and balanced recovery prevent malformed markup from swallowing later chapter boundaries.
- Zero-width destinations remain inside their chapter. Source/destination line-and-display-column metadata survives layout, and overflowing tables reveal selected links and targets. Missing, hidden, ambiguous and empty destinations cannot jump into unrelated content.
- Incremental input handles fragmented Shift-Tab, split UTF-8, lone Escape and oversized sequences with bounded pending input. Real pager checks cover search, outline, horizontal scrolling, reflow, return history and terminal restoration.

## Delivery boundary

Implementation and verification are complete. The user authorized committing and pushing the reviewed feature to `origin/main`. Package version stays at 0.8.1 with the feature in Unreleased; no release tag, publication, deployment or installed-binary replacement is part of this request. Desktop-emulator mouse activation remains unobserved; CLI/PTY evidence is not presented as proof of it.
