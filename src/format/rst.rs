// Author: machinageist
// Date: 2026-11-09
// Description: reStructuredText documents to pulldown-cmark events
// Notes: A tolerant line-based parser for the subset real READMEs use; every nested block
// is dedented and parsed again, so indentation is always relative to column zero

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd,
};

// Section adornment
const ADORNMENT_CHARS: &str = "=-`:'\"~^_*+#<>";
const MIN_ADORNMENT: usize = 2;
const MIN_SNIFF_ADORNMENT: usize = 3;
const MIN_TRANSITION: usize = 4;
const MAX_HEADING_LEVEL: usize = 6;

// Line handling
const TAB_WIDTH: usize = 8;
const MAX_DEPTH: usize = 32;

// Explicit markup
const EXPLICIT_MARK: &str = "..";
const EXPLICIT_PREFIX: &str = ".. ";
const DIRECTIVE_MARK: &str = "::";
const TARGET_PREFIX: char = '_';
const TARGET_LINE: &str = ".. _";
const SUBSTITUTION_MARK: char = '|';
const FOOTNOTE_OPEN: char = '[';
const FOOTNOTE_CLOSE: char = ']';
const DIRECTIVE_NAME_EXTRAS: &str = "-_:.";

// Directive names
const CODE_DIRECTIVES: [&str; 3] = ["code-block", "code", "sourcecode"];
const GENERIC_ADMONITION: &str = "admonition";
const IMAGE_DIRECTIVES: [&str; 2] = ["image", "figure"];
const FIGURE_DIRECTIVE: &str = "figure";
const ALT_OPTION: &str = "alt";
const ADMONITIONS: [(&str, BlockQuoteKind); 11] = [
    ("note", BlockQuoteKind::Note),
    ("tip", BlockQuoteKind::Tip),
    ("hint", BlockQuoteKind::Tip),
    ("important", BlockQuoteKind::Important),
    ("warning", BlockQuoteKind::Warning),
    ("caution", BlockQuoteKind::Caution),
    ("danger", BlockQuoteKind::Caution),
    ("error", BlockQuoteKind::Caution),
    ("attention", BlockQuoteKind::Caution),
    ("seealso", BlockQuoteKind::Note),
    ("todo", BlockQuoteKind::Note),
];

// Lists and fields
const BULLET_CHARS: [char; 3] = ['-', '*', '+'];
const AUTO_NUMBER: &str = "#";
const ENUM_CLOSERS: [char; 2] = ['.', ')'];
const ENUM_OPEN: char = '(';
const ENUM_CLOSE: char = ')';
const MAX_ENUM_DIGITS: usize = 9;
const FIELD_MARK: char = ':';
const FIELD_SUFFIX: &str = ":";
const CLASSIFIER_MARK: &str = " : ";

// Tables
const SIMPLE_BORDER: char = '=';
const GRID_CORNER: char = '+';
const GRID_BAR: char = '|';
const GRID_RULES: &str = "+-=";
const SPAN_RULE: char = '-';

// Literal block markers
const LITERAL_ALONE: &str = "::";
const LITERAL_SPACED: &str = " ::";

// Inline markup
const STRONG_MARK: [char; 2] = ['*', '*'];
const EMPHASIS_MARK: [char; 1] = ['*'];
const LITERAL_MARK: [char; 2] = ['`', '`'];
const INTERPRETED_MARK: [char; 1] = ['`'];
const ROLE_MARK: char = ':';
const BACKTICK: char = '`';
const ESCAPE: char = '\\';
const REFERENCE_MARK: char = '_';
const URL_OPEN: char = '<';
const URL_CLOSE: char = '>';
const OPEN_PRECEDERS: &str = "'\"([{<-/:";
const ROLE_NAME_EXTRAS: &str = "-_.+";
const EMBEDDED_LINK_END: &str = ">`_";
const ROLE_OPEN: &str = ":`";

// Translate reStructuredText into document events
pub fn events(input: &str) -> Vec<Event<'_>> {
    let lines: Vec<String> = input.lines().map(expand_tabs).collect();
    let mut parser = Parser::default();
    parser.blocks(&lines, 0);
    parser.out
}

// Check whether the head of an input looks like reStructuredText
pub fn sniff(head: &str) -> bool {
    let lines: Vec<&str> = head.lines().map(str::trim_end).collect();
    lines
        .iter()
        .any(|line| is_directive_line(line) || line.starts_with(TARGET_LINE))
        || lines.windows(3).any(is_over_under_title)
        || head.contains(EMBEDDED_LINK_END)
        || has_role(head)
}

