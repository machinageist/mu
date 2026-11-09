// Author: machinageist
// Date: 2026-11-09
// Description: Fit images to the cell grid and encode them as half-blocks or graphics escapes
// Notes: Every function here is pure. Kitty escapes follow
// https://sw.kovidgoyal.net/kitty/graphics-protocol/ and iTerm2 escapes follow
// https://iterm2.com/documentation-images.html

use super::{CellSize, Rgba, base64};
use crate::style::SGR_RESET;

// Half-block glyphs and SGR pieces, one cell is a top and a bottom pixel
const UPPER_HALF: char = '▀';
const LOWER_HALF: char = '▄';
const EMPTY_CELL: char = ' ';
const SGR_FG_RGB: &str = "\x1b[38;2;";
const SGR_BG_RGB: &str = "\x1b[48;2;";
const SGR_FG_DEFAULT: &str = "\x1b[39m";
const SGR_BG_DEFAULT: &str = "\x1b[49m";
const SGR_END: char = 'm';

// Alpha at or above this counts as opaque, below it shows the terminal background
const OPAQUE_ALPHA: u8 = 128;

// Kitty graphics protocol: transmit and display raw RGBA, keep the cursor, no replies
const KITTY_START: &str = "\x1b_G";
const KITTY_END: &str = "\x1b\\";
const KITTY_ACTION: &str = "a=T";
const KITTY_FORMAT: &str = "f=32";
const KITTY_PIXEL_WIDTH: &str = "s=";
const KITTY_PIXEL_HEIGHT: &str = "v=";
const KITTY_COLS: &str = "c=";
const KITTY_ROWS: &str = "r=";
const KITTY_KEEP_CURSOR: &str = "C=1";
const KITTY_QUIET: &str = "q=2";
const KITTY_MORE: &str = "m=";
const KITTY_CHUNK: usize = 4096;

// iTerm2 inline image: size in cells, exact box, keep the cursor
const ITERM_START: &str = "\x1b]1337;File=inline=1;size=";
const ITERM_WIDTH: &str = ";width=";
const ITERM_HEIGHT: &str = ";height=";
const ITERM_FLAGS: &str = ";preserveAspectRatio=0;doNotMoveCursor=1:";
const ITERM_END: char = '\x07';

// Choose the cell box for an image: exact aspect ratio, never wider than max_cols or
// taller than max_rows, never larger than the natural pixel size; None when it cannot fit
//
// One scale s <= 1 applies to both sides, then each side rounds to whole cells, so the
// drawn pixel box differs from the exact scaled box by at most half a cell per side.
// The aspect error is therefore |cols*cw/(rows*ch) - w/h| <= (h*cw + w*ch) / (2*h*rows*ch).
// The one exception is a side shorter than half a cell, clamped up to one cell, where the
// error on that side grows to under a whole cell
pub(crate) fn fit(
    natural: (u32, u32),
    max_cols: usize,
    max_rows: Option<usize>,
    cell: CellSize,
) -> Option<(usize, usize)> {
    let (width, height) = natural;
    if width == 0
        || height == 0
        || max_cols == 0
        || max_rows == Some(0)
        || cell.width == 0
        || cell.height == 0
    {
        return None;
    }
    let cell_width = f64::from(cell.width);
    let cell_height = f64::from(cell.height);
    let natural_width = f64::from(width);
    let natural_height = f64::from(height);

    // Largest single scale that fits both limits without enlarging
    let col_limit = max_cols as f64 * cell_width / natural_width;
    let row_limit = max_rows.map_or(f64::INFINITY, |rows| {
        rows as f64 * cell_height / natural_height
    });
    let scale = col_limit.min(row_limit).min(1.0);

    // Round each side to whole cells, at least one
    let cols = ((natural_width * scale / cell_width).round() as usize).clamp(1, max_cols);
    let rows = ((natural_height * scale / cell_height).round() as usize)
        .clamp(1, max_rows.unwrap_or(usize::MAX));
    Some((cols, rows))
}

