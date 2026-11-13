// Author: machinageist
// Date: 2026-11-09
// Description: AsciiDoc documents to pulldown-cmark events
// Notes: A tolerant line-based parser for the subset real READMEs use; anything it cannot place
// falls through as paragraph text, every Start gets its End, and nesting is capped

use std::collections::HashMap;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd,
};

// Lines of the header searched for an attribute entry when sniffing
const SNIFF_HEADER_LINES: usize = 10;

// Comments, headings and attribute entries
const COMMENT_PREFIX: &str = "//";
const TITLE_PREFIX: &str = "= ";
const SECTION_MARKS: [char; 2] = ['=', '#'];
const MAX_HEADING_LEVEL: usize = 6;
const ATTR_MARK: char = ':';
const ATTR_UNSET: char = '!';
const BLOCK_TITLE_MARK: char = '.';
const ANCHOR_OPEN: &str = "[[";
const ATTR_OPEN: char = '[';
const ATTR_CLOSE: char = ']';
const MD_LINK_JOIN: &str = "](";

// Delimited block lines
const MIN_DELIMITER_LEN: usize = 4;
const DELIMITER_CHARS: [char; 7] = ['-', '.', '=', '*', '_', '+', '/'];
const OPEN_BLOCK: &str = "--";
const FENCE: &str = "```";
const TABLE_DELIMITER: &str = "|===";

// Single-line blocks
const RULES: [&str; 3] = ["'''", "---", "***"];
const PAGE_BREAK: &str = "<<<";
const BLOCK_IMAGE: &str = "image::";
const DROPPED_MACROS: [&str; 7] = [
    "include::",
    "toc::",
    "ifdef::",
    "ifndef::",
    "ifeval::",
    "endif::",
    "pass::",
];
const MD_QUOTE: char = '>';
const LIST_CONTINUATION: &str = "+";
const HARD_BREAK_SUFFIX: &str = " +";

// Block styles from the attribute line
const STYLE_SOURCE: &str = "source";
const STYLE_QUOTES: [&str; 2] = ["quote", "verse"];
const ADMONITIONS: [(&str, BlockQuoteKind); 5] = [
    ("NOTE", BlockQuoteKind::Note),
    ("TIP", BlockQuoteKind::Tip),
    ("IMPORTANT", BlockQuoteKind::Important),
    ("WARNING", BlockQuoteKind::Warning),
    ("CAUTION", BlockQuoteKind::Caution),
];
const ADMONITION_SEPARATOR: &str = ": ";
const ATTRIBUTION_DASH: &str = "— ";
const ATTRIBUTION_JOIN: &str = ", ";
const COLS_KEY: &str = "cols=";
const COLS_REPEAT: char = '*';
const CELL_BAR: char = '|';
const CELL_SPEC_CHARS: &str = "0123456789.+*<>^adehlmsv";
const MAX_CELL_SPEC_LEN: usize = 6;

// List markers
const BULLET_MARKS: [char; 3] = ['*', '-', '.'];
const ORDERED_MARK: char = '.';
const SINGLE_ONLY_MARK: char = '-';
const NUMBERED_KEY: &str = "1.";
const DESCRIPTION_SEPARATORS: [&str; 4] = ["::::", ":::", "::", ";;"];
const MAX_MARKER_RUN: usize = 5;
const TASK_DONE: [&str; 3] = ["[x] ", "[X] ", "[*] "];
const TASK_TODO: &str = "[ ] ";

// Inline markup
const SOFT_BREAK: char = '\n';
const HARD_BREAK: char = '\r';
const ESCAPE: char = '\\';
const ESCAPABLE: &str = "*_#`+~^<{[\\";
const BACKTICK: &str = "`";
const DOUBLE_BACKTICK: &str = "``";
const PLUS: &str = "+";
const TRIPLE_PLUS: &str = "+++";
const STRONG: [&str; 2] = ["**", "*"];
const EMPHASIS: [&str; 2] = ["__", "_"];
const HIGHLIGHT: [&str; 2] = ["##", "#"];
const SCRIPTS: [char; 2] = ['~', '^'];
const URL_SCHEMES: [&str; 4] = ["https://", "http://", "ftp://", "irc://"];
const MAILTO: &str = "mailto:";
const LINK_MACRO: &str = "link:";
const XREF_MACRO: &str = "xref:";
const INLINE_IMAGE: &str = "image:";
const CODE_MACROS: [&str; 2] = ["kbd:[", "btn:["];
const XREF_OPEN: &str = "<<";
const XREF_CLOSE: &str = ">>";
const ANGLE_OPEN: char = '<';
const ANGLE_CLOSE: char = '>';
const URL_STOPS: &str = "[<>\"";
const URL_TRAILING: &str = ".,;:!?)'";
const NEW_WINDOW: char = '^';
const ITEM_SEPARATOR: char = ',';

// Built-in character attributes
const BUILTIN_ATTRS: [(&str, &str); 12] = [
    ("nbsp", " "),
    ("sp", " "),
    ("empty", ""),
    ("plus", "+"),
    ("vbar", "|"),
    ("startsb", "["),
    ("endsb", "]"),
    ("caret", "^"),
    ("tilde", "~"),
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
];
const REF_OPEN: char = '{';
const REF_CLOSE: char = '}';

// Nesting caps so hostile input cannot exhaust the stack
const MAX_BLOCK_DEPTH: usize = 32;
const MAX_INLINE_DEPTH: usize = 16;

// Check whether the head of an input looks like AsciiDoc
pub fn sniff(head: &str) -> bool {
    let lines: Vec<&str> = head.lines().collect();
    let first = lines
        .iter()
        .find(|line| !line.trim().is_empty() && !is_comment(line));
    if first.is_some_and(|line| is_document_title(line)) {
        return true;
    }
    if lines
        .iter()
        .take(SNIFF_HEADER_LINES)
        .any(|line| attribute_entry(line).is_some())
    {
        return true;
    }
    lines.iter().enumerate().any(|(i, line)| {
        let line = line.trim_end();
        let attributed_block = is_block_attr(line)
            && !line.starts_with(ANCHOR_OPEN)
            && lines
                .get(i + 1)
                .is_some_and(|next| matches!(delimiter(next), Some(d) if d.is_sniffable()));
        attributed_block || is_table_delimiter(line)
    })
}