// Line shape helpers

// Expand tabs to RST's eight-column stops and drop trailing space
fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        if ch == '\t' {
            let pad = TAB_WIDTH - out.chars().count() % TAB_WIDTH;
            out.extend(std::iter::repeat_n(' ', pad));
        } else {
            out.push(ch);
        }
    }
    out.truncate(out.trim_end().len());
    out
}

// Count leading spaces
fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

// Strip the smallest common indent from a run of lines
fn dedent(lines: &[String]) -> Vec<String> {
    let cut = lines
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| indent(line))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.get(cut..).unwrap_or("").to_owned())
        .collect()
}

// Take the indented lines after `start`, dedented, and the index past them
fn indented_block(lines: &[String], start: usize) -> (Vec<String>, usize) {
    let mut next = start;
    while next < lines.len() && (lines[next].is_empty() || indent(&lines[next]) > 0) {
        next += 1;
    }
    let mut end = next;
    while end > start && lines[end - 1].is_empty() {
        end -= 1;
    }
    (dedent(&lines[start..end]), next)
}

// Drop leading blank lines
fn skip_blank(lines: &[String]) -> &[String] {
    let first = lines
        .iter()
        .position(|line| !line.is_empty())
        .unwrap_or(lines.len());
    &lines[first..]
}

// The repeated punctuation of a section adornment line
fn adornment(line: &str) -> Option<char> {
    let first = line.chars().next()?;
    let uniform = ADORNMENT_CHARS.contains(first) && line.chars().all(|ch| ch == first);
    (uniform && line.chars().count() >= MIN_ADORNMENT).then_some(first)
}

// Check for a title with matching over- and underline
fn is_over_under_title(window: &[&str]) -> bool {
    let [over, text, under] = window else {
        return false;
    };
    let width = text.trim().chars().count();
    let over_len = over.chars().count();
    adornment(over).is_some()
        && adornment(over) == adornment(under)
        && adornment(text).is_none()
        && width > 0
        && over_len >= width
        && over_len >= MIN_SNIFF_ADORNMENT
}

// Check for an explicit-markup directive line such as `.. note::`
fn is_directive_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix(EXPLICIT_PREFIX) else {
        return false;
    };
    let rest = match rest.strip_prefix(SUBSTITUTION_MARK) {
        Some(sub) => sub
            .split_once(SUBSTITUTION_MARK)
            .map_or("", |(_, tail)| tail.trim()),
        None => rest,
    };
    rest.split_once(DIRECTIVE_MARK)
        .is_some_and(|(name, _)| is_directive_name(name))
}

// Check that a directive name is one word of name characters
fn is_directive_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_alphanumeric() || DIRECTIVE_NAME_EXTRAS.contains(ch))
}

// Check for an interpreted-text role such as :ref:`x`
fn has_role(head: &str) -> bool {
    head.match_indices(ROLE_OPEN).any(|(at, _)| {
        let before = &head[..at];
        let name_start = before
            .rfind(|ch: char| !(ch.is_alphanumeric() || ROLE_NAME_EXTRAS.contains(ch)))
            .map_or(0, |pos| pos + 1);
        let name = &before[name_start..];
        let opener = before[..name_start].strip_suffix(ROLE_MARK);
        !name.is_empty()
            && opener.is_some_and(|prefix| {
                prefix
                    .chars()
                    .next_back()
                    .is_none_or(|ch| ch.is_whitespace() || OPEN_PRECEDERS.contains(ch))
            })
    })
}

// Simple table border: runs of `=` separated by spaces, at least two columns
fn is_simple_border(line: &str) -> bool {
    line.starts_with(SIMPLE_BORDER)
        && line.split_whitespace().count() >= 2
        && line.chars().all(|ch| ch == SIMPLE_BORDER || ch == ' ')
}

// Grid table border such as +----+----+
fn is_grid_border(line: &str) -> bool {
    line.starts_with(GRID_CORNER)
        && line.ends_with(GRID_CORNER)
        && line.matches(GRID_CORNER).count() >= 2
        && line.len() > 2
        && line.chars().all(|ch| GRID_RULES.contains(ch))
}