// Resample to exactly width x height with area averaging
//
// Each output pixel averages the whole source pixels in its box, in premultiplied space
// so transparent pixels add no colour; upscaling degrades to nearest neighbour
pub(crate) fn resize(image: &Rgba, width: u32, height: u32) -> Rgba {
    if width == image.width && height == image.height {
        return image.clone();
    }
    let out_len = width as usize * height as usize * 4;
    if image.width == 0 || image.height == 0 {
        return Rgba {
            width,
            height,
            pixels: vec![0; out_len],
        };
    }
    let source_width = image.width as usize;
    let col_spans = spans(image.width, width);
    let row_spans = spans(image.height, height);
    let mut pixels = Vec::with_capacity(out_len);
    for &(top, bottom) in &row_spans {
        for &(left, right) in &col_spans {
            // Sum premultiplied colour and alpha over the box
            let mut sums = [0u64; 4];
            for y in top..bottom {
                let row =
                    &image.pixels[(y * source_width + left) * 4..(y * source_width + right) * 4];
                for pixel in row.as_chunks::<4>().0 {
                    let alpha = u64::from(pixel[3]);
                    sums[0] += u64::from(pixel[0]) * alpha;
                    sums[1] += u64::from(pixel[1]) * alpha;
                    sums[2] += u64::from(pixel[2]) * alpha;
                    sums[3] += alpha;
                }
            }
            let count = ((bottom - top) * (right - left)) as u64;
            // Back to straight alpha, rounded to nearest
            for channel in &sums[..3] {
                let value = (channel + sums[3] / 2).checked_div(sums[3]).unwrap_or(0);
                pixels.push(value as u8);
            }
            pixels.push(((sums[3] + count / 2) / count) as u8);
        }
    }
    Rgba {
        width,
        height,
        pixels,
    }
}

// Split source pixels into one non-empty span per target pixel
fn spans(source: u32, target: u32) -> Vec<(usize, usize)> {
    let source = u64::from(source);
    let target = u64::from(target);
    (0..target)
        .map(|index| {
            let start = index * source / target;
            let end = ((index + 1) * source / target).max(start + 1);
            (start as usize, end as usize)
        })
        .collect()
}

// Colour state of the terminal pen, so repeated SGR can be skipped
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pen {
    Unknown,
    Default,
    Rgb([u8; 3]),
}

// Draw as `rows` lines of `cols` upper-half-block cells in 24-bit color
//
// A cell whose top is transparent but bottom is not uses the lower half block, because
// SGR 39 would paint the top in the text colour rather than the background
pub(crate) fn half_blocks(image: &Rgba, cols: usize, rows: usize) -> Vec<String> {
    let (Ok(width), Ok(height)) = (u32::try_from(cols), u32::try_from(rows.saturating_mul(2)))
    else {
        return Vec::new();
    };
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let scaled = resize(image, width, height);
    (0..rows).map(|row| half_block_line(&scaled, row)).collect()
}

// Encode one row of cells from two pixel rows
fn half_block_line(image: &Rgba, row: usize) -> String {
    let mut line = String::new();
    let mut fg = Pen::Unknown;
    let mut bg = Pen::Unknown;
    for col in 0..image.width as usize {
        let top = opaque_colour(image, col, row * 2);
        let bottom = opaque_colour(image, col, row * 2 + 1);
        let (glyph, want_fg, want_bg) = match (top, bottom) {
            (None, None) => (EMPTY_CELL, fg, Pen::Default),
            (None, Some(lower)) => (LOWER_HALF, Pen::Rgb(lower), Pen::Default),
            (Some(upper), lower) => (
                UPPER_HALF,
                Pen::Rgb(upper),
                lower.map_or(Pen::Default, Pen::Rgb),
            ),
        };
        if want_fg != fg {
            push_pen(&mut line, want_fg, SGR_FG_RGB, SGR_FG_DEFAULT);
            fg = want_fg;
        }
        if want_bg != bg {
            push_pen(&mut line, want_bg, SGR_BG_RGB, SGR_BG_DEFAULT);
            bg = want_bg;
        }
        line.push(glyph);
    }
    line.push_str(SGR_RESET);
    line
}

// Read a pixel's colour, or None when it is mostly transparent
fn opaque_colour(image: &Rgba, x: usize, y: usize) -> Option<[u8; 3]> {
    let at = (y * image.width as usize + x) * 4;
    let pixel = &image.pixels[at..at + 4];
    (pixel[3] >= OPAQUE_ALPHA).then(|| [pixel[0], pixel[1], pixel[2]])
}

