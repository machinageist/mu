// Author: machinageist
// Date: 2026-11-09
// Description: XML (and plist, SVG, RSS…) syntax highlighting, re-indented when the structure is safe
// Notes: The tokenizer covers the input exactly, so the as-written fallback loses nothing; re-indenting
// needs balanced tags and no mixed text-and-element content, else the input is shown as written

use super::{Span, StyledLine};
use crate::style::{
    STYLE_DATA_COMMENT, STYLE_DATA_KEY, STYLE_DATA_LITERAL, STYLE_DATA_META, STYLE_DATA_PUNCT,
    STYLE_DATA_STRING, Style,
};

// Byte order mark some editors put at the start of a file
const BOM: char = '\u{feff}';

// One indentation step in re-indented output
const INDENT: &str = "  ";

// Deeper documents are shown as written, since their indentation would dwarf the content
const MAX_PRETTY_DEPTH: usize = 256;

// Markup delimiters
const COMMENT_OPEN: &str = "<!--";
const COMMENT_CLOSE: &str = "-->";
const CDATA_OPEN: &str = "<![CDATA[";
const CDATA_CLOSE: &str = "]]>";
const DECL_OPEN: &str = "<?";
const DECL_CLOSE: &str = "?>";
const BANG_OPEN: &str = "<!";
const END_TAG_OPEN: &str = "</";
const SELF_CLOSE: &str = "/>";
const ENTITY_MAX: usize = 32;

// Sniffing: a declaration, a non-HTML doctype, or a root from a well-known XML vocabulary
const XML_DECL: &str = "<?xml";
const DOCTYPE: &str = "<!doctype";
const HTML_MARK: &str = "<html";
const KNOWN_ROOTS: [&str; 8] = [
    "plist",
    "svg",
    "rss",
    "feed",
    "urlset",
    "sitemapindex",
    "project",
    "opml",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind<'a> {
    Decl,
    Comment,
    CData,
    Doctype,
    Open { name: &'a str, self_closing: bool },
    Close { name: &'a str },
    Text,
    // Markup that never closed: shown as text, and it rules out re-indenting
    Broken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Token<'a> {
    kind: Kind<'a>,
    text: &'a str,
}

// Check whether the head of an input looks like XML that is not HTML
pub fn sniff(head: &str) -> bool {
    let body = head.strip_prefix(BOM).unwrap_or(head).trim_start();
    let lower = body
        .get(..body.len().min(512))
        .unwrap_or(body)
        .to_ascii_lowercase();
    if lower.starts_with(XML_DECL) {
        return !head.to_ascii_lowercase().contains(HTML_MARK);
    }
    if let Some(rest) = lower.strip_prefix(DOCTYPE) {
        return !rest.trim_start().starts_with("html");
    }
    let root = lower.strip_prefix('<').map(|rest| {
        rest.split(|ch: char| !is_name_char(ch))
            .next()
            .unwrap_or("")
    });
    root.is_some_and(|root| KNOWN_ROOTS.contains(&root))
}

// Classify XML into styled lines, re-indented when the whole document allows it
pub fn highlight(input: &str) -> Vec<StyledLine> {
    let body = input.strip_prefix(BOM).unwrap_or(input);
    let tokens = tokenize(body);
    let mut lines = Lines::default();
    if is_safe_to_indent(&tokens) {
        pretty(&tokens, &mut lines);
    } else {
        for token in &tokens {
            lines.token(token);
        }
    }
    lines.finish()
}

// Classify XML into styled lines exactly as written, as code blocks need
pub fn highlight_verbatim(input: &str) -> Vec<StyledLine> {
    let mut lines = Lines::default();
    for token in &tokenize(input) {
        lines.token(token);
    }
    lines.finish()
}

// Split XML into tokens that together are exactly the input
fn tokenize(input: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < input.len() {
        let rest = &input[at..];
        let (kind, length) = if !rest.starts_with('<') {
            (Kind::Text, rest.find('<').unwrap_or(rest.len()))
        } else if rest.starts_with(COMMENT_OPEN) {
            closed(rest, COMMENT_OPEN.len(), COMMENT_CLOSE, Kind::Comment)
        } else if rest.starts_with(CDATA_OPEN) {
            closed(rest, CDATA_OPEN.len(), CDATA_CLOSE, Kind::CData)
        } else if rest.starts_with(DECL_OPEN) {
            closed(rest, DECL_OPEN.len(), DECL_CLOSE, Kind::Decl)
        } else if rest.starts_with(BANG_OPEN) {
            doctype(rest)
        } else if rest.starts_with(END_TAG_OPEN) {
            match tag_end(rest) {
                Some(end) => {
                    let name = tag_name(&rest[END_TAG_OPEN.len()..end]);
                    (Kind::Close { name }, end + 1)
                }
                None => (Kind::Broken, rest.len()),
            }
        } else if rest[1..].starts_with(is_name_start) {
            match tag_end(rest) {
                Some(end) => {
                    let name = tag_name(&rest[1..end]);
                    let self_closing = rest[..=end].ends_with(SELF_CLOSE);
                    (Kind::Open { name, self_closing }, end + 1)
                }
                None => (Kind::Broken, rest.len()),
            }
        } else {
            // A stray `<` is text; carry on from the next one
            (
                Kind::Broken,
                1 + rest[1..].find('<').unwrap_or(rest.len() - 1),
            )
        };
        tokens.push(Token {
            kind,
            text: &rest[..length],
        });
        at += length;
    }
    tokens
}

// A construct that runs from its opener to a fixed closer, or to the end as broken markup
fn closed<'a>(rest: &str, skip: usize, close: &str, kind: Kind<'a>) -> (Kind<'a>, usize) {
    match rest[skip..].find(close) {
        Some(offset) => (kind, skip + offset + close.len()),
        None => (Kind::Broken, rest.len()),
    }
}

// A `<!…>` declaration such as DOCTYPE, allowing a bracketed internal subset and quotes
fn doctype(rest: &str) -> (Kind<'static>, usize) {
    let mut depth = 0usize;
    let mut quote = None;
    for (index, ch) in rest.char_indices().skip(BANG_OPEN.len()) {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '[') => depth += 1,
            (None, ']') => depth = depth.saturating_sub(1),
            (None, '>') if depth == 0 => return (Kind::Doctype, index + 1),
            _ => {}
        }
    }
    (Kind::Broken, rest.len())
}

