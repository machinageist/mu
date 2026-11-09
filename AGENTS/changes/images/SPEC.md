# Change: images in the terminal

Ships as **0.3.0**: new features, a new `Options` field (breaking for library users below 1.0), and new dependencies.

## Outcome

Images in a document body are drawn in the terminal, not shown as `[image: alt]`. The aspect ratio is kept exactly. HTML `<img>` is the main target. Every document format gets it, because all of them emit the same `Image` events.

## Decisions (machinageist, 2026-11-09)

- **Drawing:** native graphics (Kitty protocol, iTerm2 inline images) when writing straight to a terminal that supports them. Otherwise, and always inside the pager, 24-bit-color `▀` half-blocks.
- **Fetching:** local files and `data:` URIs always load. Remote `http(s)` images load only with `--images fetch`.
- **Formats:** PNG, JPEG, GIF (first frame), WebP and SVG.
- 0.2.0 released first, so this is 0.3.0 work.

## Behaviour

- **What is drawn:** an image that is alone in its paragraph or block (HTML `<img>`, `<figure>`, `<p><img></p>`, Markdown `![](x)` on its own line, RST `.. image::`, AsciiDoc `image::`, Org `[[file.png]]`). The alt text, when present, goes on a dim caption line under it. An image inside running text stays `[image: alt]`, because a picture cannot sit inside a wrapped line.
- **Size:** the image is scaled to fit the content width (the column after quote and list gutters) and at most the terminal height minus 2 rows. It is never enlarged beyond its natural pixel size. Width and height scale by the same factor, so the aspect ratio holds exactly.
- **Cell geometry:** pixel per cell comes from `TIOCGWINSZ` `ws_xpixel`/`ws_ypixel`. When the terminal reports 0, a 1:2 width-to-height cell is assumed. A half-block cell is 1 image pixel wide and 2 tall, so its pixels are square only on a 1:2 cell. Scaling uses the real cell ratio instead of assuming square.
- **Sources:**
  - **Local files:** a relative `src` resolves against the input file's directory, or against `--base` when given. Stdin with no `--base` resolves against the current directory.
  - **`data:` URIs:** base64 `data:image/...` is decoded.
  - **Remote:** an `http(s)` URL loads only with `--images fetch`, through the system `curl` (`-fsSL`, 10 s timeout, size cap). There is no TLS crate.
- **Flags:**
  - `--images <auto|fetch|never>`, default `auto`: `auto` loads local and `data:` images, `fetch` adds remote ones, and `never` keeps today's alt text.
  - `--base <DIR|URL>` sets where relative image paths resolve from.
  - When color is off (`--color never`, `NO_COLOR`, or output that is not a terminal), images fall back to alt text, so pipes and plain output stay text.
- **Failure:** any load, decode or limit failure falls back to `[image: alt]`. It is never an error exit.
- **Pager:** half-blocks only. A resize re-scales from a cached decoded image.

## Safety

- **Decode limits:** at most 20 MB per source, 64 megapixels, and 16k pixels per side. GIF uses the first frame only.
- **SVG:** rendered without external resources. `<image href>`/`xlink:href` and remote fonts are not loaded. Text is not rendered unless it comes free with the renderer configuration that stays inside the size budget.
- **Terminal output:**
  - **Escapes:** the escapes mu writes are its own SGR codes, 24-bit colors for half-blocks, and its own graphics-protocol sequences. Payloads are mu-generated base64, never bytes from the document.
  - **Control characters:** input control characters are still scrubbed everywhere.
- **Network:** no request happens unless the user passes `--images fetch`.

## Acceptance criteria

- `mu page.html`, where the page has a local PNG in its body, draws the image. The ratio of drawn width to height, in pixels after the cell geometry, matches the source within one cell of rounding.
- Exact-output tests of half-block encoding for small synthetic images, including the aspect math across several cell geometries.
- Unit tests for `data:` URIs, path resolution, decode limits, and the fallback to alt text.
- The existing invariants still hold for all document fixtures when images are off or unavailable. With images on, no line is wider than the width.
- `--images never` gives byte-identical output to 0.2.0 on every fixture.
- The binary size is measured and recorded. The new budget is set from that measurement and agreed with machinageist before release.

## Out of scope

- Animated GIF playback, and images inline within text lines
- Sixel output, which is not needed since half-blocks cover the terminals without native support
- Caching downloads between runs
