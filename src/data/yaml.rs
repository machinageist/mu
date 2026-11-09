// Author: machinageist
// Date: 2026-11-09
// Description: YAML syntax highlighting into styled lines, plus a conservative content sniff
// Notes: A line-by-line tokenizer that never reformats; state carried between lines covers
// block scalar bodies, multi-line quoted strings and flow collections. Malformed input still
// highlights as written. The sniff reuses the tokenizer so both agree on what a key is

use super::StyledLine;
use crate::style::{
    STYLE_DATA_COMMENT, STYLE_DATA_KEY, STYLE_DATA_LITERAL, STYLE_DATA_META, STYLE_DATA_NUMBER,
    STYLE_DATA_PUNCT, STYLE_DATA_STRING, Style,
};

// Indicators and markers
const COMMENT: char = '#';
const DIRECTIVE: char = '%';
const DOC_START: &str = "---";
const DOC_END: &str = "...";
const SEQ_ENTRY: char = '-';
const COMPLEX_KEY: char = '?';
const MAP_VALUE: char = ':';
const LITERAL_BLOCK: char = '|';
const FOLDED_BLOCK: char = '>';
const BLOCK_HEADER_CHARS: &str = "+-0123456789";
const DOUBLE_QUOTE: char = '"';
const SINGLE_QUOTE: char = '\'';
const ESCAPE: char = '\\';
const FLOW_OPENERS: &str = "[{";
const FLOW_CLOSERS: &str = "]}";
const FLOW_CHARS: &str = "[]{},";
const META_STARTS: &str = "&*!";
const MERGE_KEY: &str = "<<";

// Plain keys may not open with these indicators
const KEY_FORBIDDEN_START: &str = "-?:,[]{}#&*!|>'\"%@`";

// Scalar classes, compared case-insensitively
const LITERALS: [&str; 8] = ["true", "false", "yes", "no", "on", "off", "null", "~"];
const SPECIAL_FLOATS: [&str; 2] = [".inf", ".nan"];
const HEX_PREFIX: &str = "0x";
const OCTAL_PREFIX: &str = "0o";
const SIGNS: [char; 2] = ['+', '-'];
const EXPONENT: char = 'e';
const DECIMAL_POINT: char = '.';

// Sniff thresholds and Markdown block signals
const MIN_SIGNIFICANT_LINES: usize = 3;
const MIN_SHAPED_PERCENT: usize = 80;
const MAX_HEADING_LEVEL: usize = 6;
const FENCES: [&str; 2] = ["```", "~~~"];
const QUOTE_MARKER: &str = "> ";
const TABLE_BAR: char = '|';
const MIN_TABLE_BARS: usize = 2;
const STRUCTURE_STARTS: [&str; 7] = ["-", "?", ":", DOC_START, DOC_END, "[", "{"];

// Check whether the head of an input looks like YAML
pub fn sniff(head: &str) -> bool {
    let lines: Vec<&str> = head.lines().collect();
    let mut state = State::default();
    let mut significant = 0;
    let mut shaped = 0;
    let mut keys = 0;
    let mut prev_blank = true;
    // Structure Markdown bullets lack: a top-level key, a leading marker, or an item mapping
    let mut evidence = false;
    let mut item_key_column = None;
    for (index, raw) in lines.iter().enumerate() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            state.line(line);
            prev_blank = true;
            continue;
        }
        let content = line.trim_start();
        let indent = line.len() - content.len();
        // Lines inside a block scalar, quoted string or flow collection are YAML by construction
        let continuing = state.block_parent.is_some_and(|parent| indent > parent)
            || state.quote.is_some()
            || state.flow_depth > 0;
        if continuing {
            state.line(line);
            significant += 1;
            shaped += 1;
            prev_blank = false;
            continue;
        }
        if is_markdown_block(content, indent) {
            return false;
        }
        let spans = state.line(line);
        let first = spans.iter().find(|(style, _)| *style != Style::PLAIN);
        if first.is_some_and(|(style, _)| *style == STYLE_DATA_COMMENT) {
            let next_blank = lines
                .get(index + 1)
                .is_none_or(|next| next.trim().is_empty());
            if prev_blank && next_blank && is_heading(content, index == 0) {
                return false;
            }
            prev_blank = false;
            continue;
        }
        let key_column = key_column(&spans);
        let has_key = key_column.is_some();
        let structural = first.is_some_and(|(style, text)| {
            (*style == STYLE_DATA_PUNCT && STRUCTURE_STARTS.iter().any(|s| text.starts_with(s)))
                || (*style == STYLE_DATA_META && text.starts_with(DIRECTIVE))
        });
        let opens_document = first.is_some_and(|(style, text)| {
            (*style == STYLE_DATA_PUNCT && text.starts_with(DOC_START))
                || (*style == STYLE_DATA_META && text.starts_with(DIRECTIVE))
        });
        // A sibling key aligned under the key of a `- key:` item proves a mapping in a sequence
        evidence |= (significant == 0 && opens_document)
            || key_column == Some(0)
            || (key_column == Some(indent) && item_key_column == Some(indent));
        if has_key && key_column != Some(indent) {
            item_key_column = key_column;
        }
        significant += 1;
        if has_key {
            keys += 1;
        }
        if has_key || structural {
            shaped += 1;
        } else if prev_blank {
            // A prose paragraph after a blank line
            return false;
        }
        prev_blank = false;
    }
    significant >= MIN_SIGNIFICANT_LINES
        && keys > 0
        && evidence
        && shaped * 100 >= significant * MIN_SHAPED_PERCENT
}

