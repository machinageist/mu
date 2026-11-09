// Author: machinageist
// Date: 2026-11-09
// Description: Fit images to the cell grid and encode them as half-blocks or graphics escapes
// Notes: Stub until implemented; every function here is pure

use super::{CellSize, Rgba};

// Choose the cell box for an image: exact aspect ratio, never wider than max_cols or
// taller than max_rows, never larger than the natural pixel size; None when it cannot fit
pub(crate) fn fit(
    _natural: (u32, u32),
    _max_cols: usize,
    _max_rows: Option<usize>,
    _cell: CellSize,
) -> Option<(usize, usize)> {
    None
}

// Resample to exactly width x height with area averaging
pub(crate) fn resize(image: &Rgba, _width: u32, _height: u32) -> Rgba {
    image.clone()
}

// Draw as `rows` lines of `cols` upper-half-block cells in 24-bit color
pub(crate) fn half_blocks(_image: &Rgba, _cols: usize, _rows: usize) -> Vec<String> {
    Vec::new()
}

// Kitty graphics escape that shows the image over cols x rows cells without moving the cursor
pub(crate) fn kitty(_image: &Rgba, _cols: usize, _rows: usize) -> String {
    String::new()
}

// iTerm2 inline-image escape for a PNG over cols x rows cells without moving the cursor
pub(crate) fn iterm(_png: &[u8], _cols: usize, _rows: usize) -> String {
    String::new()
}
