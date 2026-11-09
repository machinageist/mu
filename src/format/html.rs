// Author: machinageist
// Date: 2026-11-09
// Description: HTML documents to pulldown-cmark events
// Notes: A tolerant tokenizer builds a small element tree with browser-style implicit closes,
// then a walker maps elements onto the events Markdown would produce for the same document

use pulldown_cmark::{
    Alignment, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd,
};

// Byte order mark allowed before the document
const BOM: char = '\u{feff}';

// Openings that mark a document as HTML, matched case-insensitively
const SNIFF_PREFIXES: [&str; 4] = ["<!doctype html", "<html", "<head", "<body"];
const XML_DECL_OPEN: &str = "<?xml";
const XML_DECL_CLOSE: &str = "?>";

// Markup delimiters
const COMMENT_OPEN: &str = "<!--";
const COMMENT_CLOSE: &str = "-->";
const DECL_OPEN: &str = "<!";
const PI_OPEN: &str = "<?";
const END_TAG_OPEN: &str = "</";
const TAG_OPEN: &str = "<";
const TAG_CLOSE: u8 = b'>';
const SELF_CLOSE: u8 = b'/';
const ATTR_EQUALS: u8 = b'=';
const QUOTES: [u8; 2] = *b"\"'";

// Entities
const ENTITY_OPEN: char = '&';
const ENTITY_CLOSE: u8 = b';';
const ENTITY_MAX_LEN: usize = 12;
const NUMERIC_MARK: char = '#';
const HEX_MARKS: [char; 2] = ['x', 'X'];
const HEX_RADIX: u32 = 16;
const NO_BREAK_SPACE: char = '\u{a0}';
const SPACE: char = ' ';
const NAMED_ENTITIES: [(&str, char); 23] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", SPACE),
    ("mdash", '—'),
    ("ndash", '–'),
    ("hellip", '…'),
    ("copy", '©'),
    ("reg", '®'),
    ("trade", '™'),
    ("laquo", '«'),
    ("raquo", '»'),
    ("lsquo", '‘'),
    ("rsquo", '’'),
    ("ldquo", '“'),
    ("rdquo", '”'),
    ("bull", '•'),
    ("middot", '·'),
    ("times", '×'),
    ("ensp", ' '),
    ("emsp", ' '),
];

// Tag names the walker maps to events
const P: &str = "p";
const BR: &str = "br";
const HR: &str = "hr";
const UL: &str = "ul";
const OL: &str = "ol";
const LI: &str = "li";
const DL: &str = "dl";
const DT: &str = "dt";
const DD: &str = "dd";
const BLOCKQUOTE: &str = "blockquote";
const PRE: &str = "pre";
const CODE: &str = "code";
const KBD: &str = "kbd";
const SAMP: &str = "samp";
const TT: &str = "tt";
const STRONG: &str = "strong";
const B: &str = "b";
const EM: &str = "em";
const I: &str = "i";
const DEL: &str = "del";
const S: &str = "s";
const STRIKE: &str = "strike";
const A: &str = "a";
const IMG: &str = "img";
const TABLE: &str = "table";
const CAPTION: &str = "caption";
const THEAD: &str = "thead";
const TBODY: &str = "tbody";
const TFOOT: &str = "tfoot";
const TR: &str = "tr";
const TH: &str = "th";
const TD: &str = "td";
const HEAD: &str = "head";
const TITLE: &str = "title";
const SVG: &str = "svg";
const H1: &str = "h1";
const HEADINGS: [(&str, HeadingLevel); 6] = [
    (H1, HeadingLevel::H1),
    ("h2", HeadingLevel::H2),
    ("h3", HeadingLevel::H3),
    ("h4", HeadingLevel::H4),
    ("h5", HeadingLevel::H5),
    ("h6", HeadingLevel::H6),
];

