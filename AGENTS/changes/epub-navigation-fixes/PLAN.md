# Plan: EPUB navigation reliability

Baseline: `main` at `5b70a6a`, clean working tree. The current development build includes EPUB navigation; the older installed binary does not. The user approved replacing the installed binary after verification.

- [x] Confirm repository state, contribution rules and installed/current executable mismatch.
- [x] Reproduce table reflow dropping Backspace history in an actual ZIP/PTY session.
- [x] Add a public failing regression, then separate stable source identity from display ordering without changing public navigation metadata. The actual ZIP/PTY case passes in both width-change directions, while checking the new visible Tab order.
- [x] Investigate and reproduce other reported navigation failures; fix confirmed defects in scope with actual-ZIP anchor lifecycle/entity regressions and real-pager tests.
- [x] Update user-facing docs and changelog, and independently review standards and behavior. Both corrective review axes PASS with no remaining material findings.
- [x] Freeze the corrected source and run the full release/style/lint/docs gate. All 554 release tests pass, with no failures or ignored tests; formatting, strict all-target Clippy, docs and whitespace checks pass against the unchanged 97-file source snapshot.
- [x] Install the verified current build, verify the PATH executable, and repeat actual EPUB interaction checks. Cargo replaced `/home/mgeist/.cargo/bin/mu`; its SHA-256 matches the tested release executable, `--links` is present, and all 30 actual CLI/PTY checks pass against the installed path with terminal restoration verified.

No new commits, pushes, tags or release/version changes are authorized by this request. The existing feature remains Unreleased at package version 0.8.1.

## Investigation findings

- Read-only audits reproduced lost history and wrong returns for identical labels/targets after table reflow. The parent preserves source encounter order and derives an independent rendered-order index; both resize directions and the duplicate-source case pass real PTY regressions.
- Repeated footnote labels previously produced identical frames while changing the destination. Source-word focus and a status ordinal now distinguish them, including with CJK/joined-emoji prefixes and color disabled.
- Initial reverse selection and horizontally scrolled entry ignored visible source positions. Viewport-first entry now passes a real PTY regression without changing continued cycling behavior.
- Deferred inline/page-break and unconsumed table-group anchors now carry forward to painted content, with chapter-boundary expiration; XHTML named references resolve consistently without recursive decoding or fetching DTDs.

Before corrective review, the parent debug gate passed 548 tests, formatting and strict all-target Clippy. Final verification remains pending the corrections below.

## Corrective review

- Standards found positive-width Unicode whitespace could consume a deferred ID. A real-ZIP regression confirms NBSP incorrectly targets preceding content instead of the following glyph. Correct the inline and outer pending-anchor paths without changing rendered spacing or identity strings.
- The spec review's final response hit a provider quota, but its saved report and completed ZIP/PTY probes were recovered. It independently confirms the Unicode whitespace issue and also finds focus can emphasize only a leading zero-width word. The latter now has a failing-then-passing real pager regression with spaces, line breaks and color on/off.
- Both findings are corrected and independently re-reviewed: Standards PASS and Spec/Correctness PASS. The failed initial final response was not treated as a PASS.

The parent confirms all 30 current CLI/PTY smoke checks and 14 lifecycle, 11 entity and 17 real-pager regressions pass, alongside formatting, strict all-target Clippy, docs and whitespace checks. The full frozen-source gate passes all 554 release tests. The same 30 smoke checks also pass against the release executable and then the installed PATH executable. All 97 frozen source/test/manifest files remain unchanged.

Complete within scope: the installed executable is byte-identical to the tested release build (`09bf60b51fc7e4c42a40143e941dcad61e6befa1aea55ae78440f507e3173bc2`). Package version remains 0.8.1; no commits, pushes, tags, publication or wiki deployment occurred. User books were not changed.

## Follow-up authorization

The user subsequently approved recording this verified feature baseline as 0.9.0, then delivering actual mouse navigation and sharper pager images as 0.9.1, including local commits, installation and a push when complete. The earlier no-commit/no-version-change restriction describes the completed original request, not the newly authorized follow-up. Tags, publication and wiki deployment remain outside the authorized scope.
