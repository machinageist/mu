// Author: machinageist
// Date: 2026-11-09
// Description: JSON pretty-printing and syntax highlighting into styled lines
// Notes: One iterative parser with an explicit container stack serves both sniffing and
// re-indenting, so deep nesting cannot overflow; anything invalid is highlighted as written

use super::{Span, StyledLine};
use crate::style::{
    STYLE_DATA_KEY, STYLE_DATA_LITERAL, STYLE_DATA_NUMBER, STYLE_DATA_PUNCT, STYLE_DATA_STRING,
    Style,
};

// Byte order mark some editors put at the start of a file
const BOM: char = '\u{feff}';

// One indentation step in re-indented output
const INDENT: &str = "  ";

// Separator written after a key's colon
const KEY_GAP: &str = " ";

// Deeper documents are highlighted as written, since their indentation would dwarf the content
const MAX_PRETTY_DEPTH: usize = 256;

// Bare words JSON accepts as values
const LITERALS: [&str; 3] = ["true", "false", "null"];

// Structural punctuation
const OBJECT_OPEN: u8 = b'{';
const OBJECT_CLOSE: u8 = b'}';
const ARRAY_OPEN: u8 = b'[';
const ARRAY_CLOSE: u8 = b']';
const COLON: u8 = b':';
const COMMA: u8 = b',';
const QUOTE: u8 = b'"';
const BACKSLASH: u8 = b'\\';
const MINUS: u8 = b'-';
const PUNCTUATION: &str = "{}[]:,";

// Escapes allowed after a backslash, plus the one that takes four hex digits
const SIMPLE_ESCAPES: &[u8] = b"\"\\/bfnrt";
const UNICODE_ESCAPE: u8 = b'u';
const UNICODE_ESCAPE_DIGITS: usize = 4;

// Why the parser stopped before a clean end
enum Stop {
    // The input breaks JSON grammar or nests deeper than allowed
    Invalid,
    // The input is valid so far but ends mid-token or mid-container
    Truncated,
}

// What the parser expects next
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    Value,
    ValueOrClose,
    Key,
    KeyOrClose,
    Colon,
    CommaOrClose,
}

// Structural events the parser reports in document order
enum Event<'a> {
    Open(&'a str),
    Close(&'a str, bool),
    Key(&'a str),
    Scalar(Style, &'a str),
    Colon,
    Comma,
    EndTop,
}

// Check whether the head of an input looks like JSON
pub fn sniff(head: &str) -> bool {
    let body = head.strip_prefix(BOM).unwrap_or(head);
    let starts_container = body
        .trim_start()
        .starts_with([OBJECT_OPEN as char, ARRAY_OPEN as char]);
    starts_container && !matches!(parse(body, usize::MAX, |_| {}), Err(Stop::Invalid))
}

// Classify JSON text into styled lines, re-indented when the whole input parses
pub fn highlight(input: &str) -> Vec<StyledLine> {
    let body = input.strip_prefix(BOM).unwrap_or(input);
    let mut pretty = Pretty {
        lines: Vec::new(),
        line: Vec::new(),
        depth: 0,
        need_break: false,
    };
    match parse(body, MAX_PRETTY_DEPTH, |event| pretty.event(event)) {
        Ok(()) => pretty.lines,
        Err(_) => input.lines().map(highlight_line).collect(),
    }
}

// Walk the input as a stream of JSON values, reporting events and stopping at the first problem
fn parse<'a>(
    input: &'a str,
    max_depth: usize,
    mut emit: impl FnMut(Event<'a>),
) -> Result<(), Stop> {
    let bytes = input.as_bytes();
    let mut stack: Vec<u8> = Vec::new();
    let mut expect = Expect::Value;
    let mut i = 0;
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i == bytes.len() {
            return if stack.is_empty() && expect == Expect::Value {
                Ok(())
            } else {
                Err(Stop::Truncated)
            };
        }
        let byte = bytes[i];
        let closes = matches!(
            expect,
            Expect::ValueOrClose | Expect::KeyOrClose | Expect::CommaOrClose
        );
        // Closing the innermost container
        if closes && stack.last().is_some_and(|open| closer(*open) == byte) {
            let empty = expect != Expect::CommaOrClose;
            stack.pop();
            emit(Event::Close(&input[i..i + 1], empty));
            i += 1;
            expect = after_value(&stack, &mut emit);
            continue;
        }
        match expect {
            Expect::Value | Expect::ValueOrClose => {
                if byte == OBJECT_OPEN || byte == ARRAY_OPEN {
                    if stack.len() >= max_depth {
                        return Err(Stop::Invalid);
                    }
                    stack.push(byte);
                    emit(Event::Open(&input[i..i + 1]));
                    i += 1;
                    expect = if byte == OBJECT_OPEN {
                        Expect::KeyOrClose
                    } else {
                        Expect::ValueOrClose
                    };
                    continue;
                }
                let (end, style) = if byte == QUOTE {
                    (scan_string(bytes, i)?, STYLE_DATA_STRING)
                } else if byte == MINUS || byte.is_ascii_digit() {
                    (scan_number(bytes, i)?, STYLE_DATA_NUMBER)
                } else {
                    (scan_literal(&input[i..])? + i, STYLE_DATA_LITERAL)
                };
                if style != STYLE_DATA_STRING && bytes.get(end).is_some_and(|b| is_word(*b)) {
                    return Err(Stop::Invalid);
                }
                emit(Event::Scalar(style, &input[i..end]));
                i = end;
                expect = after_value(&stack, &mut emit);
            }
            Expect::Key | Expect::KeyOrClose => {
                if byte != QUOTE {
                    return Err(Stop::Invalid);
                }
                let end = scan_string(bytes, i)?;
                emit(Event::Key(&input[i..end]));
                i = end;
                expect = Expect::Colon;
            }
            Expect::Colon => {
                if byte != COLON {
                    return Err(Stop::Invalid);
                }
                emit(Event::Colon);
                i += 1;
                expect = Expect::Value;
            }
            Expect::CommaOrClose => {
                if byte != COMMA {
                    return Err(Stop::Invalid);
                }
                emit(Event::Comma);
                i += 1;
                expect = if stack.last() == Some(&OBJECT_OPEN) {
                    Expect::Key
                } else {
                    Expect::Value
                };
            }
        }
    }
}

// Decide what follows a finished value, closing a top-level value when the stack is empty
fn after_value<'a>(stack: &[u8], emit: &mut impl FnMut(Event<'a>)) -> Expect {
    if stack.is_empty() {
        emit(Event::EndTop);
        Expect::Value
    } else {
        Expect::CommaOrClose
    }
}