// Translate AsciiDoc into document events
pub fn events(input: &str) -> Vec<Event<'_>> {
    let lines: Vec<&str> = input.lines().collect();
    let mut converter = Converter {
        attrs: HashMap::new(),
        out: Vec::new(),
        depth: 0,
    };
    let start = converter.header(&lines);
    converter.blocks(&lines[start..]);
    converter.out
}

// Kinds of delimited block
#[derive(Clone, Copy, PartialEq, Eq)]
enum Delimiter {
    Listing,
    Literal,
    Example,
    Sidebar,
    Quote,
    Pass,
    Comment,
    Open,
    Fence,
    Table,
}

impl Delimiter {
    // Delimiters that, after a block attribute line, clearly mark AsciiDoc
    fn is_sniffable(self) -> bool {
        matches!(
            self,
            Delimiter::Listing | Delimiter::Quote | Delimiter::Example
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Ordered(u64),
    Description,
}

// One list item line split into its parts
struct Marker<'a> {
    key: String,
    kind: ListKind,
    term: &'a str,
    text: &'a str,
}

// Block attribute line and block title waiting for the next block
#[derive(Default)]
struct Pending {
    attr: Option<String>,
    title: Option<String>,
}

struct Converter {
    attrs: HashMap<String, String>,
    out: Vec<Event<'static>>,
    depth: usize,
}

// Check for a `//` comment line, which is not a `////` block delimiter
fn is_comment(line: &str) -> bool {
    line.starts_with(COMMENT_PREFIX) && delimiter(line).is_none()
}

// Check for a level-0 document title
fn is_document_title(line: &str) -> bool {
    line.strip_prefix(TITLE_PREFIX)
        .is_some_and(|rest| !rest.trim().is_empty())
}

// Split `:name: value` into its name and value
fn attribute_entry(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix(ATTR_MARK)?;
    let end = rest.find(ATTR_MARK)?;
    let name = &rest[..end];
    let value = &rest[end + 1..];
    let valid_name = !name.is_empty()
        && name
            .trim_end_matches(ATTR_UNSET)
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-');
    (valid_name && (value.is_empty() || value.starts_with(' '))).then_some((name, value.trim()))
}

// Check for a whole-line block attribute list like `[source,rust]`
fn is_block_attr(line: &str) -> bool {
    line.len() > 1
        && line.starts_with(ATTR_OPEN)
        && line.ends_with(ATTR_CLOSE)
        && !line.contains(MD_LINK_JOIN)
}

// Check for a `|===` table delimiter
fn is_table_delimiter(line: &str) -> bool {
    line.strip_prefix(TABLE_DELIMITER)
        .is_some_and(|rest| rest.chars().all(|ch| ch == '='))
}

// Classify a delimited block opening or closing line
fn delimiter(line: &str) -> Option<Delimiter> {
    let line = line.trim_end();
    if line == OPEN_BLOCK {
        return Some(Delimiter::Open);
    }
    if line.starts_with(FENCE) {
        return Some(Delimiter::Fence);
    }
    if is_table_delimiter(line) {
        return Some(Delimiter::Table);
    }
    let first = line.chars().next()?;
    if line.len() < MIN_DELIMITER_LEN
        || !DELIMITER_CHARS.contains(&first)
        || !line.chars().all(|ch| ch == first)
    {
        return None;
    }
    Some(match first {
        '-' => Delimiter::Listing,
        '.' => Delimiter::Literal,
        '=' => Delimiter::Example,
        '*' => Delimiter::Sidebar,
        '_' => Delimiter::Quote,
        '+' => Delimiter::Pass,
        _ => Delimiter::Comment,
    })
}

// Check whether a line closes the block opened by `open`
fn closes(open: &str, kind: Delimiter, line: &str) -> bool {
    let line = line.trim_end();
    match kind {
        Delimiter::Fence => line == FENCE,
        _ => line == open.trim_end(),
    }
}

// Check for a `.Title` line, which is not a list item or literal delimiter
fn is_block_title(line: &str) -> bool {
    let mut chars = line.chars();
    chars.next() == Some(BLOCK_TITLE_MARK)
        && chars
            .next()
            .is_some_and(|ch| !ch.is_whitespace() && ch != BLOCK_TITLE_MARK)
}

// Split a section heading into its level and text
fn heading(line: &str) -> Option<(usize, &str)> {
    let mark = line
        .chars()
        .next()
        .filter(|ch| SECTION_MARKS.contains(ch))?;
    let level = line.chars().take_while(|&ch| ch == mark).count();
    let rest = &line[level..];
    let text = rest.strip_prefix(' ')?.trim();
    (level <= MAX_HEADING_LEVEL && !text.is_empty()).then_some((level, text))
}

// Map a heading level number to pulldown-cmark's enum
fn heading_level(level: usize) -> HeadingLevel {
    match level {
        1 => HeadingLevel::H1,
        2 => HeadingLevel::H2,
        3 => HeadingLevel::H3,
        4 => HeadingLevel::H4,
        5 => HeadingLevel::H5,
        _ => HeadingLevel::H6,
    }
}

// Split an attribute list into trimmed positional entries
fn positional(attr: &str) -> Vec<&str> {
    attr.split(ITEM_SEPARATOR).map(str::trim).collect()
}

// Block style: the first positional entry without its id, role or options
fn style(attr: &str) -> &str {
    let first = positional(attr).first().copied().unwrap_or("");
    let end = first.find(['%', '#', '.']).unwrap_or(first.len());
    &first[..end]
}

// Look up an admonition kind by its label
fn admonition(label: &str) -> Option<BlockQuoteKind> {
    ADMONITIONS
        .iter()
        .find(|(name, _)| *name == label)
        .map(|(_, kind)| *kind)
}

// Source language from `[source,lang]` or `[,lang]`
fn source_lang(attr: &str) -> String {
    let style = style(attr);
    if style != STYLE_SOURCE && !style.is_empty() {
        return String::new();
    }
    positional(attr)
        .get(1)
        .filter(|lang| !lang.contains('='))
        .map_or(String::new(), |lang| {
            let lang = *lang;
            lang.strip_prefix('"')
                .and_then(|lang| lang.strip_suffix('"'))
                .or_else(|| {
                    lang.strip_prefix('\'')
                        .and_then(|lang| lang.strip_suffix('\''))
                })
                .unwrap_or(lang)
                .to_owned()
        })
}

// Column count from a `cols=` attribute, if any
fn column_count(attr: &str) -> Option<usize> {
    let start = attr.find(COLS_KEY)? + COLS_KEY.len();
    let rest = &attr[start..];
    let value = if let Some(quoted) = rest.strip_prefix('"') {
        &quoted[..quoted.find('"').unwrap_or(quoted.len())]
    } else {
        &rest[..rest.find(ITEM_SEPARATOR).unwrap_or(rest.len())]
    };
    if let Some((count, _)) = value.split_once(COLS_REPEAT) {
        return count.trim().parse().ok().filter(|&count| count > 0);
    }
    Some(value.split(ITEM_SEPARATOR).count()).filter(|&count| count > 0)
}

// Check whether a table cell prefix is a cell spec like `2+` or `a`
fn is_cell_spec(piece: &str) -> bool {
    !piece.is_empty()
        && piece.len() <= MAX_CELL_SPEC_LEN
        && piece.chars().all(|ch| CELL_SPEC_CHARS.contains(ch))
}

// Split one table line on unescaped `|`, returning the text before the first bar and the cells
fn split_cells(line: &str) -> (String, Vec<String>) {
    let mut lead = None;
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == ESCAPE && chars.peek() == Some(&CELL_BAR) {
            current.push(CELL_BAR);
            chars.next();
        } else if ch == CELL_BAR {
            let piece = std::mem::take(&mut current);
            match lead {
                None => lead = Some(piece),
                Some(_) => cells.push(piece.trim().to_owned()),
            }
        } else {
            current.push(ch);
        }
    }
    match lead {
        None => (current, cells),
        Some(lead) => {
            cells.push(current.trim().to_owned());
            (lead, cells)
        }
    }
}