// Attributes the walker reads
const HREF: &str = "href";
const SRC: &str = "src";
const ALT: &str = "alt";
const CLASS: &str = "class";
const START: &str = "start";
const LANGUAGE_PREFIXES: [&str; 2] = ["language-", "lang-"];
const DEFAULT_START: u64 = 1;

// Whitespace handling
const WORD_GAP: &str = " ";
const LINE_END: char = '\n';
const LEADING_NEWLINES: [&str; 2] = ["\r\n", "\n"];

// Elements with no content or end tag
const VOID: [&str; 15] = [
    "area", "base", BR, "col", "embed", HR, IMG, "input", "link", "meta", "param", "source",
    "track", "wbr", "keygen",
];

// Elements whose content is raw text up to the matching end tag
const RAW_TEXT: [&str; 6] = ["script", "style", "textarea", TITLE, "iframe", "xmp"];

// Elements whose content is never shown
const DROPPED: [&str; 13] = [
    "script", "style", HEAD, TITLE, "noscript", "template", SVG, "iframe", "input", "select",
    "textarea", "button", "math",
];

// Elements allowed inside head; any other start tag closes it
const HEAD_CONTENT: [&str; 8] = [
    TITLE, "meta", "link", "style", "script", "base", "noscript", "template",
];

// Blocks that only separate paragraphs and pass their content through
const TRANSPARENT_BLOCKS: [&str; 20] = [
    "html",
    "body",
    "div",
    "section",
    "article",
    "main",
    "header",
    "footer",
    "nav",
    "aside",
    "form",
    "figure",
    "figcaption",
    "address",
    "center",
    "details",
    "summary",
    "fieldset",
    "hgroup",
    "legend",
];

// Blocks with their own mapping
const MAPPED_BLOCKS: [&str; 24] = [
    P, H1, "h2", "h3", "h4", "h5", "h6", UL, OL, LI, BLOCKQUOTE, PRE, HR, TABLE, CAPTION, THEAD,
    TBODY, TFOOT, TR, TH, TD, DL, DT, DD,
];

// Implicit-close groups
const LISTS: [&str; 2] = [UL, OL];
const DEFINITION_PARTS: [&str; 2] = [DT, DD];
const CELLS: [&str; 2] = [TD, TH];
const ROW_GROUPS: [&str; 3] = [THEAD, TBODY, TFOOT];
const P_SCOPE_STOPS: [&str; 3] = [TD, TH, TABLE];

// One parsed node
enum Node {
    Text(String),
    Element(Element),
}

// One parsed element with lowercase name and decoded attributes
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

impl Element {
    // Create an empty element
    fn new(name: String, attrs: Vec<(String, String)>) -> Element {
        Element {
            name,
            attrs,
            children: Vec::new(),
        }
    }

    // Look up an attribute value
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    // Iterate child elements, skipping text
    fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }
}

// Check whether the head of an input looks like HTML
pub fn sniff(head: &str) -> bool {
    let mut rest = head.trim_start_matches(BOM);
    let mut seen_xml = false;
    // Skip leading comments and one XML declaration
    loop {
        rest = rest.trim_start();
        let skip = if starts_with_ignore_case(rest, COMMENT_OPEN) {
            COMMENT_CLOSE
        } else if !seen_xml && starts_with_ignore_case(rest, XML_DECL_OPEN) {
            seen_xml = true;
            XML_DECL_CLOSE
        } else {
            break;
        };
        match rest.find(skip) {
            Some(end) => rest = &rest[end + skip.len()..],
            None => return false,
        }
    }
    SNIFF_PREFIXES.iter().any(|prefix| {
        starts_with_ignore_case(rest, prefix)
            && !rest
                .as_bytes()
                .get(prefix.len())
                .is_some_and(u8::is_ascii_alphanumeric)
    })
}

