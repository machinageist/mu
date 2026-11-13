// Author: machinageist
// Date: 2026-11-09
// Description: Walks pulldown-cmark events and lays out Markdown blocks for the terminal
// Notes: Quotes, list items and footnotes are containers that add a gutter to every line they hold;
// inline content is buffered per block, wrapped once, then written behind those gutters

use std::collections::HashSet;

use pulldown_cmark::{
    BlockQuoteKind, CodeBlockKind, Event, LinkType, Options as ParseOptions, Parser, Tag, TagEnd,
};
use unicode_width::UnicodeWidthChar;

use crate::data::Span;
use crate::highlight;
use crate::hyperlink;
use crate::image::{self, ImageMode, ImageOptions};
use crate::inline::{Atom, CONTROL_REPLACEMENT, Inline, Line, push_clean, text_width, wrap};
use crate::style::{
    ATTR_BOLD, ATTR_ITALIC, ATTR_STRIKE, ATTR_UNDERLINE, FG_CODE, FG_LINK, STYLE_ALERT_CAUTION,
    STYLE_ALERT_IMPORTANT, STYLE_ALERT_NOTE, STYLE_ALERT_TIP, STYLE_ALERT_WARNING, STYLE_BULLET,
    STYLE_CODE_LABEL, STYLE_FOOTNOTE, STYLE_FRAME, STYLE_FRONT_MATTER, STYLE_HEADINGS, STYLE_HTML,
    STYLE_IMAGE, STYLE_IMAGE_CAPTION, STYLE_TABLE_HEAD, STYLE_TASK_DONE, STYLE_TASK_TODO,
    STYLE_URL, Style, paint, paint_repeat,
};
use crate::table::{self, Cell, Table};

// List and task glyphs
const BULLETS: [&str; 3] = ["•", "◦", "▪"];
// A definition's body sits this far in from its term
const DEFINITION_INDENT: usize = 4;
const TASK_DONE: &str = "☑";
const TASK_TODO: &str = "☐";

// Block decoration glyphs
const QUOTE_BAR: &str = "│";
const RULE_GLYPH: &str = "─";
const HEADING_RULES: [&str; 2] = ["═", "─"];
const FRAME_TOP: &str = "╭─";
const FRAME_BOTTOM: &str = "╰─";
const FRAME_FILL: &str = "─";
const FRAME_BAR: &str = "│ ";
pub(crate) const FRAME_WRAP_BAR: &str = "┆ ";
const FRAME_EDGE_WIDTH: usize = 2;

// Inline decoration text
const IMAGE_OPEN: &str = "[image:";
const IMAGE_BARE: &str = "[image]";
const IMAGE_CLOSE: &str = "]";
const URL_OPEN: &str = "(";
const URL_CLOSE: &str = ")";
const FOOTNOTE_OPEN: &str = "[";
const FOOTNOTE_CLOSE: &str = "]";

// GitHub alert labels
const ALERT_NOTE: &str = "Note";
const ALERT_TIP: &str = "Tip";
const ALERT_IMPORTANT: &str = "Important";
const ALERT_WARNING: &str = "Warning";
const ALERT_CAUTION: &str = "Caution";

// Raw block handling
const FRONT_MATTER_FENCE: &str = "---";
const FRONT_MATTER_LABEL: &str = "front matter";
const HTML_COMMENT_OPEN: &str = "<!--";
const HTML_COMMENT_CLOSE: &str = "-->";
const LINE_BREAK_TAGS: [&str; 3] = ["<br>", "<br/>", "<br />"];
pub(crate) const TAB_STOP: usize = 4;

// Narrowest content column, even when deeply nested on a narrow terminal
const MIN_CONTENT_WIDTH: usize = 16;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub width: usize,
    pub color: bool,
    // Emit validated external OSC 8 hyperlinks, independently of SGR color
    pub links: bool,
    pub images: ImageOptions,
    // Language for source files, as an extension or name such as `rs` or `python`
    pub language: Option<String>,
}

