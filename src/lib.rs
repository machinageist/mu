// Author: machinageist
// Date: 2026-11-09
// Description: mu library, renders Markdown and other markup or data to ANSI-styled terminal text
// Notes: The binary is a thin CLI around `render`; tests drive it with color off

mod data;
mod format;
mod inline;
mod render;
mod style;
mod table;

pub use format::{Format, detect, present, sniff};
pub use render::{Options, render};