// Byte index of the `>` closing a tag, skipping any inside quoted attribute values
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote = None;
    for (index, ch) in rest.char_indices().skip(1) {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '>') => return Some(index),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

// The element name at the start of a tag's inside
fn tag_name(inside: &str) -> &str {
    let end = inside
        .find(|ch: char| !is_name_char(ch))
        .unwrap_or(inside.len());
    &inside[..end]
}

// Characters that may start an XML name
fn is_name_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_' || ch == ':'
}

// Characters that may continue an XML name
fn is_name_char(ch: char) -> bool {
    is_name_start(ch) || ch.is_ascii_digit() || matches!(ch, '-' | '.')
}

// Check that tags balance, nesting is bounded, and no element mixes text with other nodes
fn is_safe_to_indent(tokens: &[Token]) -> bool {
    // Per open element: whether it holds non-blank text, and whether it holds other nodes
    let mut stack: Vec<(&str, bool, bool)> = Vec::new();
    for token in tokens {
        let is_text = match token.kind {
            Kind::Broken => return false,
            Kind::Text if token.text.trim().is_empty() => continue,
            Kind::Text | Kind::CData => true,
            _ => false,
        };
        let Some(top) = stack.last_mut() else {
            // Outside the root only markup may appear
            if is_text {
                return false;
            }
            if let Kind::Open {
                name,
                self_closing: false,
            } = token.kind
            {
                stack.push((name, false, false));
            } else if let Kind::Close { .. } = token.kind {
                return false;
            }
            continue;
        };
        match token.kind {
            Kind::Close { name } => {
                if top.0 != name {
                    return false;
                }
                stack.pop();
                continue;
            }
            _ if is_text => top.1 = true,
            _ => top.2 = true,
        }
        if top.1 && top.2 {
            return false;
        }
        if let Kind::Open {
            name,
            self_closing: false,
        } = token.kind
        {
            if stack.len() >= MAX_PRETTY_DEPTH {
                return false;
            }
            stack.push((name, false, false));
        }
    }
    stack.is_empty()
}