// Parse a list item marker at the start of a line
fn list_marker(line: &str) -> Option<Marker<'_>> {
    let trimmed = line.trim_start();
    let first = trimmed.chars().next()?;
    if BULLET_MARKS.contains(&first) {
        let run = trimmed.chars().take_while(|&ch| ch == first).count();
        let rest = &trimmed[run..];
        let run_ok = run <= MAX_MARKER_RUN && (first != SINGLE_ONLY_MARK || run == 1);
        if run_ok && rest.starts_with([' ', '\t']) && !rest.trim().is_empty() {
            let kind = if first == ORDERED_MARK {
                ListKind::Ordered(1)
            } else {
                ListKind::Bullet
            };
            return Some(Marker {
                key: trimmed[..run].to_owned(),
                kind,
                term: "",
                text: rest.trim(),
            });
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &trimmed[digits..];
        if let Some(text) = rest
            .strip_prefix(". ")
            .filter(|text| !text.trim().is_empty())
        {
            let start = trimmed[..digits].parse().unwrap_or(1);
            return Some(Marker {
                key: NUMBERED_KEY.to_owned(),
                kind: ListKind::Ordered(start),
                term: "",
                text: text.trim(),
            });
        }
    }
    description_marker(trimmed)
}

// Parse a `term:: description` line
fn description_marker(line: &str) -> Option<Marker<'_>> {
    if line.starts_with(COMMENT_PREFIX) {
        return None;
    }
    for separator in DESCRIPTION_SEPARATORS {
        let mut from = 0;
        while let Some(found) = line[from..].find(separator) {
            let at = from + found;
            let after = &line[at + separator.len()..];
            let term = line[..at].trim();
            let clean_end = after.is_empty() || after.starts_with([' ', '\t']);
            let run_ends = !after.starts_with([':', ';']);
            if clean_end && run_ends && !term.is_empty() && !line[..at].ends_with([':', ';']) {
                return Some(Marker {
                    key: separator.to_owned(),
                    kind: ListKind::Description,
                    term,
                    text: after.trim(),
                });
            }
            from = at + separator.len();
        }
    }
    None
}

// Join paragraph lines, marking soft and hard breaks for the inline pass
fn join_lines<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    let mut joined = String::new();
    let mut hard = false;
    for (i, line) in lines.enumerate() {
        if i > 0 {
            joined.push(if hard { HARD_BREAK } else { SOFT_BREAK });
        }
        let line = line.trim();
        hard = line.ends_with(HARD_BREAK_SUFFIX);
        joined.push_str(line.strip_suffix(HARD_BREAK_SUFFIX).unwrap_or(line));
    }
    joined
}

// Check for a word character, which blocks constrained markup
fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

// Find a constrained closing marker: no space before it, no word character after it
fn find_constrained(text: &str, from: usize, marker: char) -> Option<usize> {
    let mut prev = None;
    for (offset, ch) in text[from..].char_indices() {
        let at = from + offset;
        if ch == marker && offset > 0 && prev.is_some_and(|p: char| !p.is_whitespace()) {
            let next = text[at + ch.len_utf8()..].chars().next();
            if next.is_none_or(|n| !is_word(n) && n != marker) {
                return Some(at);
            }
        }
        prev = Some(ch);
    }
    None
}

