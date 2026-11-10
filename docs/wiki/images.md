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
| kitty, Ghostty (printing straight to the terminal) | Kitty graphics protocol, raw RGBA |
| iTerm2, WezTerm (printing straight to the terminal) | iTerm2 inline images, PNG-encoded |
| The pager, tmux, pipes, every other terminal | 24-bit-color `▀` half blocks: top pixel as foreground, bottom as background |

The pager always uses half blocks. Native protocols are only used when writing straight to a supporting terminal outside tmux.

## Image formats {#image-formats}

PNG, JPEG, GIF (first frame) and WebP are decoded by format, detected from their magic bytes. SVG is rendered with `resvg`. SVG text isn't rendered, and an SVG can't load external files or URLs.

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
- No EXIF orientation for JPEGs, no animated GIFs, no SVG text, and no images inline within a line of text.

The full flow is [logic map 7](/wiki/logic-maps#map-7).
