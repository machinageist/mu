// Author: machinageist
// Date: 2026-11-09
// Description: Org-mode documents to pulldown-cmark events
// Notes: A tolerant line-based parser; blocks nest by re-parsing their dedented body lines,
// and every event owns its text so the output does not borrow from the input

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd,
};

// Keyword lines
const KEYWORD_PREFIX: &str = "#+";
const KEYWORD_TITLE: &str = "TITLE";
const KEYWORD_AUTHOR: &str = "AUTHOR";
const KEYWORD_DATE: &str = "DATE";
const BYLINE_SEPARATOR: &str = " · ";

// Blocks, matched case-insensitively
const BEGIN_PREFIX: &str = "#+begin_";
const END_PREFIX: &str = "#+end_";
const BLOCK_SRC: &str = "src";
const BLOCK_EXAMPLE: &str = "example";
const BLOCK_QUOTE: &str = "quote";
const BLOCK_VERSE: &str = "verse";
const BLOCK_COMMENT: &str = "comment";
const SPECIAL_BLOCKS: [(&str, BlockQuoteKind); 5] = [
    ("note", BlockQuoteKind::Note),
    ("tip", BlockQuoteKind::Tip),
    ("important", BlockQuoteKind::Important),
    ("warning", BlockQuoteKind::Warning),
    ("caution", BlockQuoteKind::Caution),
];
const ESCAPE_COMMA: char = ',';

// Comments, fixed-width lines and drawers
const COMMENT_MARK: &str = "#";
const COMMENT_PREFIX: &str = "# ";
const FIXED_MARK: &str = ":";
const FIXED_PREFIX: &str = ": ";
const DRAWER_END: &str = ":END:";
const PROPERTIES_DRAWER: &str = ":PROPERTIES:";

// Headings
const HEADING_STAR: char = '*';
const MAX_HEADING_LEVEL: usize = 6;
const TODO_KEYWORDS: [&str; 6] = ["TODO", "DONE", "NEXT", "WAITING", "CANCELLED", "CANCELED"];
const PLANNING_WORDS: [&str; 3] = ["SCHEDULED:", "DEADLINE:", "CLOSED:"];
const TAG_MARK: char = ':';
const TAG_EXTRA_CHARS: &str = "_@#%";

// Lists
const BULLET_MARKS: [char; 2] = ['-', '+'];
const ORDERED_DELIMS: [char; 2] = ['.', ')'];
const MAX_ORDER_DIGITS: usize = 9;
const CHECKBOX_DONE: [&str; 2] = ["[X]", "[x]"];
const CHECKBOX_OPEN: [&str; 2] = ["[ ]", "[-]"];
const DESCRIPTION_SEPARATOR: &str = " :: ";
const DESCRIPTION_TAIL: &str = " ::";

// Tables and rules
const TABLE_BAR: char = '|';
const TABLE_RULE: &str = "|-";
const RULE_CHAR: char = '-';
const MIN_RULE_LENGTH: usize = 5;

// Inline syntax
const LINK_OPEN: &str = "[[";
const LINK_CLOSE: &str = "]]";
const LINK_SPLIT: &str = "][";
const SNIFF_LINK_MARKERS: [char; 3] = [':', '/', '.'];
const FOOTNOTE_PREFIX: &str = "[fn:";
const FOOTNOTE_CLOSE: char = ']';
const FILE_PREFIX: &str = "file:";
const IMAGE_EXTENSIONS: [&str; 7] = [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".bmp"];
const EMPHASIS_MARKS: &str = "*/_+=~";
const EMPHASIS_PRE: &str = "-('\"{";
const EMPHASIS_POST: &str = "-.,;:!?')\"}[";

// Check whether the head of an input looks like Org-mode
pub fn sniff(head: &str) -> bool {
    let mut has_heading = false;
    let mut has_link = false;
    for line in head.lines() {
        let trimmed = line.trim();
        if keyword(trimmed).is_some()
            || block_name(trimmed).is_some()
            || trimmed == PROPERTIES_DRAWER
        {
            return true;
        }
        has_heading |= heading_level(line).is_some();
        has_link |= has_sniff_link(line);
    }
    has_heading && has_link
}

