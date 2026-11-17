---
title: Images
group: Guide
order: 5
summary: Pictures in the terminal: protocols and limits
---

Pictures in documents are drawn in the terminal with their exact aspect ratio and the alt text as a caption. Added in 0.3.0.

## When an image is drawn {#when}

An image that stands alone in its paragraph or block is drawn as a picture, with its alt text as a dim caption underneath:

- HTML `<img>` and `<figure>`
- Markdown `![alt](src)` on its own line
- RST `.. image::`
- AsciiDoc `image::`
- Org `[[file.png]]`

An image inside running text stays `[image: alt]`, because a picture can't sit inside a wrapped line.

## Sizing and aspect ratio {#sizing}

mu reads the terminal's cell size in pixels and scales the image by one factor, so it fits both the text width and the screen height:

```text
scale = min(1, max_width_px / image_width, max_height_px / image_height)
```

- Width and height shrink together, so the aspect ratio is exact to within half a cell.
- Images are never enlarged past their natural size, never wider than the content column, and never taller than the screen minus 2 rows.
- When a terminal doesn't report pixel sizes, mu assumes the common 8×16 (1:2) cell.

## Drawing protocols {#protocols}

| Where | Protocol |
|---|---|
| Direct kitty, Ghostty (pager or straight output) | Kitty graphics protocol, raw RGBA; the pager acknowledges and reuses clipped virtual placements |
| iTerm2, WezTerm (printing straight to the terminal) | iTerm2 inline images, PNG-encoded |
| Other pagers/terminals, tmux, screen and pipes | 24-bit-color `▀` half blocks: top pixel as foreground, bottom as background |

The pager keeps a pixel-resolution image sidecar instead of shrinking covers to one pixel per text column. On a direct Kitty/Ghostty terminal, an image is uploaded once, acknowledged, then painted through virtual text placements clipped to the visible body. Scrolling and redraws reuse the same upload; outlines hide images and resizing refits them without touching the footer. Only the pager's own image IDs are deleted on eviction, reflow and exit. No delete-all command is used.

Half-block text remains available throughout. Unknown or multiplexed terminals, a rejected upload/placement, a missing acknowledgement or a resource limit keep that fallback rather than leaving holes in the page. iTerm2 inline images remain available for straight output, not the pager. `--images never` and color-off behavior are unchanged.

## Image formats {#image-formats}

PNG, JPEG, GIF (first frame) and WebP are decoded by format, detected from their magic bytes. SVG is rendered with `resvg`. SVG text isn't rendered, and an SVG can't load external files or URLs.

> [!NOTE]
> This is about pictures inside a document. An `.svg` file given to mu directly is XML, so `mu logo.svg` shows its highlighted source. See [XML](/wiki/formats#xml).

## Sources {#sources}

| Source | Loaded when |
|---|---|
| Local paths | Always (unless images are off). Relative to the input file, or to `--base`. Piped input needs `--base` |
| `data:` URIs | Always (unless images are off). Base64 or percent-encoded |
| Remote `http(s)` | Only with `--images fetch`, which is the default when the input itself is a URL. Fetched with the system `curl`, 10 s timeout, 20 MB cap, http(s) only including on redirect |

> [!NOTE]
> A local file can't make mu touch the network unless you ask it to.

Decoded images are cached for the run, so a pager resize redraws without reloading.

## Fallback {#fallback}

With `--images never`, with color off, or for any image that fails to load or decode, the `[image: alt]` text is shown instead. Image decoding runs under `catch_unwind`, so a bug in a third-party decoder also falls back to alt text rather than crashing mu.

## Limits {#image-limits}

- Sources over 20 MB, images over 64 megapixels, and images over 16384 pixels on a side are refused before decoding.
- Interactive pixel sidecars and the pager's native cache each have a 32 MiB RGBA budget; the cache has at most eight live images. Source decode limits still apply independently. Oversized or excess images keep their half-block fallback.
- No EXIF orientation for JPEGs, no animated GIFs, no SVG text, and no images inline within a line of text.

The full flow is [logic map 7](/wiki/logic-maps#map-7).