// Match an opening bracket to its closer
fn closer(open: u8) -> u8 {
    if open == OBJECT_OPEN {
        OBJECT_CLOSE
    } else {
        ARRAY_CLOSE
    }
}

// Check whether a byte would glue onto a number or literal
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_'
}

// Find the end of a quoted string starting at a quote, validating escapes
fn scan_string(bytes: &[u8], start: usize) -> Result<usize, Stop> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            QUOTE => return Ok(i + 1),
            BACKSLASH => {
                let Some(&escape) = bytes.get(i + 1) else {
                    return Err(Stop::Truncated);
                };
                if escape == UNICODE_ESCAPE {
                    for offset in 0..UNICODE_ESCAPE_DIGITS {
                        match bytes.get(i + 2 + offset) {
                            None => return Err(Stop::Truncated),
                            Some(digit) if digit.is_ascii_hexdigit() => {}
                            Some(_) => return Err(Stop::Invalid),
                        }
                    }
                    i += 2 + UNICODE_ESCAPE_DIGITS;
                } else if SIMPLE_ESCAPES.contains(&escape) {
                    i += 2;
                } else {
                    return Err(Stop::Invalid);
                }
            }
            byte if byte < b' ' => return Err(Stop::Invalid),
            _ => i += 1,
        }
    }
    Err(Stop::Truncated)
}

// Find the end of a number per the JSON grammar
fn scan_number(bytes: &[u8], start: usize) -> Result<usize, Stop> {
    let mut i = start;
    if bytes[i] == MINUS {
        i += 1;
    }
    // Integer part, a lone zero or digits without a leading zero
    match bytes.get(i) {
        None => return Err(Stop::Truncated),
        Some(b'0') => i += 1,
        Some(digit) if digit.is_ascii_digit() => i = skip_digits(bytes, i),
        Some(_) => return Err(Stop::Invalid),
    }
    // Optional fraction
    if bytes.get(i) == Some(&b'.') {
        i = required_digits(bytes, i + 1)?;
    }
    // Optional exponent
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        i = required_digits(bytes, i)?;
    }
    Ok(i)
}

// Skip a run of digits
fn skip_digits(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    i
}