// Classify YAML text into styled lines
pub fn highlight(input: &str) -> Vec<StyledLine> {
    let mut state = State::default();
    input
        .lines()
        .map(|line| state.line(line.strip_suffix('\r').unwrap_or(line)))
        .collect()
}

// Detect fences, quotes and tables, which YAML outside a block scalar never has
fn is_markdown_block(content: &str, indent: usize) -> bool {
    FENCES.iter().any(|fence| content.starts_with(fence))
        || (indent == 0 && content.starts_with(QUOTE_MARKER))
        || (content.starts_with(TABLE_BAR) && content.matches(TABLE_BAR).count() >= MIN_TABLE_BARS)
}

// Detect an ATX heading shape, one to six hashes then a space
// Notes: a lone single-hash line opening the file is more often a YAML header comment
fn is_heading(content: &str, first_line: bool) -> bool {
    let hashes = content.len() - content.trim_start_matches(COMMENT).len();
    let shaped = (1..=MAX_HEADING_LEVEL).contains(&hashes) && content[hashes..].starts_with(' ');
    shaped && !(first_line && hashes == 1)
}

// Find the column of the line's first YAML-shaped key
fn key_column(spans: &StyledLine) -> Option<usize> {
    let mut column = 0;
    for (style, text) in spans {
        if is_sniff_key(*style, text) {
            return Some(column);
        }
        column += text.len();
    }
    None
}

// Accept a key span as YAML-shaped only when it is quoted or has no inner spaces
fn is_sniff_key(style: Style, text: &str) -> bool {
    match style {
        STYLE_DATA_KEY => {
            text.starts_with([DOUBLE_QUOTE, SINGLE_QUOTE]) || !text.contains(char::is_whitespace)
        }
        STYLE_DATA_META => text == MERGE_KEY,
        _ => false,
    }
}

// Tokenizer state carried from one line to the next
#[derive(Default)]
struct State {
    // Column of the node that opened a block scalar; deeper lines are its body
    block_parent: Option<usize>,
    // Quote char of a string still open at the end of the previous line
    quote: Option<char>,
    // Nesting of open flow collections
    flow_depth: usize,
}

// Spans for one line, merging neighbours that share a style
#[derive(Default)]
struct Spans(StyledLine);

impl Spans {
    // Append text in a style, skipping empties
    fn push(&mut self, style: Style, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.0.last_mut() {
            Some((last, run)) if *last == style => run.push_str(text),
            _ => self.0.push((style, text.to_owned())),
        }
    }

    // Report whether the line so far is empty or ends in whitespace
    fn after_space(&self) -> bool {
        self.0
            .last()
            .and_then(|(_, run)| run.chars().last())
            .is_none_or(char::is_whitespace)
    }
}

impl State {
    // Tokenize one source line
    fn line(&mut self, line: &str) -> StyledLine {
        let mut out = Spans::default();
        let rest = line.trim_start_matches(' ');
        let indent = line.len() - rest.len();
        if let Some(parent) = self.block_parent {
            if rest.trim().is_empty() || indent > parent {
                out.push(Style::PLAIN, &line[..indent]);
                out.push(STYLE_DATA_STRING, rest);
                return out.0;
            }
            self.block_parent = None;
        }
        out.push(Style::PLAIN, &line[..indent]);
        if let Some(quote) = self.quote {
            match quote_end(rest, quote) {
                Some(end) => {
                    out.push(STYLE_DATA_STRING, &rest[..end]);
                    self.quote = None;
                    self.value(&rest[end..], indent, &mut out);
                }
                None => out.push(STYLE_DATA_STRING, rest),
            }
        } else if self.flow_depth > 0 {
            self.value(rest, indent, &mut out);
        } else {
            self.node(rest, indent, &mut out);
        }
        out.0
    }