impl Options {
    // Build text-only options at a width, with images off
    pub fn new(width: usize, color: bool) -> Options {
        Options {
            width,
            color,
            links: false,
            images: ImageOptions::default(),
            language: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContainerKind {
    Quote,
    Item,
    Footnote,
    Definition,
}

// A gutter that prefixes every line inside a quote, list item or footnote
struct Container {
    kind: ContainerKind,
    first: String,
    rest: String,
    blank: String,
    width: usize,
    first_used: bool,
}

struct List {
    next: Option<u64>,
    loose: bool,
}

enum RawKind {
    Code(String),
    Html,
    FrontMatter,
}

// Verbatim block text collected until its end tag
struct Raw {
    kind: RawKind,
    text: String,
    anchors: Vec<(usize, String)>,
}

// Map ordered source-byte annotations through code's display-column wrapping
fn raw_anchor_rows(raw: &Raw, limit: usize) -> Vec<(String, usize, usize)> {
    let mut mapped = Vec::with_capacity(raw.anchors.len());
    let (mut index, mut row, mut column, mut source_column) = (0, 0, 0, 0);
    for (offset, ch) in raw
        .text
        .char_indices()
        .map(|(at, ch)| (at, Some(ch)))
        .chain(std::iter::once((raw.text.len(), None)))
    {
        let width = match ch {
            Some('\n' | '\r') | None => 0,
            Some('\t') => TAB_STOP - source_column % TAB_STOP,
            Some(ch) if ch.is_control() => 1,
            Some(ch) => ch.width().unwrap_or(0),
        };
        let first = if ch == Some('\t') { 1 } else { width };
        if column > 0 && column + first > limit {
            row += 1;
            column = 0;
        }
        while let Some((at, id)) = raw.anchors.get(index) {
            if *at > offset {
                break;
            }
            mapped.push((id.clone(), row, column));
            index += 1;
        }
        if ch == Some('\n') {
            row += 1;
            column = 0;
            source_column = 0;
        } else if ch == Some('\t') {
            for _ in 0..width {
                if column >= limit {
                    row += 1;
                    column = 0;
                }
                column += 1;
            }
            source_column += width;
        } else {
            column += width;
            source_column += width;
        }
    }
    mapped
}

// A heading as written to the output: its level (1-6), plain text, and first output line
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

// A chapter-scoped destination at its first rendered output line
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub id: String,
    pub line: usize,
    pub column: usize,
}

// An internal book link, with its semantic label and first rendered output line
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationLink {
    pub target: String,
    pub label: String,
    pub line: usize,
    pub column: usize,
}

// Structured source annotations never travel through terminal-control strings
pub(crate) enum DocumentEvent<'a> {
    Markup(Event<'a>),
    Anchor(String),
    ChapterStart,
    ChapterEnd,
}

// Rendered output plus the headings found in it, for outlines and jumping
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    pub headings: Vec<Heading>,
    pub anchors: Vec<Anchor>,
    pub links: Vec<NavigationLink>,
}

struct Renderer<'o> {
    options: &'o Options,
    out: String,
    headings: Vec<Heading>,
    anchors: Vec<Anchor>,
    anchor_ids: HashSet<String>,
    pending_anchors: Vec<String>,
    navigation: bool,
    navigation_links: Vec<NavigationLink>,
    // Newlines in `out` up to `counted_bytes`, so each heading's line costs only the new text
    counted_bytes: usize,
    counted_lines: usize,
    inline: Inline,
    inline_open: bool,
    containers: Vec<Container>,
    lists: Vec<List>,
    pending_blank: bool,
    heading: usize,
    raw: Option<Raw>,
    table: Option<Table>,
    link_urls: Vec<LinkState>,
    image_mark: usize,
    images: Vec<ImageSpan>,
    // Narrowest a table column may shrink to; wider for data tables, which overflow instead
    table_min_column: usize,
}

// Nested source links retain their own destination, label span and fallback
struct LinkState {
    shown: Option<String>,
    external: Option<String>,
    navigation: Option<usize>,
    start: usize,
}

// An image in the current inline block: its source, alt text and atom range
struct ImageSpan {
    src: String,
    alt: String,
    start: usize,
    end: usize,
    open: bool,
}

// Render a Markdown document into terminal-ready text
pub fn render(input: &str, options: &Options) -> String {
    render_markdown(input, options).text
}

// Render a Markdown document, keeping its headings
pub(crate) fn render_markdown(input: &str, options: &Options) -> Rendered {
    render_events(
        Parser::new_ext(input, parse_options(input)),
        options,
        input.len(),
    )
}

// Lay out any document event stream, whichever markup produced it
pub(crate) fn render_events<'a>(
    events: impl IntoIterator<Item = Event<'a>>,
    options: &Options,
    capacity: usize,
) -> Rendered {
    let mut renderer = Renderer::new(options, capacity);
    for event in events {
        renderer.event(event);
    }
    renderer.finish()
}

// Lay out EPUB events with structured, chapter-scoped navigation annotations
pub(crate) fn render_annotated_events<'a>(
    events: impl IntoIterator<Item = DocumentEvent<'a>>,
    options: &Options,
    capacity: usize,
) -> Rendered {
    let mut renderer = Renderer::new(options, capacity);
    renderer.navigation = true;
    for event in events {
        match event {
            DocumentEvent::Markup(event) => renderer.event(event),
            DocumentEvent::Anchor(id) => renderer.anchor(id),
            DocumentEvent::ChapterStart | DocumentEvent::ChapterEnd => renderer.chapter_boundary(),
        }
    }
    renderer.finish()
}

// Lay out events for a data table, whose columns stop shrinking at `min_column` and overflow instead
pub(crate) fn render_table_events<'a>(
    events: impl IntoIterator<Item = Event<'a>>,
    options: &Options,
    capacity: usize,
    min_column: usize,
) -> Rendered {
    let mut renderer = Renderer::new(options, capacity);
    renderer.table_min_column = min_column;
    for event in events {
        renderer.event(event);
    }
    renderer.finish()
}

// Pick the Markdown extensions to enable for this document
fn parse_options(input: &str) -> ParseOptions {
    let mut options = ParseOptions::ENABLE_TABLES
        | ParseOptions::ENABLE_FOOTNOTES
        | ParseOptions::ENABLE_STRIKETHROUGH
        | ParseOptions::ENABLE_TASKLISTS
        | ParseOptions::ENABLE_GFM;
    // Front matter only at the very top, so `---` rules mid-document stay rules
    if input.starts_with(FRONT_MATTER_FENCE) {
        options |= ParseOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    }
    options
}

// Map a GitHub alert kind to its label and color
fn alert(kind: BlockQuoteKind) -> (&'static str, Style) {
    match kind {
        BlockQuoteKind::Note => (ALERT_NOTE, STYLE_ALERT_NOTE),
        BlockQuoteKind::Tip => (ALERT_TIP, STYLE_ALERT_TIP),
        BlockQuoteKind::Important => (ALERT_IMPORTANT, STYLE_ALERT_IMPORTANT),
        BlockQuoteKind::Warning => (ALERT_WARNING, STYLE_ALERT_WARNING),
        BlockQuoteKind::Caution => (ALERT_CAUTION, STYLE_ALERT_CAUTION),
    }
}