// Column spans of a simple table border, in chars
fn column_spans(border: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    for (index, ch) in border.chars().chain(std::iter::once(' ')).enumerate() {
        match (ch == ' ', start) {
            (false, None) => start = Some(index),
            (true, Some(begin)) => {
                spans.push((begin, index));
                start = None;
            }
            _ => {}
        }
    }
    spans
}

// Slice chars between two columns, clipped to the line
fn char_slice(chars: &[char], start: usize, end: usize) -> String {
    let end = end.min(chars.len());
    let start = start.min(end);
    chars[start..end]
        .iter()
        .collect::<String>()
        .trim()
        .to_owned()
}

// A list item marker: bullet char or None for enumerated, text offset, start number
fn list_item(line: &str) -> Option<(Option<char>, usize, u64)> {
    let first = line.chars().next()?;
    if BULLET_CHARS.contains(&first) {
        let rest = &line[1..];
        return (rest.is_empty() || rest.starts_with(' '))
            .then(|| (Some(first), 1 + indent(rest), 0));
    }
    let (label, after) = match line.strip_prefix(ENUM_OPEN) {
        Some(rest) => {
            let close = rest.find(ENUM_CLOSE)?;
            (&rest[..close], close + 2)
        }
        None => {
            let close = line.find(ENUM_CLOSERS)?;
            (&line[..close], close + 1)
        }
    };
    let number = if label == AUTO_NUMBER {
        1
    } else if !label.is_empty()
        && label.len() <= MAX_ENUM_DIGITS
        && label.bytes().all(|byte| byte.is_ascii_digit())
    {
        label.parse().ok()?
    } else {
        return None;
    };
    let rest = &line[after..];
    (rest.is_empty() || rest.starts_with(' ')).then(|| (None, after + indent(rest), number))
}

// A field list line `:name: value`
fn field(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix(FIELD_MARK)?;
    let (name, value) = rest.split_once(FIELD_MARK)?;
    let named = !name.is_empty() && !name.starts_with(' ') && !name.contains(BACKTICK);
    let spaced = value.is_empty() || value.starts_with(' ');
    (named && spaced).then(|| (name, value.trim()))
}

// A term line with an indented definition right under it
fn is_definition(lines: &[String], at: usize) -> bool {
    let term = &lines[at];
    lines
        .get(at + 1)
        .is_some_and(|body| !body.is_empty() && indent(body) > 0)
        && !term.ends_with(DIRECTIVE_MARK)
}

// Reduce a literal-block paragraph's trailing `::` as RST does
fn strip_literal_mark(text: &str) -> &str {
    if text == LITERAL_ALONE {
        ""
    } else if let Some(head) = text.strip_suffix(LITERAL_SPACED) {
        head.trim_end()
    } else {
        &text[..text.len() - 1]
    }
}

// Inline markup helpers

// Check that inline markup may start between `prev` and `next`
fn can_open(prev: Option<char>, next: Option<&char>) -> bool {
    let before = prev.is_none_or(|ch| ch.is_whitespace() || OPEN_PRECEDERS.contains(ch));
    let after = next.is_some_and(|ch| !ch.is_whitespace());
    before && after
}

// Find a closing mark after `from` that ends inline markup
fn find_close(chars: &[char], from: usize, mark: &[char]) -> Option<usize> {
    (from + 1..chars.len()).find(|&at| {
        chars[at..].starts_with(mark)
            && !chars[at - 1].is_whitespace()
            && chars
                .get(at + mark.len())
                .is_none_or(|ch| !ch.is_alphanumeric())
    })
}

// Split `text <url>` into its text and url
fn embedded_url(inner: &str) -> Option<(&str, &str)> {
    let body = inner.strip_suffix(URL_CLOSE)?;
    let open = body.rfind(URL_OPEN)?;
    Some((body[..open].trim(), &body[open + 1..]))
}

// Count an item's top-level paragraphs and top-level blocks
fn top_level_blocks(events: &[Event]) -> (usize, usize) {
    let mut level = 0usize;
    let mut paragraphs = 0;
    let mut blocks = 0;
    for event in events {
        match event {
            Event::Start(tag) => {
                if level == 0 {
                    blocks += 1;
                    paragraphs += usize::from(matches!(tag, Tag::Paragraph));
                }
                level += 1;
            }
            Event::End(_) => level = level.saturating_sub(1),
            _ => {}
        }
    }
    (paragraphs, blocks)
}

#[derive(Default)]
struct Parser {
    styles: Vec<(char, bool)>,
    out: Vec<Event<'static>>,
}