// Skip a run of at least one digit
fn required_digits(bytes: &[u8], i: usize) -> Result<usize, Stop> {
    match bytes.get(i) {
        None => Err(Stop::Truncated),
        Some(digit) if digit.is_ascii_digit() => Ok(skip_digits(bytes, i)),
        Some(_) => Err(Stop::Invalid),
    }
}

// Measure a literal at the start of the text, allowing one cut short by the end of input
fn scan_literal(text: &str) -> Result<usize, Stop> {
    for literal in LITERALS {
        if text.starts_with(literal) {
            return Ok(literal.len());
        }
        if !text.is_empty() && literal.starts_with(text) {
            return Err(Stop::Truncated);
        }
    }
    Err(Stop::Invalid)
}

// Builder for re-indented lines
struct Pretty {
    lines: Vec<StyledLine>,
    line: StyledLine,
    depth: usize,
    need_break: bool,
}

impl Pretty {
    // Lay out one parser event
    fn event(&mut self, event: Event) {
        match event {
            Event::Open(bracket) => {
                self.begin_item();
                self.push(STYLE_DATA_PUNCT, bracket);
                self.depth += 1;
                self.need_break = true;
            }
            Event::Close(bracket, empty) => {
                self.depth -= 1;
                if !empty {
                    self.break_line();
                }
                self.need_break = false;
                self.push(STYLE_DATA_PUNCT, bracket);
            }
            Event::Key(text) => {
                self.begin_item();
                self.push(STYLE_DATA_KEY, text);
            }
            Event::Scalar(style, text) => {
                self.begin_item();
                self.push(style, text);
            }
            Event::Colon => {
                self.push(STYLE_DATA_PUNCT, ":");
                self.push(Style::PLAIN, KEY_GAP);
            }
            Event::Comma => {
                self.push(STYLE_DATA_PUNCT, ",");
                self.need_break = true;
            }
            Event::EndTop => {
                self.lines.push(std::mem::take(&mut self.line));
                self.need_break = false;
            }
        }
    }

    // Start a new indented line when the previous item asked for one
    fn begin_item(&mut self) {
        if self.need_break {
            self.break_line();
            self.need_break = false;
        }
    }

    // Finish the current line and indent the next one to the current depth
    fn break_line(&mut self) {
        self.lines.push(std::mem::take(&mut self.line));
        if self.depth > 0 {
            self.line.push((Style::PLAIN, INDENT.repeat(self.depth)));
        }
    }

    // Append a span
    fn push(&mut self, style: Style, text: &str) {
        self.line.push((style, text.to_owned()));
    }
}

// Highlight one line as written, tolerating anything
fn highlight_line(line: &str) -> StyledLine {
    let mut spans: StyledLine = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        let (end, style) = if byte == QUOTE {
            let end = loose_string_end(bytes, i);
            let rest = line[end..].trim_start();
            let style = if rest.as_bytes().first() == Some(&COLON) {
                STYLE_DATA_KEY
            } else {
                STYLE_DATA_STRING
            };
            (end, style)
        } else if PUNCTUATION.as_bytes().contains(&byte) {
            (i + 1, STYLE_DATA_PUNCT)
        } else if byte == MINUS || byte.is_ascii_digit() || byte.is_ascii_alphabetic() {
            let end = i + bytes[i..]
                .iter()
                .take_while(|b| is_word(**b) || matches!(b, b'+' | b'-'))
                .count();
            let word = &line[i..end];
            let style = if LITERALS.contains(&word) {
                STYLE_DATA_LITERAL
            } else if byte == MINUS || byte.is_ascii_digit() {
                STYLE_DATA_NUMBER
            } else {
                Style::PLAIN
            };
            (end, style)
        } else {
            // Any other char, kept whole so multibyte text is never split
            let width = line[i..].chars().next().map_or(1, char::len_utf8);
            (i + width, Style::PLAIN)
        };
        push_merged(&mut spans, style, &line[i..end]);
        i = end;
    }
    spans
}

// Find where a string ends on one line, or the line's end when it is unterminated
fn loose_string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            QUOTE => return i + 1,
            BACKSLASH => i += 2,
            _ => i += 1,
        }
    }
    bytes.len()
}

// Append text, extending the last span when the style matches
fn push_merged(spans: &mut Vec<Span>, style: Style, text: &str) {
    match spans.last_mut() {
        Some((last, run)) if *last == style => run.push_str(text),
        _ => spans.push((style, text.to_owned())),
    }
}