// Translate HTML into document events
pub fn events(input: &str) -> Vec<Event<'_>> {
    let root = parse(input);
    let mut out = Vec::new();
    // The title stands in for a missing top-level heading
    if find(&root, H1).is_none()
        && let Some(title) = find(&root, TITLE)
    {
        let mut run = Run::default();
        run.text(&text_of(title));
        if let Some(events) = run.finish() {
            out.push(Event::Start(heading_tag(HeadingLevel::H1)));
            out.extend(events);
            out.push(Event::End(TagEnd::Heading(HeadingLevel::H1)));
        }
    }
    block(&root.children, true, &mut out);
    out
}

// Compare a prefix ignoring ASCII case
fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

// Check whether an element name is block-level
fn is_block(name: &str) -> bool {
    TRANSPARENT_BLOCKS.contains(&name) || MAPPED_BLOCKS.contains(&name)
}

// Find the first element with a name, depth first, outside SVG
fn find<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    element
        .elements()
        .filter(|child| child.name != SVG)
        .find_map(|child| {
            if child.name == name {
                Some(child)
            } else {
                find(child, name)
            }
        })
}

// Concatenate all descendant text
fn text_of(element: &Element) -> String {
    let mut text = String::new();
    collect_text(element, &mut text);
    text
}

// Append all descendant text to a buffer
fn collect_text(element: &Element, out: &mut String) {
    for node in &element.children {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Element(child) => collect_text(child, out),
        }
    }
}

// Decode character references, leaving unknown ones as written
fn decode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find(ENTITY_OPEN) {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        match entity(rest) {
            Some((ch, len)) => {
                out.push(ch);
                rest = &rest[len..];
            }
            None => {
                out.push(ENTITY_OPEN);
                rest = &rest[ENTITY_OPEN.len_utf8()..];
            }
        }
    }
    out.push_str(rest);
    out
}

// Resolve one reference at the start of text into its char and byte length
fn entity(text: &str) -> Option<(char, usize)> {
    let semi = text
        .bytes()
        .take(ENTITY_MAX_LEN)
        .position(|byte| byte == ENTITY_CLOSE)?;
    let body = &text[ENTITY_OPEN.len_utf8()..semi];
    let ch = match body.strip_prefix(NUMERIC_MARK) {
        Some(number) => {
            let code = match number.strip_prefix(HEX_MARKS) {
                Some(hex) => u32::from_str_radix(hex, HEX_RADIX).ok()?,
                None => number.parse().ok()?,
            };
            match char::from_u32(code)? {
                NO_BREAK_SPACE => SPACE,
                ch => ch,
            }
        }
        None => NAMED_ENTITIES.iter().find(|(name, _)| *name == body)?.1,
    };
    Some((ch, semi + 1))
}

// Builds the element tree from tags, closing elements the way browsers imply
struct Builder {
    stack: Vec<Element>,
}

impl Builder {
    // Add text to the open element
    fn text(&mut self, raw: &str) {
        if raw.is_empty() {
            return;
        }
        // Visible text ends an unclosed head
        let in_head = self.stack.last().is_some_and(|top| top.name == HEAD);
        if in_head && !raw.trim_ascii().is_empty() {
            self.close_to(self.stack.len() - 1);
        }
        if let Some(top) = self.stack.last_mut() {
            top.children.push(Node::Text(decode(raw)));
        }
    }

    // Open an element, or attach it directly when it has no content
    fn start(&mut self, name: String, attrs: Vec<(String, String)>, self_closing: bool) {
        self.close_implied(&name);
        let void = self_closing || VOID.contains(&name.as_str());
        let element = Element::new(name, attrs);
        if void {
            self.attach(element);
        } else {
            self.stack.push(element);
        }
    }

    // Close the nearest open element with this name, ignoring stray end tags
    fn end(&mut self, name: &str) {
        if let Some(index) = self.find_open(&[name], &[]) {
            self.close_to(index);
        }
    }