// Find an unmistakable Org link: described, or aimed at a URL or file
// Notes: a bare `[[identifier]]` is also an AsciiDoc anchor, so it proves nothing
fn has_sniff_link(line: &str) -> bool {
    let Some(start) = line.find(LINK_OPEN) else {
        return false;
    };
    let rest = &line[start + LINK_OPEN.len()..];
    let Some(end) = rest.find(LINK_CLOSE) else {
        return false;
    };
    let target = &rest[..end];
    target.contains(LINK_SPLIT) || target.contains(SNIFF_LINK_MARKERS)
}

// Translate Org-mode into document events
pub fn events(input: &str) -> Vec<Event<'_>> {
    let lines: Vec<&str> = input.lines().collect();
    let mut out = Vec::new();
    let mut title = None;
    let mut byline = Vec::new();
    for line in &lines {
        if let Some((key, value)) = keyword(line.trim()) {
            if key.eq_ignore_ascii_case(KEYWORD_TITLE) && title.is_none() {
                title = Some(value.to_owned());
            } else if key.eq_ignore_ascii_case(KEYWORD_AUTHOR)
                || key.eq_ignore_ascii_case(KEYWORD_DATE)
            {
                byline.push(value.to_owned());
            }
        }
    }
    let shift = usize::from(title.is_some());

    // Document header from the title, author and date keywords
    if let Some(title) = title.filter(|title| !title.is_empty()) {
        let level = HeadingLevel::H1;
        out.push(Event::Start(heading_tag(level)));
        inline(&title, &mut out);
        out.push(Event::End(TagEnd::Heading(level)));
    }
    byline.retain(|part| !part.is_empty());
    if !byline.is_empty() {
        out.push(Event::Start(Tag::Paragraph));
        out.push(text(byline.join(BYLINE_SEPARATOR)));
        out.push(Event::End(TagEnd::Paragraph));
    }

    Parser {
        lines,
        pos: 0,
        shift,
        headings: true,
        out,
    }
    .run()
}

// Block parser over a slice of lines
struct Parser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
    shift: usize,
    headings: bool,
    out: Vec<Event<'static>>,
}

