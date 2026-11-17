// Author: machinageist
// Date: 2026-11-09
// Description: Inline content as styled word atoms, plus the word wrapper
// Notes: Text lives in one arena string so a paragraph costs a handful of allocations,
// and atoms can be re-wrapped at any width (table cells measure first, wrap second)

use crate::display;
use crate::hyperlink;
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
    Link(Option<(usize, usize)>),
    Navigation(Option<usize>),
    Anchor {
        start: usize,
        end: usize,
    },
}

#[derive(Debug, Default)]
pub struct Line {
    pub text: String,
    pub width: usize,
    // The first positive-width non-whitespace glyph, excluding gutters and borders.
    pub first_content_column: Option<usize>,
    pub anchors: Vec<(String, usize)>,
    // Keyboard sources remain navigable even if a mixed-owner grapheme has no
    // exclusively clickable cells. These starts are not mouse hit regions.
    pub navigation_starts: Vec<(usize, usize)>,
    pub links: Vec<(usize, usize, usize)>,
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

    // Mark a validated destination without making its URI visible text
    pub fn link(&mut self, target: Option<&str>) {
        let span = target.map(|target| {
            let start = self.arena.len();
            self.arena.push_str(target);
            (start, self.arena.len())
        });
        self.atoms.push(Atom::Link(span));
    }

    // Mark an internal link by its document-local metadata index
    pub fn navigate(&mut self, index: Option<usize>) {
        self.atoms.push(Atom::Navigation(index));
    }

    // Keep a source anchor at its actual position in the wrapped content
    pub fn anchor(&mut self, id: &str) {
        let start = self.arena.len();
        self.arena.push_str(id);
        self.atoms.push(Atom::Anchor {
            start,
            end: self.arena.len(),
        });
    }

    // Plain semantic text, independent of wrapping and inline style boundaries.
    pub fn plain_text(&self) -> String {
        self.plain_text_from(0)
    }

    // IDs after the final indivisible painted unit's start must wait for following
    // content, including IDs between styled fragments of that final unit.
    pub(crate) fn trailing_anchors(&self, start: usize) -> impl Iterator<Item = &str> {
        let atoms = &self.atoms[start..];
        let mut last_content = None;
        if atoms.iter().any(|atom| matches!(atom, Atom::Anchor { .. })) {
            let mut visible = String::new();
            for atom in atoms {
                match *atom {
                    Atom::Text { start, end, .. } => visible.push_str(&self.arena[start..end]),
                    Atom::Space => visible.push(' '),
                    Atom::Break => visible.push('\n'),
                    _ => {}
                }
            }
            let mut offset = 0;
            for (unit, width) in display::units(&visible) {
                if width > 0 && !unit.chars().all(char::is_whitespace) {
                    last_content = Some(offset);
                }
                offset += unit.len();
            }
        }
        let mut offset = 0;
        atoms.iter().filter_map(move |atom| match *atom {
            Atom::Text { start, end, .. } => {
                offset += end - start;
                None
            }
            Atom::Space | Atom::Break => {
                offset += 1;
                None
            }
            Atom::Anchor { start, end } if last_content.is_none_or(|at| offset > at) => {
                Some(&self.arena[start..end])
            }
            _ => None,
        })
    }

    // Collect a semantic label without exposing link targets or anchors
    pub fn plain_text_from(&self, start: usize) -> String {
        // A short link label must not reserve the entire paragraph's text and URI arena
        let mut out = String::new();
        let mut space = false;
        for atom in self.atoms.iter().skip(start) {
            match *atom {
                Atom::Text { start, end, .. } => {
                    if space && !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(&self.arena[start..end]);
                    space = false;
                }
                Atom::Space | Atom::Break => space = true,
                Atom::Style(_) | Atom::Link(_) | Atom::Navigation(_) | Atom::Anchor { .. } => {}
            }
        }
        out
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
        display::width(text)
    }
}

// Spacing and zero-width characters still render, but cannot receive a destination.
fn first_content_column(text: &str) -> Option<usize> {
    let mut column = 0;
    for (unit, width) in display::units(text) {
        if width > 0 && !unit.chars().all(char::is_whitespace) {
            return Some(column);
        }
        column += width;
    }
    None
}

