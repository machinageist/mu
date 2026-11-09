// Author: machinageist
// Date: 2026-11-09
// Description: TOML syntax highlighting into styled lines, plus a conservative content sniff
// Notes: Text is never reformatted; a small lexer carries multi-line strings and open
// arrays or inline tables across lines, and the sniff reuses it for continuation lines

use super::StyledLine;
use crate::style::{
    STYLE_DATA_COMMENT, STYLE_DATA_KEY, STYLE_DATA_LITERAL, STYLE_DATA_NUMBER, STYLE_DATA_PUNCT,
    STYLE_DATA_SECTION, STYLE_DATA_STRING, Style,
};

// Syntax bytes
const COMMENT: u8 = b'#';
const BASIC_QUOTE: u8 = b'"';
const LITERAL_QUOTE: u8 = b'\'';
const ESCAPE: u8 = b'\\';
const EQUALS: u8 = b'=';
const COMMA: u8 = b',';
const DOT: u8 = b'.';
const ARRAY_OPEN: u8 = b'[';
const ARRAY_CLOSE: u8 = b']';
const TABLE_OPEN: u8 = b'{';
const TABLE_CLOSE: u8 = b'}';
const DATE_SEPARATOR: u8 = b'-';
const TIME_SEPARATOR: u8 = b':';

// Bytes that end a bare word in value position
const WORD_BREAKS: &[u8] = b" \t=,[]{}#\"'";

// Bytes allowed in decimal ints and floats after the first digit
const DECIMAL_BYTES: &[u8] = b"0123456789_.eE+-";

// Bytes allowed in dates, times and offset date-times
const DATE_TIME_BYTES: &[u8] = b"0123456789-:.TtZz+ ";

// Quotes that open or close a multi-line string
const TRIPLE_QUOTES: usize = 3;

// Longest quote run that closes a multi-line string: two content quotes plus the closer
const MAX_CLOSING_QUOTES: usize = 5;

// Shortest date-time worth classifying, a bare HH:MM
const MIN_DATE_TIME_LEN: usize = 5;

// Positions of the separators in YYYY-MM-DD and HH:MM
const DATE_SEPARATOR_AT: usize = 4;
const TIME_SEPARATOR_AT: usize = 2;

// Value keywords
const BOOLEANS: [&str; 2] = ["true", "false"];
const SPECIAL_FLOATS: [&str; 2] = ["inf", "nan"];

// Integer prefixes and their radix
const RADIX_PREFIXES: [(&str, u32); 3] = [("0x", 16), ("0o", 8), ("0b", 2)];

// Sniff thresholds
const MIN_SIGNIFICANT_LINES: usize = 2;
const MIN_SHAPED_PERCENT: usize = 80;
const PERCENT: usize = 100;

// Kind of multi-line string left open at the end of a line
#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    Basic,
    Literal,
}

// Lexer state carried from one line to the next
struct Lexer {
    open_string: Option<Quote>,
    nesting: Vec<u8>,
    expect_key: bool,
}

impl Lexer {
    // Start at the top of a document, expecting a key or a table header
    fn new() -> Lexer {
        Lexer {
            open_string: None,
            nesting: Vec::new(),
            expect_key: true,
        }
    }

    // Whether the next line continues a multi-line string or an open array or table
    fn continuing(&self) -> bool {
        self.open_string.is_some() || !self.nesting.is_empty()
    }

    // Split one source line into styled spans, updating the carried state
    fn line(&mut self, line: &str) -> StyledLine {
        let bytes = line.as_bytes();
        let mut spans = StyledLine::new();
        let mut i = 0;

        // Finish a multi-line string opened on an earlier line
        if let Some(quote) = self.open_string {
            i = self.string_body(bytes, 0, quote);
            push(&mut spans, STYLE_DATA_STRING, &line[..i]);
        }

        // A top-level line starts with a key or a table header
        let at_top = !self.continuing() && i == 0;
        if at_top {
            self.expect_key = true;
            let indent = skip_blanks(bytes, 0);
            push(&mut spans, Style::PLAIN, &line[..indent]);
            i = indent;
            if bytes.get(i) == Some(&ARRAY_OPEN) {
                let end = header_end(bytes, i);
                push(&mut spans, STYLE_DATA_SECTION, &line[i..end]);
                i = end;
            }
        }

        // Tokens until the end of the line
        while i < bytes.len() {
            let start = i;
            let style = match bytes[i] {
                b' ' | b'\t' => {
                    i = skip_blanks(bytes, i);
                    Style::PLAIN
                }
                COMMENT => {
                    i = bytes.len();
                    STYLE_DATA_COMMENT
                }
                BASIC_QUOTE | LITERAL_QUOTE => self.string(bytes, &mut i),
                EQUALS => {
                    self.expect_key = false;
                    i += 1;
                    STYLE_DATA_PUNCT
                }
                COMMA => {
                    self.expect_key = self.nesting.last() == Some(&TABLE_OPEN);
                    i += 1;
                    STYLE_DATA_PUNCT
                }
                TABLE_OPEN | ARRAY_OPEN => {
                    self.nesting.push(bytes[i]);
                    self.expect_key = bytes[i] == TABLE_OPEN;
                    i += 1;
                    STYLE_DATA_PUNCT
                }
                TABLE_CLOSE | ARRAY_CLOSE => {
                    self.nesting.pop();
                    self.expect_key = false;
                    i += 1;
                    STYLE_DATA_PUNCT
                }
                DOT if self.expect_key => {
                    i += 1;
                    STYLE_DATA_PUNCT
                }
                _ => {
                    i = self.word_end(bytes, i);
                    if self.expect_key {
                        STYLE_DATA_KEY
                    } else {
                        scalar_style(&line[start..i])
                    }
                }
            };
            push(&mut spans, style, &line[start..i]);
        }
        spans
    }

