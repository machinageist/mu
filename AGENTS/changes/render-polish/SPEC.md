# Change: highlighting and HTML rendering fixes

Patch-level maintenance of the existing 0.8.0 behavior, released locally as 0.8.1 before the next minor release.

## Scope

- Repair language selection and lexer bugs affecting languages and document formats already supported.
- Keep common HTML heading formatting readable, without decorative anchor URLs interrupting the title.
- Make only measured or structurally obvious small optimizations in touched paths.
- Add regression coverage across supported source extensions and document code-block adapters.

## Invariants

- Highlighting changes styles only; stripped color output equals plain output at every tested width.
- Preserve code whitespace, terminal-control scrubbing, per-line style resets, and heading outline destinations.
- Unknown languages remain plain; do not introduce new formats, dependencies, flags, or next-minor features.
- Update the changelog, affected wiki pages, and long-lived feature contract. The user approved a local fix commit followed by a dedicated `Release 0.8.1` commit per `CONTRIBUTING.md`; the version bump belongs only in that release commit. Do not push, tag, publish, or replace an installed binary.

## Acceptance

Focused regressions reproduce before fixes and pass afterward. The full formatting, release-test, strict Clippy and documentation gates pass. Exercise representative CLI output and independently review the completed diff.