    // Close elements that a new start tag ends implicitly
    fn close_implied(&mut self, name: &str) {
        if !HEAD_CONTENT.contains(&name)
            && let Some(index) = self.find_open(&[HEAD], &[])
        {
            self.close_to(index);
        }
        let (targets, stops): (&[&str], &[&str]) = match name {
            LI => (&[LI], &LISTS),
            DT | DD => (&DEFINITION_PARTS, &[DL]),
            TR => (&[TR], &[TABLE]),
            TD | TH => (&CELLS, &[TR, TABLE]),
            THEAD | TBODY | TFOOT => (&ROW_GROUPS, &[TABLE]),
            _ => (&[], &[]),
        };
        if let Some(index) = self.find_open(targets, stops) {
            self.close_to(index);
        }
        if is_block(name)
            && let Some(index) = self.find_open(&[P], &P_SCOPE_STOPS)
        {
            self.close_to(index);
        }
    }

    // Find an open element by name, searching inward-out until a stop element
    fn find_open(&self, targets: &[&str], stops: &[&str]) -> Option<usize> {
        for index in (1..self.stack.len()).rev() {
            let name = self.stack[index].name.as_str();
            if targets.contains(&name) {
                return Some(index);
            }
            if stops.contains(&name) {
                return None;
            }
        }
        None
    }

    // Pop open elements down to and including index, attaching each to its parent
    fn close_to(&mut self, index: usize) {
        while self.stack.len() > index.max(1) {
            if let Some(element) = self.stack.pop() {
                self.attach(element);
            }
        }
    }

    // Append an element to the open element
    fn attach(&mut self, element: Element) {
        if let Some(top) = self.stack.last_mut() {
            top.children.push(Node::Element(element));
        }
    }

    // Close everything and return the root
    fn finish(mut self) -> Element {
        self.close_to(1);
        self.stack
            .pop()
            .unwrap_or_else(|| Element::new(String::new(), Vec::new()))
    }
}

// Parse HTML into an element tree under a nameless root
fn parse(input: &str) -> Element {
    let mut builder = Builder {
        stack: vec![Element::new(String::new(), Vec::new())],
    };
    let mut pos = 0;
    while pos < input.len() {
        let Some(offset) = input[pos..].find(TAG_OPEN) else {
            builder.text(&input[pos..]);
            break;
        };
        let lt = pos + offset;
        builder.text(&input[pos..lt]);
        pos = markup(input, lt, &mut builder);
    }
    builder.finish()
}

// Consume the markup starting at `<`, returning the position after it
fn markup(input: &str, lt: usize, builder: &mut Builder) -> usize {
    let rest = &input[lt..];
    if let Some(body) = rest.strip_prefix(COMMENT_OPEN) {
        return body.find(COMMENT_CLOSE).map_or(input.len(), |end| {
            lt + COMMENT_OPEN.len() + end + COMMENT_CLOSE.len()
        });
    }
    if rest.starts_with(DECL_OPEN) || rest.starts_with(PI_OPEN) {
        return skip_past_close(input, lt);
    }
    if let Some(after) = rest.strip_prefix(END_TAG_OPEN)
        && after
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
    {
        let start = lt + END_TAG_OPEN.len();
        let name = input[start..name_end(input, start)].to_ascii_lowercase();
        builder.end(&name);
        return skip_past_close(input, start);
    }
    if rest
        .as_bytes()
        .get(TAG_OPEN.len())
        .is_some_and(u8::is_ascii_alphabetic)
    {
        return start_tag(input, lt, builder);
    }
    builder.text(TAG_OPEN);
    lt + TAG_OPEN.len()
}

// Find the position just past the next `>`, or the end of input
fn skip_past_close(input: &str, from: usize) -> usize {
    input.as_bytes()[from..]
        .iter()
        .position(|&byte| byte == TAG_CLOSE)
        .map_or(input.len(), |offset| from + offset + 1)
}