    // Consume a quoted key or string starting at a quote, returning its style
    fn string(&mut self, bytes: &[u8], i: &mut usize) -> Style {
        let quote_byte = bytes[*i];
        let quote = if quote_byte == BASIC_QUOTE {
            Quote::Basic
        } else {
            Quote::Literal
        };
        let run = quote_run(bytes, *i);
        if run >= TRIPLE_QUOTES {
            self.open_string = Some(quote);
            *i = self.string_body(bytes, *i + TRIPLE_QUOTES, quote);
            return STYLE_DATA_STRING;
        }
        let mut j = *i + 1;
        while j < bytes.len() {
            if bytes[j] == ESCAPE && quote == Quote::Basic {
                j += 2;
            } else if bytes[j] == quote_byte {
                j += 1;
                break;
            } else {
                j += 1;
            }
        }
        *i = j.min(bytes.len());
        if self.expect_key {
            STYLE_DATA_KEY
        } else {
            STYLE_DATA_STRING
        }
    }

    // Scan the inside of a multi-line string, returning where it closes or the line end
    fn string_body(&mut self, bytes: &[u8], from: usize, quote: Quote) -> usize {
        let quote_byte = match quote {
            Quote::Basic => BASIC_QUOTE,
            Quote::Literal => LITERAL_QUOTE,
        };
        let mut j = from;
        while j < bytes.len() {
            if bytes[j] == ESCAPE && quote == Quote::Basic {
                j += 2;
            } else if bytes[j] == quote_byte {
                let run = quote_run(bytes, j);
                if run >= TRIPLE_QUOTES {
                    self.open_string = None;
                    return j + run.min(MAX_CLOSING_QUOTES);
                }
                j += run;
            } else {
                j += 1;
            }
        }
        bytes.len()
    }

    // Find the end of a bare key or value word, joining a date to a following time
    fn word_end(&self, bytes: &[u8], from: usize) -> usize {
        let breaks_at = |b: u8| WORD_BREAKS.contains(&b) || (self.expect_key && b == DOT);
        let run_end = |start: usize| {
            let mut j = start;
            while j < bytes.len() && !breaks_at(bytes[j]) {
                j += 1;
            }
            j
        };
        let end = run_end(from);
        let time_start = end + 1;
        let joins_time = !self.expect_key
            && is_date(&bytes[from..end])
            && bytes.get(end) == Some(&b' ')
            && bytes.get(time_start..).is_some_and(is_time);
        if joins_time { run_end(time_start) } else { end }
    }
}

// Append a span unless it is empty
fn push(spans: &mut StyledLine, style: Style, text: &str) {
    if !text.is_empty() {
        spans.push((style, text.to_owned()));
    }
}

// Skip spaces and tabs
fn skip_blanks(bytes: &[u8], from: usize) -> usize {
    let mut j = from;
    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
        j += 1;
    }
    j
}

// Count identical quote bytes starting at a position
fn quote_run(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .take_while(|&&b| b == bytes[from])
        .count()
}

// Find where a table header ends: after its closing brackets, or before a comment
fn header_end(bytes: &[u8], from: usize) -> usize {
    let mut j = from;
    while j < bytes.len() && bytes[j] == ARRAY_OPEN {
        j += 1;
    }
    while j < bytes.len() {
        match bytes[j] {
            BASIC_QUOTE | LITERAL_QUOTE => {
                let quote_byte = bytes[j];
                j += 1;
                while j < bytes.len() && bytes[j] != quote_byte {
                    j += if bytes[j] == ESCAPE && quote_byte == BASIC_QUOTE {
                        2
                    } else {
                        1
                    };
                }
                j = (j + 1).min(bytes.len());
            }
            ARRAY_CLOSE => {
                while j < bytes.len() && bytes[j] == ARRAY_CLOSE {
                    j += 1;
                }
                return j;
            }
            COMMENT => return j,
            _ => j += 1,
        }
    }
    bytes.len()
}

