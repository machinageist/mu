# Contributing

## Versions

mu follows [Semantic Versioning 2.0.0](https://semver.org). The version lives in one place, `Cargo.toml`, and `mu --version` prints it.

While mu is below 1.0:

| Change | Bump | Example |
|---|---|---|
| Breaking: a removed or renamed flag, binary or library API, or an exit-code meaning | MINOR | 0.2.0 → 0.3.0 |
| New feature: a format, flag or pager key | MINOR | 0.2.0 → 0.3.0 |
| Bug fix, performance, docs or internal refactor | PATCH | 0.2.0 → 0.2.1 |

From 1.0 on, breaking changes bump MAJOR and features bump MINOR. 1.0 means the CLI flags, exit codes and `mu::{render, present, Format, Options}` are stable.

## Changelog

`CHANGELOG.md` follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

- Every user-visible change adds a line under `## Unreleased` in the same commit or merge that ships it. Use the sections Added, Changed, Deprecated, Removed, Fixed and Security.
- Write for the user of the tool, not the code: "JSON input is re-indented", not "add json.rs".
- Internal-only changes (tests, refactors, specs) get no entry.

## Commits

- One logical change per commit, and the test suite passes at every commit
- Subject line: imperative, capitalised, no period, ≤ 72 chars ("Add HTML presenter")
- Body: explain why when it is not obvious
- Feature branches merge with `--no-ff` and a descriptive message ("Merge HTML presenter"), so each feature reads as one unit in `git log --first-parent`

## Releasing

1. Make sure `cargo fmt --check && cargo test --release && cargo clippy --all-targets -- -D warnings` passes
2. Choose the bump from the table above, using the largest change in `Unreleased`
3. Set `version` in `Cargo.toml` and run `cargo build` so `Cargo.lock` updates
4. In `CHANGELOG.md`, rename `Unreleased` to `X.Y.Z - YYYY-MM-DD` and open a fresh empty `Unreleased` above it
5. Commit as `Release X.Y.Z`, with nothing else in that commit
6. Tag it: `git tag -a vX.Y.Z -m "mu X.Y.Z"`
7. Check that `cargo run --release -- --version` prints `mu X.Y.Z`
8. Push the commit and tag: `git push origin main --follow-tags`
9. Publish a GitHub release from the tag, with that version's `CHANGELOG.md` section as its notes: `gh release create vX.Y.Z --title "mu X.Y.Z" --notes-file <section>`

`v0.1.0` predates this process. It marks the last commit released as `mdread`, so it has no `Release` commit and no `CHANGELOG.md` in its tree; its notes live in the 0.1.0 section of today's changelog. Every tag from `v0.2.0` on points at its `Release X.Y.Z` commit.
