// Author: machinageist
// Date: 2026-11-09
// Description: Inline content as styled word atoms, plus the word wrapper
// Notes: Text lives in one arena string so a paragraph costs a handful of allocations,
// and atoms can be re-wrapped at any width (table cells measure first, wrap second)

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::style::{SGR_RESET, Style};

// Stand-in for control characters so untrusted files cannot drive the terminal
pub const CONTROL_REPLACEMENT: char = '\u{FFFD}';

// Lead byte of UTF-8 encoded C1 controls (U+0080..U+009F) and of their neighbours
const C1_LEAD_BYTE: u8 = 0xC2;

#[derive(Clone, Copy, Debug)]
pub enum Atom {
    Text {
        start: usize,
        end: usize,
        width: usize,
    },
    Space,
    Break,
    Style(Style),
}

#[derive(Debug)]
pub struct Line {
    pub text: String,
    pub width: usize,
}

#[derive(Default)]
pub struct Inline {
    pub atoms: Vec<Atom>,
    pub arena: String,
    pub base: Style,
    stack: Vec<Style>,
}

impl Inline {
    // Start a run of inline content on top of a base style, keeping earlier atoms
    pub fn begin(&mut self, base: Style) {
        self.base = base;
        self.stack.clear();
    }

    // Drop every collected atom and its text
    pub fn clear(&mut self) {
        self.atoms.clear();
        self.arena.clear();
        self.stack.clear();
    }

    // Report the style in effect at the end of the content
    pub fn style(&self) -> Style {
        *self.stack.last().unwrap_or(&self.base)
    }

    // Enter a nested style
    pub fn push_style(&mut self, style: Style) {
        self.stack.push(style);
        self.atoms.push(Atom::Style(style));
    }

    // Return to the enclosing style
    pub fn pop_style(&mut self) {
        self.stack.pop();
        self.atoms.push(Atom::Style(self.style()));
    }

    // Add prose, splitting it into words at ASCII whitespace
    pub fn text(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index].is_ascii_whitespace() {
                self.space();
                while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
            } else {
                let start = index;
                while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                self.word(&text[start..index]);
            }
        }
    }

    // Add text that glues onto its neighbours with no break opportunity
    pub fn word(&mut self, text: &str) {
        let start = self.arena.len();
        push_clean(&mut self.arena, text);
        let end = self.arena.len();
        if end > start {
            let width = text_width(&self.arena[start..end]);
            self.atoms.push(Atom::Text { start, end, width });
        }
    }

    // Add a break opportunity, collapsing runs of whitespace
    pub fn space(&mut self) {
        if !matches!(self.atoms.last(), Some(Atom::Space)) {
            self.atoms.push(Atom::Space);
        }
    }

    // Force a line break
    pub fn hard_break(&mut self) {
        self.atoms.push(Atom::Break);
    }
}

// Copy text into the output, replacing control characters
pub fn push_clean(out: &mut String, text: &str) {
    let suspicious = text
        .bytes()
        .any(|byte| byte < b' ' || byte == 0x7F || byte == C1_LEAD_BYTE);
    if !suspicious {
        out.push_str(text);
        return;
    }
    for ch in text.chars() {
        match ch {
            '\r' => {}
            _ if ch.is_control() => out.push(CONTROL_REPLACEMENT),
            _ => out.push(ch),
        }
    }
}

// Measure display columns, with a fast path for ASCII
pub fn text_width(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        UnicodeWidthStr::width(text)
    }
}

// Find where the word starting at `start` ends and how wide it is
fn word_span(atoms: &[Atom], start: usize) -> (usize, usize) {
    let mut end = start;
    let mut width = 0;
    while end < atoms.len() {
        match atoms[end] {
            Atom::Text { width: w, .. } => width += w,
            Atom::Style(_) => {}
            Atom::Space | Atom::Break => break,
        }
        end += 1;
    }
    (end, width)
}