impl<'a> Parser<'a> {
    // Parse every line into block events
    fn run(mut self) -> Vec<Event<'static>> {
        while self.pos < self.lines.len() {
            self.block();
        }
        self.out
    }

    // Parse nested lines with the same settings, minus headings when inside a list item
    fn nested(&self, lines: Vec<&str>, headings: bool) -> Vec<Event<'static>> {
        Parser {
            lines,
            pos: 0,
            shift: self.shift,
            headings,
            out: Vec::new(),
        }
        .run()
    }

    // Parse the block starting at the current line
    fn block(&mut self) {
        let line = self.lines[self.pos];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            self.pos += 1;
        } else if let Some(level) = heading_level(line).filter(|_| self.headings) {
            self.heading(line, level);
        } else if let Some(name) = block_name(trimmed) {
            self.special_block(&name, trimmed);
        } else if trimmed.starts_with(KEYWORD_PREFIX) || is_comment(trimmed) || is_planning(trimmed)
        {
            self.pos += 1;
        } else if is_drawer_start(trimmed)
            && let Some(end) = self.drawer_end()
        {
            self.pos = end + 1;
        } else if is_fixed(trimmed) {
            self.fixed_width();
        } else if is_rule(trimmed) {
            self.out.push(Event::Rule);
            self.pos += 1;
        } else if trimmed.starts_with(TABLE_BAR) {
            self.table();
        } else if list_marker(line).is_some() {
            self.list();
        } else if let Some((label, rest)) = footnote_definition(line) {
            self.footnote(label, rest);
        } else {
            self.paragraph();
        }
    }

    // Find the :END: line closing the drawer at the current line
    fn drawer_end(&self) -> Option<usize> {
        (self.pos + 1..self.lines.len())
            .find(|&index| {
                let trimmed = self.lines[index].trim();
                trimmed.eq_ignore_ascii_case(DRAWER_END)
                    || heading_level(self.lines[index]).is_some()
            })
            .filter(|&index| self.lines[index].trim().eq_ignore_ascii_case(DRAWER_END))
    }

    // Emit a heading with its TODO keyword, text and tags
    fn heading(&mut self, line: &str, stars: usize) {
        self.pos += 1;
        let level = heading_from(stars + self.shift);
        let mut rest = line[stars..].trim();
        self.out.push(Event::Start(heading_tag(level)));

        // Leading TODO keyword as bold text
        let (word, after) = rest.split_once(' ').unwrap_or((rest, ""));
        if TODO_KEYWORDS.contains(&word) {
            self.out.push(Event::Start(Tag::Strong));
            self.out.push(text(word.to_owned()));
            self.out.push(Event::End(TagEnd::Strong));
            rest = after.trim_start();
            if !rest.is_empty() {
                self.out.push(text(" ".to_owned()));
            }
        }

        // Trailing :tag:tags: as code
        let (body, tags) = split_tags(rest);
        inline(body, &mut self.out);
        if let Some(tags) = tags {
            if !body.is_empty() {
                self.out.push(text(" ".to_owned()));
            }
            self.out.push(Event::Code(CowStr::from(tags.to_owned())));
        }
        self.out.push(Event::End(TagEnd::Heading(level)));
    }

    // Emit a #+BEGIN_… block by kind
    fn special_block(&mut self, name: &str, first: &str) {
        let start = self.pos + 1;
        let end_marker = format!("{END_PREFIX}{name}");
        let end = (start..self.lines.len())
            .find(|&index| {
                let trimmed = self.lines[index].trim();
                trimmed.len() >= end_marker.len()
                    && trimmed[..end_marker.len()].eq_ignore_ascii_case(&end_marker)
            })
            .unwrap_or(self.lines.len());
        self.pos = (end + 1).min(self.lines.len()).max(start);
        let body = dedent(&self.lines[start..end]);
        let args = first[BEGIN_PREFIX.len() + name.len()..].trim();

        if name == BLOCK_COMMENT {
            return;
        }
        if name == BLOCK_SRC || name == BLOCK_EXAMPLE {
            let lang = if name == BLOCK_SRC {
                args.split_whitespace().next().unwrap_or("")
            } else {
                ""
            };
            let mut code = String::new();
            for line in body {
                code.push_str(unescape(line));
                code.push('\n');
            }
            self.code_block(lang, code);
        } else if name == BLOCK_VERSE {
            self.out.push(Event::Start(Tag::Paragraph));
            let mut first_line = true;
            for line in body.iter().filter(|line| !line.trim().is_empty()) {
                if !first_line {
                    self.out.push(Event::HardBreak);
                }
                first_line = false;
                inline(line.trim(), &mut self.out);
            }
            self.out.push(Event::End(TagEnd::Paragraph));
        } else {
            let kind = SPECIAL_BLOCKS
                .iter()
                .find(|(special, _)| *special == name)
                .map(|(_, kind)| *kind);
            let quoted = kind.is_some() || name == BLOCK_QUOTE;
            let inner = self.nested(body, self.headings);
            if quoted {
                self.out.push(Event::Start(Tag::BlockQuote(kind)));
                self.out.extend(inner);
                self.out.push(Event::End(TagEnd::BlockQuote(kind)));
            } else {
                self.out.extend(inner);
            }
        }
    }

    // Emit a fenced code block
    fn code_block(&mut self, lang: &str, code: String) {
        self.out
            .push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(
                CowStr::from(lang.to_owned()),
            ))));
        self.out.push(text(code));
        self.out.push(Event::End(TagEnd::CodeBlock));
    }

    // Collect consecutive `: ` lines into a code block
    fn fixed_width(&mut self) {
        let mut code = String::new();
        while let Some(line) = self.lines.get(self.pos).map(|line| line.trim())
            && is_fixed(line)
        {
            code.push_str(line.strip_prefix(FIXED_PREFIX).unwrap_or(""));
            code.push('\n');
            self.pos += 1;
        }
        self.code_block("", code);
    }

    // Emit a table with its first row as the head, since Markdown tables hold one head row
    fn table(&mut self) {
        let mut rows: Vec<Vec<String>> = Vec::new();
        while let Some(line) = self.lines.get(self.pos).map(|line| line.trim())
            && line.starts_with(TABLE_BAR)
        {
            self.pos += 1;
            if line.starts_with(TABLE_RULE) {
                continue;
            }
            let inner = line[1..].strip_suffix(TABLE_BAR).unwrap_or(&line[1..]);
            rows.push(
                inner
                    .split(TABLE_BAR)
                    .map(|cell| cell.trim().to_owned())
                    .collect(),
            );
        }
        if rows.is_empty() {
            return;
        }
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        self.out
            .push(Event::Start(Tag::Table(vec![Alignment::None; columns])));
        for (index, row) in rows.iter().enumerate() {
            let is_head = index == 0;
            if is_head {
                self.out.push(Event::Start(Tag::TableHead));
            } else {
                self.out.push(Event::Start(Tag::TableRow));
            }
            for column in 0..columns {
                self.out.push(Event::Start(Tag::TableCell));
                if let Some(cell) = row.get(column) {
                    inline(cell, &mut self.out);
                }
                self.out.push(Event::End(TagEnd::TableCell));
            }
            if is_head {
                self.out.push(Event::End(TagEnd::TableHead));
            } else {
                self.out.push(Event::End(TagEnd::TableRow));
            }
        }
        self.out.push(Event::End(TagEnd::Table));
    }

    // Emit a list of items sharing the current item's indent and kind
    fn list(&mut self) {
        let Some(first) = list_marker(self.lines[self.pos]) else {
            return;
        };
        let indent = first.indent;
        let ordered = first.number;
        let mut items: Vec<(Marker, Vec<&str>)> = Vec::new();
        let mut loose = false;

        // Gather each item's dedented body lines
        while let Some(marker) = self.lines.get(self.pos).and_then(|line| list_marker(line))
            && marker.indent == indent
            && marker.number.is_some() == ordered.is_some()
        {
            let mut body = vec![marker.content];
            let content_col = self.lines[self.pos].len() - marker.content.len();
            self.pos += 1;
            let mut blanks = 0;
            while let Some(line) = self.lines.get(self.pos) {
                let line_indent = indent_of(line);
                if line.trim().is_empty() {
                    blanks += 1;
                    if blanks >= 2 {
                        break;
                    }
                } else if line_indent > indent {
                    if blanks > 0 {
                        loose = true;
                        body.push("");
                    }
                    blanks = 0;
                    body.push(&line[line_indent.min(content_col)..]);
                } else {
                    break;
                }
                self.pos += 1;
            }
            let next_is_sibling = self
                .lines
                .get(self.pos)
                .and_then(|line| list_marker(line))
                .is_some_and(|next| {
                    next.indent == indent && next.number.is_some() == ordered.is_some()
                });
            if blanks == 1 && next_is_sibling {
                loose = true;
            }
            items.push((marker, body));
            if blanks >= 2 {
                break;
            }
        }

        // Emit the items
        self.out.push(Event::Start(Tag::List(ordered)));
        for (marker, mut body) in items {
            self.out.push(Event::Start(Tag::Item));
            if let Some(done) = marker.checkbox {
                self.out.push(Event::TaskListMarker(done));
            }
            if let Some(desc) = marker.description {
                body[0] = desc;
            }
            let mut inner = self.nested(body, false);
            if !loose {
                inner = unwrap_paragraphs(inner);
            }

            // Description term in bold ahead of its text
            if let Some(term) = marker.term {
                let at = usize::from(matches!(inner.first(), Some(Event::Start(Tag::Paragraph))));
                let mut bold = vec![Event::Start(Tag::Strong)];
                inline(term, &mut bold);
                bold.push(Event::End(TagEnd::Strong));
                if inner.get(at).is_some_and(|next| !is_block_edge(next)) {
                    bold.push(text(" ".to_owned()));
                }
                inner.splice(at..at, bold);
            }
            self.out.extend(inner);
            self.out.push(Event::End(TagEnd::Item));
        }
        self.out.push(Event::End(TagEnd::List(ordered.is_some())));
    }

    // Emit a footnote definition and its continuation lines
    fn footnote(&mut self, label: &str, rest: &str) {
        self.pos += 1;
        let mut body = String::from(rest.trim());
        self.extend_paragraph(&mut body);
        self.out
            .push(Event::Start(Tag::FootnoteDefinition(CowStr::from(
                label.to_owned(),
            ))));
        self.out.push(Event::Start(Tag::Paragraph));
        inline(&body, &mut self.out);
        self.out.push(Event::End(TagEnd::Paragraph));
        self.out.push(Event::End(TagEnd::FootnoteDefinition));
    }

    // Emit consecutive plain lines as one paragraph
    fn paragraph(&mut self) {
        let mut body = String::from(self.lines[self.pos].trim());
        self.pos += 1;
        self.extend_paragraph(&mut body);
        self.out.push(Event::Start(Tag::Paragraph));
        inline(&body, &mut self.out);
        self.out.push(Event::End(TagEnd::Paragraph));
    }

    // Append following lines until a blank line or another block starts
    fn extend_paragraph(&mut self, body: &mut String) {
        while let Some(line) = self.lines.get(self.pos)
            && !self.starts_block(line)
        {
            body.push('\n');
            body.push_str(line.trim());
            self.pos += 1;
        }
    }

    // Check whether a line ends a paragraph
    fn starts_block(&self, line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.is_empty()
            || (self.headings && heading_level(line).is_some())
            || trimmed.starts_with(KEYWORD_PREFIX)
            || is_comment(trimmed)
            || is_fixed(trimmed)
            || is_rule(trimmed)
            || trimmed.starts_with(TABLE_BAR)
            || list_marker(line).is_some()
            || footnote_definition(line).is_some()
    }
}

