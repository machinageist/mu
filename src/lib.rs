// Author: machinageist
// Date: 2026-11-09
// Description: mu library, renders Markdown and other markup or data to ANSI-styled terminal text
// Notes: The binary is a thin CLI around `render`; tests drive it with color off

mod book;
mod data;
pub mod display;
mod format;
mod highlight;
mod hyperlink;
mod image;
mod inline;
mod outline;
mod render;
mod style;
mod table;

pub use book::{extract, sniff_bytes};
pub use format::{Format, detect, present, present_interactive, present_outlined, sniff};
pub use image::{CellSize, ImageMode, ImageOptions, Protocol};
pub use outline::{outline, outline_entries};
pub use render::{
    Anchor, Heading, InteractiveRendered, LinkRegion, NavigationLink, NavigationSpan, Options,
    RasterImage, Rendered, RenderedImage, render,
};