// Find where a tag or attribute name ends
fn name_end(input: &str, from: usize) -> usize {
    let bytes = input.as_bytes();
    let mut pos = from;
    while pos < bytes.len()
        && !bytes[pos].is_ascii_whitespace()
        && bytes[pos] != TAG_CLOSE
        && bytes[pos] != SELF_CLOSE
        && bytes[pos] != ATTR_EQUALS
    {
        pos += 1;
    }
    pos
}

// Skip ASCII whitespace
fn skip_space(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

// Parse a start tag and its attributes, then any raw text content
fn start_tag(input: &str, lt: usize, builder: &mut Builder) -> usize {
    let bytes = input.as_bytes();
    let name_start = lt + TAG_OPEN.len();
    let mut pos = name_end(input, name_start);
    let name = input[name_start..pos].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut self_closing = false;
    // Attributes, each name optionally followed by a quoted or bare value
    loop {
        pos = skip_space(bytes, pos);
        let Some(&byte) = bytes.get(pos) else {
            break;
        };
        if byte == TAG_CLOSE {
            pos += 1;
            break;
        }
        if byte == SELF_CLOSE {
            self_closing = true;
            pos += 1;
            continue;
        }
        self_closing = false;
        let key_end = name_end(input, pos + 1);
        let key = input[pos..key_end].to_ascii_lowercase();
        pos = skip_space(bytes, key_end);
        let mut value = String::new();
        if bytes.get(pos) == Some(&ATTR_EQUALS) {
            pos = skip_space(bytes, pos + 1);
            let value_end = match bytes.get(pos) {
                Some(quote) if QUOTES.contains(quote) => {
                    pos += 1;
                    bytes[pos..]
                        .iter()
                        .position(|byte| byte == quote)
                        .map_or(bytes.len(), |offset| pos + offset)
                }
                _ => {
                    let mut end = pos;
                    while end < bytes.len()
                        && !bytes[end].is_ascii_whitespace()
                        && bytes[end] != TAG_CLOSE
                    {
                        end += 1;
                    }
                    end
                }
            };
            value = decode(&input[pos..value_end]);
            pos = value_end;
            if bytes.get(pos).is_some_and(|byte| QUOTES.contains(byte)) {
                pos += 1;
            }
        }
        attrs.push((key, value));
    }
    let raw = RAW_TEXT.contains(&name.as_str()) && !self_closing;
    builder.start(name.clone(), attrs, self_closing);
    if !raw {
        return pos;
    }
    // Raw text runs to the matching end tag, found case-insensitively
    let closing = format!("{END_TAG_OPEN}{name}");
    let close = input[pos..]
        .to_ascii_lowercase()
        .find(&closing)
        .map_or(input.len(), |offset| pos + offset);
    builder.text(&input[pos..close]);
    builder.end(&name);
    if close == input.len() {
        close
    } else {
        skip_past_close(input, close)
    }
}

// Collects inline events with collapsed whitespace and no leading or trailing breaks
#[derive(Default)]
struct Run {
    events: Vec<Event<'static>>,
    buf: String,
    space: bool,
    brk: bool,
    content: bool,
}

impl Run {
    // Add text, collapsing whitespace runs into single spaces
    fn text(&mut self, text: &str) {
        if text.starts_with(|ch: char| ch.is_ascii_whitespace()) {
            self.space = true;
        }
        for word in text.split_ascii_whitespace() {
            self.lead();
            self.buf.push_str(word);
            self.content = true;
            self.space = true;
        }
        if !text.ends_with(|ch: char| ch.is_ascii_whitespace()) {
            self.space = false;
        }
    }

    // Resolve a pending break or space before visible output
    fn lead(&mut self) {
        if self.content {
            if self.brk {
                self.flush();
                self.events.push(Event::HardBreak);
            } else if self.space {
                self.buf.push_str(WORD_GAP);
            }
        }
        self.brk = false;
        self.space = false;
    }

    // Emit buffered text as one event
    fn flush(&mut self) {
        if !self.buf.is_empty() {
            let text = std::mem::take(&mut self.buf);
            self.events.push(Event::Text(CowStr::from(text)));
        }
    }

    // Open an inline tag
    fn open(&mut self, tag: Tag<'static>) {
        self.lead();
        self.flush();
        self.events.push(Event::Start(tag));
    }

    // Close an inline tag
    fn close(&mut self, tag: TagEnd) {
        self.flush();
        self.events.push(Event::End(tag));
    }

    // Add a code span
    fn code(&mut self, code: String) {
        if code.is_empty() {
            return;
        }
        self.lead();
        self.flush();
        self.events.push(Event::Code(CowStr::from(code)));
        self.content = true;
    }

    // Request a line break, dropped at the start or end of the run
    fn hard_break(&mut self) {
        if self.content {
            self.brk = true;
        }
    }

    // Request a word gap
    fn space(&mut self) {
        self.space = true;
    }

    // Return the events, or nothing when the run holds nothing visible
    fn finish(mut self) -> Option<Vec<Event<'static>>> {
        self.flush();
        self.content.then_some(self.events)
    }
}