// A list item's marker and what follows it
struct Marker<'a> {
    indent: usize,
    number: Option<u64>,
    checkbox: Option<bool>,
    term: Option<&'a str>,
    description: Option<&'a str>,
    content: &'a str,
}

// Parse a list item marker, checkbox and description term
fn list_marker(line: &str) -> Option<Marker<'_>> {
    let indent = indent_of(line);
    let rest = &line[indent..];
    let (number, after) = if let Some(after) = rest.strip_prefix(BULLET_MARKS) {
        (None, after)
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > MAX_ORDER_DIGITS {
            return None;
        }
        let after = rest[digits..].strip_prefix(ORDERED_DELIMS)?;
        (rest[..digits].parse().ok(), after)
    };
    if !(after.is_empty() || after.starts_with(' ')) {
        return None;
    }
    let mut content = after.trim_start();

    // Checkbox
    let mut checkbox = None;
    for (boxes, done) in [(CHECKBOX_DONE, true), (CHECKBOX_OPEN, false)] {
        if let Some(found) = boxes.iter().find(|mark| content.starts_with(**mark)) {
            checkbox = Some(done);
            content = content[found.len()..].trim_start();
        }
    }

    // Description term, bullets only
    let mut term = None;
    let mut description = None;
    if number.is_none() {
        if let Some((left, right)) = content.split_once(DESCRIPTION_SEPARATOR) {
            term = Some(left.trim());
            description = Some(right.trim());
        } else if let Some(left) = content.strip_suffix(DESCRIPTION_TAIL) {
            term = Some(left.trim());
            description = Some("");
        }
    }
    Some(Marker {
        indent,
        number,
        checkbox,
        term,
        description,
        content,
    })
}

