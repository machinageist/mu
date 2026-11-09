// Author: machinageist
// Date: 2026-11-09
// Description: mdread library, renders Markdown to ANSI-styled terminal text
// Notes: The binary is a thin CLI around `render`; tests drive it with color off

mod inline;
mod render;
mod style;
mod table;

pub use render::{Options, render};