// Cut a bracketed macro target like `[text]` off the front of a string
fn bracketed(rest: &str) -> Option<(&str, usize)> {
    let body = rest.strip_prefix(ATTR_OPEN)?;
    let close = body.find(ATTR_CLOSE)?;
    Some((&body[..close], close + 2))
}

// Strip link options like a trailing `^` from link text
fn link_text(text: &str) -> &str {
    text.trim().trim_end_matches(NEW_WINDOW).trim()
}

impl Converter {
    // Consume the document header, emitting the title and recording attributes
    fn header(&mut self, lines: &[&str]) -> usize {
        let Some(first) = lines
            .iter()
            .position(|line| !line.trim().is_empty() && !is_comment(line))
        else {
            return lines.len();
        };
        if !is_document_title(lines[first]) {
            return 0;
        }
        self.heading(1, &lines[first][TITLE_PREFIX.len()..]);
        let mut i = first + 1;
        while i < lines.len() && !lines[i].trim().is_empty() {
            if let Some((name, value)) = attribute_entry(lines[i]) {
                self.set_attr(name, value);
            }
            i += 1;
        }
        i
    }

    // Record or unset a document attribute
    fn set_attr(&mut self, name: &str, value: &str) {
        if let Some(name) = name.strip_suffix(ATTR_UNSET) {
            self.attrs.remove(name);
        } else {
            self.attrs.insert(name.to_owned(), value.to_owned());
        }
    }

    // Parse a run of lines as blocks
    fn blocks(&mut self, lines: &[&str]) {
        let mut pending = Pending::default();
        let mut i = 0;
        while i < lines.len() {
            i = self.block(lines, i, &mut pending).0;
        }
        self.flush_title(&mut pending);
    }

    // Parse blocks one level deeper, falling back to verbatim text past the nesting cap
    fn nested(&mut self, lines: &[&str]) {
        if self.depth >= MAX_BLOCK_DEPTH {
            self.code_block(String::new(), lines);
            return;
        }
        self.depth += 1;
        self.blocks(lines);
        self.depth -= 1;
    }

    // Parse exactly one block, with any attribute or title lines before it
    fn one_block(&mut self, lines: &[&str], mut i: usize) -> usize {
        if self.depth >= MAX_BLOCK_DEPTH {
            return i;
        }
        self.depth += 1;
        let mut pending = Pending::default();
        while i < lines.len() {
            let (next, done) = self.block(lines, i, &mut pending);
            i = next;
            if done {
                break;
            }
        }
        self.flush_title(&mut pending);
        self.depth -= 1;
        i
    }

