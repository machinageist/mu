# mu — agent instructions

- Versioning, changelog, commit and release rules live in `CONTRIBUTING.md`. Follow them for every change
- Each user-visible change adds a line under `## Unreleased` in `CHANGELOG.md`, in the commit or merge that ships it
- Do not bump `Cargo.toml` `version` or create tags outside a `Release X.Y.Z` commit made per `CONTRIBUTING.md`
- Long-lived behaviour lives in `FEATURE/FEATURE_SPEC.md`, and the agreed release order to 1.0 in `FEATURE/ROADMAP.md`. In-flight work lives in `AGENTS/changes/<id>/SPEC.md` and `PLAN.md`
- Gate for every commit: `cargo fmt --check && cargo test --release && cargo clippy --all-targets -- -D warnings`