// Build a heading tag with no id, classes or attributes
fn heading_tag(level: HeadingLevel) -> Tag<'static> {
    Tag::Heading {
        level,
        id: None,
        classes: Vec::new(),
        attrs: Vec::new(),
    }
}

// Emit a finished run, wrapped in a paragraph when asked
fn flush_run(run: Run, wrap: bool, out: &mut Vec<Event<'static>>) {
    let Some(events) = run.finish() else {
        return;
    };
    if wrap {
        out.push(Event::Start(Tag::Paragraph));
    }
    out.extend(events);
    if wrap {
        out.push(Event::End(TagEnd::Paragraph));
    }
}

// Walk nodes in block context, grouping inline content into paragraphs
fn block<'n>(nodes: impl IntoIterator<Item = &'n Node>, wrap: bool, out: &mut Vec<Event<'static>>) {
    let mut run = Run::default();
    for node in nodes {
        let element = match node {
            Node::Text(text) => {
                run.text(text);
                continue;
            }
            Node::Element(element) => element,
        };
        let name = element.name.as_str();
        if DROPPED.contains(&name) {
            continue;
        }
        if !is_block(name) {
            inline_element(element, &mut run, false);
            continue;
        }
        flush_run(std::mem::take(&mut run), wrap, out);
        block_element(element, wrap, out);
    }
    flush_run(run, wrap, out);
}

// Map one block element onto events
fn block_element(element: &Element, wrap: bool, out: &mut Vec<Event<'static>>) {
    let name = element.name.as_str();
    if let Some((_, level)) = HEADINGS.iter().find(|(tag, _)| *tag == name) {
        let mut run = Run::default();
        inline_nodes(&element.children, &mut run, true);
        if let Some(events) = run.finish() {
            out.push(Event::Start(heading_tag(*level)));
            out.extend(events);
            out.push(Event::End(TagEnd::Heading(*level)));
        }
        return;
    }
    match name {
        P => {
            let mut run = Run::default();
            inline_nodes(&element.children, &mut run, false);
            flush_run(run, true, out);
        }
        UL | OL => list(element, out),
        BLOCKQUOTE => {
            out.push(Event::Start(Tag::BlockQuote(None)));
            block(&element.children, true, out);
            out.push(Event::End(TagEnd::BlockQuote(None)));
        }
        PRE => code_block(element, out),
        HR => out.push(Event::Rule),
        TABLE => table(element, out),
        DL => definitions(element, out),
        _ => block(&element.children, wrap, out),
    }
}

// Walk nodes in inline context
fn inline_nodes(nodes: &[Node], run: &mut Run, flat: bool) {
    for node in nodes {
        match node {
            Node::Text(text) => run.text(text),
            Node::Element(element) => inline_element(element, run, flat),
        }
    }
}

// Wrap an element's inline content in a start and end tag
fn styled(element: &Element, run: &mut Run, flat: bool, start: Tag<'static>, end: TagEnd) {
    run.open(start);
    inline_nodes(&element.children, run, flat);
    run.close(end);
}

