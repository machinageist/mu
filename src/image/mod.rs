// Author: machinageist
// Date: 2026-11-09
// Description: Image settings, decoded pixels, and the load and draw entry points
// Notes: The library default is ImageMode::Never so `render` and `present` stay pure text
// with no file or network access unless a caller opts in

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageMode {
    // Show alt text only, as before images were supported
    #[default]
    Never,
    // Load local files and data: URIs
    Auto,
    // Also download http(s) images
    Fetch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protocol {
    // 24-bit color upper-half-block text, works in any truecolor terminal and in the pager
    #[default]
    HalfBlock,
    // Kitty graphics protocol (kitty, Ghostty, WezTerm)
    Kitty,
    // iTerm2 inline images (iTerm2, WezTerm)
    Iterm,
}

// Pixels per terminal cell, used only for its ratio when the terminal reports nothing
const DEFAULT_CELL_WIDTH: u32 = 8;
const DEFAULT_CELL_HEIGHT: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSize {
    pub width: u32,
    pub height: u32,
}

impl Default for CellSize {
    // Assume the common 1:2 cell
    fn default() -> CellSize {
        CellSize {
            width: DEFAULT_CELL_WIDTH,
            height: DEFAULT_CELL_HEIGHT,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageOptions {
    pub mode: ImageMode,
    // Directory or URL that relative image sources resolve against
    pub base: Option<String>,
    pub cell: CellSize,
    pub protocol: Protocol,
    // Tallest image in rows, usually the terminal height minus a little
    pub max_rows: Option<usize>,
}

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub(crate) mod base64;
mod codec;
mod encode;
mod source;
mod svg;

// Decode limits so a hostile file cannot exhaust memory
pub(crate) const MAX_SOURCE_BYTES: usize = 20 * 1024 * 1024;
pub(crate) const MAX_PIXELS: u64 = 64_000_000;
pub(crate) const MAX_SIDE: u32 = 16_384;

// Decoded pixels, 8-bit RGBA, row-major, straight (not premultiplied) alpha
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    // Check that the pixel buffer matches the dimensions and the limits
    pub(crate) fn is_valid(&self) -> bool {
        within_limits(self.width, self.height)
            && self.pixels.len() as u64 == u64::from(self.width) * u64::from(self.height) * 4
    }
}

// Check dimensions against the decode limits before allocating
pub(crate) fn within_limits(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_SIDE
        && height <= MAX_SIDE
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

// A loaded image, raster now or vector until its target size is known
pub(crate) enum Picture {
    Raster(Rgba),
    Vector(svg::Svg),
}

impl Picture {
    // Natural size in pixels
    pub(crate) fn size(&self) -> (u32, u32) {
        match self {
            Picture::Raster(image) => (image.width, image.height),
            Picture::Vector(svg) => svg.size(),
        }
    }
}

// Decoded images by source and base, so a pager re-layout never reads or downloads twice
// Cache key is the source as written plus the base it resolved against
type CacheKey = (String, Option<String>);

thread_local! {
    static CACHE: RefCell<HashMap<CacheKey, Option<Rc<Picture>>>> = RefCell::new(HashMap::new());
}

// Load an image once per process, remembering failures too
pub(crate) fn load_cached(src: &str, options: &ImageOptions) -> Option<Rc<Picture>> {
    let key = (src.to_owned(), options.base.clone());
    if let Some(hit) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return hit;
    }
    let loaded = guarded(|| load(src, options)).flatten().map(Rc::new);
    CACHE.with(|cache| cache.borrow_mut().insert(key, loaded.clone()));
    loaded
}

// Load and decode an image source, or None when images are off or anything fails
fn load(src: &str, options: &ImageOptions) -> Option<Picture> {
    let bytes = source::fetch(src, options)?;
    if let Some(image) = codec::decode(&bytes).filter(Rgba::is_valid) {
        return Some(Picture::Raster(image));
    }
    svg::Svg::parse(&bytes).map(Picture::Vector)
}

// Draw a picture, treating a panic in the rasteriser like any other failure
pub(crate) fn draw_guarded(
    picture: &Picture,
    max_cols: usize,
    options: &ImageOptions,
) -> Vec<String> {
    guarded(|| draw(picture, max_cols, options)).unwrap_or_default()
}

// Run third-party decoding code, turning a panic into None without printing it
// Notes: the panic hook is swapped for a silent one meanwhile, so a caught panic neither
// writes over the page nor fires the pager's restore-the-terminal hook; mu is single-threaded
fn guarded<T>(work: impl FnOnce() -> T) -> Option<T> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    std::panic::set_hook(previous);
    result.ok()
}

// Draw a picture as terminal lines, at most `max_cols` wide; empty when it cannot be drawn
fn draw(picture: &Picture, max_cols: usize, options: &ImageOptions) -> Vec<String> {
    let Some((cols, rows)) = encode::fit(picture.size(), max_cols, options.max_rows, options.cell)
    else {
        return Vec::new();
    };
    // Rasterise at the pixel size the cells will show, so vectors stay sharp
    let target = (
        u32::try_from(cols)
            .unwrap_or(u32::MAX)
            .saturating_mul(options.cell.width),
        u32::try_from(rows)
            .unwrap_or(u32::MAX)
            .saturating_mul(options.cell.height),
    );
    let raster = match picture {
        Picture::Raster(image) => std::borrow::Cow::Borrowed(image),
        Picture::Vector(svg) => match svg.rasterize(target.0, target.1) {
            Some(image) => std::borrow::Cow::Owned(image),
            None => return Vec::new(),
        },
    };
    match options.protocol {
        Protocol::HalfBlock => encode::half_blocks(&raster, cols, rows),
        Protocol::Kitty => {
            let scaled = encode::resize(&raster, target.0, target.1);
            reserve(encode::kitty(&scaled, cols, rows), rows)
        }
        Protocol::Iterm => {
            let scaled = encode::resize(&raster, target.0, target.1);
            reserve(encode::iterm(&codec::encode_png(&scaled), cols, rows), rows)
        }
    }
}

// Put a native graphics escape on the first of `rows` otherwise empty lines
fn reserve(escape: String, rows: usize) -> Vec<String> {
    let mut lines = vec![String::new(); rows.max(1)];
    lines[0] = escape;
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panics_in_guarded_work_become_none() {
        assert_eq!(guarded(|| -> u8 { panic!("decoder bug") }), None);
        assert_eq!(guarded(|| 7), Some(7));
        // The normal hook is back afterwards
        assert_eq!(guarded(|| 8), Some(8));
    }
}