// Join visible text before segmentation: style, URI and anchor boundaries must
// not split a combining/ZWJ cluster or change its measured width.
fn word_span(atoms: &[Atom], arena: &str, start: usize) -> (usize, String, usize) {
    let mut end = start;
    let mut text = String::new();
    while end < atoms.len() {
        match atoms[end] {
            Atom::Text { start, end, .. } => text.push_str(&arena[start..end]),
            Atom::Style(_) | Atom::Link(_) | Atom::Navigation(_) | Atom::Anchor { .. } => {}
            Atom::Space | Atom::Break => break,
        }
        end += 1;
    }
    let width = text_width(&text);
    (end, text, width)
}

// Measure the widest line the atoms produce when nothing forces a wrap
pub fn measure(atoms: &[Atom], arena: &str) -> usize {
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
            Atom::Text { .. }
            | Atom::Style(_)
            | Atom::Link(_)
            | Atom::Navigation(_)
            | Atom::Anchor { .. } => {
                let (end, _, width) = word_span(atoms, arena, index);
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
struct LineBuilder<'a> {
    lines: Vec<Line>,
    text: String,
    width: usize,
    first_content_column: Option<usize>,
    style: Style,
    emitted: Style,
    color: bool,
    link: Option<&'a str>,
    emitted_link: Option<&'a str>,
    navigation: Option<usize>,
    pending_anchors: Vec<String>,
    anchors: Vec<(String, usize)>,
    navigation_starts: Vec<(usize, usize)>,
    links: Vec<(usize, usize, usize)>,
    pending_owned_space: Option<(usize, usize, usize)>,
}

impl<'a> LineBuilder<'a> {
    // Start with an empty line in the base style
    fn new(base: Style, color: bool) -> LineBuilder<'a> {
        LineBuilder {
            lines: Vec::new(),
            text: String::new(),
            width: 0,
            first_content_column: None,
            style: base,
            emitted: Style::PLAIN,
            color,
            link: None,
            emitted_link: None,
            navigation: None,
            pending_anchors: Vec::new(),
            anchors: Vec::new(),
            navigation_starts: Vec::new(),
            links: Vec::new(),
            pending_owned_space: None,
        }
    }

    // Bring the terminal up to the wanted style before writing text
    fn sync_style(&mut self, content_column: Option<usize>) {
        // A following character may wrap first; bind its anchors only after that decision
        if let Some(offset) = content_column {
            let column = self.width + offset;
            self.first_content_column.get_or_insert(column);
            self.anchors
                .extend(self.pending_anchors.drain(..).map(|id| (id, column)));
        }
        if self.link != self.emitted_link {
            self.close_link();
            if let Some(target) = self.link {
                self.text.push_str(hyperlink::OPEN);
                self.text.push_str(target);
                self.text.push_str(hyperlink::TERMINATOR);
                self.emitted_link = Some(target);
            }
        }
        if self.color && self.style != self.emitted {
            self.style.write(&mut self.text);
            self.emitted = self.style;
        }
    }

    // Reset any style left open on the current line
    fn close_style(&mut self) {
        self.close_link();
        if self.emitted != Style::PLAIN {
            self.text.push_str(SGR_RESET);
            self.emitted = Style::PLAIN;
        }
    }

    // Close hyperlinks independently of SGR state
    fn close_link(&mut self) {
        if self.emitted_link.take().is_some() {
            self.text.push_str(hyperlink::CLOSE);
        }
    }

    // Finish the current line; the next one reopens the style lazily
    fn newline(&mut self) {
        self.close_style();
        let text = std::mem::take(&mut self.text);
        self.lines.push(Line {
            text,
            width: self.width,
            first_content_column: self.first_content_column.take(),
            anchors: std::mem::take(&mut self.anchors),
            navigation_starts: std::mem::take(&mut self.navigation_starts),
            links: std::mem::take(&mut self.links),
        });
        self.width = 0;
        self.pending_owned_space = None;
    }

    // Switch the wanted style without writing anything yet
    fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    // Collapsed whitespace separates content but cannot consume its pending identities.
    fn space(&mut self) {
        self.sync_style(None);
        self.record_cells(" ", 1);
        self.text.push(' ');
        self.width += 1;
    }

    // Whitespace is owned only when actual cells on both sides share the same label.
    fn record_cells(&mut self, text: &str, width: usize) {
        if width == 0 {
            return;
        }
        let end = self.width + width;
        if text.chars().all(char::is_whitespace) {
            if let Some(index) = self.navigation
                && self
                    .links
                    .last()
                    .is_some_and(|&(key, _, at)| key == index && at == self.width)
            {
                self.pending_owned_space = Some((index, self.width, end));
            } else if let Some((index, start, at)) = self.pending_owned_space
                && Some(index) == self.navigation
                && at == self.width
            {
                self.pending_owned_space = Some((index, start, end));
            } else {
                self.pending_owned_space = None;
            }
            return;
        }
        let pending = self.pending_owned_space.take();
        if let Some(index) = self.navigation {
            let start = pending
                .filter(|&(key, _, at)| key == index && at == self.width)
                .map_or(self.width, |(_, start, _)| start);
            if let Some((key, _, at)) = self.links.last_mut()
                && *key == index
                && *at == start
            {
                *at = end;
            } else {
                self.links.push((index, start, end));
            }
        }
    }

    // Apply zero-width metadata without giving pending identities a destination.
    fn annotation(&mut self, atom: Atom, arena: &'a str) {
        match atom {
            Atom::Style(style) => self.set_style(style),
            Atom::Link(span) => self.link = span.map(|(start, end)| &arena[start..end]),
            Atom::Navigation(index) => self.navigation = index,
            Atom::Anchor { start, end } => self.pending_anchors.push(arena[start..end].to_owned()),
            Atom::Text { .. } | Atom::Space | Atom::Break => {}
        }
    }

    // Segment the whole word, then paint its original styled fragments without
    // allowing annotation boundaries to split or double-count a display unit.
    fn word(&mut self, atoms: &[Atom], arena: &'a str, text: &str, limit: usize) {
        let (mut index, mut offset) = (0, 0);
        for (unit, width) in display::units(text) {
            if width > 0 && self.width + width > limit && self.width > 0 {
                self.newline();
            }
            let mut remaining = unit.len();
            let mut first = true;
            let mut owner = None;
            let mut mixed = false;
            while remaining > 0 {
                match atoms[index] {
                    Atom::Text { start, end, .. } => {
                        let take = (end - start - offset).min(remaining);
                        let fragment = &arena[start + offset..start + offset + take];
                        let content = first && width > 0 && !unit.chars().all(char::is_whitespace);
                        self.sync_style(content.then_some(0));
                        if width > 0
                            && first_content_column(fragment).is_some()
                            && let Some(link) = self.navigation
                            && self.navigation_starts.last().map(|&(key, _)| key) != Some(link)
                        {
                            self.navigation_starts.push((link, self.width));
                        }
                        if let Some(previous) = owner {
                            mixed |= previous != self.navigation;
                        } else {
                            owner = Some(self.navigation);
                        }
                        self.text.push_str(fragment);
                        first = false;
                        remaining -= take;
                        offset += take;
                        if start + offset == end {
                            index += 1;
                            offset = 0;
                        }
                    }
                    atom => {
                        self.annotation(atom, arena);
                        index += 1;
                    }
                }
            }
            // A cluster crossing different source owners has no exclusive owner;
            // never turn a non-link part of one terminal cell into a hit target.
            let navigation = self.navigation;
            self.navigation = if mixed { None } else { owner.flatten() };
            self.record_cells(unit, width);
            self.navigation = navigation;
            self.width += width;
        }
        for &atom in &atoms[index..] {
            self.annotation(atom, arena);
        }
    }

    // Close the last line and hand back everything
    fn finish(mut self) -> Vec<Line> {
        self.close_style();
        let text = std::mem::take(&mut self.text);
        self.lines.push(Line {
            text,
            width: self.width,
            first_content_column: self.first_content_column,
            anchors: self.anchors,
            navigation_starts: self.navigation_starts,
            links: self.links,
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
            Atom::Text { .. }
            | Atom::Style(_)
            | Atom::Link(_)
            | Atom::Navigation(_)
            | Atom::Anchor { .. } => {
                let (end, text, word_width) = word_span(atoms, arena, index);
                if word_width > 0 {
                    if pending_space {
                        if builder.width + 1 + word_width <= limit {
                            builder.space();
                        } else {
                            builder.newline();
                        }
                    } else if builder.width > 0 && builder.width + word_width > limit {
                        builder.newline();
                    }
                    pending_space = false;
                }
                builder.word(&atoms[index..end], arena, &text, limit);
                index = end;
            }
        }
    }
    builder.finish()
}