    // Parse whatever starts at line `i`; returns the next line and whether a block was produced
    fn block(&mut self, lines: &[&str], i: usize, pending: &mut Pending) -> (usize, bool) {
        let line = lines[i].trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return (i + 1, false);
        }
        if let Some(kind) = delimiter(line) {
            return (self.delimited(lines, i, kind, pending), true);
        }
        if is_comment(line) || DROPPED_MACROS.iter().any(|m| line.starts_with(m)) {
            return (i + 1, false);
        }
        if let Some((name, value)) = attribute_entry(line) {
            self.set_attr(name, value);
            return (i + 1, false);
        }
        if is_block_attr(line) {
            if !line.starts_with(ANCHOR_OPEN) {
                pending.attr = Some(line[1..line.len() - 1].to_owned());
            }
            return (i + 1, false);
        }
        if is_block_title(line) {
            pending.title = Some(line[1..].to_owned());
            return (i + 1, false);
        }
        self.flush_title(pending);
        let attr = pending.attr.take().unwrap_or_default();
        if let Some((level, text)) = heading(line) {
            self.heading(level, text);
            return (i + 1, true);
        }
        if RULES.contains(&trimmed) {
            self.out.push(Event::Rule);
            return (i + 1, true);
        }
        if trimmed == PAGE_BREAK {
            return (i + 1, true);
        }
        if let Some(rest) = line.strip_prefix(BLOCK_IMAGE) {
            self.block_image(rest);
            return (i + 1, true);
        }
        if list_marker(line).is_some() {
            return (self.list(lines, i), true);
        }
        if line.starts_with(MD_QUOTE) {
            return (self.md_quote(lines, i), true);
        }
        if line.starts_with([' ', '\t']) {
            let end = paragraph_end(lines, i);
            self.code_block(String::new(), &dedent(&lines[i..end])[..]);
            return (end, true);
        }
        let end = paragraph_end(lines, i);
        let style = style(&attr);
        if let Some(kind) = admonition(style) {
            self.container(Some(kind), &lines[i..end]);
        } else if STYLE_QUOTES.contains(&style) {
            self.quote(&attr, &lines[i..end]);
        } else if style == STYLE_SOURCE {
            self.code_block(source_lang(&attr), &lines[i..end]);
        } else {
            self.paragraph(&lines[i..end]);
        }
        (end, true)
    }

    // Emit a heading at the given level
    fn heading(&mut self, level: usize, text: &str) {
        let level = heading_level(level);
        self.out.push(Event::Start(Tag::Heading {
            level,
            id: None,
            classes: vec![],
            attrs: vec![],
        }));
        self.inline(text.trim());
        self.out.push(Event::End(TagEnd::Heading(level)));
    }

    // Emit a pending block title as a bold paragraph
    fn flush_title(&mut self, pending: &mut Pending) {
        if let Some(title) = pending.title.take() {
            self.out.push(Event::Start(Tag::Paragraph));
            self.out.push(Event::Start(Tag::Strong));
            self.inline(&title);
            self.out.push(Event::End(TagEnd::Strong));
            self.out.push(Event::End(TagEnd::Paragraph));
        }
    }

    // Emit a paragraph, or an admonition when it opens with `NOTE: ` and friends
    fn paragraph(&mut self, lines: &[&str]) {
        let first = lines[0].trim();
        let labelled = first
            .split_once(ADMONITION_SEPARATOR)
            .and_then(|(label, rest)| admonition(label).map(|kind| (kind, rest)));
        if let Some((kind, rest)) = labelled {
            let text = join_lines(std::iter::once(rest).chain(lines[1..].iter().copied()));
            self.out.push(Event::Start(Tag::BlockQuote(Some(kind))));
            self.inline_paragraph(&text);
            self.out.push(Event::End(TagEnd::BlockQuote(Some(kind))));
            return;
        }
        let text = join_lines(lines.iter().copied());
        self.inline_paragraph(&text);
    }

    // Wrap inline text in a paragraph
    fn inline_paragraph(&mut self, text: &str) {
        self.out.push(Event::Start(Tag::Paragraph));
        self.inline(text);
        self.out.push(Event::End(TagEnd::Paragraph));
    }

    // Emit verbatim lines as a code block
    fn code_block<S: AsRef<str>>(&mut self, lang: String, lines: &[S]) {
        self.out
            .push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(
                CowStr::from(lang),
            ))));
        if !lines.is_empty() {
            let mut text = String::new();
            for line in lines {
                text.push_str(line.as_ref());
                text.push('\n');
            }
            self.out.push(Event::Text(CowStr::from(text)));
        }
        self.out.push(Event::End(TagEnd::CodeBlock));
    }

    // Emit nested blocks inside a quote, optionally an admonition
    fn container(&mut self, kind: Option<BlockQuoteKind>, lines: &[&str]) {
        self.out.push(Event::Start(Tag::BlockQuote(kind)));
        self.nested(lines);
        self.out.push(Event::End(TagEnd::BlockQuote(kind)));
    }

    // Emit a quote with its attribution line from `[quote,author,source]`
    fn quote(&mut self, attr: &str, lines: &[&str]) {
        self.out.push(Event::Start(Tag::BlockQuote(None)));
        self.nested(lines);
        let parts = positional(attr);
        let credit: Vec<&str> = parts
            .iter()
            .skip(1)
            .take(2)
            .copied()
            .filter(|part| !part.is_empty() && !part.contains('='))
            .collect();
        if !credit.is_empty() {
            let line = format!("{ATTRIBUTION_DASH}{}", credit.join(ATTRIBUTION_JOIN));
            self.inline_paragraph(&line);
        }
        self.out.push(Event::End(TagEnd::BlockQuote(None)));
    }

    // Emit a Markdown-style `>` quote
    fn md_quote(&mut self, lines: &[&str], i: usize) -> usize {
        let end = (i..lines.len())
            .find(|&j| !lines[j].starts_with(MD_QUOTE))
            .unwrap_or(lines.len());
        let body: Vec<&str> = lines[i..end]
            .iter()
            .map(|line| {
                let rest = &line[1..];
                rest.strip_prefix(' ').unwrap_or(rest)
            })
            .collect();
        self.container(None, &body);
        end
    }

    // Emit a block image from the text after `image::`
    fn block_image(&mut self, rest: &str) {
        self.out.push(Event::Start(Tag::Paragraph));
        let open = rest.find(ATTR_OPEN).unwrap_or(rest.len());
        let alt = bracketed(&rest[open..]).map_or("", |(body, _)| body);
        let target = self.substitute(&rest[..open]);
        push_image(&mut self.out, target, alt);
        self.out.push(Event::End(TagEnd::Paragraph));
    }

    // Emit a delimited block and return the line after its closing delimiter
    fn delimited(
        &mut self,
        lines: &[&str],
        i: usize,
        kind: Delimiter,
        pending: &mut Pending,
    ) -> usize {
        let open = lines[i];
        let close = (i + 1..lines.len()).find(|&j| closes(open, kind, lines[j]));
        let body = &lines[i + 1..close.unwrap_or(lines.len())];
        let next = close.map_or(lines.len(), |j| j + 1);
        let attr = pending.attr.take().unwrap_or_default();
        if kind == Delimiter::Comment {
            pending.title = None;
            return next;
        }
        self.flush_title(pending);
        let style = style(&attr);
        match kind {
            Delimiter::Listing => {
                let lang = self.substitute(&source_lang(&attr));
                self.code_block(lang, body);
            }
            Delimiter::Fence => {
                let lang = open.trim()[FENCE.len()..].trim().to_owned();
                self.code_block(lang, body);
            }
            Delimiter::Literal | Delimiter::Pass => self.code_block(String::new(), body),
            Delimiter::Quote if STYLE_QUOTES.contains(&style) => self.quote(&attr, body),
            Delimiter::Example | Delimiter::Open | Delimiter::Quote | Delimiter::Sidebar => {
                match admonition(style) {
                    Some(admonition) => self.container(Some(admonition), body),
                    None if kind == Delimiter::Open => self.nested(body),
                    None => self.container(None, body),
                }
            }
            Delimiter::Table => self.table(&attr, body),
            Delimiter::Comment => {}
        }
        next
    }

    // Emit a `|===` table; the first row is always the header
    fn table(&mut self, attr: &str, lines: &[&str]) {
        let mut cells: Vec<String> = Vec::new();
        let mut first_row = 0;
        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (lead, mut row) = split_cells(line);
            let lead = lead.trim();
            if !lead.is_empty() && !(is_cell_spec(lead) && !row.is_empty()) {
                match cells.last_mut() {
                    Some(last) => {
                        last.push(' ');
                        last.push_str(lead);
                    }
                    None => row.insert(0, lead.to_owned()),
                }
            }
            if first_row == 0 {
                first_row = row.len();
            }
            cells.extend(row);
        }
        if cells.is_empty() {
            return;
        }
        let cols = column_count(attr).unwrap_or(first_row).max(1);
        self.out
            .push(Event::Start(Tag::Table(vec![Alignment::None; cols])));
        for (row, chunk) in cells.chunks(cols).enumerate() {
            let (start, end) = if row == 0 {
                (Tag::TableHead, TagEnd::TableHead)
            } else {
                (Tag::TableRow, TagEnd::TableRow)
            };
            self.out.push(Event::Start(start));
            for col in 0..cols {
                self.out.push(Event::Start(Tag::TableCell));
                if let Some(cell) = chunk.get(col) {
                    self.inline(cell);
                }
                self.out.push(Event::End(TagEnd::TableCell));
            }
            self.out.push(Event::End(end));
        }
        self.out.push(Event::End(TagEnd::Table));
    }

    // Emit a list, nesting by the order markers first appear; returns the line after it
    fn list(&mut self, lines: &[&str], mut i: usize) -> usize {
        let mut stack: Vec<(String, bool)> = Vec::new();
        while let Some(marker) = list_marker(lines[i]) {
            match stack.iter().position(|(key, _)| *key == marker.key) {
                Some(level) => {
                    while stack.len() > level + 1 {
                        self.close_list(&mut stack);
                    }
                    self.out.push(Event::End(TagEnd::Item));
                }
                None => {
                    let (start, ordered) = match marker.kind {
                        ListKind::Ordered(start) => (Some(start), true),
                        _ => (None, false),
                    };
                    self.out.push(Event::Start(Tag::List(start)));
                    stack.push((marker.key.clone(), ordered));
                }
            }
            self.out.push(Event::Start(Tag::Item));
            i = self.item(lines, i, &marker);
            let next = (i..lines.len())
                .find(|&j| !lines[j].trim().is_empty())
                .unwrap_or(lines.len());
            let continues = lines
                .get(next)
                .and_then(|line| list_marker(line))
                .is_some_and(|after| next == i || same_family(&stack, &after.key));
            if !continues {
                break;
            }
            i = next;
        }
        while !stack.is_empty() {
            self.close_list(&mut stack);
        }
        i
    }

    // Close the innermost open item and its list
    fn close_list(&mut self, stack: &mut Vec<(String, bool)>) {
        if let Some((_, ordered)) = stack.pop() {
            self.out.push(Event::End(TagEnd::Item));
            self.out.push(Event::End(TagEnd::List(ordered)));
        }
    }

    // Emit one list item's content and attached blocks; returns the line after it
    fn item(&mut self, lines: &[&str], i: usize, marker: &Marker) -> usize {
        let mut text = marker.text;
        if marker.kind == ListKind::Bullet {
            if let Some(rest) = TASK_DONE.iter().find_map(|done| text.strip_prefix(done)) {
                self.out.push(Event::TaskListMarker(true));
                text = rest;
            } else if let Some(rest) = text.strip_prefix(TASK_TODO) {
                self.out.push(Event::TaskListMarker(false));
                text = rest;
            }
        }
        let mut j = i + 1;
        let mut body = vec![text];
        while j < lines.len() {
            let line = lines[j].trim();
            let stops = line.is_empty()
                || line == LIST_CONTINUATION
                || list_marker(line).is_some()
                || delimiter(line).is_some()
                || is_block_attr(line);
            if stops {
                break;
            }
            if !is_comment(line) {
                body.push(line);
            }
            j += 1;
        }
        body.retain(|line| !line.is_empty());
        if marker.kind == ListKind::Description {
            self.out.push(Event::Start(Tag::Strong));
            self.inline(marker.term);
            self.out.push(Event::End(TagEnd::Strong));
            if !body.is_empty() {
                self.out.push(Event::HardBreak);
            }
        }
        self.inline(&join_lines(body.into_iter()));
        while j < lines.len() && lines[j].trim() == LIST_CONTINUATION {
            j = self.one_block(lines, j + 1);
        }
        j
    }

    // Replace `{name}` references with attribute values, leaving unknown ones as written
    fn substitute(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(open) = rest.find(REF_OPEN) {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let close = after.find(REF_CLOSE);
            let name = close.map(|close| &after[..close]).filter(|name| {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
            });
            let value = name.and_then(|name| {
                self.attrs.get(name).map(String::as_str).or_else(|| {
                    BUILTIN_ATTRS
                        .iter()
                        .find(|(builtin, _)| *builtin == name)
                        .map(|(_, value)| *value)
                })
            });
            match (value, close) {
                (Some(value), Some(close)) => {
                    out.push_str(value);
                    rest = &after[close + 1..];
                }
                _ => {
                    out.push(REF_OPEN);
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    // Emit inline content after attribute substitution
    fn inline(&mut self, text: &str) {
        let text = self.substitute(text);
        Spans {
            out: &mut self.out,
            buf: String::new(),
            dead: Vec::new(),
        }
        .run(&text, 0);
    }
}

// Check whether a marker after a blank line belongs to the open list, not a new one
fn same_family(stack: &[(String, bool)], key: &str) -> bool {
    stack.iter().any(|(open, _)| open == key)
        || stack
            .last()
            .is_some_and(|(open, _)| open.chars().next() == key.chars().next())
}

// Emit an image whose alt text is the first positional attribute
fn push_image(out: &mut Vec<Event<'static>>, target: String, attrs: &str) {
    let alt = positional(attrs).first().copied().unwrap_or("").to_owned();
    out.push(Event::Start(Tag::Image {
        link_type: LinkType::Inline,
        dest_url: CowStr::from(target),
        title: CowStr::Borrowed(""),
        id: CowStr::Borrowed(""),
    }));
    if !alt.is_empty() {
        out.push(Event::Text(CowStr::from(alt)));
    }
    out.push(Event::End(TagEnd::Image));
}

// Find where a paragraph that starts at `i` ends
fn paragraph_end(lines: &[&str], i: usize) -> usize {
    (i + 1..lines.len())
        .find(|&j| {
            let line = lines[j].trim_end();
            line.trim().is_empty()
                || delimiter(line).is_some()
                || list_marker(line).is_some() && !lines[i].starts_with([' ', '\t'])
                || is_block_attr(line)
        })
        .unwrap_or(lines.len())
}

// Remove the indent shared by every non-blank line
fn dedent(lines: &[&str]) -> Vec<String> {
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.get(indent..).unwrap_or("").to_owned())
        .collect()
}

// Inline scanner that writes events, buffering plain text between them
struct Spans<'o> {
    out: &'o mut Vec<Event<'static>>,
    buf: String,
    dead: Vec<&'static str>,
}

impl Spans<'_> {
    // Scan text for inline markup
    fn run(&mut self, text: &str, depth: usize) {
        let mut i = 0;
        while let Some(ch) = text[i..].chars().next() {
            if let Some(used) = self.span(text, i, ch, depth) {
                i += used;
                continue;
            }
            match ch {
                SOFT_BREAK => self.push(Event::SoftBreak),
                HARD_BREAK => self.push(Event::HardBreak),
                _ => self.buf.push(ch),
            }
            i += ch.len_utf8();
        }
        self.flush();
    }

    // Write buffered text as one Text event
    fn flush(&mut self) {
        if !self.buf.is_empty() {
            let text = std::mem::take(&mut self.buf);
            self.out.push(Event::Text(CowStr::from(text)));
        }
    }

    // Write an event after any buffered text
    fn push(&mut self, event: Event<'static>) {
        self.flush();
        self.out.push(event);
    }

    // Wrap inner text in a style tag
    fn styled(&mut self, start: Tag<'static>, end: TagEnd, inner: &str, depth: usize) {
        if depth >= MAX_INLINE_DEPTH {
            self.buf.push_str(inner);
            return;
        }
        self.push(Event::Start(start));
        let mut nested = Spans {
            out: &mut *self.out,
            buf: String::new(),
            dead: Vec::new(),
        };
        nested.run(inner, depth + 1);
        self.out.push(Event::End(end));
    }

    // Find a closing marker, remembering markers that never close again
    fn find(&mut self, text: &str, from: usize, marker: &'static str) -> Option<usize> {
        if self.dead.contains(&marker) {
            return None;
        }
        let found = text[from..].find(marker).map(|at| from + at);
        if found.is_none() {
            self.dead.push(marker);
        }
        found
    }

    // Find a constrained closing marker, remembering markers that never close again
    fn find_constrained(&mut self, text: &str, from: usize, marker: &'static str) -> Option<usize> {
        if self.dead.contains(&marker) {
            return None;
        }
        let ch = marker.chars().next().unwrap_or(' ');
        let found = find_constrained(text, from, ch);
        if found.is_none() {
            self.dead.push(marker);
        }
        found
    }

    // Try every inline construct at `i`; returns bytes consumed when one matched
    fn span(&mut self, text: &str, i: usize, ch: char, depth: usize) -> Option<usize> {
        let rest = &text[i..];
        let prev = text[..i].chars().next_back();
        let boundary = prev.is_none_or(|p| !is_word(p));
        match ch {
            ESCAPE => {
                let next = rest[1..]
                    .chars()
                    .next()
                    .filter(|n| ESCAPABLE.contains(*n))?;
                self.buf.push(next);
                Some(1 + next.len_utf8())
            }
            '`' => self.code(text, i),
            '+' => self.passthrough(text, i, boundary),
            '<' => self.angle(rest),
            '*' => self.quoted(text, i, STRONG, boundary, depth),
            '_' => self.quoted(text, i, EMPHASIS, boundary, depth),
            '#' => self.quoted(text, i, HIGHLIGHT, boundary, depth),
            _ if SCRIPTS.contains(&ch) => self.script(rest, ch),
            _ if boundary => self.macro_at(rest, depth),
            _ => None,
        }
    }

    // Monospace in single or double backticks
    fn code(&mut self, text: &str, i: usize) -> Option<usize> {
        let marker = if text[i..].starts_with(DOUBLE_BACKTICK) {
            DOUBLE_BACKTICK
        } else {
            BACKTICK
        };
        let from = i + marker.len();
        let close = self.find(text, from, marker)?;
        let inner = &text[from..close];
        if inner.trim().is_empty() {
            return None;
        }
        let inner = inner
            .strip_prefix(PLUS)
            .and_then(|body| body.strip_suffix(PLUS))
            .unwrap_or(inner);
        self.push(Event::Code(CowStr::from(
            inner.replace([SOFT_BREAK, HARD_BREAK], " "),
        )));
        Some(close + marker.len() - i)
    }

    // `+text+` passthrough as code, `+++text+++` as plain text
    fn passthrough(&mut self, text: &str, i: usize, boundary: bool) -> Option<usize> {
        if text[i..].starts_with(TRIPLE_PLUS) {
            let from = i + TRIPLE_PLUS.len();
            let close = self.find(text, from, TRIPLE_PLUS)?;
            self.buf.push_str(&text[from..close]);
            return Some(close + TRIPLE_PLUS.len() - i);
        }
        let from = i + PLUS.len();
        let opens = boundary && text[from..].starts_with(|c: char| !c.is_whitespace() && c != '+');
        if !opens {
            return None;
        }
        let close = self.find_constrained(text, from, PLUS)?;
        self.push(Event::Code(CowStr::from(text[from..close].to_owned())));
        Some(close + PLUS.len() - i)
    }

    // Strong, emphasis or highlight, unconstrained when doubled
    fn quoted(
        &mut self,
        text: &str,
        i: usize,
        markers: [&'static str; 2],
        boundary: bool,
        depth: usize,
    ) -> Option<usize> {
        let [double, single] = markers;
        let (start, end) = if single == EMPHASIS[1] {
            (Tag::Emphasis, TagEnd::Emphasis)
        } else {
            (Tag::Strong, TagEnd::Strong)
        };
        if text[i..].starts_with(double) {
            let from = i + double.len();
            if let Some(close) = self.find(text, from, double).filter(|&close| close > from) {
                self.styled(start, end, &text[from..close], depth);
                return Some(close + double.len() - i);
            }
        }
        let from = i + single.len();
        let opens = boundary
            && text[from..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace() && !single.starts_with(c));
        if !opens {
            return None;
        }
        let close = self.find_constrained(text, from, single)?;
        self.styled(start, end, &text[from..close], depth);
        Some(close + single.len() - i)
    }

    // `~sub~` and `^sup^` shown as plain text
    fn script(&mut self, rest: &str, marker: char) -> Option<usize> {
        let body = &rest[marker.len_utf8()..];
        let close = body.find(marker)?;
        let inner = &body[..close];
        if inner.is_empty() || inner.contains(char::is_whitespace) {
            return None;
        }
        self.buf.push_str(inner);
        Some(close + 2 * marker.len_utf8())
    }

    // `<<id,text>>` cross references and `<https://…>` autolinks
    fn angle(&mut self, rest: &str) -> Option<usize> {
        if let Some(body) = rest.strip_prefix(XREF_OPEN) {
            let close = body.find(XREF_CLOSE)?;
            let inner = &body[..close];
            let (id, label) = inner.split_once(ITEM_SEPARATOR).unwrap_or((inner, inner));
            if id.is_empty() || id.contains(char::is_whitespace) {
                return None;
            }
            self.buf.push_str(label.trim());
            return Some(XREF_OPEN.len() + close + XREF_CLOSE.len());
        }
        let body = &rest[1..];
        if !URL_SCHEMES.iter().any(|scheme| body.starts_with(scheme)) {
            return None;
        }
        let close = body.find(ANGLE_CLOSE)?;
        let url = &body[..close];
        if url.contains(char::is_whitespace) || url.contains(ANGLE_OPEN) {
            return None;
        }
        self.autolink(LinkType::Autolink, url, url);
        Some(close + 2)
    }

    // Emit a link whose text is its own address
    fn autolink(&mut self, link_type: LinkType, dest: &str, shown: &str) {
        self.push(Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from(dest.to_owned()),
            title: CowStr::Borrowed(""),
            id: CowStr::Borrowed(""),
        }));
        self.out.push(Event::Text(CowStr::from(shown.to_owned())));
        self.out.push(Event::End(TagEnd::Link));
    }

    // URLs and `name:target[text]` macros at a word boundary
    fn macro_at(&mut self, rest: &str, depth: usize) -> Option<usize> {
        if let Some(scheme) = URL_SCHEMES.iter().find(|scheme| rest.starts_with(**scheme)) {
            return self.url(rest, 0, scheme.len(), depth);
        }
        if rest.starts_with(MAILTO) {
            return self.url(rest, 0, MAILTO.len(), depth);
        }
        if let Some(body) = rest.strip_prefix(LINK_MACRO) {
            let open = body.find([ATTR_OPEN, ' ', '\t'])?;
            if !body[open..].starts_with(ATTR_OPEN) || open == 0 {
                return None;
            }
            return self.url(rest, LINK_MACRO.len(), 0, depth);
        }
        if let Some(body) = rest.strip_prefix(XREF_MACRO) {
            let open = body.find(ATTR_OPEN)?;
            let id = &body[..open];
            let (label, used) = bracketed(&body[open..])?;
            if id.is_empty() || id.contains(char::is_whitespace) {
                return None;
            }
            self.buf
                .push_str(if label.is_empty() { id } else { label.trim() });
            return Some(XREF_MACRO.len() + open + used);
        }
        if let Some(body) = rest.strip_prefix(INLINE_IMAGE) {
            let open = body.find(ATTR_OPEN)?;
            let target = &body[..open];
            if target.is_empty()
                || target.starts_with(ATTR_MARK)
                || target.contains(char::is_whitespace)
            {
                return None;
            }
            let (alt, used) = bracketed(&body[open..])?;
            self.flush();
            push_image(self.out, target.to_owned(), alt);
            return Some(INLINE_IMAGE.len() + open + used);
        }
        let code_macro = CODE_MACROS.iter().find(|m| rest.starts_with(**m))?;
        let (inner, used) = bracketed(&rest[code_macro.len() - 1..])?;
        self.push(Event::Code(CowStr::from(inner.to_owned())));
        Some(code_macro.len() - 1 + used)
    }

    // A URL starting at `skip`, with optional `[text]`; `min` is the shortest address body
    fn url(&mut self, rest: &str, skip: usize, min: usize, depth: usize) -> Option<usize> {
        let body = &rest[skip..];
        let end = body
            .find(|c: char| c.is_whitespace() || URL_STOPS.contains(c))
            .unwrap_or(body.len());
        let mut url = &body[..end];
        if let Some((label, used)) = bracketed(&body[end..]) {
            if url.len() <= min {
                return None;
            }
            let label = link_text(label);
            let shown = url.strip_prefix(MAILTO).unwrap_or(url);
            if label.is_empty() {
                self.autolink(LinkType::Autolink, url, shown);
            } else {
                self.styled_link(url, label, depth);
            }
            return Some(skip + end + used);
        }
        if skip > 0 {
            return None;
        }
        url = url.trim_end_matches(|c: char| URL_TRAILING.contains(c));
        if url.len() <= min {
            return None;
        }
        match url.strip_prefix(MAILTO) {
            Some(address) => self.autolink(LinkType::Email, address, address),
            None => self.autolink(LinkType::Autolink, url, url),
        }
        Some(url.len())
    }

    // Emit a link with marked-up text
    fn styled_link(&mut self, url: &str, label: &str, depth: usize) {
        let start = Tag::Link {
            link_type: LinkType::Inline,
            dest_url: CowStr::from(url.to_owned()),
            title: CowStr::Borrowed(""),
            id: CowStr::Borrowed(""),
        };
        self.styled(start, TagEnd::Link, label, depth);
    }
}