// Check whether an event opens or closes a block rather than inline content
fn is_block_edge(event: &Event) -> bool {
    match event {
        Event::Start(tag) => matches!(
            tag,
            Tag::Paragraph
                | Tag::List(_)
                | Tag::CodeBlock(_)
                | Tag::BlockQuote(_)
                | Tag::Table(_)
                | Tag::FootnoteDefinition(_)
        ),
        Event::End(_) => true,
        Event::Rule => true,
        _ => false,
    }
}

// Drop the paragraph tags that sit directly in a tight list item
fn unwrap_paragraphs(events: Vec<Event<'static>>) -> Vec<Event<'static>> {
    let mut depth = 0usize;
    let mut out = Vec::with_capacity(events.len());
    for event in events {
        match &event {
            Event::Start(Tag::Paragraph) if depth == 0 => continue,
            Event::End(TagEnd::Paragraph) if depth == 0 => continue,
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        out.push(event);
    }
    out
}

// Count heading stars when the line is an outline heading
fn heading_level(line: &str) -> Option<usize> {
    let stars = line.chars().take_while(|&ch| ch == HEADING_STAR).count();
    let after = &line[stars..];
    (stars > 0 && (after.is_empty() || after.starts_with(' '))).then_some(stars)
}

// Clamp an outline depth to a heading level
fn heading_from(depth: usize) -> HeadingLevel {
    HeadingLevel::try_from(depth.min(MAX_HEADING_LEVEL)).unwrap_or(HeadingLevel::H6)
}

// Build a heading tag with no attributes
fn heading_tag(level: HeadingLevel) -> Tag<'static> {
    Tag::Heading {
        level,
        id: None,
        classes: vec![],
        attrs: vec![],
    }
}

// Split trailing :tag:tags: from heading text
fn split_tags(text: &str) -> (&str, Option<&str>) {
    let start = text.rfind(' ').map_or(0, |space| space + 1);
    let last = &text[start..];
    let is_tags = last.len() > 2
        && last.starts_with(TAG_MARK)
        && last.ends_with(TAG_MARK)
        && last.split(TAG_MARK).all(|tag| {
            tag.chars()
                .all(|ch| ch.is_alphanumeric() || TAG_EXTRA_CHARS.contains(ch))
        })
        && !last.contains("::");
    if is_tags {
        (text[..start].trim_end(), Some(last))
    } else {
        (text, None)
    }
}

// Parse a #+KEYWORD: value line
fn keyword(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix(KEYWORD_PREFIX)?;
    let (key, value) = rest.split_once(':')?;
    let valid = !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
    valid.then(|| (key, value.trim()))
}

// Lowercased name of a #+BEGIN_… line
fn block_name(line: &str) -> Option<String> {
    let head = line.get(..BEGIN_PREFIX.len())?;
    if !head.eq_ignore_ascii_case(BEGIN_PREFIX) {
        return None;
    }
    let name: String = line[BEGIN_PREFIX.len()..]
        .chars()
        .take_while(|ch| !ch.is_whitespace())
        .collect();
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

// Check for a `# ` comment line
fn is_comment(line: &str) -> bool {
    line == COMMENT_MARK || line.starts_with(COMMENT_PREFIX)
}

// Check for a SCHEDULED/DEADLINE/CLOSED planning line
fn is_planning(line: &str) -> bool {
    PLANNING_WORDS.iter().any(|word| line.starts_with(word))
}

// Check for a `: ` fixed-width line
fn is_fixed(line: &str) -> bool {
    line == FIXED_MARK || line.starts_with(FIXED_PREFIX)
}

// Check for a drawer opening like :PROPERTIES: or :LOGBOOK:
fn is_drawer_start(line: &str) -> bool {
    line.len() > 2
        && line.starts_with(TAG_MARK)
        && line.ends_with(TAG_MARK)
        && !line.eq_ignore_ascii_case(DRAWER_END)
        && line[1..line.len() - 1]
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
}

// Check for a horizontal rule of five or more dashes
fn is_rule(line: &str) -> bool {
    line.len() >= MIN_RULE_LENGTH && line.chars().all(|ch| ch == RULE_CHAR)
}

// Parse `[fn:label] text` at the start of a line
fn footnote_definition(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix(FOOTNOTE_PREFIX)?;
    let (label, text) = rest.split_once(FOOTNOTE_CLOSE)?;
    let valid = !label.is_empty() && !label.contains(':') && !label.contains(' ');
    valid.then_some((label, text))
}

// Count leading whitespace bytes
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

// Strip the indent shared by every non-blank line
fn dedent<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let common = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| indent_of(line))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.get(common..).unwrap_or(""))
        .collect()
}