// Check for a date prefix shaped YYYY-MM-DD
fn is_date(bytes: &[u8]) -> bool {
    bytes.len() > DATE_SEPARATOR_AT
        && bytes[..DATE_SEPARATOR_AT].iter().all(u8::is_ascii_digit)
        && bytes[DATE_SEPARATOR_AT] == DATE_SEPARATOR
}

// Check for a time prefix shaped HH:MM
fn is_time(bytes: &[u8]) -> bool {
    bytes.len() > TIME_SEPARATOR_AT
        && bytes[..TIME_SEPARATOR_AT].iter().all(u8::is_ascii_digit)
        && bytes[TIME_SEPARATOR_AT] == TIME_SEPARATOR
}

// Check for a date, time or date-time made only of date-time characters
fn is_date_time(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() >= MIN_DATE_TIME_LEN
        && (is_date(bytes) || is_time(bytes))
        && bytes.iter().all(|b| DATE_TIME_BYTES.contains(b))
}

// Check for an integer or float in any TOML notation
fn is_number(word: &str) -> bool {
    let unsigned = word.strip_prefix(['+', '-']).unwrap_or(word);
    if SPECIAL_FLOATS.contains(&unsigned) {
        return true;
    }
    for (prefix, radix) in RADIX_PREFIXES {
        if let Some(digits) = unsigned.strip_prefix(prefix) {
            return !digits.is_empty() && digits.chars().all(|c| c == '_' || c.is_digit(radix));
        }
    }
    unsigned.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && unsigned.bytes().all(|b| DECIMAL_BYTES.contains(&b))
}

// Classify a bare value word
fn scalar_style(word: &str) -> Style {
    if BOOLEANS.contains(&word) || is_date_time(word) {
        STYLE_DATA_LITERAL
    } else if is_number(word) {
        STYLE_DATA_NUMBER
    } else {
        Style::PLAIN
    }
}

// Find the end of a dotted key path of bare or quoted keys, if one starts the text
fn key_path_end(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut j = skip_blanks(bytes, 0);
    loop {
        let start = j;
        match bytes.get(j) {
            Some(&quote @ (BASIC_QUOTE | LITERAL_QUOTE)) => {
                j += 1;
                while j < bytes.len() && bytes[j] != quote {
                    j += if bytes[j] == ESCAPE && quote == BASIC_QUOTE {
                        2
                    } else {
                        1
                    };
                }
                if j >= bytes.len() {
                    return None;
                }
                j += 1;
            }
            _ => {
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-')
                {
                    j += 1;
                }
                if j == start {
                    return None;
                }
            }
        }
        j = skip_blanks(bytes, j);
        if bytes.get(j) != Some(&DOT) {
            return Some(j);
        }
        j += 1;
        j = skip_blanks(bytes, j);
    }
}

// Check for a whole-line [table] or [[array.table]] header
fn is_header(trimmed: &str) -> bool {
    let Some(inner) = trimmed.strip_prefix('[') else {
        return false;
    };
    let double = inner.starts_with('[');
    let inner = if double { &inner[1..] } else { inner };
    let Some(end) = key_path_end(inner) else {
        return false;
    };
    let closer = if double { "]]" } else { "]" };
    inner[end..]
        .strip_prefix(closer)
        .is_some_and(|rest| rest.trim().is_empty() || rest.trim_start().starts_with('#'))
}

// Check for a key = value line whose value starts like a TOML value
fn is_pair(trimmed: &str) -> bool {
    let Some(end) = key_path_end(trimmed) else {
        return false;
    };
    let Some(value) = trimmed[end..].strip_prefix('=') else {
        return false;
    };
    let value = value.trim();
    if value.starts_with(['"', '\'', '[', '{']) {
        return true;
    }
    let scalar = value.split(" #").next().unwrap_or(value).trim_end();
    scalar_style(scalar) != Style::PLAIN
}

// Check whether the head of an input looks like TOML
pub fn sniff(head: &str) -> bool {
    let mut lexer = Lexer::new();
    let mut significant = 0;
    let mut shaped = 0;
    let mut pairs = 0;
    for line in head.lines() {
        let trimmed = line.trim();
        let skippable = trimmed.is_empty() || trimmed.starts_with('#');
        if lexer.open_string.is_none() && skippable {
            continue;
        }
        significant += 1;
        let pair = !lexer.continuing() && is_pair(trimmed);
        if lexer.continuing() || pair || is_header(trimmed) {
            shaped += 1;
            pairs += usize::from(pair);
            lexer.line(line);
        }
    }
    significant >= MIN_SIGNIFICANT_LINES
        && pairs > 0
        && shaped * PERCENT >= significant * MIN_SHAPED_PERCENT
}

// Classify TOML text into styled lines
pub fn highlight(input: &str) -> Vec<StyledLine> {
    let mut lexer = Lexer::new();
    input.lines().map(|line| lexer.line(line)).collect()
}