// Measure the widest line the atoms produce when nothing forces a wrap
pub fn measure(atoms: &[Atom]) -> usize {
    let mut widest = 0;
    let mut line_width = 0;
    let mut pending_space = false;
    let mut index = 0;
    while index < atoms.len() {
        match atoms[index] {
            Atom::Space => {
                pending_space = line_width > 0;
                index += 1;
            }
            Atom::Break => {
                widest = widest.max(line_width);
                line_width = 0;
                pending_space = false;
                index += 1;
            }
            Atom::Text { .. } | Atom::Style(_) => {
                let (end, width) = word_span(atoms, index);
                if width > 0 {
                    line_width += width + usize::from(pending_space);
                    pending_space = false;
                }
                index = end;
            }
        }
    }
    widest.max(line_width)
}

// Accumulates wrapped output lines, writing a style only when text needs it
struct LineBuilder {
    lines: Vec<Line>,
    text: String,
    width: usize,
    style: Style,
    emitted: Style,
    color: bool,
}

impl LineBuilder {
    // Start with an empty line in the base style
    fn new(base: Style, color: bool) -> LineBuilder {
        LineBuilder {
            lines: Vec::new(),
            text: String::new(),
            width: 0,
            style: base,
            emitted: Style::PLAIN,
            color,
        }
    }

    // Bring the terminal up to the wanted style before writing text
    fn sync_style(&mut self) {
        if self.color && self.style != self.emitted {
            self.style.write(&mut self.text);
            self.emitted = self.style;
        }
    }

    // Reset any style left open on the current line
    fn close_style(&mut self) {
        if self.emitted != Style::PLAIN {
            self.text.push_str(SGR_RESET);
            self.emitted = Style::PLAIN;
        }
    }

    // Finish the current line; the next one reopens the style lazily
    fn newline(&mut self) {
        self.close_style();
        let text = std::mem::take(&mut self.text);
        self.lines.push(Line {
            text,
            width: self.width,
        });
        self.width = 0;
    }

    // Switch the wanted style without writing anything yet
    fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    // Append a run of text that is known to fit
    fn push(&mut self, text: &str, width: usize) {
        self.sync_style();
        self.text.push_str(text);
        self.width += width;
    }

    // Append text one character at a time, breaking wherever the line fills
    fn push_breaking(&mut self, text: &str, limit: usize) {
        for ch in text.chars() {
            let ch_width = ch.width().unwrap_or(0);
            if self.width + ch_width > limit && self.width > 0 {
                self.newline();
            }
            self.sync_style();
            self.text.push(ch);
            self.width += ch_width;
        }
    }

    // Close the last line and hand back everything
    fn finish(mut self) -> Vec<Line> {
        self.close_style();
        let text = std::mem::take(&mut self.text);
        self.lines.push(Line {
            text,
            width: self.width,
        });
        self.lines
    }
}

// Wrap atoms into lines no wider than `limit`, each one self-contained in style
pub fn wrap(atoms: &[Atom], arena: &str, limit: usize, base: Style, color: bool) -> Vec<Line> {
    let limit = limit.max(1);
    let mut builder = LineBuilder::new(base, color);
    let mut pending_space = false;
    let mut index = 0;
    while index < atoms.len() {
        match atoms[index] {
            Atom::Space => {
                pending_space = builder.width > 0;
                index += 1;
            }
            Atom::Break => {
                builder.newline();
                pending_space = false;
                index += 1;
            }
            Atom::Text { .. } | Atom::Style(_) => {
                let (end, word_width) = word_span(atoms, index);
                if word_width > 0 {
                    if pending_space {
                        if builder.width + 1 + word_width <= limit {
                            builder.push(" ", 1);
                        } else {
                            builder.newline();
                        }
                    } else if builder.width > 0 && builder.width + word_width > limit {
                        builder.newline();
                    }
                    pending_space = false;
                }
                for atom in &atoms[index..end] {
                    match *atom {
                        Atom::Style(style) => builder.set_style(style),
                        Atom::Text { start, end, width } => {
                            let text = &arena[start..end];
                            if builder.width + width <= limit {
                                builder.push(text, width);
                            } else {
                                builder.push_breaking(text, limit);
                            }
                        }
                        Atom::Space | Atom::Break => {}
                    }
                }
                index = end;
            }
        }
    }
    builder.finish()
}