// Remove the comma that escapes `*` or `#+` at the start of a code line
fn unescape(line: &str) -> &str {
    match line.strip_prefix(ESCAPE_COMMA) {
        Some(rest) if rest.starts_with(HEADING_STAR) || rest.starts_with(KEYWORD_PREFIX) => rest,
        _ => line,
    }
}

// Make an owned text event
fn text(value: String) -> Event<'static> {
    Event::Text(CowStr::from(value))
}

// Check whether a link target is an image file
fn is_image(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

// Translate inline Org markup into events, newlines becoming soft breaks
fn inline(source: &str, out: &mut Vec<Event<'static>>) {
    let mut buffer = String::new();
    let mut index = 0;
    let mut prev: Option<char> = None;
    while let Some(ch) = source[index..].chars().next() {
        let rest = &source[index..];

        // Links and images
        if rest.starts_with(LINK_OPEN)
            && let Some(close) = rest.find(LINK_CLOSE)
        {
            flush(&mut buffer, out);
            link(&rest[LINK_OPEN.len()..close], out);
            index += close + LINK_CLOSE.len();
            prev = Some(']');
            continue;
        }

        // Footnote references
        if let Some(after) = rest.strip_prefix(FOOTNOTE_PREFIX)
            && let Some(close) = after.find(FOOTNOTE_CLOSE)
            && !after[..close].is_empty()
            && !after[..close].contains([':', ' '])
        {
            flush(&mut buffer, out);
            out.push(Event::FootnoteReference(CowStr::from(
                after[..close].to_owned(),
            )));
            index += FOOTNOTE_PREFIX.len() + close + 1;
            prev = Some(FOOTNOTE_CLOSE);
            continue;
        }

        // Emphasis and verbatim
        if EMPHASIS_MARKS.contains(ch)
            && let Some(len) = emphasis_len(rest, ch, prev)
        {
            flush(&mut buffer, out);
            let content = &rest[1..1 + len];
            match ch {
                '=' | '~' => out.push(Event::Code(CowStr::from(content.to_owned()))),
                _ => {
                    let (open, close) = match ch {
                        '*' => (Tag::Strong, TagEnd::Strong),
                        '+' => (Tag::Strikethrough, TagEnd::Strikethrough),
                        _ => (Tag::Emphasis, TagEnd::Emphasis),
                    };
                    out.push(Event::Start(open));
                    inline(content, out);
                    out.push(Event::End(close));
                }
            }
            index += len + 2;
            prev = Some(ch);
            continue;
        }

        // Plain text, newlines as soft breaks
        if ch == '\n' {
            flush(&mut buffer, out);
            out.push(Event::SoftBreak);
        } else {
            buffer.push(ch);
        }
        index += ch.len_utf8();
        prev = Some(ch);
    }
    flush(&mut buffer, out);
}

// Length of emphasis content when `mark` at the start of `rest` opens a valid span
fn emphasis_len(rest: &str, mark: char, prev: Option<char>) -> Option<usize> {
    let pre_ok = prev.is_none_or(|ch| ch.is_whitespace() || EMPHASIS_PRE.contains(ch));
    let body = &rest[1..];
    let first = body.chars().next()?;
    if !pre_ok || first.is_whitespace() {
        return None;
    }
    let len = body.find(mark)?;
    let content = &body[..len];
    let last = content.chars().next_back()?;
    let post = body[len + 1..].chars().next();
    let post_ok = post.is_none_or(|ch| ch.is_whitespace() || EMPHASIS_POST.contains(ch));
    (post_ok && !last.is_whitespace() && !content.contains('\n')).then_some(len)
}

// Emit a link or image from the inside of [[…]]
fn link(inner: &str, out: &mut Vec<Event<'static>>) {
    let (target, label) = match inner.split_once(LINK_SPLIT) {
        Some((target, label)) => (target, Some(label)),
        None => (inner, None),
    };
    let url = target
        .strip_prefix(FILE_PREFIX)
        .unwrap_or(target)
        .to_owned();
    if label.is_none() && is_image(&url) {
        out.push(Event::Start(Tag::Image {
            link_type: LinkType::Inline,
            dest_url: CowStr::from(url.clone()),
            title: CowStr::from(""),
            id: CowStr::from(""),
        }));
        out.push(text(url));
        out.push(Event::End(TagEnd::Image));
        return;
    }
    let link_type = if label.is_some() {
        LinkType::Inline
    } else {
        LinkType::Autolink
    };
    out.push(Event::Start(Tag::Link {
        link_type,
        dest_url: CowStr::from(url.clone()),
        title: CowStr::from(""),
        id: CowStr::from(""),
    }));
    match label {
        Some(label) => inline(label, out),
        None => out.push(text(url)),
    }
    out.push(Event::End(TagEnd::Link));
}

// Move buffered text into a text event
fn flush(buffer: &mut String, out: &mut Vec<Event<'static>>) {
    if !buffer.is_empty() {
        out.push(text(std::mem::take(buffer)));
    }
}