// Map one element inside inline content; flat turns breaks and blocks into spaces
fn inline_element(element: &Element, run: &mut Run, flat: bool) {
    let name = element.name.as_str();
    if DROPPED.contains(&name) {
        return;
    }
    match name {
        STRONG | B => styled(element, run, flat, Tag::Strong, TagEnd::Strong),
        EM | I => styled(element, run, flat, Tag::Emphasis, TagEnd::Emphasis),
        DEL | S | STRIKE => styled(
            element,
            run,
            flat,
            Tag::Strikethrough,
            TagEnd::Strikethrough,
        ),
        CODE | KBD | SAMP | TT => {
            let text = text_of(element);
            let words: Vec<&str> = text.split_ascii_whitespace().collect();
            run.code(words.join(WORD_GAP));
        }
        A => match element.attr(HREF) {
            Some(href) => {
                let tag = Tag::Link {
                    link_type: LinkType::Inline,
                    dest_url: CowStr::from(href.to_owned()),
                    title: CowStr::Borrowed(""),
                    id: CowStr::Borrowed(""),
                };
                styled(element, run, flat, tag, TagEnd::Link);
            }
            None => inline_nodes(&element.children, run, flat),
        },
        IMG => {
            run.open(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::from(element.attr(SRC).unwrap_or_default().to_owned()),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            });
            run.text(element.attr(ALT).unwrap_or_default().trim_ascii());
            run.content = true;
            run.close(TagEnd::Image);
        }
        BR if flat => run.space(),
        BR => run.hard_break(),
        _ if is_block(name) => {
            run.space();
            inline_nodes(&element.children, run, flat);
            run.space();
        }
        _ => inline_nodes(&element.children, run, flat),
    }
}

// Check whether an item's content needs paragraphs, as a loose list item does
fn needs_paragraphs<'n>(nodes: impl IntoIterator<Item = &'n Node>) -> bool {
    nodes.into_iter().any(|node| match node {
        Node::Element(element) => {
            is_block(&element.name) && !LISTS.contains(&element.name.as_str())
        }
        Node::Text(_) => false,
    })
}

// Emit one list item around block content
fn item(nodes: &[&Node], out: &mut Vec<Event<'static>>) {
    out.push(Event::Start(Tag::Item));
    block(
        nodes.iter().copied(),
        needs_paragraphs(nodes.iter().copied()),
        out,
    );
    out.push(Event::End(TagEnd::Item));
}

// Map ul or ol, attaching stray content to the item before it
fn list(element: &Element, out: &mut Vec<Event<'static>>) {
    let start = (element.name == OL).then(|| {
        element
            .attr(START)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(DEFAULT_START)
    });
    let mut items: Vec<Vec<&Node>> = Vec::new();
    for node in &element.children {
        match node {
            Node::Element(child) if child.name == LI => {
                items.push(child.children.iter().collect());
            }
            Node::Text(text) if text.trim_ascii().is_empty() => {}
            _ => match items.last_mut() {
                Some(last) => last.push(node),
                None => items.push(vec![node]),
            },
        }
    }
    if items.is_empty() {
        return;
    }
    out.push(Event::Start(Tag::List(start)));
    for nodes in &items {
        item(nodes, out);
    }
    out.push(Event::End(TagEnd::List(start.is_some())));
}

// Map dl as bold term paragraphs, each followed by its definitions as a list
fn definitions(element: &Element, out: &mut Vec<Event<'static>>) {
    let mut group: Vec<&Element> = Vec::new();
    for node in &element.children {
        match node {
            Node::Text(text) if text.trim_ascii().is_empty() => continue,
            Node::Element(child) if child.name == DD => {
                group.push(child);
                continue;
            }
            _ => {}
        }
        definition_list(&mut group, out);
        match node {
            Node::Element(child) if child.name == DT => {
                let mut run = Run::default();
                run.open(Tag::Strong);
                inline_nodes(&child.children, &mut run, true);
                run.close(TagEnd::Strong);
                flush_run(run, true, out);
            }
            _ => block([node], true, out),
        }
    }
    definition_list(&mut group, out);
}

