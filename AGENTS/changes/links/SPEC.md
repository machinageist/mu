# Change: OSC 8 hyperlinks (0.9.0)

Status: implementation complete and verified; the feature remains Unreleased. The user instructed implementation after the proposed defaults and test seams were presented, explicitly added working EPUB chapter links, and subsequently authorized committing and pushing the finished work.

## Scope

Implement the Links release in `FEATURE/ROADMAP.md`: OSC 8 hyperlinks when supported, with the existing printed-URL fallback. Apply this to semantic document links through the shared renderer; do not turn arbitrary source-code strings into links.

Do not include 0.10.0 configuration/themes, 0.11.0 distribution work, new document formats, or unrelated refactors. Prefer existing dependencies. Keep the package version at 0.8.1 during development; a version bump belongs in a separately authorized `Release 0.9.0` commit. Starting development did not authorize Git or publication changes. The user subsequently requested "finish then commit and push", authorizing the completed, verified feature commit and a normal push to `origin/main`. Tags, a release/version bump, publication, deployment and installed-binary replacement remain outside that authorization.

## Agreed implementation contract

1. Control: `--links auto|always|never`, default `auto`. Auto requires terminal output, a recognized supporting environment, and styled output enabled. Unknown terminals and uncertain multiplexer paths fall back without querying the terminal. `always` is an explicit override, including with `--color never`/`NO_COLOR`; `never` emits no OSC 8. The library receives an explicit setting and does not inspect the environment.
2. Presentation: clickable labels without a redundant URL suffix when a usable hyperlink is emitted. Otherwise preserve the existing printed URL and autolink behavior. This makes visible layout depend on the resolved hyperlink policy, not on SGR color alone.
3. External targets: absolute `http`, `https`, and `mailto` links. Relative links, fragments, local files, and other schemes retain the existing printed representation outside EPUB. Do not silently reuse image settings to resolve links.
4. EPUB chapter links: resolve relative chapter paths and fragments within the extracted book, namespace IDs by chapter, and retain real output-line destinations through reflow. Use Tab/Shift-Tab to select internal links, Enter to follow, and Backspace to return in the pager. Internal navigation is independent of OSC 8 capability/color and remains available with `--links never`. Do not fabricate file URIs for ZIP members or launch an external opener. Invalid/unresolved chapter links remain readable but cannot jump to an unrelated chapter. Terminal-emulator Ctrl-click is for external OSC 8 links, not an in-pager chapter-click promise.
5. Test at all three public boundaries below, including actual ZIP extraction and EPUB chapter navigation through the real pager session.

## Required invariants

- Only mu-generated, validated hyperlink controls may enter rendered output. Source terminal-control sequences are still scrubbed, not replayed.
- Validate targets before encoding; do not sanitize a control-bearing target into a different actionable URI. Preserve semantic links' original schemes across format adapters. Reject unsafe/control-bearing targets and ambiguous replacement-character destinations and retain readable fallback text.
- Emit the OSC 8 payload as a portable ASCII URI of at most 8192 encoded bytes, percent-encoding Unicode/unsafe URI bytes as needed. Use the standard ESC-backslash string terminator, not BEL. Images inside active links retain actionable alt text rather than losing their destination in native graphics.
- Hyperlink state closes before line endings, padding, list/quote gutters, table borders, status bars, and pager exit. Wrapped/clipped labels retain the correct destination without leaking it onto neighboring text.
- At a fixed hyperlink policy, SGR styling changes no visible text or layout. Default non-terminal output and disabled hyperlinks retain the existing plain representation without OSC 8.
- Pager search matches visible text, not hidden destination bytes. Highlighting, horizontal scrolling, resize/reflow, outline navigation, and terminal restoration remain correct.
- Heading identities remain semantic and independent of terminal escape bytes and line wrapping. Internal source and destination metadata retains both rendered line and display column so overflowing-table links can be revealed without clearing selection.
- XML literal/instruction content cannot acquire HTML actions during extraction, including in package metadata. Hidden IDs and ambiguous repaired destinations remain inactive; pending anchors end at chapter boundaries instead of binding to a later chapter.
- Invalid/unsupported targets do not fail the document or trigger fetching, shell execution, file opening, or terminal capability queries.

## Confirmed public test seams

- `mu::render` / `mu::present`: verify destinations, fallback, wrapping, Unicode width, nesting, tables, outline metadata, and malicious input; these do not prove CLI environment policy or actual emulator support.
- CLI subprocess/PTY: verify flags, auto/forced/disabled modes, non-terminal/plain fallback, and environment combinations; a PTY proves emitted bytes, not that an emulator makes a link clickable.
- Actual pager session in a PTY: verify search, horizontal clipping, resize/reflow, exit, and protocol balancing; this does not prove a desktop emulator's mouse gesture or external opener.

## Acceptance

Work in vertical RED-to-GREEN slices at the confirmed seams. Exercise every already-supported semantic-link document adapter, including extraction boundaries for EPUB where applicable. Retain regressions for unsafe URIs, controls, mixed inline styles, long labels, wide characters, and terminal-state cleanup. Update `FEATURE/FEATURE_SPEC.md`, `CHANGELOG.md` under Unreleased, the affected `docs/wiki/` pages and mirrored wiki changelog, and CLI documentation with shipped behavior.

Before calling the feature complete, pass `cargo fmt --check`, `cargo test --release`, `cargo clippy --all-targets -- -D warnings`, `scripts/check-docs.sh`, and `git diff --check`; run real CLI/PTY smoke tests and independently review the final diff. Do not claim live terminal clickability solely from byte assertions.

## Protocol reference

The original OSC 8 proposal specifies `ESC ] 8 ; params ; URI ESC \\`, an empty-target closing sequence, URI encoding, and no universal capability query:
https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda

Its framing, encoding, and capability sections were retrieved directly from the author's raw source while preparing this draft. Environment detection must be documented as a conservative heuristic, not a verified capability handshake.