    // Tokenize the start of a block line: markers, entries and a key, then its value
    fn node(&mut self, mut rest: &str, indent: usize, out: &mut Spans) {
        if rest.starts_with(COMMENT) {
            out.push(STYLE_DATA_COMMENT, rest);
            return;
        }
        if indent == 0 && rest.starts_with(DIRECTIVE) {
            out.push(STYLE_DATA_META, rest);
            return;
        }
        for marker in [DOC_START, DOC_END] {
            if indent == 0 && rest.starts_with(marker) && is_boundary(&rest[marker.len()..]) {
                out.push(STYLE_DATA_PUNCT, marker);
                self.flow_depth = 0;
                self.value(&rest[marker.len()..], indent, out);
                return;
            }
        }
        let mut column = indent;
        // Sequence entries and complex keys, possibly nested on one line
        while rest.starts_with([SEQ_ENTRY, COMPLEX_KEY]) && is_boundary(&rest[1..]) {
            column = out_len(out);
            out.push(STYLE_DATA_PUNCT, &rest[..1]);
            let gap = space_len(&rest[1..]);
            out.push(Style::PLAIN, &rest[1..1 + gap]);
            rest = &rest[1 + gap..];
        }
        if let Some((key_end, colon)) = key_span(rest) {
            column = out_len(out);
            let key = &rest[..key_end];
            let style = if key == MERGE_KEY {
                STYLE_DATA_META
            } else {
                STYLE_DATA_KEY
            };
            out.push(style, key);
            out.push(Style::PLAIN, &rest[key_end..colon]);
            out.push(STYLE_DATA_PUNCT, &rest[colon..colon + 1]);
            rest = &rest[colon + 1..];
        }
        self.value(rest, column, out);
    }

    // Tokenize scalars, flow punctuation, properties and comments up to the end of the line
    fn value(&mut self, mut rest: &str, column: usize, out: &mut Spans) {
        while let Some(ch) = rest.chars().next() {
            let gap = space_len(rest);
            if gap > 0 {
                out.push(Style::PLAIN, &rest[..gap]);
                rest = &rest[gap..];
                continue;
            }
            let in_flow = self.flow_depth > 0;
            let len = if ch == COMMENT && out.after_space() {
                out.push(STYLE_DATA_COMMENT, rest);
                rest.len()
            } else if !in_flow
                && (ch == LITERAL_BLOCK || ch == FOLDED_BLOCK)
                && is_block_header(rest)
            {
                let len = 1 + rest[1..]
                    .find(|c| !BLOCK_HEADER_CHARS.contains(c))
                    .unwrap_or(rest.len() - 1);
                out.push(STYLE_DATA_PUNCT, &rest[..len]);
                self.block_parent = Some(column);
                len
            } else if ch == DOUBLE_QUOTE || ch == SINGLE_QUOTE {
                match quote_end(&rest[1..], ch) {
                    Some(end) => {
                        let len = 1 + end;
                        out.push(self.scalar_style(&rest[..len], &rest[len..]), &rest[..len]);
                        len
                    }
                    None => {
                        out.push(STYLE_DATA_STRING, rest);
                        self.quote = Some(ch);
                        rest.len()
                    }
                }
            } else if FLOW_CHARS.contains(ch) {
                if FLOW_OPENERS.contains(ch) {
                    self.flow_depth += 1;
                } else if FLOW_CLOSERS.contains(ch) {
                    self.flow_depth = self.flow_depth.saturating_sub(1);
                }
                out.push(STYLE_DATA_PUNCT, &rest[..1]);
                1
            } else if ch == MAP_VALUE && is_value_end(&rest[1..], in_flow) {
                out.push(STYLE_DATA_PUNCT, &rest[..1]);
                1
            } else if META_STARTS.contains(ch) {
                let len = rest
                    .find(|c: char| c.is_whitespace() || (in_flow && FLOW_CHARS.contains(c)))
                    .unwrap_or(rest.len());
                out.push(STYLE_DATA_META, &rest[..len]);
                len
            } else {
                let len = plain_end(rest, in_flow).max(ch.len_utf8());
                out.push(self.scalar_style(&rest[..len], &rest[len..]), &rest[..len]);
                len
            };
            rest = &rest[len..];
        }
    }

    // Style a scalar, as a key when a flow mapping colon follows it
    fn scalar_style(&self, text: &str, after: &str) -> Style {
        let next = after.trim_start_matches(' ');
        if self.flow_depth > 0 && next.starts_with(MAP_VALUE) && is_value_end(&next[1..], true) {
            return STYLE_DATA_KEY;
        }
        classify(text)
    }
}

// Count bytes already emitted on this line, which is the current column for ASCII indentation
fn out_len(out: &Spans) -> usize {
    out.0.iter().map(|(_, run)| run.len()).sum()
}