impl Parser {
    // Parse a run of dedented lines into blocks
    fn blocks(&mut self, lines: &[String], depth: usize) {
        if depth >= MAX_DEPTH {
            self.code_block(String::new(), lines);
            return;
        }
        let mut at = 0;
        while at < lines.len() {
            let line = &lines[at];
            at = if line.is_empty() {
                at + 1
            } else if indent(line) > 0 {
                self.block_quote(lines, at, depth)
            } else if let Some(next) = self.section(lines, at) {
                next
            } else if is_simple_border(line) {
                self.simple_table(lines, at)
            } else if is_grid_border(line) {
                self.grid_table(lines, at)
            } else if line == EXPLICIT_MARK || line.starts_with(EXPLICIT_PREFIX) {
                self.explicit(lines, at, depth)
            } else if let Some(marker) = list_item(line) {
                self.list(lines, at, marker.0, marker.2, depth)
            } else if field(line).is_some() {
                self.field(lines, at)
            } else if is_definition(lines, at) {
                self.definitions(lines, at, depth)
            } else {
                self.paragraph(lines, at)
            };
        }
    }

    // Emit a title or transition starting at `at`, if there is one
    fn section(&mut self, lines: &[String], at: usize) -> Option<usize> {
        let line = &lines[at];
        if let Some(mark) = adornment(line) {
            if let (Some(text), Some(under)) = (lines.get(at + 1), lines.get(at + 2)) {
                let titled = !text.trim().is_empty() && adornment(text).is_none();
                if titled && adornment(under) == Some(mark) {
                    self.heading((mark, true), text.trim());
                    return Some(at + 3);
                }
            }
            let alone = lines.get(at + 1).is_none_or(|next| next.is_empty());
            if line.chars().count() >= MIN_TRANSITION && alone {
                self.out.push(Event::Rule);
                return Some(at + 1);
            }
            return None;
        }
        let mark = adornment(lines.get(at + 1)?)?;
        let width = line.trim().chars().count();
        if lines[at + 1].chars().count() >= width.min(MIN_TRANSITION) {
            self.heading((mark, false), line.trim());
            return Some(at + 2);
        }
        None
    }

    // Emit a heading, its level set by when its adornment style first appeared
    fn heading(&mut self, style: (char, bool), text: &str) {
        let index = match self.styles.iter().position(|seen| *seen == style) {
            Some(index) => index,
            None => {
                self.styles.push(style);
                self.styles.len() - 1
            }
        };
        let level = HeadingLevel::try_from(index.min(MAX_HEADING_LEVEL - 1) + 1)
            .unwrap_or(HeadingLevel::H6);
        self.out.push(Event::Start(Tag::Heading {
            level,
            id: None,
            classes: vec![],
            attrs: vec![],
        }));
        self.inline(text);
        self.out.push(Event::End(TagEnd::Heading(level)));
    }

    // Emit a paragraph and any literal block its `::` introduces
    fn paragraph(&mut self, lines: &[String], at: usize) -> usize {
        let mut next = at;
        while next < lines.len() && !lines[next].is_empty() && indent(&lines[next]) == 0 {
            next += 1;
        }
        let joined = lines[at..next].join(" ");
        let literal = joined.ends_with(DIRECTIVE_MARK);
        let text = if literal {
            strip_literal_mark(&joined)
        } else {
            &joined
        };
        if !text.is_empty() {
            self.text_paragraph(text);
        }
        if !literal {
            return next;
        }
        let mut body = next;
        while body < lines.len() && lines[body].is_empty() {
            body += 1;
        }
        if body >= lines.len() || indent(&lines[body]) == 0 {
            return next;
        }
        let (code, end) = indented_block(lines, body);
        self.code_block(String::new(), &code);
        end
    }

    // Emit an indented block as a quote
    fn block_quote(&mut self, lines: &[String], at: usize, depth: usize) -> usize {
        let (body, end) = indented_block(lines, at);
        self.out.push(Event::Start(Tag::BlockQuote(None)));
        self.blocks(&body, depth + 1);
        self.out.push(Event::End(TagEnd::BlockQuote(None)));
        end
    }