// Append the SGR that sets a pen colour
fn push_pen(line: &mut String, pen: Pen, rgb_prefix: &str, default: &str) {
    match pen {
        Pen::Rgb([red, green, blue]) => {
            line.push_str(&format!("{rgb_prefix}{red};{green};{blue}{SGR_END}"));
        }
        Pen::Default => line.push_str(default),
        Pen::Unknown => {}
    }
}

// Kitty graphics escape that shows the image over cols x rows cells without moving the cursor
//
// No image id is sent, so each image is anonymous: nothing can collide with ids other
// programs use, and mu never needs to refer to an image after drawing it
pub(crate) fn kitty(image: &Rgba, cols: usize, rows: usize) -> String {
    if image.width == 0 || image.height == 0 || image.pixels.is_empty() {
        return String::new();
    }
    let payload = base64::encode(&image.pixels);
    let mut out = String::with_capacity(payload.len() + payload.len() / KITTY_CHUNK * 16 + 64);
    for start in (0..payload.len()).step_by(KITTY_CHUNK) {
        let end = (start + KITTY_CHUNK).min(payload.len());
        let more = u8::from(end < payload.len());
        out.push_str(KITTY_START);
        // Only the first chunk carries the image keys
        if start == 0 {
            out.push_str(&format!(
                "{KITTY_ACTION},{KITTY_FORMAT},{KITTY_PIXEL_WIDTH}{},{KITTY_PIXEL_HEIGHT}{},\
                 {KITTY_COLS}{cols},{KITTY_ROWS}{rows},{KITTY_KEEP_CURSOR},{KITTY_QUIET},",
                image.width, image.height
            ));
        }
        out.push_str(&format!("{KITTY_MORE}{more};{}", &payload[start..end]));
        out.push_str(KITTY_END);
    }
    out
}