// Check whether HTML is nothing but a single comment
fn is_html_comment(html: &str) -> bool {
    let html = html.trim();
    let Some(body) = html.strip_prefix(HTML_COMMENT_OPEN) else {
        return false;
    };
    body.find(HTML_COMMENT_CLOSE) == Some(body.len().saturating_sub(HTML_COMMENT_CLOSE.len()))
}

// Cuts one verbatim line into pieces that fit the frame
struct Chunker<'c> {
    chunks: &'c mut Vec<String>,
    chunk: String,
    width: usize,
    column: usize,
    limit: usize,
}

impl Chunker<'_> {
    // Append one glyph, starting a new chunk when the current one is full
    fn push(&mut self, glyph: char, width: usize) {
        if self.width + width > self.limit && self.width > 0 {
            self.chunks.push(std::mem::take(&mut self.chunk));
            self.width = 0;
        }
        self.chunk.push(glyph);
        self.width += width;
        self.column += width;
    }
}

// Split one verbatim line into chunks no wider than `limit`, expanding tabs and scrubbing controls
fn verbatim_chunks(line: &str, limit: usize, chunks: &mut Vec<String>) {
    let plain_ascii = line.bytes().all(|byte| (b' '..0x7F).contains(&byte));
    if plain_ascii && line.len() <= limit {
        chunks.push(line.to_owned());
        return;
    }
    let mut chunker = Chunker {
        chunks,
        chunk: String::new(),
        width: 0,
        column: 0,
        limit,
    };
    for ch in line.chars() {
        match ch {
            '\r' => {}
            '\t' => {
                for _ in 0..TAB_STOP - chunker.column % TAB_STOP {
                    chunker.push(' ', 1);
                }
            }
            _ if ch.is_control() => chunker.push(CONTROL_REPLACEMENT, 1),
            _ => chunker.push(ch, ch.width().unwrap_or(0)),
        }
    }
    let last = chunker.chunk;
    chunks.push(last);
}

// Cuts one highlighted line into styled pieces that fit the frame
struct StyledChunker {
    chunks: Vec<Vec<Span>>,
    width: usize,
    column: usize,
    limit: usize,
}

impl StyledChunker {
    // Append one glyph in a style, starting a new chunk when the current one is full
    fn push(&mut self, glyph: char, width: usize, style: Style) {
        if self.width + width > self.limit && self.width > 0 {
            self.chunks.push(Vec::new());
            self.width = 0;
        }
        if let Some(chunk) = self.chunks.last_mut() {
            match chunk.last_mut() {
                Some((last, text)) if *last == style => text.push(glyph),
                _ => chunk.push((style, glyph.to_string())),
            }
        }
        self.width += width;
        self.column += width;
    }
}

// Split one highlighted line into chunks no wider than `limit`, with the same tab, control
// and wrapping rules as plain verbatim lines, so styling never moves a character
fn styled_chunks(line: &[Span], limit: usize) -> Vec<Vec<Span>> {
    let mut chunker = StyledChunker {
        chunks: vec![Vec::new()],
        width: 0,
        column: 0,
        limit,
    };
    for (style, text) in line {
        for ch in text.chars() {
            match ch {
                '\r' => {}
                '\t' => {
                    for _ in 0..TAB_STOP - chunker.column % TAB_STOP {
                        chunker.push(' ', 1, *style);
                    }
                }
                _ if ch.is_control() => chunker.push(CONTROL_REPLACEMENT, 1, *style),
                _ => chunker.push(ch, ch.width().unwrap_or(0), *style),
            }
        }
    }
    chunker.chunks
}