// Write a checked token stream one node per line, text-only elements on a single line
fn pretty(tokens: &[Token], lines: &mut Lines) {
    let mut depth = 0usize;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        index += 1;
        match token.kind {
            Kind::Text if token.text.trim().is_empty() => continue,
            Kind::Close { .. } => {
                depth = depth.saturating_sub(1);
                lines.indent(depth);
                lines.tag(token.text);
                lines.break_line();
            }
            Kind::Open {
                self_closing: false,
                ..
            } => {
                // Text-only content: everything up to the next close is text or CDATA
                let content_end = tokens[index..]
                    .iter()
                    .position(|next| !matches!(next.kind, Kind::Text | Kind::CData))
                    .map(|offset| index + offset);
                let text_only =
                    content_end.is_some_and(|end| matches!(tokens[end].kind, Kind::Close { .. }));
                lines.indent(depth);
                lines.tag(token.text);
                match content_end.filter(|_| text_only) {
                    Some(end) => {
                        let content = &tokens[index..end];
                        if content.iter().any(|part| !part.text.trim().is_empty()) {
                            for part in content {
                                lines.token(part);
                            }
                        }
                        lines.tag(tokens[end].text);
                        index = end + 1;
                    }
                    None => depth += 1,
                }
                lines.break_line();
            }
            _ => {
                lines.indent(depth);
                match token.kind {
                    Kind::Open { .. } => lines.tag(token.text),
                    _ => lines.token(token),
                }
                lines.break_line();
            }
        }
    }
}

// Styled lines under construction
#[derive(Default)]
struct Lines {
    lines: Vec<StyledLine>,
    line: StyledLine,
}

impl Lines {
    // Add text in one style, starting a new line at each line break
    fn push(&mut self, style: Style, text: &str) {
        let mut parts = text.split('\n');
        if let Some(first) = parts.next() {
            self.span(style, first);
        }
        for part in parts {
            self.break_line();
            self.span(style, part);
        }
    }

    // Add text that holds no line break
    fn span(&mut self, style: Style, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.line.last_mut() {
            Some((last, existing)) if *last == style => existing.push_str(text),
            _ => self.line.push((style, text.to_owned())),
        }
    }

    // End the current line
    fn break_line(&mut self) {
        self.lines.push(std::mem::take(&mut self.line));
    }

    // Indent a fresh line by depth
    fn indent(&mut self, depth: usize) {
        self.span(Style::PLAIN, &INDENT.repeat(depth));
    }

    // Write a token as written, highlighted by kind
    fn token(&mut self, token: &Token) {
        match token.kind {
            Kind::Comment => self.push(STYLE_DATA_COMMENT, token.text),
            Kind::Decl | Kind::Doctype => self.push(STYLE_DATA_META, token.text),
            Kind::CData => {
                let inner = &token.text[CDATA_OPEN.len()..token.text.len() - CDATA_CLOSE.len()];
                self.push(STYLE_DATA_PUNCT, CDATA_OPEN);
                self.push(STYLE_DATA_STRING, inner);
                self.push(STYLE_DATA_PUNCT, CDATA_CLOSE);
            }
            Kind::Open { .. } | Kind::Close { .. } => {
                for (style, text) in tag_spans(token.text, false) {
                    self.push(style, &text);
                }
            }
            Kind::Text | Kind::Broken => self.text(token.text),
        }
    }

    // Write a tag with the whitespace between its attributes collapsed
    fn tag(&mut self, text: &str) {
        for (style, text) in tag_spans(text, true) {
            self.push(style, &text);
        }
    }

