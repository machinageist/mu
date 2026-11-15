# Change: EPUB navigation reliability

Status: implemented, independently reviewed, verified and installed. The user reported buggy EPUB navigation, then explicitly approved updating the installed `mu` after verification. The exact book/key sequence was not supplied.

Follow-up authorization: the user has now approved local fix/release commits for the 0.9.0 baseline, a subsequent 0.9.1 mouse/image corrective release, installation and a final push. Original-request restrictions below are historical; the follow-up contract governs new delivery actions.

## Scope

Fix reproducible defects in the existing EPUB keyboard navigation, keeping its controls and safety boundaries. Do not add formats, new navigation modes or external openers. Preserve the current Unreleased 0.9.0 development boundary; do not bump versions, tag, commit, push or publish under this request.

The installed binary is older than the current repository build despite both reporting 0.8.1: its help lacks `--links`. Update the installed executable only after the corrected current build passes verification, then exercise that exact executable.

## Confirmed regression

A table's links can change physical display order after reflow. The pager currently sorts its source-link vector by rendered line, then treats each sorted vector index as a persistent source identity. Following the second link at a wide width and shrinking the window can therefore discard return history even though the source link still exists.

Keep stable source identity separate from display-order selection. Tab/Shift-Tab must still visit links in rendered reading order; Backspace history must survive ordinary width/height changes and return to the matching source link. Repeated labels/targets must not be rebound to another occurrence.

Further audit cases in scope:
- Repeated labels currently produce indistinguishable selected frames. Show the current source's first rendered word in focus styling plus a selection ordinal, preserving visible text and existing document styles/protocols.
- With no active selection, enter at the first visible source for Tab or the last visible source for Shift-Tab, considering both line and display column. If no source point is visible, continue in the requested reading direction and wrap as before.
- Empty trailing inline/page-break and table-group IDs must defer to following visible content in the same chapter, never bind backward to text or a table border, and remain unresolved when the chapter ends first.
- Equivalent XHTML named/numeric/literal/percent-encoded identifiers must match in EPUB navigation, preserving exact identity whitespace and single-pass decoding without fetching DTDs or expanding user-defined entities.

## Invariants and test seams

- Preserve existing Tab/Shift-Tab, Enter, Backspace, search and outline behavior except where a confirmed bug requires correction.
- Only real, chapter-scoped rendered destinations are selectable. Preserve URI validation, literal/hidden content handling, and chapter boundaries.
- Source line/display-column metadata must still reveal wide-table links and targets without terminal-control leakage.
- Use public extraction/rendering and real CLI/PTY sessions. Test table reflow, duplicate links, jump/return, terminal restoration and the installed executable; do not substitute private pager-state tests for interaction evidence.
- Add other investigation findings only when reproduced and within existing behavior.

## Acceptance

Pass focused regressions and independent corrective review before the expensive frozen-tree release gate. Then pass `cargo fmt --check`, `cargo test --release`, `cargo clippy --all-targets -- -D warnings`, `scripts/check-docs.sh` and whitespace checks. Update the Unreleased changelog, mirrored wiki changelog, relevant wiki pages and long-lived contract. Install the verified build with the user's approval, verify executable provenance and repeat representative actual EPUB navigation on the PATH binary. Leave unrelated files and the user's books untouched.
