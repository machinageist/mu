# Change: Mouse navigation and native pager images

Status: authorized implementation. User approved all recommendations and explicitly requested commit and push when done.

## Accepted scope and delivery

- Record the already-verified links/navigation baseline as local 0.9.0, then deliver this corrective batch as 0.9.1. Every delivered bug-fix batch increments PATCH. Follow dedicated `Release X.Y.Z` commits; push only after final verification. No tags, GitHub release publication, wiki deployment or changes to personal books/configuration.
- Ordinary unmodified left-click follows internal EPUB links. External web/mailto activation remains the terminal's responsibility; never add an opener or manufacture ZIP-member file URIs.
- Open and close an owned temporary Ghostty QA window for actual mouse and visual image acceptance. Preserve existing windows, terminal configuration, user files and current desktop session.
- Preserve all existing keyboard navigation, source identity/history, scoped anchors, entity/provenance validation and plain-output invariants.

## Mouse contract

- Enable 1000 and SGR 1006 mouse reporting only for an interactive document with resolvable internal links; disable requested modes on normal/error/panic exit. Preserve wheel scrolling and document the terminal selection override.
- Decode SGR presses/releases/wheel reports incrementally and boundedly. Consume unsupported, malformed and legacy mouse packets atomically, including fragmented payloads; payload bytes must never become `q`, search or navigation keys.
- Unmodified left press follows exactly one canonical source link, including any visible wrapped continuation or table-aligned segment. Releases, motion, modified/other buttons, status/chrome, padding/borders, prose gaps, clipped replacement spaces, prompt and outline must not activate links.
- Record actual half-open display-column spans during layout, translate both endpoints through gutters/table alignment, and filter/reindex span ownership atomically with unresolved links. Never infer whole-label rectangles from semantic text.
- Keep source encounter order independent of physical traversal. Mouse follow saves the current viewport directly, including when the first source line is off-screen; Backspace must restore it exactly and survive reflow.
- Use one shared grapheme/display-unit model for wrapping, clipping and hit testing; preserve combining/ZWJ sequences and zero-width anchor lifetime.

## Native image contract

- In direct supported Kitty/Ghostty terminals, the pager can display native-resolution images rather than one-by-two samples per cell. Keep conservative unsupported/multiplexer/color-disabled fallback. No new terminal setting or network permission is required.
- Keep native payloads out of document text, search, width calculations, highlighting and clipping. Keep existing half-block/alt text as a usable fallback while upload/virtual-placement acknowledgment is pending or unavailable.
- Use pager-owned Kitty virtual placements and explicit per-cell Unicode placeholder row/column coordinates generated after text processing. Clip every edge to body rows/columns; never draw into gutters/captions/status/outline or leave stale graphics after moving away.
- Register only owned image IDs before writing controls; bounded uploads/cache, no retransmission on each redraw, and deletion of owned IDs on eviction/reflow/normal/error/panic cleanup. Never delete all terminal images.
- Consume bounded fragmented APC replies separately from keys; rejection or bounded timeout falls back without swallowing later user input. Verify actual upload/placement acceptance before painting native placeholders.
- Prevent literal source placeholder characters from acquiring image semantics while managed graphics are active. Preserve generated SGR/OSC8 and transparency.
- Refit/reflow images on complete terminal geometry (columns, rows and pixels), including height-only/font changes and explicit `--width` sessions.

## Compatibility and shared implementation interfaces

Preserve the 0.9.0 `Rendered`, `NavigationLink`, `Options` and `ImageOptions` field shapes and the existing functions. Add a separate opt-in interactive rendering result rather than breaking existing public struct literals in a patch release.

The shared interface is:

- `NavigationSpan { line: usize, start: usize, end: usize }` (zero-based, half-open columns).
- `LinkRegion { link: usize, spans: Vec<NavigationSpan> }`, indexing the final filtered canonical `Rendered.links` vector.
- `RasterImage { width: u32, height: u32, pixels: Arc<[u8]> }`, row-major straight RGBA.
- `RenderedImage { line: usize, column: usize, columns: usize, rows: usize, raster: Arc<RasterImage> }`.
- `InteractiveRendered { document: Rendered, link_regions: Vec<LinkRegion>, images: Vec<RenderedImage> }` with Default/Clone/Debug/PartialEq/Eq.
- `present_interactive(input: &str, format: Format, options: &Options) -> InteractiveRendered`. This preserves legacy visible document output and records link regions. With `options.images.protocol == Protocol::Kitty` it records bounded full-pixel image assets while its document text remains the legacy fallback, never APC bytes. Other protocols/data formats remain usable without assets.
- Asset dimensions and buffers must be validated before encoding. Cap total captured native RGBA at 32 MiB per layout and retain fallback for omitted assets. The pager also bounds live native assets/uploads.
- A shared additive display-units helper may use `unicode-segmentation` with `unicode-width`; do not implement a partial ad-hoc grapheme algorithm. No unrelated dependencies or refactors.

## Agreed test seams and acceptance

- Actual ZIP extraction through the public interactive render boundary: precise full-link spans, ownership/reindexing, Unicode, wrap/gutters/tables and native asset pixel dimensions/content; old public struct literal compilation remains valid.
- Real CLI/PTY: press/release, all fragmentation boundaries, legacy/malformed/overflow inputs, wheel/modes/terminal restoration; wrapped/offscreen/horizontal clicks and exact Backspace/reflow; graphics ACK success/rejection/timeout, clipping, redraw/outline/search, pixel/height/explicit-width resize, bounded uploads and owned cleanup. No URI activation or personal files in tests.
- Real Ghostty: recognizable generated/redistributable high-detail portrait cover, actual left-click jump/Backspace, all image edges, resize, outline/search/jump/back, clean gutters/status and no residual graphics on exit. Save before/after captures and exact binary identity; PTY bytes do not substitute for visual or physical click acceptance.
- Parent verification and independent standards/spec reviews, corrective regressions, then a frozen-source full gate for each commit/release as required by CONTRIBUTING. Install final 0.9.1 explicitly, verify PATH/hash/version and repeat representative real interactions. Push exact verified commits and read back origin/main; no force push.