    // Write character data, highlighting entity references
    fn text(&mut self, text: &str) {
        let mut rest = text;
        while let Some(start) = rest.find('&') {
            self.push(Style::PLAIN, &rest[..start]);
            let after = &rest[start..];
            let entity = after
                .char_indices()
                .take(ENTITY_MAX)
                .find(|(_, ch)| *ch == ';' || ch.is_whitespace() || *ch == '<')
                .filter(|(_, ch)| *ch == ';')
                .map(|(end, _)| end + 1);
            match entity {
                Some(end) if end > 2 => {
                    self.push(STYLE_DATA_LITERAL, &after[..end]);
                    rest = &after[end..];
                }
                _ => {
                    self.push(Style::PLAIN, "&");
                    rest = &after[1..];
                }
            }
        }
        self.push(Style::PLAIN, rest);
    }

    // Hand back the lines, keeping a last line that has content
    fn finish(mut self) -> Vec<StyledLine> {
        if !self.line.is_empty() {
            self.break_line();
        }
        self.lines
    }
}

// Split a start or end tag into styled spans; `collapse` squeezes whitespace runs to one space
fn tag_spans(text: &str, collapse: bool) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut push = |style: Style, part: &str| {
        if !part.is_empty() {
            spans.push((style, part.to_owned()));
        }
    };
    let opener = if text.starts_with(END_TAG_OPEN) { 2 } else { 1 };
    push(STYLE_DATA_PUNCT, &text[..opener]);
    let closer = if text.ends_with(SELF_CLOSE) { 2 } else { 1 };
    let inside = &text[opener..text.len() - closer];
    let name_end = inside
        .find(|ch: char| !is_name_char(ch))
        .unwrap_or(inside.len());
    push(STYLE_DATA_KEY, &inside[..name_end]);
    let mut rest = &inside[name_end..];
    // An unquoted word straight after `=` is a value, otherwise an attribute name
    let mut after_equals = false;
    while !rest.is_empty() {
        let ch = rest.chars().next().unwrap_or(' ');
        let length = if ch.is_whitespace() {
            let end = rest
                .find(|ch: char| !ch.is_whitespace())
                .unwrap_or(rest.len());
            push(Style::PLAIN, if collapse { " " } else { &rest[..end] });
            // Whitespace around `=` does not end the attribute
            rest = &rest[end..];
            continue;
        } else if ch == '=' {
            push(STYLE_DATA_PUNCT, "=");
            after_equals = true;
            rest = &rest[1..];
            continue;
        } else if ch == '"' || ch == '\'' {
            let end = rest[1..].find(ch).map_or(rest.len(), |offset| offset + 2);
            push(STYLE_DATA_STRING, &rest[..end]);
            end
        } else {
            let end = rest
                .find(|ch: char| ch.is_whitespace() || ch == '=')
                .unwrap_or(rest.len());
            let style = if after_equals {
                STYLE_DATA_STRING
            } else {
                STYLE_DATA_META
            };
            push(style, &rest[..end.max(1)]);
            end.max(1)
        };
        after_equals = false;
        rest = &rest[length..];
    }
    push(STYLE_DATA_PUNCT, &text[text.len() - closer..]);
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    // Lines as plain text
    fn plain(lines: &[StyledLine]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.iter().map(|(_, text)| text.as_str()).collect())
            .collect()
    }

    // Re-indent or show as written, as plain text joined by newlines
    fn shown(input: &str) -> String {
        plain(&highlight(input)).join("\n")
    }

    #[test]
    fn tokenizing_loses_nothing() {
        let inputs = [
            "<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY a \"b>\">]><x a='1>2'>t &amp; <![CDATA[<y>]]><!-- c --><z/></x>",
            "<a><b>unclosed",
            "text < not a tag > here",
            "<!-- never closed",
            "",
        ];
        for input in inputs {
            let joined: String = tokenize(input).iter().map(|token| token.text).collect();
            assert_eq!(joined, input);
        }
    }

    #[test]
    fn every_token_kind_is_recognised() {
        let kinds: Vec<&str> =
            tokenize("<?xml?><!DOCTYPE d><!--c--><a k=\"v\"><![CDATA[x]]></a><b/>text<")
                .iter()
                .map(|token| match token.kind {
                    Kind::Decl => "decl",
                    Kind::Comment => "comment",
                    Kind::CData => "cdata",
                    Kind::Doctype => "doctype",
                    Kind::Open {
                        self_closing: true, ..
                    } => "empty",
                    Kind::Open { .. } => "open",
                    Kind::Close { .. } => "close",
                    Kind::Text => "text",
                    Kind::Broken => "broken",
                })
                .collect();
        assert_eq!(
            kinds,
            [
                "decl", "doctype", "comment", "open", "cdata", "close", "empty", "text", "broken"
            ]
        );
    }

    #[test]
    fn balanced_documents_are_re_indented() {
        assert_eq!(
            shown(
                "<?xml version=\"1.0\"?>\n<root><item   id=\"1\"\n   kind='a'>One &amp; two</item><empty/><e></e><!-- note --><list><v>1</v></list></root>"
            ),
            "<?xml version=\"1.0\"?>\n<root>\n  <item id=\"1\" kind='a'>One &amp; two</item>\n  <empty/>\n  <e></e>\n  <!-- note -->\n  <list>\n    <v>1</v>\n  </list>\n</root>"
        );
    }

    #[test]
    fn text_keeps_its_line_breaks_and_cdata_stays_inline() {
        assert_eq!(
            shown("<a><b>line one\nline two</b><c><![CDATA[x < y]]></c></a>"),
            "<a>\n  <b>line one\nline two</b>\n  <c><![CDATA[x < y]]></c>\n</a>"
        );
    }

    #[test]
    fn unsafe_documents_are_shown_as_written() {
        let cases = [
            "<p>Mixed <b>content</b> here</p>",
            "<a><b></a></b>",
            "<a><b>",
            "stray text <a/>",
            "<a>\n  <!-- never closed\n</a>",
        ];
        for input in cases {
            assert_eq!(shown(input), input, "{input:?}");
        }
        let deep = format!("{}{}", "<a>".repeat(300), "</a>".repeat(300));
        assert_eq!(shown(&deep), deep);
    }

    #[test]
    fn tags_are_styled_by_part() {
        let spans = tag_spans("<a href=\"x\" n=1/>", false);
        assert_eq!(
            spans,
            [
                (STYLE_DATA_PUNCT, "<".to_owned()),
                (STYLE_DATA_KEY, "a".to_owned()),
                (Style::PLAIN, " ".to_owned()),
                (STYLE_DATA_META, "href".to_owned()),
                (STYLE_DATA_PUNCT, "=".to_owned()),
                (STYLE_DATA_STRING, "\"x\"".to_owned()),
                (Style::PLAIN, " ".to_owned()),
                (STYLE_DATA_META, "n".to_owned()),
                (STYLE_DATA_PUNCT, "=".to_owned()),
                (STYLE_DATA_STRING, "1".to_owned()),
                (STYLE_DATA_PUNCT, "/>".to_owned()),
            ]
        );
    }

    #[test]
    fn spaced_unquoted_values_are_values() {
        let spans = tag_spans("<a n = 1>", false);
        assert!(
            spans.contains(&(STYLE_DATA_STRING, "1".to_owned())),
            "{spans:?}"
        );
    }

    #[test]
    fn entities_are_literals() {
        let lines = highlight("<a>x &lt; y &amp z</a>");
        assert!(lines[0].contains(&(STYLE_DATA_LITERAL, "&lt;".to_owned())));
        assert!(!lines[0].iter().any(|(_, text)| text == "&amp"));
    }

    #[test]
    fn sniffing_finds_xml_but_not_html_or_readmes() {
        assert!(sniff("<?xml version=\"1.0\"?>\n<plist/>"));
        assert!(sniff("\u{feff}  <!DOCTYPE plist PUBLIC \"x\">"));
        assert!(sniff("<rss version=\"2.0\">"));
        assert!(sniff("<svg xmlns=\"http://www.w3.org/2000/svg\">"));
        assert!(!sniff("<?xml version=\"1.0\"?><!DOCTYPE html><html>"));
        assert!(!sniff("<!DOCTYPE html>"));
        assert!(!sniff("<div align=\"center\">\n# Project\n</div>"));
        assert!(!sniff("<svgfoo>"));
        assert!(!sniff("# Title"));
    }
}