// iTerm2 inline-image escape for a PNG over cols x rows cells without moving the cursor
//
// preserveAspectRatio=0 because fit already chose the exact box from the pixel geometry
pub(crate) fn iterm(png: &[u8], cols: usize, rows: usize) -> String {
    if png.is_empty() {
        return String::new();
    }
    format!(
        "{ITERM_START}{}{ITERM_WIDTH}{cols}{ITERM_HEIGHT}{rows}{ITERM_FLAGS}{}{ITERM_END}",
        png.len(),
        base64::encode(png)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    // Build an image from rows of pixels
    fn image(rows: &[&[[u8; 4]]]) -> Rgba {
        Rgba {
            width: rows[0].len() as u32,
            height: rows.len() as u32,
            pixels: rows
                .iter()
                .flat_map(|row| row.iter().flatten())
                .copied()
                .collect(),
        }
    }

    // Build a cell size
    fn cell(width: u32, height: u32) -> CellSize {
        CellSize { width, height }
    }

    // Remove CSI SGR sequences
    fn strip_sgr(line: &str) -> String {
        let mut out = String::new();
        let mut chars = line.chars();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                for inner in chars.by_ref() {
                    if inner == 'm' {
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn fit_table() {
        let cases = [
            // 8x16 cells: wide, tall, square, small, col-limited, sliver
            ((1600, 400), 80, None, cell(8, 16), Some((80, 10))),
            ((400, 1600), 80, Some(24), cell(8, 16), Some((12, 24))),
            ((1000, 1000), 80, Some(24), cell(8, 16), Some((48, 24))),
            ((40, 32), 80, None, cell(8, 16), Some((5, 2))),
            ((2000, 100), 50, Some(40), cell(8, 16), Some((50, 1))),
            ((1000, 1), 10, None, cell(8, 16), Some((10, 1))),
            // 10x20 cells: wide, row-limited
            ((1920, 1080), 100, Some(50), cell(10, 20), Some((100, 28))),
            ((300, 900), 100, Some(10), cell(10, 20), Some((7, 10))),
            // 9x21 cells: square, small, wide
            ((900, 900), 200, Some(30), cell(9, 21), Some((70, 30))),
            ((45, 42), 80, None, cell(9, 21), Some((5, 2))),
            ((1800, 210), 40, None, cell(9, 21), Some((40, 2))),
            // 1:1 cells
            ((300, 200), 150, None, cell(1, 1), Some((150, 100))),
            ((50, 50), 80, Some(30), cell(1, 1), Some((30, 30))),
            ((20, 10), 80, Some(30), cell(1, 1), Some((20, 10))),
            // Unfittable
            ((0, 10), 80, None, cell(8, 16), None),
            ((10, 0), 80, None, cell(8, 16), None),
            ((10, 10), 0, None, cell(8, 16), None),
            ((10, 10), 80, Some(0), cell(8, 16), None),
            ((10, 10), 80, None, cell(0, 16), None),
            ((10, 10), 80, None, cell(8, 0), None),
            ((10, 10), 80, None, cell(0, 0), None),
        ];
        for (natural, max_cols, max_rows, geometry, expected) in cases {
            assert_eq!(
                fit(natural, max_cols, max_rows, geometry),
                expected,
                "{natural:?} in {max_cols}x{max_rows:?} at {geometry:?}"
            );
        }
    }

    #[test]
    fn fit_keeps_aspect_within_half_a_cell() {
        let geometries = [cell(8, 16), cell(10, 20), cell(9, 21), cell(1, 1)];
        let mut seed = 0x2545_f491_u64;
        let mut next = |bound: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % bound + 1
        };
        for _ in 0..5000 {
            let width = next(300) as u32;
            let height = next(300) as u32;
            let max_cols = next(120) as usize;
            let max_rows = (next(2) == 1).then(|| next(60) as usize);
            let geometry = geometries[next(4) as usize - 1];
            let (cols, rows) = fit((width, height), max_cols, max_rows, geometry).unwrap();
            assert!(cols <= max_cols && rows <= max_rows.unwrap_or(usize::MAX));

            // Drawn box never exceeds the natural size by more than rounding
            let (cw, ch) = (f64::from(geometry.width), f64::from(geometry.height));
            let (w, h) = (f64::from(width), f64::from(height));
            assert!(cols as f64 * cw <= w + cw / 2.0 || cols == 1);
            assert!(rows as f64 * ch <= h + ch / 2.0 || rows == 1);

            // Half a cell per side, a whole cell when a side was clamped up to one
            let slack = if cols == 1 || rows == 1 { 1.0 } else { 0.5 };
            let bound = slack * (h * cw + w * ch) / (h * rows as f64 * ch);
            let error = (cols as f64 * cw / (rows as f64 * ch) - w / h).abs();
            assert!(
                error <= bound + 1e-9,
                "{width}x{height} -> {cols}x{rows} at {geometry:?}: {error} > {bound}"
            );
        }
    }

    #[test]
    fn resize_averages_a_box() {
        let source = image(&[&[[0, 0, 0, 255], [255, 255, 255, 255]], &[RED, BLUE]]);
        let scaled = resize(&source, 1, 1);
        assert_eq!(scaled.pixels, vec![128, 64, 128, 255]);
        assert_eq!(resize(&source, 2, 2), source);
    }

    #[test]
    fn resize_ignores_transparent_colour() {
        let source = image(&[&[RED, CLEAR]]);
        assert_eq!(resize(&source, 1, 1).pixels, vec![255, 0, 0, 128]);
    }

    #[test]
    fn resize_upscales_by_nearest() {
        let source = image(&[&[RED, BLUE]]);
        assert_eq!(resize(&source, 4, 1), image(&[&[RED, RED, BLUE, BLUE]]));
    }

    #[test]
    fn resize_is_fast() {
        let source = Rgba {
            width: 4000,
            height: 3000,
            pixels: vec![200; 4000 * 3000 * 4],
        };
        let started = std::time::Instant::now();
        let scaled = resize(&source, 200, 100);
        let elapsed = started.elapsed();
        assert_eq!(scaled.pixels.len(), 200 * 100 * 4);
        if !cfg!(debug_assertions) {
            assert!(elapsed.as_millis() < 100, "{elapsed:?}");
        }
    }

    #[test]
    fn half_blocks_one_cell() {
        let lines = half_blocks(&image(&[&[RED], &[BLUE]]), 1, 1);
        assert_eq!(lines, vec!["\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▀\x1b[0m"]);
    }

    #[test]
    fn half_blocks_collapse_repeated_colours() {
        let lines = half_blocks(&image(&[&[RED, GREEN], &[BLUE, BLUE]]), 2, 1);
        assert_eq!(
            lines,
            vec!["\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▀\x1b[38;2;0;255;0m▀\x1b[0m"]
        );
        let lines = half_blocks(&image(&[&[RED, RED, RED], &[BLUE, BLUE, BLUE]]), 3, 1);
        assert_eq!(
            lines,
            vec!["\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▀▀▀\x1b[0m"]
        );
    }

    #[test]
    fn half_blocks_two_rows() {
        let source = image(&[&[RED, RED], &[BLUE, BLUE], &[GREEN, RED], &[GREEN, RED]]);
        let lines = half_blocks(&source, 2, 2);
        assert_eq!(
            lines,
            vec![
                "\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▀▀\x1b[0m",
                "\x1b[38;2;0;255;0m\x1b[48;2;0;255;0m▀\x1b[38;2;255;0;0m\x1b[48;2;255;0;0m▀\x1b[0m",
            ]
        );
    }

    #[test]
    fn half_blocks_show_background_through_transparency() {
        let faint = [9, 9, 9, 127];
        let solid = [255, 0, 0, 128];
        let source = image(&[&[faint, CLEAR, solid], &[BLUE, CLEAR, faint]]);
        let lines = half_blocks(&source, 3, 1);
        assert_eq!(
            lines,
            vec!["\x1b[38;2;0;0;255m\x1b[49m▄ \x1b[38;2;255;0;0m▀\x1b[0m"]
        );
        let lines = half_blocks(&image(&[&[CLEAR], &[CLEAR]]), 1, 1);
        assert_eq!(lines, vec!["\x1b[49m \x1b[0m"]);
    }

    #[test]
    fn half_blocks_fill_exactly_cols_cells() {
        let row: Vec<[u8; 4]> = (0..7).map(|x| [x * 30, 100, 200, 255]).collect();
        let rows: Vec<&[[u8; 4]]> = (0..5).map(|_| row.as_slice()).collect();
        let lines = half_blocks(&image(&rows), 4, 3);
        assert_eq!(lines.len(), 3);
        for line in lines {
            assert!(!line.contains('\n') && line.ends_with(SGR_RESET));
            let text = strip_sgr(&line);
            assert_eq!(text.chars().count(), 4);
            assert_eq!(text.width(), 4);
        }
    }

    #[test]
    fn kitty_one_pixel() {
        let source = image(&[&[[1, 2, 3, 4]]]);
        assert_eq!(
            kitty(&source, 1, 1),
            "\x1b_Ga=T,f=32,s=1,v=1,c=1,r=1,C=1,q=2,m=0;AQIDBA==\x1b\\"
        );
        let empty = Rgba {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
        assert_eq!(kitty(&empty, 1, 1), "");
    }

    #[test]
    fn kitty_chunks_long_payloads() {
        let source = Rgba {
            width: 40,
            height: 40,
            pixels: vec![7; 40 * 40 * 4],
        };
        let escape = kitty(&source, 5, 3);
        let chunks: Vec<&str> = escape.split_inclusive(KITTY_END).collect();
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].starts_with("\x1b_Ga=T,f=32,s=40,v=40,c=5,r=3,C=1,q=2,m=1;"));
        assert!(chunks[1].starts_with("\x1b_Gm=1;"));
        assert!(chunks[2].starts_with("\x1b_Gm=0;"));
        let mut payload = String::new();
        for chunk in &chunks {
            assert!(chunk.starts_with(KITTY_START) && chunk.ends_with(KITTY_END));
            let data = &chunk[chunk.find(';').unwrap() + 1..chunk.len() - KITTY_END.len()];
            assert!(data.len() <= KITTY_CHUNK);
            payload.push_str(data);
        }
        assert_eq!(base64::decode(&payload), Some(source.pixels));
    }

    #[test]
    fn iterm_tiny_file() {
        assert_eq!(
            iterm(b"foo", 2, 1),
            "\x1b]1337;File=inline=1;size=3;width=2;height=1;\
             preserveAspectRatio=0;doNotMoveCursor=1:Zm9v\x07"
        );
        assert_eq!(iterm(b"", 2, 1), "");
    }
}
