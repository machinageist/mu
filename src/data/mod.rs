// Author: machinageist
// Date: 2026-11-09
// Description: Data formats as syntax-highlighted lines, printed full width with hard wrapping
// Notes: Highlighters only classify text; this printer owns scrubbing, tabs, width and SGR,
// so every data format keeps the same output invariants as documents

pub mod code;
pub mod json;
pub mod overstrike;
pub mod toml;
pub mod xml;
pub mod yaml;

use unicode_width::UnicodeWidthChar;

use crate::inline::{CONTROL_REPLACEMENT, text_width};
use crate::render::{FRAME_WRAP_BAR, Options, TAB_STOP};
use crate::style::{SGR_RESET, STYLE_FRAME, Style, paint};

// One run of text in one style, never containing a newline
pub type Span = (Style, String);

// One source line as styled runs
pub type StyledLine = Vec<Span>;

// Narrowest usable line even on an absurd width
const MIN_DATA_WIDTH: usize = 8;

// Write styled lines, hard-wrapping long ones behind a dashed continuation bar
pub fn print(lines: &[StyledLine], options: &Options) -> String {
    print_counted(lines, options).0
}

// Write styled lines, also returning the output line each input line starts on
pub fn print_counted(lines: &[StyledLine], options: &Options) -> (String, Vec<usize>) {
    let mut printer = Printer {
        out: String::new(),
        color: options.color,
        limit: options.width.max(MIN_DATA_WIDTH),
        column: 0,
        active: Style::PLAIN,
        lines: 0,
    };
    let mut starts = Vec::with_capacity(lines.len());
    for line in lines {
        starts.push(printer.lines);
        for (style, text) in line {
            for ch in text.chars() {
                printer.push(ch, *style);
            }
        }
        printer.end_line();
    }
    (printer.out, starts)
}

struct Printer {
    out: String,
    color: bool,
    limit: usize,
    column: usize,
    active: Style,
    // Newlines written so far
    lines: usize,
}

impl Printer {
    // Append one char of source text, expanding tabs and scrubbing controls
    fn push(&mut self, ch: char, style: Style) {
        match ch {
            '\r' | '\n' => {}
            '\t' => {
                for _ in 0..TAB_STOP - self.column % TAB_STOP {
                    self.glyph(' ', 1, style);
                }
            }
            _ if ch.is_control() => self.glyph(CONTROL_REPLACEMENT, 1, style),
            _ => self.glyph(ch, ch.width().unwrap_or(0), style),
        }
    }

    // Place one glyph, wrapping first when it would overflow
    fn glyph(&mut self, glyph: char, width: usize, style: Style) {
        let bar_width = text_width(FRAME_WRAP_BAR);
        if self.column + width > self.limit && self.column > bar_width {
            self.end_line();
            paint(&mut self.out, FRAME_WRAP_BAR, STYLE_FRAME, self.color);
            self.column = bar_width;
        }
        if self.color && style != self.active {
            if style == Style::PLAIN {
                self.out.push_str(SGR_RESET);
            } else {
                style.write(&mut self.out);
            }
            self.active = style;
        }
        self.out.push(glyph);
        self.column += width;
    }

    // Close any open style and finish the line
    fn end_line(&mut self) {
        if self.active != Style::PLAIN {
            self.out.push_str(SGR_RESET);
            self.active = Style::PLAIN;
        }
        self.out.push('\n');
        self.column = 0;
        self.lines += 1;
    }
}

// Split text into unstyled lines, for printer tests
#[cfg(test)]
fn plain_lines(input: &str) -> Vec<StyledLine> {
    input
        .lines()
        .map(|line| vec![(Style::PLAIN, line.to_owned())])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_lines_wrap_behind_the_bar() {
        let options = Options::new(10, false);
        let out = print(&plain_lines("0123456789abcdef\nx"), &options);
        assert_eq!(out, "0123456789\n┆ abcdef\nx\n");
    }

    #[test]
    fn controls_are_scrubbed() {
        let options = Options::new(40, true);
        let out = print(&plain_lines("a\x1b[2Jb"), &options);
        assert!(!out.contains('\x1b'), "{out:?}");
    }
}