// Measure leading spaces and tabs
fn space_len(text: &str) -> usize {
    text.len() - text.trim_start_matches([' ', '\t']).len()
}

// Check that an indicator stands alone: end of line or whitespace next
fn is_boundary(after: &str) -> bool {
    after.is_empty() || after.starts_with([' ', '\t'])
}

// Check that a colon ends a key: whitespace or end next, or a flow char inside flow
fn is_value_end(after: &str, in_flow: bool) -> bool {
    is_boundary(after) || (in_flow && after.starts_with(|c| FLOW_CHARS.contains(c)))
}

// Check that a `|` or `>` opens a block scalar: only header chars, then space or end
fn is_block_header(text: &str) -> bool {
    let header = text[1..].trim_start_matches(|c| BLOCK_HEADER_CHARS.contains(c));
    is_boundary(header)
}

// Find the byte just past the closing quote, honouring escapes; None when unclosed
fn quote_end(text: &str, quote: char) -> Option<usize> {
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if quote == DOUBLE_QUOTE && ch == ESCAPE {
            chars.next();
        } else if ch == quote {
            if quote == SINGLE_QUOTE && chars.peek().is_some_and(|(_, next)| *next == quote) {
                chars.next();
                continue;
            }
            return Some(index + 1);
        }
    }
    None
}

// Find a block mapping key: returns the key's end and the colon's offset
fn key_span(text: &str) -> Option<(usize, usize)> {
    let first = text.chars().next()?;
    let key_end = if first == DOUBLE_QUOTE || first == SINGLE_QUOTE {
        1 + quote_end(&text[1..], first)?
    } else if KEY_FORBIDDEN_START.contains(first) && !text.starts_with(MERGE_KEY) {
        return None;
    } else {
        let mut prev_space = false;
        let mut found = None;
        for (index, ch) in text.char_indices() {
            let quoted = (ch == DOUBLE_QUOTE || ch == SINGLE_QUOTE) && prev_space;
            if (ch == COMMENT && prev_space) || quoted {
                return None;
            }
            if ch == MAP_VALUE && is_boundary(&text[index + 1..]) {
                found = Some(text[..index].trim_end().len());
                break;
            }
            prev_space = ch.is_whitespace();
        }
        found?
    };
    let colon = key_end + space_len(&text[key_end..]);
    let is_key = text[colon..].starts_with(MAP_VALUE) && is_boundary(&text[colon + 1..]);
    is_key.then_some((key_end, colon))
}

// Find where a plain scalar ends: a comment, or flow syntax inside a flow collection
fn plain_end(text: &str, in_flow: bool) -> usize {
    let mut prev_space = false;
    for (index, ch) in text.char_indices() {
        let stop = (ch == COMMENT && prev_space)
            || (in_flow && FLOW_CHARS.contains(ch))
            || (in_flow && ch == MAP_VALUE && is_value_end(&text[index + 1..], true));
        if stop {
            return text[..index].trim_end().len();
        }
        prev_space = ch.is_whitespace();
    }
    text.trim_end().len()
}

// Pick a style for a scalar by its content
fn classify(text: &str) -> Style {
    let lower = text.to_ascii_lowercase();
    if text.starts_with([DOUBLE_QUOTE, SINGLE_QUOTE]) {
        STYLE_DATA_STRING
    } else if LITERALS.contains(&lower.as_str()) {
        STYLE_DATA_LITERAL
    } else if is_number(&lower) {
        STYLE_DATA_NUMBER
    } else {
        STYLE_DATA_STRING
    }
}

// Recognise YAML ints, floats, hex, octal, infinity and not-a-number, lowercased
fn is_number(lower: &str) -> bool {
    let unsigned = lower.strip_prefix(SIGNS).unwrap_or(lower);
    if SPECIAL_FLOATS.contains(&unsigned) {
        return true;
    }
    if let Some(digits) = unsigned.strip_prefix(HEX_PREFIX) {
        return !digits.is_empty() && digits.chars().all(|c| c.is_ascii_hexdigit());
    }
    if let Some(digits) = unsigned.strip_prefix(OCTAL_PREFIX) {
        return !digits.is_empty() && digits.chars().all(|c| ('0'..='7').contains(&c));
    }
    let (mantissa, exponent) = match unsigned.split_once(EXPONENT) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let exponent_ok = exponent.is_none_or(|exp| {
        let digits = exp.strip_prefix(SIGNS).unwrap_or(exp);
        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
    });
    let (whole, fraction) = mantissa.split_once(DECIMAL_POINT).unwrap_or((mantissa, ""));
    let all_digits = |part: &str| part.chars().all(|c| c.is_ascii_digit());
    exponent_ok && whole.len() + fraction.len() > 0 && all_digits(whole) && all_digits(fraction)
}