// Emit and clear a group of definitions
fn definition_list(group: &mut Vec<&Element>, out: &mut Vec<Event<'static>>) {
    if group.is_empty() {
        return;
    }
    out.push(Event::Start(Tag::List(None)));
    for definition in group.drain(..) {
        let nodes: Vec<&Node> = definition.children.iter().collect();
        item(&nodes, out);
    }
    out.push(Event::End(TagEnd::List(false)));
}

// Read a language name from a class attribute
fn language(element: &Element) -> Option<String> {
    element
        .attr(CLASS)?
        .split_ascii_whitespace()
        .find_map(|class| {
            LANGUAGE_PREFIXES
                .iter()
                .find_map(|prefix| class.strip_prefix(prefix))
        })
        .map(str::to_owned)
}

// Map pre as a fenced code block with its whitespace kept exactly
fn code_block(element: &Element, out: &mut Vec<Event<'static>>) {
    let inner_code = element.elements().find(|child| child.name == CODE);
    let lang = language(element)
        .or_else(|| inner_code.and_then(language))
        .unwrap_or_default();
    let mut text = text_of(element);
    // A newline right after the opening tag is not content
    if let Some(stripped) = LEADING_NEWLINES
        .iter()
        .find_map(|newline| text.strip_prefix(newline))
    {
        text = stripped.to_owned();
    }
    if !text.is_empty() && !text.ends_with(LINE_END) {
        text.push(LINE_END);
    }
    out.push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(
        CowStr::from(lang),
    ))));
    if !text.is_empty() {
        out.push(Event::Text(CowStr::from(text)));
    }
    out.push(Event::End(TagEnd::CodeBlock));
}

// Gather table rows through any row groups
fn collect_rows<'a>(element: &'a Element, rows: &mut Vec<&'a Element>) {
    for child in element.elements() {
        match child.name.as_str() {
            TR => rows.push(child),
            THEAD | TBODY | TFOOT => collect_rows(child, rows),
            _ => {}
        }
    }
}

// Map a table with its first row as the head, padding short rows
fn table(element: &Element, out: &mut Vec<Event<'static>>) {
    if let Some(caption) = element.elements().find(|child| child.name == CAPTION) {
        let mut run = Run::default();
        inline_nodes(&caption.children, &mut run, true);
        flush_run(run, true, out);
    }
    let mut rows = Vec::new();
    collect_rows(element, &mut rows);
    let cells = |row: &&Element| {
        row.elements()
            .filter(|child| CELLS.contains(&child.name.as_str()))
            .count()
    };
    let columns = rows.iter().map(cells).max().unwrap_or(0);
    if columns == 0 {
        return;
    }
    out.push(Event::Start(Tag::Table(vec![Alignment::None; columns])));
    for (index, row) in rows.iter().enumerate() {
        let (start, end) = if index == 0 {
            (Tag::TableHead, TagEnd::TableHead)
        } else {
            (Tag::TableRow, TagEnd::TableRow)
        };
        out.push(Event::Start(start));
        let mut count = 0;
        for cell in row
            .elements()
            .filter(|child| CELLS.contains(&child.name.as_str()))
        {
            let mut run = Run::default();
            inline_nodes(&cell.children, &mut run, true);
            out.push(Event::Start(Tag::TableCell));
            out.extend(run.finish().unwrap_or_default());
            out.push(Event::End(TagEnd::TableCell));
            count += 1;
        }
        for _ in count..columns {
            out.push(Event::Start(Tag::TableCell));
            out.push(Event::End(TagEnd::TableCell));
        }
        out.push(Event::End(end));
    }
    out.push(Event::End(TagEnd::Table));
}
