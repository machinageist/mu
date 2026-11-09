# Plan: images in the terminal (0.3.0)

One task per commit. Each user-visible task adds its `CHANGELOG.md` Unreleased line. Gate:

```sh
cargo fmt --check && cargo test --release --no-fail-fast && cargo clippy --all-targets -- -D warnings
```

## 1. Options and geometry (lead) ✅
- `Options` gains `images: ImageOptions { mode, base, cell: CellSize, protocol }` with `Default`
- `Options::new(width, color)` so tests stop spelling every field
- `term::size` also returns pixel size
→ verify: all tests green, and `--images never` output is unchanged

## 2. Image module skeleton (lead) ✅
`src/image/mod.rs`: `load(src, &ImageOptions) -> Option<Rgba>`, `Rgba { width, height, pixels }`, the limits, and one stub file per area below so the parallel tasks own disjoint files.
→ verify: builds, and the stubs return None

## 3. Parallel (one agent each, worktrees) ✅
- **3a. Sources** (`image/source.rs`): path resolution, `data:` base64 (hand-written decoder), remote via `curl` only in fetch mode, size cap
- **3b. Raster decoders** (`image/decode.rs`): PNG, JPEG, GIF first frame and WebP into RGBA, with the dimension and megapixel limits
- **3c. SVG** (`image/svg.rs`): resvg with external resources off, rasterised at the target pixel size
- **3d. Encoders** (`image/encode.rs`): fit-to-box scaling with exact aspect math, area-average downscale, `▀` half-block lines with 24-bit SGR, and Kitty / iTerm2 sequences, all pure functions
→ verify: unit tests per file; encoders have exact-output tests

## 4. Renderer integration (lead) ✅
A standalone image paragraph becomes an image block behind container gutters, with an alt caption. Inline images are unchanged. Load failure gives alt text.
→ verify: fixtures with local PNG/SVG render; invariants hold; `--images never` matches 0.2.0

## 5. CLI and pager (lead) ✅
`--images`, `--base`, protocol detection (`KITTY_WINDOW_ID`/`TERM=xterm-kitty`/Ghostty, `TERM_PROGRAM=iTerm.app`/WezTerm), half-blocks in the pager, re-scale on resize from cached RGBA.
→ verify: CLI tests; manual check in machinageist's terminal passed 2026-11-09

## 6. Docs and release ✅
README image section, FEATURE_SPEC invariants updated (24-bit color and graphics escapes for images), CHANGELOG, binary size recorded, release 0.3.0 per `CONTRIBUTING.md`.
→ verify: `mu --version` prints 0.3.0, and `git describe` prints v0.3.0