impl<'o> Renderer<'o> {
    // Set up an empty renderer sized for the input
    fn new(options: &'o Options, capacity: usize) -> Renderer<'o> {
        Renderer {
            options,
            out: String::with_capacity(capacity * 2),
            headings: Vec::new(),
            anchors: Vec::new(),
            anchor_ids: HashSet::new(),
            pending_anchors: Vec::new(),
            navigation: false,
            navigation_links: Vec::new(),
            counted_bytes: 0,
            counted_lines: 0,
            inline: Inline::default(),
            inline_open: false,
            containers: Vec::new(),
            lists: Vec::new(),
            pending_blank: false,
            heading: 0,
            raw: None,
            table: None,
            link_urls: Vec::new(),
            image_mark: 0,
            images: Vec::new(),
            table_min_column: table::MIN_COLUMN_WIDTH,
        }
    }

    // Flush whatever is still buffered and hand back the output
    fn finish(mut self) -> Rendered {
        self.flush_inline();
        let last = self.current_line().saturating_sub(1);
        for id in std::mem::take(&mut self.pending_anchors) {
            self.record_anchor(id, last, 0);
        }
        self.navigation_links
            .retain(|link| link.line != usize::MAX && self.anchor_ids.contains(&link.target));
        Rendered {
            text: self.out,
            headings: self.headings,
            anchors: self.anchors,
            links: self.navigation_links,
        }
    }

    // Attach an ID to inline content, or to the next visible block if none is open
    fn anchor(&mut self, id: String) {
        if id.is_empty() || id.chars().any(char::is_control) {
            return;
        }
        if let Some(raw) = &mut self.raw {
            raw.anchors.push((raw.text.len(), id));
        } else if self.inline_open {
            self.inline.anchor(&id);
        } else {
            self.pending_anchors.push(id);
        }
    }

    // Finish chapter-local output before abandoning IDs with no rendered destination.
    // Neither an empty chapter nor a trailing zero-width anchor can borrow the next chapter.
    fn chapter_boundary(&mut self) {
        self.flush_inline();
        self.pending_anchors.clear();
    }

    // Keep the first occurrence of an ID, as with HTML fragment navigation
    fn record_anchor(&mut self, id: String, line: usize, column: usize) {
        if self.anchor_ids.insert(id.clone()) {
            self.anchors.push(Anchor { id, line, column });
        }
    }

    // Index of the line the next write starts on
    fn current_line(&mut self) -> usize {
        self.counted_lines += self.out.as_bytes()[self.counted_bytes..]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        self.counted_bytes = self.out.len();
        self.counted_lines
    }

    // Dispatch one parser event
    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) | Event::InlineMath(code) | Event::DisplayMath(code) => {
                self.code_span(&code)
            }
            Event::Html(html) => match &mut self.raw {
                Some(raw) => raw.text.push_str(&html),
                None => self.inline_html(&html),
            },
            Event::InlineHtml(html) => self.inline_html(&html),
            Event::FootnoteReference(label) => self.footnote_reference(&label),
            Event::SoftBreak => {
                self.open_inline();
                self.inline.space();
            }
            Event::HardBreak => {
                self.open_inline();
                self.inline.hard_break();
            }
            Event::Rule => self.rule(),
            Event::TaskListMarker(done) => self.task_marker(done),
        }
    }

    // Handle an opening tag
    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                self.begin_block();
                self.mark_loose();
                self.inline.begin(Style::PLAIN);
                self.inline_open = true;
            }
            Tag::Heading { level, .. } => {
                self.begin_block();
                self.heading = level as usize - 1;
                self.inline.begin(STYLE_HEADINGS[self.heading]);
                self.inline_open = true;
            }
            Tag::BlockQuote(kind) => self.start_quote(kind),
            Tag::CodeBlock(kind) => {
                self.begin_block();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.raw = Some(Raw {
                    kind: RawKind::Code(lang),
                    text: String::new(),
                    anchors: Vec::new(),
                });
            }
            Tag::HtmlBlock => {
                self.flush_inline();
                self.raw = Some(Raw {
                    kind: RawKind::Html,
                    text: String::new(),
                    anchors: Vec::new(),
                });
            }
            Tag::MetadataBlock(_) => {
                self.begin_block();
                self.raw = Some(Raw {
                    kind: RawKind::FrontMatter,
                    text: String::new(),
                    anchors: Vec::new(),
                });
            }
            Tag::List(start) => {
                self.begin_block();
                self.lists.push(List {
                    next: start,
                    loose: false,
                });
            }
            Tag::Item => self.start_item(),
            Tag::FootnoteDefinition(label) => self.start_footnote(&label),
            Tag::Table(aligns) => {
                self.begin_block();
                self.inline.clear();
                self.images.clear();
                self.table = Some(Table::new(aligns));
            }
            Tag::TableHead => {
                if let Some(table) = &mut self.table {
                    table.in_head = true;
                }
            }
            Tag::TableCell => self.start_cell(),
            Tag::Emphasis => self.push_attrs(ATTR_ITALIC),
            Tag::Strong => self.push_attrs(ATTR_BOLD),
            Tag::Strikethrough => self.push_attrs(ATTR_STRIKE),
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => self.start_link(link_type, &dest_url),
            Tag::Image { dest_url, .. } => self.start_image(&dest_url),
            Tag::DefinitionList => self.begin_block(),
            Tag::DefinitionListTitle => {
                self.begin_block();
                self.inline.begin(Style::PLAIN);
                self.inline_open = true;
            }
            Tag::DefinitionListDefinition => self.start_definition(),
            Tag::TableRow | Tag::Superscript | Tag::Subscript => {}
        }
    }

    // Handle a closing tag
    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_inline();
                self.pending_blank = true;
            }
            TagEnd::Heading(_) => self.end_heading(),
            TagEnd::BlockQuote(_) | TagEnd::FootnoteDefinition => self.end_container(),
            TagEnd::CodeBlock | TagEnd::MetadataBlock(_) => self.end_framed(),
            TagEnd::HtmlBlock => self.end_html_block(),
            TagEnd::List(_) => {
                self.flush_inline();
                self.lists.pop();
                self.pending_blank = true;
            }
            TagEnd::Item => self.end_container(),
            TagEnd::Table => self.end_table(),
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(table) = &mut self.table {
                    table.end_row();
                    table.in_head = false;
                }
            }
            TagEnd::TableCell => self.end_cell(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.inline.pop_style(),
            TagEnd::Link => self.end_link(),
            TagEnd::Image => self.end_image(),
            TagEnd::DefinitionList => {
                self.flush_inline();
                self.pending_blank = true;
            }
            TagEnd::DefinitionListTitle => self.flush_inline(),
            TagEnd::DefinitionListDefinition => self.end_container(),
            TagEnd::Superscript | TagEnd::Subscript => {}
        }
    }

    // Display columns occupied by the enclosing container gutters
    fn gutter_width(&self) -> usize {
        self.containers
            .iter()
            .map(|container| container.width)
            .sum()
    }

    // Columns left for content after container gutters
    fn avail(&self) -> usize {
        let used: usize = self
            .containers
            .iter()
            .map(|container| container.width)
            .sum();
        self.options
            .width
            .saturating_sub(used)
            .max(MIN_CONTENT_WIDTH)
    }

    // Write the container gutters that open a content line
    fn line_prefix(&mut self) {
        if !self.pending_anchors.is_empty() {
            let line = self.current_line();
            let column = self.gutter_width();
            for id in std::mem::take(&mut self.pending_anchors) {
                self.record_anchor(id, line, column);
            }
        }
        for container in &mut self.containers {
            if container.first_used {
                self.out.push_str(&container.rest);
            } else {
                self.out.push_str(&container.first);
                container.first_used = true;
            }
        }
    }

    // Write an empty line that keeps quote bars running through it
    fn blank_line(&mut self) {
        let last_visible = self
            .containers
            .iter()
            .rposition(|container| !container.blank.is_empty());
        if let Some(last) = last_visible {
            for container in &self.containers[..last] {
                self.out.push_str(&container.rest);
            }
            self.out.push_str(&self.containers[last].blank);
        }
        self.out.push('\n');
    }

    // Write wrapped lines behind the container gutters
    fn emit_lines(&mut self, lines: &[Line]) {
        let gutter = self.gutter_width();
        for line in lines {
            let index = self.current_line();
            for (id, column) in &line.anchors {
                self.record_anchor(id.clone(), index, gutter + column);
            }
            for &(link, column) in &line.links {
                if let Some(link) = self.navigation_links.get_mut(link)
                    && link.line == usize::MAX
                {
                    link.line = index;
                    link.column = gutter + column;
                }
            }
            self.line_prefix();
            self.out.push_str(&line.text);
            self.out.push('\n');
        }
    }

    // Flush loose inline text and emit the blank line that separates blocks
    fn begin_block(&mut self) {
        self.flush_inline();
        if self.pending_blank {
            self.blank_line();
            self.pending_blank = false;
        }
    }

    // Start an implicit paragraph for text outside one, as in tight list items
    fn open_inline(&mut self) {
        if self.inline_open {
            return;
        }
        if self.pending_blank {
            self.blank_line();
            self.pending_blank = false;
        }
        self.inline.begin(Style::PLAIN);
        self.inline_open = true;
    }

    // Wrap and write buffered inline content as a paragraph
    fn flush_inline(&mut self) {
        if !self.inline_open {
            return;
        }
        self.inline_open = false;
        if self.draw_standalone_image() {
            self.inline.clear();
            self.images.clear();
            return;
        }
        let has_text = self
            .inline
            .atoms
            .iter()
            .any(|atom| matches!(atom, Atom::Text { .. }));
        if has_text {
            let lines = wrap(
                &self.inline.atoms,
                &self.inline.arena,
                self.avail(),
                self.inline.base,
                self.options.color,
            );
            self.emit_lines(&lines);
        } else {
            self.defer_inline_anchors();
        }
        self.inline.clear();
        self.images.clear();
    }

    // Preserve empty inline anchors until the next visible output line
    fn defer_inline_anchors(&mut self) {
        for atom in &self.inline.atoms {
            if let Atom::Anchor { start, end } = *atom {
                self.pending_anchors
                    .push(self.inline.arena[start..end].to_owned());
            }
        }
    }

    // Mark the enclosing list loose when a paragraph sits directly in one of its items
    fn mark_loose(&mut self) {
        let in_item = self
            .containers
            .last()
            .is_some_and(|container| container.kind == ContainerKind::Item);
        if let (true, Some(list)) = (in_item, self.lists.last_mut()) {
            list.loose = true;
        }
    }

    // Push a container whose first line shows `marker` and later lines indent to match
    fn push_marker_container(
        &mut self,
        kind: ContainerKind,
        marker: &str,
        style: Style,
        width: usize,
    ) {
        let mut first = String::new();
        paint(&mut first, marker, style, self.options.color);
        first.push_str(&" ".repeat(width.saturating_sub(text_width(marker))));
        self.containers.push(Container {
            kind,
            first,
            rest: " ".repeat(width),
            blank: String::new(),
            width,
            first_used: false,
        });
    }

    // Open a block quote or GitHub alert
    fn start_quote(&mut self, kind: Option<BlockQuoteKind>) {
        self.begin_block();
        let color = self.options.color;
        let alert = kind.map(alert);
        let bar_style = alert.map_or(STYLE_FRAME, |(_, style)| Style::new(0, style.fg));
        let mut blank = String::new();
        paint(&mut blank, QUOTE_BAR, bar_style, color);
        let bar = format!("{blank} ");
        self.containers.push(Container {
            kind: ContainerKind::Quote,
            first: bar.clone(),
            rest: bar,
            blank,
            width: text_width(QUOTE_BAR) + 1,
            first_used: false,
        });
        if let Some((label, style)) = alert {
            self.line_prefix();
            paint(&mut self.out, label, style, color);
            self.out.push('\n');
        }
    }

    // Open a list item with its bullet or number
    fn start_item(&mut self) {
        self.flush_inline();
        let depth = self.lists.len().saturating_sub(1);
        let (marker, loose) = match self.lists.last_mut() {
            Some(List {
                next: Some(number),
                loose,
            }) => {
                let marker = format!("{number}.");
                *number += 1;
                (marker, *loose)
            }
            Some(List { next: None, loose }) => (BULLETS[depth % BULLETS.len()].to_owned(), *loose),
            None => (BULLETS[0].to_owned(), false),
        };
        // Tight lists never separate items, even after a nested block
        if self.pending_blank && loose {
            self.blank_line();
        }
        self.pending_blank = false;
        let width = text_width(&marker) + 1;
        self.push_marker_container(ContainerKind::Item, &marker, STYLE_BULLET, width);
    }

    // Open a definition body, indented under its term with no marker
    fn start_definition(&mut self) {
        self.flush_inline();
        let indent = " ".repeat(DEFINITION_INDENT);
        self.containers.push(Container {
            kind: ContainerKind::Definition,
            first: indent.clone(),
            rest: indent,
            blank: String::new(),
            width: DEFINITION_INDENT,
            first_used: false,
        });
    }

    // Open a footnote definition labelled like its references
    fn start_footnote(&mut self, label: &str) {
        self.begin_block();
        let mut marker = String::from(FOOTNOTE_OPEN);
        push_clean(&mut marker, label);
        marker.push_str(FOOTNOTE_CLOSE);
        let width = text_width(&marker) + 1;
        self.push_marker_container(ContainerKind::Footnote, &marker, STYLE_FOOTNOTE, width);
    }

    // Close a quote, item or footnote, printing a bare marker for empty items
    fn end_container(&mut self) {
        self.flush_inline();
        let Some(container) = self.containers.last() else {
            return;
        };
        let kind = container.kind;
        if !container.first_used && matches!(kind, ContainerKind::Item | ContainerKind::Footnote) {
            self.line_prefix();
            self.out.push('\n');
        }
        self.containers.pop();
        if kind != ContainerKind::Item {
            self.pending_blank = true;
        }
    }

    // Swap the pending item's bullet for a checkbox
    fn task_marker(&mut self, done: bool) {
        let color = self.options.color;
        let item = self
            .containers
            .iter_mut()
            .rev()
            .find(|container| container.kind == ContainerKind::Item);
        let Some(item) = item.filter(|item| !item.first_used) else {
            return;
        };
        let (glyph, style) = if done {
            (TASK_DONE, STYLE_TASK_DONE)
        } else {
            (TASK_TODO, STYLE_TASK_TODO)
        };
        let mut first = String::new();
        paint(&mut first, glyph, style, color);
        first.push_str(&" ".repeat(item.width.saturating_sub(text_width(glyph))));
        item.first = first;
    }

    // Draw a horizontal rule across the content width
    fn rule(&mut self) {
        self.begin_block();
        let width = self.avail();
        self.line_prefix();
        paint_repeat(
            &mut self.out,
            RULE_GLYPH,
            width,
            STYLE_FRAME,
            self.options.color,
        );
        self.out.push('\n');
        self.pending_blank = true;
    }

    // Write a heading, underlining the top two levels
    fn end_heading(&mut self) {
        let lines = wrap(
            &self.inline.atoms,
            &self.inline.arena,
            self.avail(),
            self.inline.base,
            self.options.color,
        );
        // Layout-generated line breaks must not split words in outline labels.
        let text = self.inline.plain_text();
        let line = self.current_line();
        self.headings.push(Heading {
            level: u8::try_from(self.heading + 1).unwrap_or(u8::MAX),
            text,
            line,
        });
        self.emit_lines(&lines);
        if let Some(glyph) = HEADING_RULES.get(self.heading) {
            let widest = lines.iter().map(|line| line.width).max().unwrap_or(0);
            let style = Style::new(0, STYLE_HEADINGS[self.heading].fg);
            self.line_prefix();
            paint_repeat(&mut self.out, glyph, widest, style, self.options.color);
            self.out.push('\n');
        }
        self.inline.clear();
        self.images.clear();
        self.inline_open = false;
        self.pending_blank = true;
    }

    // Write a code block or front matter inside a labelled frame
    fn end_framed(&mut self) {
        let Some(raw) = self.raw.take() else {
            return;
        };
        let (label, body) = match &raw.kind {
            RawKind::Code(lang) => (lang.as_str(), Style::PLAIN),
            RawKind::FrontMatter => (FRONT_MATTER_LABEL, STYLE_FRONT_MATTER),
            RawKind::Html => ("", STYLE_HTML),
        };
        let color = self.options.color;
        let width = self.avail();

        // Top edge with the language label
        let anchors = std::mem::take(&mut self.pending_anchors);
        self.line_prefix();
        paint(&mut self.out, FRAME_TOP, STYLE_FRAME, color);
        let mut used = FRAME_EDGE_WIDTH;
        if !label.is_empty() {
            let mut clean = String::new();
            push_clean(&mut clean, label);
            self.out.push(' ');
            paint(&mut self.out, &clean, STYLE_CODE_LABEL, color);
            self.out.push(' ');
            used += text_width(&clean) + 2;
        }
        paint_repeat(
            &mut self.out,
            FRAME_FILL,
            width.saturating_sub(used),
            STYLE_FRAME,
            color,
        );
        self.out.push('\n');

        // Source anchors point to code text rather than the decorative top border
        self.pending_anchors.extend(anchors);
        let body_start = self.current_line();

        // Highlighted body, when color is on and the language is known
        let language = match &raw.kind {
            RawKind::Code(lang) if color && lang.is_empty() => {
                highlight::language_from_shebang(&raw.text)
            }
            RawKind::Code(lang) if color => highlight::language(lang),
            _ => None,
        };
        if let Some(language) = language {
            let limit = width.saturating_sub(FRAME_EDGE_WIDTH).max(1);
            for line in highlight::highlight(&raw.text, language) {
                for (index, chunk) in styled_chunks(&line, limit).into_iter().enumerate() {
                    let bar = if index == 0 {
                        FRAME_BAR
                    } else {
                        FRAME_WRAP_BAR
                    };
                    self.line_prefix();
                    paint(&mut self.out, bar, STYLE_FRAME, color);
                    for (style, text) in chunk {
                        paint(&mut self.out, &text, style, color);
                    }
                    self.out.push('\n');
                }
            }
            self.record_raw_anchors(&raw, body_start, width);
            self.frame_bottom(width);
            return;
        }

        // Body, hard-wrapped with a dashed bar marking continuations
        let mut chunks = Vec::new();
        for line in raw.text.lines() {
            verbatim_chunks(
                line,
                width.saturating_sub(FRAME_EDGE_WIDTH).max(1),
                &mut chunks,
            );
            for (index, chunk) in chunks.drain(..).enumerate() {
                let bar = if index == 0 {
                    FRAME_BAR
                } else {
                    FRAME_WRAP_BAR
                };
                self.line_prefix();
                paint(&mut self.out, bar, STYLE_FRAME, color);
                paint(&mut self.out, &chunk, body, color);
                self.out.push('\n');
            }
        }

        self.record_raw_anchors(&raw, body_start, width);
        self.frame_bottom(width);
    }

    // Bind raw-content anchors after the actual body height is known
    fn record_raw_anchors(&mut self, raw: &Raw, start: usize, width: usize) {
        if raw.anchors.is_empty() {
            return;
        }
        let last = self.current_line().saturating_sub(start).saturating_sub(1);
        let gutter = self.gutter_width() + FRAME_EDGE_WIDTH;
        for (id, row, column) in raw_anchor_rows(raw, width.saturating_sub(FRAME_EDGE_WIDTH).max(1))
        {
            self.record_anchor(id, start + row.min(last), gutter + column);
        }
    }

    // Close a code frame with its bottom edge
    fn frame_bottom(&mut self, width: usize) {
        let color = self.options.color;
        self.line_prefix();
        paint(&mut self.out, FRAME_BOTTOM, STYLE_FRAME, color);
        paint_repeat(
            &mut self.out,
            FRAME_FILL,
            width.saturating_sub(FRAME_EDGE_WIDTH),
            STYLE_FRAME,
            color,
        );
        self.out.push('\n');
        self.pending_blank = true;
    }

    // Write raw HTML dimmed, dropping blocks that are only comments
    fn end_html_block(&mut self) {
        let Some(raw) = self.raw.take() else {
            return;
        };
        if raw.text.trim().is_empty() || is_html_comment(&raw.text) {
            return;
        }
        if self.pending_blank {
            self.blank_line();
        }
        let width = self.avail();
        let mut chunks = Vec::new();
        for line in raw.text.trim_end().lines() {
            verbatim_chunks(line, width, &mut chunks);
            for chunk in chunks.drain(..) {
                self.line_prefix();
                paint(&mut self.out, &chunk, STYLE_HTML, self.options.color);
                self.out.push('\n');
            }
        }
        self.pending_blank = true;
    }

    // Begin collecting one table cell
    fn start_cell(&mut self) {
        let Some(table) = &mut self.table else {
            return;
        };
        let base = if table.in_head {
            STYLE_TABLE_HEAD
        } else {
            Style::PLAIN
        };
        table.cell_start = self.inline.atoms.len();
        self.inline.begin(base);
        self.inline_open = true;
        for id in std::mem::take(&mut self.pending_anchors) {
            self.inline.anchor(&id);
        }
    }

    // Record the atoms of the finished cell
    fn end_cell(&mut self) {
        self.inline_open = false;
        let Some(table) = &mut self.table else {
            return;
        };
        let atoms = table.cell_start..self.inline.atoms.len();
        table.cells.push(Cell {
            atoms,
            base: self.inline.base,
        });
    }

    // Lay out and write a finished table
    fn end_table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        let lines = table::render(
            &table,
            &self.inline,
            self.avail(),
            self.table_min_column,
            self.options.color,
        );
        self.emit_lines(&lines);
        self.inline.clear();
        self.images.clear();
        self.pending_blank = true;
    }

    // Route text to the open raw block or the inline buffer
    fn text(&mut self, text: &str) {
        if let Some(raw) = &mut self.raw {
            raw.text.push_str(text);
            return;
        }
        self.open_inline();
        if let Some(image) = self.images.last_mut().filter(|image| image.open) {
            image.alt.push_str(text);
        }
        self.inline.text(text);
    }

    // Layer attributes onto the current inline style
    fn push_attrs(&mut self, attrs: u8) {
        self.open_inline();
        let style = self.inline.style().add(attrs);
        self.inline.push_style(style);
    }

    // Add an inline code span
    fn code_span(&mut self, code: &str) {
        self.open_inline();
        let style = self.inline.style().with_fg(FG_CODE);
        self.inline.push_style(style);
        self.inline.text(code);
        self.inline.pop_style();
    }

    // Add inline HTML dimmed, turning <br> into a line break and hiding comments
    fn inline_html(&mut self, html: &str) {
        self.open_inline();
        if LINE_BREAK_TAGS
            .iter()
            .any(|tag| html.trim().eq_ignore_ascii_case(tag))
        {
            self.inline.hard_break();
            return;
        }
        if is_html_comment(html) {
            return;
        }
        self.inline.push_style(STYLE_HTML);
        self.inline.text(html);
        self.inline.pop_style();
    }

    // Add a footnote reference glued to the preceding word
    fn footnote_reference(&mut self, label: &str) {
        self.open_inline();
        self.inline.push_style(STYLE_FOOTNOTE);
        self.inline.word(FOOTNOTE_OPEN);
        self.inline.word(label);
        self.inline.word(FOOTNOTE_CLOSE);
        self.inline.pop_style();
    }

    // Open a link, remembering its URL for display after the text
    fn start_link(&mut self, link_type: LinkType, url: &str) {
        self.open_inline();
        let self_describing = matches!(link_type, LinkType::Autolink | LinkType::Email);
        let target = self
            .options
            .links
            .then(|| {
                if matches!(link_type, LinkType::Email)
                    && !url
                        .get(..7)
                        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("mailto:"))
                {
                    hyperlink::destination(&format!("mailto:{url}"))
                } else {
                    hyperlink::destination(url)
                }
            })
            .flatten();
        let navigation = self
            .navigation
            .then(|| url.strip_prefix('#'))
            .flatten()
            .map(|id| {
                let index = self.navigation_links.len();
                self.navigation_links.push(NavigationLink {
                    target: id.to_owned(),
                    label: String::new(),
                    line: usize::MAX,
                    column: 0,
                });
                index
            });
        let shown =
            (target.is_none() && navigation.is_none() && !self_describing && !url.is_empty())
                .then(|| url.to_owned());
        self.inline.link(target.as_deref());
        self.inline.navigate(navigation);
        self.link_urls.push(LinkState {
            shown,
            external: target,
            navigation,
            start: self.inline.atoms.len(),
        });
        let style = self.inline.style().add(ATTR_UNDERLINE).with_fg(FG_LINK);
        self.inline.push_style(style);
    }

    // Close a link and append its URL dimmed
    fn end_link(&mut self) {
        self.inline.link(None);
        self.inline.navigate(None);
        self.inline.pop_style();
        let Some(link) = self.link_urls.pop() else {
            return;
        };
        if let Some(index) = link.navigation {
            self.navigation_links[index].label = self.inline.plain_text_from(link.start);
        }
        let empty_label = !self
            .inline
            .atoms
            .iter()
            .skip(link.start)
            .any(|atom| matches!(atom, Atom::Text { width, .. } if *width > 0));
        let shown = link
            .shown
            .or_else(|| empty_label.then_some(link.external).flatten());
        if let Some(url) = shown {
            self.inline.space();
            self.inline.push_style(STYLE_URL);
            self.inline.word(URL_OPEN);
            self.inline.word(&url);
            self.inline.word(URL_CLOSE);
            self.inline.pop_style();
        }
        if let Some(parent) = self.link_urls.last() {
            self.inline.link(parent.external.as_deref());
            self.inline.navigate(parent.navigation);
        }
    }

    // Open an image, which renders as its alt text unless it can be drawn on its own
    fn start_image(&mut self, src: &str) {
        self.open_inline();
        self.images.push(ImageSpan {
            src: src.to_owned(),
            alt: String::new(),
            start: self.inline.atoms.len(),
            end: self.inline.atoms.len(),
            open: true,
        });
        self.inline.push_style(STYLE_IMAGE);
        self.inline.word(IMAGE_OPEN);
        self.inline.space();
        self.image_mark = self.inline.atoms.len();
    }

    // Close an image, collapsing to a bare marker when there was no alt text
    fn end_image(&mut self) {
        if self.inline.atoms.len() == self.image_mark {
            self.inline
                .atoms
                .truncate(self.image_mark.saturating_sub(2));
            self.inline.word(IMAGE_BARE);
        } else {
            self.inline.word(IMAGE_CLOSE);
        }
        self.inline.pop_style();
        let end = self.inline.atoms.len();
        if let Some(image) = self.images.last_mut() {
            image.end = end;
            image.open = false;
        }
    }

    // Draw the block's only image as a picture with an alt caption, if it stands alone and loads
    fn draw_standalone_image(&mut self) -> bool {
        // Keep a linked image's actionable alt text rather than losing its destination
        if self
            .inline
            .atoms
            .iter()
            .any(|atom| matches!(atom, Atom::Link(Some(_)) | Atom::Navigation(Some(_))))
        {
            return false;
        }
        let options = &self.options.images;
        if !self.options.color || options.mode == ImageMode::Never {
            return false;
        }
        let [image] = self.images.as_slice() else {
            return false;
        };
        let alone = self.inline.atoms.iter().enumerate().all(|(index, atom)| {
            !matches!(atom, Atom::Text { .. }) || (image.start..image.end).contains(&index)
        });
        if !alone || image.open {
            return false;
        }
        let Some(picture) = image::load_cached(&image.src, options) else {
            return false;
        };
        let lines = image::draw_guarded(&picture, self.avail(), options);
        if lines.is_empty() {
            return false;
        }
        let alt = image.alt.trim().to_owned();
        self.defer_inline_anchors();
        for line in lines {
            self.line_prefix();
            self.out.push_str(&line);
            self.out.push('\n');
        }
        if !alt.is_empty() {
            let mut caption = Inline::default();
            caption.begin(STYLE_IMAGE_CAPTION);
            caption.text(&alt);
            let lines = wrap(
                &caption.atoms,
                &caption.arena,
                self.avail(),
                caption.base,
                self.options.color,
            );
            self.emit_lines(&lines);
        }
        true
    }
}