    // Emit a bullet or enumerated list, tight unless its items hold several paragraphs
    fn list(
        &mut self,
        lines: &[String],
        mut at: usize,
        bullet: Option<char>,
        start: u64,
        depth: usize,
    ) -> usize {
        let kind = bullet.is_none().then_some(start);
        let list_start = self.out.len();
        let mut loose = false;
        self.out.push(Event::Start(Tag::List(kind)));
        while let Some((_, offset, _)) = list_item(&lines[at]) {
            let (rest, next) = indented_block(lines, at + 1);
            let mut content = vec![lines[at].get(offset..).unwrap_or("").to_owned()];
            content.extend(rest);
            self.out.push(Event::Start(Tag::Item));
            let item_start = self.out.len();
            self.blocks(skip_blank(&content), depth + 1);
            let (paragraphs, blocks) = top_level_blocks(&self.out[item_start..]);
            loose |= paragraphs > 1;
            self.out.push(Event::End(TagEnd::Item));
            let mut follow = next;
            while follow < lines.len() && lines[follow].is_empty() {
                follow += 1;
            }
            let same = lines
                .get(follow)
                .and_then(|line| list_item(line))
                .is_some_and(|(other, _, _)| other == bullet);
            if !same {
                at = next;
                break;
            }
            // A blank line between plain items makes the list loose
            loose |= lines[follow - 1].is_empty() && blocks == paragraphs;
            at = follow;
        }
        self.out.push(Event::End(TagEnd::List(kind.is_some())));
        if !loose {
            self.tighten(list_start);
        }
        at
    }

    // Remove the paragraph wrappers directly inside a tight list's items
    fn tighten(&mut self, list_start: usize) {
        let mut level = 0usize;
        let mut kept = Vec::with_capacity(self.out.len() - list_start);
        for event in self.out.drain(list_start..) {
            match &event {
                Event::Start(Tag::Paragraph) if level == 2 => {
                    level += 1;
                    continue;
                }
                Event::End(TagEnd::Paragraph) if level == 3 => {
                    level -= 1;
                    continue;
                }
                Event::Start(_) => level += 1,
                Event::End(_) => level = level.saturating_sub(1),
                _ => {}
            }
            kept.push(event);
        }
        self.out.extend(kept);
    }

    // Emit a definition list as items holding a bold term and its body
    fn definitions(&mut self, lines: &[String], mut at: usize, depth: usize) -> usize {
        self.out.push(Event::Start(Tag::List(None)));
        loop {
            let term = lines[at].split(CLASSIFIER_MARK).next().unwrap_or("");
            let (body, next) = indented_block(lines, at + 1);
            self.out.push(Event::Start(Tag::Item));
            self.out.push(Event::Start(Tag::Strong));
            self.inline(term);
            self.out.push(Event::End(TagEnd::Strong));
            self.blocks(&body, depth + 1);
            self.out.push(Event::End(TagEnd::Item));
            at = next;
            let mut follow = next;
            while follow < lines.len() && lines[follow].is_empty() {
                follow += 1;
            }
            let more = follow < lines.len()
                && indent(&lines[follow]) == 0
                && list_item(&lines[follow]).is_none()
                && field(&lines[follow]).is_none()
                && !lines[follow].starts_with(EXPLICIT_MARK)
                && is_definition(lines, follow);
            if !more {
                break;
            }
            at = follow;
        }
        self.out.push(Event::End(TagEnd::List(false)));
        at
    }

    // Emit one field as a paragraph led by its bold name
    fn field(&mut self, lines: &[String], at: usize) -> usize {
        let Some((name, value)) = field(&lines[at]) else {
            return at + 1;
        };
        let (rest, next) = indented_block(lines, at + 1);
        let mut text = value.to_owned();
        for line in rest.iter().filter(|line| !line.is_empty()) {
            text.push(' ');
            text.push_str(line.trim());
        }
        self.out.push(Event::Start(Tag::Paragraph));
        self.out.push(Event::Start(Tag::Strong));
        self.out
            .push(Event::Text(CowStr::from(format!("{name}{FIELD_SUFFIX}"))));
        self.out.push(Event::End(TagEnd::Strong));
        if !text.is_empty() {
            self.out.push(Event::Text(CowStr::from(" ")));
            self.inline(text.trim());
        }
        self.out.push(Event::End(TagEnd::Paragraph));
        next
    }

