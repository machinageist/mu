// Author: machinageist
// Date: 2026-11-09
// Description: Image settings shared by the renderer and the CLI
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
