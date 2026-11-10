---
title: Development
group: Project
order: 1
summary: Tests, commits, versioning and releases
---

How to build and test mu, and the rules every change and release follows.

## Build and test {#build}

```sh
cargo test --release
cargo clippy --all-targets
cargo run --release -- -w 60 tests/fixtures/kitchen-sink.md
```

Every commit must pass this gate:

```sh
cargo fmt --check && cargo test --release && cargo clippy --all-targets -- -D warnings
```

## Test suite {#tests}

| File | Covers |
|---|---|
| `tests/render.rs` | Exact output for each Markdown construct, plus invariants swept across widths 30–120 |
| `tests/html.rs`, `rst.rs`, `org.rs`, `asciidoc.rs` | One file per document format, with exact-output cases |
| `tests/json.rs`, `yaml.rs`, `toml.rs`, `xml.rs` | One file per data format |
| `tests/csv.rs` | CSV and TSV tables, including the 10-column floor and overflow |
| `tests/code.rs` | Source files shown verbatim, never read as Markdown |
| `tests/outline.rs` | Recorded headings point at the lines that show them, in every document format and across widths |
| `tests/sniff.rs` | Sniffs every fixture against every format, so one format's rule can't quietly claim another's files |
| `tests/cli_detect.rs` | `--format` through the CLI, and unknown names rejected |
| `tests/images.rs` | Image loading, fitting and fallbacks |
| `tests/url.rs` | URL fetching, run against a local server |
| `src/pager/` unit tests | Key decoding, scroll clamping, search, the outline, sideways cutting, resize behaviour and the status bar |
| `tests/pager_pty.rs` | The real pager in a pseudo-terminal: resizing, search, the outline and sideways scrolling |

The invariant sweeps check that no line exceeds the width, that color output with SGR stripped equals plain output, that every styled line resets before its newline, and that no control character from the input reaches the output. See [Safety & invariants](/wiki/safety#invariants).

Fixtures live in `tests/fixtures/`: `kitchen-sink.md`, one or more `sample.*` per format (`sample.plist` and `sample.rss` for XML, `sample.c` and `sample.sh` for source code), and `images/`.

## Specs and plans {#workflow}

```text
FEATURE/FEATURE_SPEC.md              long-lived behaviour and invariants
AGENTS/changes/<id>/SPEC.md          desired outcome, acceptance criteria
AGENTS/changes/<id>/PLAN.md          atomic tasks and verification
```

Past changes: `multi-format` (0.2.0), `images` (0.3.0), `url-input` (0.4.0), `pager-navigation` (0.5.0) and `data-inputs` (0.6.0). The agreed order of releases up to 1.0 is in `FEATURE/ROADMAP.md`. When behaviour ships, `FEATURE_SPEC.md` is updated so it always describes the current state.

## Commits {#commits}

- One logical change per commit, and the test suite passes at every commit.
- Subject line: imperative, capitalised, no period, ≤ 72 chars, e.g. “Add HTML presenter”.
- Body: explain why when it isn't obvious.
- Feature branches merge with `--no-ff` and a descriptive message (“Merge HTML presenter”), so each feature reads as one unit in `git log --first-parent`.
- Every user-visible change adds a line under `## Unreleased` in `CHANGELOG.md` in the same commit or merge. Write for the user of the tool, not the code: “JSON input is re-indented”, not “add json.rs”. Internal-only changes get no entry.

## Versioning {#versioning}

[Semantic Versioning 2.0.0](https://semver.org). The version lives in one place, `Cargo.toml`, and `mu --version` prints it. While mu is below 1.0:

| Change | Bump | Example |
|---|---|---|
| Breaking: a removed or renamed flag, binary or library API, or an exit-code meaning | MINOR | 0.2.0 → 0.3.0 |
| New feature: a format, flag or pager key | MINOR | 0.2.0 → 0.3.0 |
| Bug fix, performance, docs or internal refactor | PATCH | 0.2.0 → 0.2.1 |

From 1.0 on, breaking changes bump MAJOR and features bump MINOR. 1.0 means the CLI flags, exit codes and `mu::{render, present, Format, Options}` are stable.

## Releasing {#releasing}

1. Make sure the commit gate passes.
2. Choose the bump from the table above, using the largest change in `Unreleased`.
3. Set `version` in `Cargo.toml` and run `cargo build` so `Cargo.lock` updates.
4. In `CHANGELOG.md`, rename `Unreleased` to `X.Y.Z - YYYY-MM-DD` and open a fresh empty `Unreleased` above it.
5. Commit as `Release X.Y.Z`, with nothing else in that commit.
6. Tag it: `git tag -a vX.Y.Z -m "mu X.Y.Z"`.
7. Check that `cargo run --release -- --version` prints `mu X.Y.Z`.
8. Push the commit and tag: `git push origin main --follow-tags`.
9. Publish a GitHub release from the tag, with that version's changelog section as its notes: `gh release create vX.Y.Z --title "mu X.Y.Z" --notes-file <section>`.

> [!NOTE]
> `v0.1.0` predates this process. It marks the last commit released as `mdread`, so it has no `Release` commit and no `CHANGELOG.md` in its tree. Every tag from `v0.2.0` on points at its `Release X.Y.Z` commit.