    // Handle a `..` block: directive, target, substitution, footnote or comment
    fn explicit(&mut self, lines: &[String], at: usize, depth: usize) -> usize {
        let rest = lines[at][EXPLICIT_MARK.len()..].trim();
        let (body, next) = indented_block(lines, at + 1);
        if rest.starts_with(TARGET_PREFIX) || rest.starts_with(SUBSTITUTION_MARK) {
            return next;
        }
        if let Some(label) = rest.strip_prefix(FOOTNOTE_OPEN)
            && let Some((label, text)) = label.split_once(FOOTNOTE_CLOSE)
        {
            let mut joined = format!("{FOOTNOTE_OPEN}{label}{FOOTNOTE_CLOSE} {}", text.trim());
            for line in body.iter().filter(|line| !line.is_empty()) {
                joined.push(' ');
                joined.push_str(line.trim());
            }
            self.text_paragraph(joined.trim());
            return next;
        }
        if let Some((name, arg)) = rest.split_once(DIRECTIVE_MARK)
            && is_directive_name(name)
        {
            self.directive(&name.to_ascii_lowercase(), arg.trim(), &body, depth);
        }
        next
    }

    // Emit a directive by name
    fn directive(&mut self, name: &str, arg: &str, body: &[String], depth: usize) {
        let options = body
            .iter()
            .position(|line| field(line).is_none())
            .unwrap_or(body.len());
        let content = skip_blank(&body[options..]);
        if CODE_DIRECTIVES.contains(&name) {
            self.code_block(arg.to_owned(), content);
        } else if let Some((_, kind)) = ADMONITIONS.iter().find(|(known, _)| *known == name) {
            let mut lines = Vec::new();
            if !arg.is_empty() {
                lines.push(arg.to_owned());
            }
            lines.extend(content.iter().cloned());
            self.out.push(Event::Start(Tag::BlockQuote(Some(*kind))));
            self.blocks(&lines, depth + 1);
            self.out.push(Event::End(TagEnd::BlockQuote(Some(*kind))));
        } else if name == GENERIC_ADMONITION {
            self.out.push(Event::Start(Tag::BlockQuote(None)));
            if !arg.is_empty() {
                self.out.push(Event::Start(Tag::Paragraph));
                self.out.push(Event::Start(Tag::Strong));
                self.inline(arg);
                self.out.push(Event::End(TagEnd::Strong));
                self.out.push(Event::End(TagEnd::Paragraph));
            }
            self.blocks(content, depth + 1);
            self.out.push(Event::End(TagEnd::BlockQuote(None)));
        } else if IMAGE_DIRECTIVES.contains(&name) {
            let alt = body[..options]
                .iter()
                .filter_map(|line| field(line))
                .find(|(option, _)| *option == ALT_OPTION)
                .map_or("", |(_, value)| value);
            self.image(arg, alt);
            if name == FIGURE_DIRECTIVE {
                self.blocks(content, depth + 1);
            }
        } else {
            let mut lines = Vec::new();
            if !arg.is_empty() {
                lines.push(arg.to_owned());
            }
            lines.extend(body.iter().cloned());
            self.code_block(name.to_owned(), &lines);
        }
    }

    // Emit an image paragraph
    fn image(&mut self, url: &str, alt: &str) {
        self.out.push(Event::Start(Tag::Paragraph));
        self.out.push(Event::Start(Tag::Image {
            link_type: LinkType::Inline,
            dest_url: CowStr::from(url.to_owned()),
            title: CowStr::from(""),
            id: CowStr::from(""),
        }));
        if !alt.is_empty() {
            self.out.push(Event::Text(CowStr::from(alt.to_owned())));
        }
        self.out.push(Event::End(TagEnd::Image));
        self.out.push(Event::End(TagEnd::Paragraph));
    }

    // Emit a code block, skipped when it holds no text
    fn code_block(&mut self, lang: String, lines: &[String]) {
        if lines.iter().all(String::is_empty) {
            return;
        }
        let kind = if lang.is_empty() {
            CodeBlockKind::Indented
        } else {
            CodeBlockKind::Fenced(CowStr::from(lang))
        };
        let mut text = lines.join("\n");
        text.push('\n');
        self.out.push(Event::Start(Tag::CodeBlock(kind)));
        self.out.push(Event::Text(CowStr::from(text)));
        self.out.push(Event::End(TagEnd::CodeBlock));
    }

    // Emit a simple table bounded by `===  ===` borders
    fn simple_table(&mut self, lines: &[String], at: usize) -> usize {
        let spans = column_spans(&lines[at]);
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut next = at + 1;
        while next < lines.len() {
            let line = &lines[next];
            next += 1;
            if is_simple_border(line) {
                if lines.get(next).is_none_or(|after| after.is_empty()) {
                    break;
                }
                continue;
            }
            if line.is_empty() || line.chars().all(|ch| ch == SPAN_RULE || ch == ' ') {
                continue;
            }
            let chars: Vec<char> = line.chars().collect();
            let cells: Vec<String> = spans
                .iter()
                .enumerate()
                .map(|(column, (start, _))| {
                    let end = spans.get(column + 1).map_or(chars.len(), |span| span.0);
                    char_slice(&chars, *start, end)
                })
                .collect();
            match rows.last_mut() {
                Some(last) if cells[0].is_empty() => {
                    for (cell, extra) in last.iter_mut().zip(cells) {
                        if !extra.is_empty() {
                            cell.push(' ');
                            cell.push_str(&extra);
                        }
                    }
                }
                _ => rows.push(cells),
            }
        }
        self.table(spans.len(), &rows);
        next
    }

    // Emit a grid table drawn with + - = and |
    fn grid_table(&mut self, lines: &[String], at: usize) -> usize {
        let bounds: Vec<usize> = lines[at]
            .chars()
            .enumerate()
            .filter(|(_, ch)| *ch == GRID_CORNER)
            .map(|(index, _)| index)
            .collect();
        let columns = bounds.len().saturating_sub(1);
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut current: Vec<String> = vec![String::new(); columns];
        let mut next = at;
        while next < lines.len() {
            let line = &lines[next];
            if line.starts_with(GRID_CORNER) {
                if current.iter().any(|cell| !cell.is_empty()) {
                    rows.push(std::mem::replace(
                        &mut current,
                        vec![String::new(); columns],
                    ));
                }
            } else if line.starts_with(GRID_BAR) {
                let chars: Vec<char> = line.chars().collect();
                for (column, cell) in current.iter_mut().enumerate() {
                    let piece = char_slice(&chars, bounds[column] + 1, bounds[column + 1]);
                    let piece = piece.trim_matches(GRID_BAR).trim();
                    if !piece.is_empty() {
                        if !cell.is_empty() {
                            cell.push(' ');
                        }
                        cell.push_str(piece);
                    }
                }
            } else {
                break;
            }
            next += 1;
        }
        if current.iter().any(|cell| !cell.is_empty()) {
            rows.push(current);
        }
        self.table(columns, &rows);
        next
    }

    // Emit table events, the first row as the head
    fn table(&mut self, columns: usize, rows: &[Vec<String>]) {
        let Some((head, body)) = rows.split_first() else {
            return;
        };
        if columns == 0 {
            return;
        }
        self.out
            .push(Event::Start(Tag::Table(vec![Alignment::None; columns])));
        self.out.push(Event::Start(Tag::TableHead));
        self.cells(head, columns);
        self.out.push(Event::End(TagEnd::TableHead));
        for row in body {
            self.out.push(Event::Start(Tag::TableRow));
            self.cells(row, columns);
            self.out.push(Event::End(TagEnd::TableRow));
        }
        self.out.push(Event::End(TagEnd::Table));
    }

    // Emit one row's cells, padded to the column count
    fn cells(&mut self, row: &[String], columns: usize) {
        for column in 0..columns {
            self.out.push(Event::Start(Tag::TableCell));
            self.inline(row.get(column).map_or("", String::as_str));
            self.out.push(Event::End(TagEnd::TableCell));
        }
    }

    // Emit a paragraph of inline text
    fn text_paragraph(&mut self, text: &str) {
        self.out.push(Event::Start(Tag::Paragraph));
        self.inline(text);
        self.out.push(Event::End(TagEnd::Paragraph));
    }

    // Translate inline markup into events
    fn inline(&mut self, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        let mut plain = String::new();
        let mut at = 0;
        while at < chars.len() {
            let ch = chars[at];
            let prev = at.checked_sub(1).map(|before| chars[before]);
            if ch == ESCAPE {
                if let Some(&escaped) = chars.get(at + 1) {
                    plain.push(escaped);
                }
                at += 2;
                continue;
            }
            let opens = |mark: &[char]| {
                chars[at..].starts_with(mark) && can_open(prev, chars.get(at + mark.len()))
            };
            let span = if opens(&LITERAL_MARK) {
                self.wrapped(&chars, at, &LITERAL_MARK, &mut plain, Wrap::Code)
            } else if opens(&STRONG_MARK) {
                self.wrapped(&chars, at, &STRONG_MARK, &mut plain, Wrap::Strong)
            } else if opens(&EMPHASIS_MARK) {
                self.wrapped(&chars, at, &EMPHASIS_MARK, &mut plain, Wrap::Emphasis)
            } else if opens(&INTERPRETED_MARK) {
                self.interpreted(&chars, at, &mut plain)
            } else if ch == ROLE_MARK && can_open(prev, chars.get(at + 1)) {
                self.role(&chars, at, &mut plain)
            } else {
                None
            };
            match span {
                Some(next) => at = next,
                None => {
                    plain.push(ch);
                    at += 1;
                }
            }
        }
        self.flush_text(&mut plain);
    }

    // Flush buffered plain text as one event
    fn flush_text(&mut self, plain: &mut String) {
        if !plain.is_empty() {
            self.out
                .push(Event::Text(CowStr::from(std::mem::take(plain))));
        }
    }

    // Emit markup between a mark and its closing twin
    fn wrapped(
        &mut self,
        chars: &[char],
        at: usize,
        mark: &[char],
        plain: &mut String,
        wrap: Wrap,
    ) -> Option<usize> {
        let start = at + mark.len();
        let close = find_close(chars, start, mark)?;
        let inner: String = chars[start..close].iter().collect();
        self.flush_text(plain);
        self.wrap(wrap, inner);
        Some(close + mark.len())
    }

    // Emit inner text in the given inline style
    fn wrap(&mut self, wrap: Wrap, inner: String) {
        match wrap {
            Wrap::Code => self.out.push(Event::Code(CowStr::from(inner))),
            Wrap::Strong | Wrap::Emphasis => {
                let (open, close) = match wrap {
                    Wrap::Strong => (Tag::Strong, TagEnd::Strong),
                    _ => (Tag::Emphasis, TagEnd::Emphasis),
                };
                self.out.push(Event::Start(open));
                self.out.push(Event::Text(CowStr::from(inner)));
                self.out.push(Event::End(close));
            }
        }
    }

    // Emit `text`, a reference `text`_ or a hyperlink `text <url>`_
    fn interpreted(&mut self, chars: &[char], at: usize, plain: &mut String) -> Option<usize> {
        let close = find_close(chars, at + 1, &INTERPRETED_MARK)?;
        let inner: String = chars[at + 1..close].iter().collect();
        let mut next = close + 1;
        let mut reference = false;
        while chars.get(next) == Some(&REFERENCE_MARK) && next <= close + 2 {
            reference = true;
            next += 1;
        }
        self.flush_text(plain);
        if !reference {
            self.wrap(Wrap::Emphasis, inner);
            return Some(next);
        }
        match embedded_url(&inner) {
            Some((label, url)) => {
                let link_type = if label.is_empty() {
                    LinkType::Autolink
                } else {
                    LinkType::Inline
                };
                let shown = if label.is_empty() { url } else { label };
                self.out.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: CowStr::from(url.to_owned()),
                    title: CowStr::from(""),
                    id: CowStr::from(""),
                }));
                self.out.push(Event::Text(CowStr::from(shown.to_owned())));
                self.out.push(Event::End(TagEnd::Link));
            }
            None => self.out.push(Event::Text(CowStr::from(inner))),
        }
        Some(next)
    }

    // Emit a role such as :code:`x` as code, keeping only the title of `title <target>`
    fn role(&mut self, chars: &[char], at: usize, plain: &mut String) -> Option<usize> {
        let name_end = (at + 1..chars.len()).find(|&index| {
            !(chars[index].is_alphanumeric() || ROLE_NAME_EXTRAS.contains(chars[index]))
        })?;
        let opens = name_end > at + 1
            && chars[name_end] == ROLE_MARK
            && chars.get(name_end + 1) == Some(&BACKTICK);
        if !opens {
            return None;
        }
        let start = name_end + 2;
        let close = find_close(chars, start, &INTERPRETED_MARK)?;
        let inner: String = chars[start..close].iter().collect();
        let shown = match embedded_url(&inner) {
            Some((label, _)) if !label.is_empty() => label.to_owned(),
            _ => inner,
        };
        self.flush_text(plain);
        self.out.push(Event::Code(CowStr::from(shown)));
        Some(close + 1)
    }
}

#[derive(Clone, Copy)]
enum Wrap {
    Code,
    Strong,
    Emphasis,
}
