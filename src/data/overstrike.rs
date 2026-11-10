// Author: machinageist
// Date: 2026-11-09
// Description: Formatted man output (`man ls | mu`) with backspace overstrikes turned into styles
// Notes: `X\bX` is bold and `_\bX` underlined, the way nroff marks them for a terminal; SGR bold
// and underline from `groff -T utf8` are understood too, and any other escape is dropped

use super::StyledLine;
use crate::render::Heading;
use crate::style::{ATTR_BOLD, ATTR_UNDERLINE, FG_DEFAULT, STYLE_HEADINGS, Style};

const BACKSPACE: char = '\u{8}';
const ESC: char = '\u{1b}';
const BEL: char = '\u{7}';
const UNDERSCORE: char = '_';
// `+` struck over `o` is nroff's bullet
const BULLET_PARTS: (char, char) = ('+', 'o');
const BULLET: char = '•';

// Escape sequences: CSI ends at a final byte, OSC at BEL or ST
const CSI_OPEN: char = '[';
const OSC_OPEN: char = ']';
const STRING_TERMINATOR: char = '\\';
const SGR_FINAL: char = 'm';
const CSI_FINALS: std::ops::RangeInclusive<char> = '@'..='~';
const SGR_RESET: u8 = 0;
const SGR_BOLD: u8 = 1;
const SGR_ITALIC: u8 = 3;
const SGR_UNDERLINE: u8 = 4;
const SGR_NORMAL: u8 = 22;
const SGR_NO_ITALIC: u8 = 23;
const SGR_NO_UNDERLINE: u8 = 24;

// Sniffing: this many overstrikes, or SGR styles under a man page header
const MIN_OVERSTRIKES: usize = 8;
const MIN_SGR_STYLES: usize = 4;
const SGR_STYLE_MARKS: [&str; 2] = ["\u{1b}[1m", "\u{1b}[4m"];
const SECTION_OPEN: char = '(';
const SECTION_CLOSE: char = ')';

// Section headings start in column 0, subsections at most this far in
const MAX_SUBSECTION_INDENT: usize = 3;
const SECTION_LEVEL: u8 = 2;
const SUBSECTION_LEVEL: u8 = 3;

// True when input carries terminal formatting rather than roff source
pub fn is_formatted(input: &str) -> bool {
    input.contains(BACKSPACE) || input.contains(ESC)
}

// Check whether the head of an input is formatted man output
pub fn sniff(head: &str) -> bool {
    let overstrikes = head.matches(BACKSPACE).count();
    if overstrikes >= MIN_OVERSTRIKES {
        return true;
    }
    let styles: usize = SGR_STYLE_MARKS
        .iter()
        .map(|mark| head.matches(mark).count())
        .sum();
    styles >= MIN_SGR_STYLES && has_page_header(head)
}

// A first line like `LS(1)   General Commands Manual   LS(1)`
fn has_page_header(head: &str) -> bool {
    let Some(first) = head.lines().find(|line| !line.trim().is_empty()) else {
        return false;
    };
    let plain: String = cells(first).into_iter().map(|(ch, _)| ch).collect();
    let words: Vec<&str> = plain.split_whitespace().collect();
    let is_title = |word: &&str| word.ends_with(SECTION_CLOSE) && word.contains(SECTION_OPEN);
    words.len() >= 2 && words.first().is_some_and(is_title) && words.last().is_some_and(is_title)
}

// Resolve one line into characters with their attribute bits
fn cells(line: &str) -> Vec<(char, u8)> {
    let mut out: Vec<(char, u8)> = Vec::with_capacity(line.len());
    let mut attrs = 0;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            BACKSPACE => {
                let Some(&after) = chars.peek() else {
                    continue;
                };
                let Some((before, mut bits)) = out.pop() else {
                    continue;
                };
                chars.next();
                let shown = if before == after {
                    bits |= ATTR_BOLD;
                    after
                } else if before == UNDERSCORE {
                    bits |= ATTR_UNDERLINE;
                    after
                } else if after == UNDERSCORE {
                    bits |= ATTR_UNDERLINE;
                    before
                } else if (before, after) == BULLET_PARTS {
                    BULLET
                } else {
                    after
                };
                out.push((shown, bits));
            }
            ESC => attrs = escape(&mut chars, attrs),
            _ => out.push((ch, attrs)),
        }
    }
    out
}

// Consume one escape sequence, returning the attributes after it
fn escape(chars: &mut std::iter::Peekable<std::str::Chars>, attrs: u8) -> u8 {
    match chars.next() {
        Some(CSI_OPEN) => {
            let mut params = String::new();
            for ch in chars.by_ref() {
                if CSI_FINALS.contains(&ch) {
                    return if ch == SGR_FINAL {
                        apply_sgr(&params, attrs)
                    } else {
                        attrs
                    };
                }
                params.push(ch);
            }
            attrs
        }
        Some(OSC_OPEN) => {
            while let Some(ch) = chars.next() {
                if ch == BEL || (ch == ESC && chars.next_if_eq(&STRING_TERMINATOR).is_some()) {
                    break;
                }
            }
            attrs
        }
        _ => attrs,
    }
}

// Apply SGR parameters to attribute bits; only bold, underline and resets are kept
fn apply_sgr(params: &str, mut attrs: u8) -> u8 {
    for param in params.split(';') {
        match param.parse::<u8>().unwrap_or(SGR_RESET) {
            SGR_RESET => attrs = 0,
            SGR_BOLD => attrs |= ATTR_BOLD,
            SGR_ITALIC | SGR_UNDERLINE => attrs |= ATTR_UNDERLINE,
            SGR_NORMAL => attrs &= !ATTR_BOLD,
            SGR_NO_ITALIC | SGR_NO_UNDERLINE => attrs &= !ATTR_UNDERLINE,
            _ => {}
        }
    }
    attrs
}

// Styled lines for the printer, plus each heading's text, level and input line
pub fn lines(input: &str) -> (Vec<StyledLine>, Vec<Heading>) {
    let resolved: Vec<Vec<(char, u8)>> = input.lines().map(cells).collect();
    let content: Vec<usize> = (0..resolved.len())
        .filter(|at| resolved[*at].iter().any(|(ch, _)| !ch.is_whitespace()))
        .collect();
    let (first, last) = (content.first().copied(), content.last().copied());
    let mut headings = Vec::new();
    let mut styled = Vec::with_capacity(resolved.len());
    for (at, line) in resolved.iter().enumerate() {
        let is_edge = Some(at) == first || Some(at) == last;
        match heading_level(line).filter(|_| !is_edge) {
            Some(level) => {
                let text: String = line.iter().map(|(ch, _)| *ch).collect();
                styled.push(vec![(STYLE_HEADINGS[usize::from(level) - 1], text.clone())]);
                headings.push(Heading {
                    level,
                    text: text.trim().to_owned(),
                    line: at,
                });
            }
            None => styled.push(spans(line)),
        }
    }
    (styled, headings)
}

// A heading is a line, at most a little indented, whose visible text is all bold
fn heading_level(line: &[(char, u8)]) -> Option<u8> {
    let indent = line.iter().take_while(|(ch, _)| ch.is_whitespace()).count();
    let mut visible = line.iter().filter(|(ch, _)| !ch.is_whitespace()).peekable();
    visible.peek()?;
    if !visible.all(|(_, bits)| bits & ATTR_BOLD != 0) {
        return None;
    }
    match indent {
        0 => Some(SECTION_LEVEL),
        1..=MAX_SUBSECTION_INDENT => Some(SUBSECTION_LEVEL),
        _ => None,
    }
}

// Group a line's characters into runs of one style
fn spans(line: &[(char, u8)]) -> StyledLine {
    let mut out: StyledLine = Vec::new();
    for &(ch, bits) in line {
        let style = Style::new(bits, FG_DEFAULT);
        match out.last_mut() {
            Some((last, text)) if *last == style => text.push(ch),
            _ => out.push((style, ch.to_string())),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Plain text of resolved cells
    fn text(line: &str) -> String {
        cells(line).into_iter().map(|(ch, _)| ch).collect()
    }

    #[test]
    fn overstrikes_become_bold_and_underline() {
        let line = "N\u{8}NA\u{8}A _\u{8}f_\u{8}x +\u{8}o";
        assert_eq!(text(line), "NA fx •");
        let bits: Vec<u8> = cells(line).into_iter().map(|(_, bits)| bits).collect();
        assert_eq!(bits[0], ATTR_BOLD);
        assert_eq!(bits[3], ATTR_UNDERLINE);
        assert_eq!(bits[2], 0);
    }

    #[test]
    fn stray_backspaces_and_escapes_vanish() {
        assert_eq!(text("\u{8}a\u{8}"), "a");
        assert_eq!(
            text(
                "\u{1b}[1mbold\u{1b}[0m \u{1b}]8;;http://x\u{1b}\\link\u{1b}]8;;\u{1b}\\ \u{1b}[2Jz"
            ),
            "bold link z"
        );
        let bits: Vec<u8> = cells("\u{1b}[1mb\u{1b}[22mn")
            .into_iter()
            .map(|(_, bits)| bits)
            .collect();
        assert_eq!(bits, [ATTR_BOLD, 0]);
    }

    #[test]
    fn sniff_needs_overstrikes_or_a_styled_page() {
        assert!(sniff(&"B\u{8}B".repeat(8)));
        assert!(!sniff("plain text\n"));
        let styled = "LS(1)   Manual   LS(1)\n\n\u{1b}[1mNAME\u{1b}[0m\n  \u{1b}[1mls\u{1b}[0m \u{1b}[4mx\u{1b}[0m \u{1b}[1my\u{1b}[0m\n";
        assert!(sniff(styled));
        assert!(!sniff(
            "\u{1b}[1mERROR\u{1b}[0m \u{1b}[1ma\u{1b}[1mb\u{1b}[4mc\n"
        ));
    }

    #[test]
    fn bold_lines_at_the_margin_are_headings() {
        let page = "LS(1)  Manual  LS(1)\n\nN\u{8}NA\u{8}AM\u{8}ME\u{8}E\n     l\u{8}ls\u{8}s - list\n   T\u{8}Th\u{8}he\u{8}e F\u{8}Fo\u{8}or\u{8}rm\u{8}m\n     -\u{8}-a\u{8}a\n\nmacOS  2020  macOS\n";
        let (_, headings) = lines(page);
        let found: Vec<(u8, &str, usize)> = headings
            .iter()
            .map(|heading| (heading.level, heading.text.as_str(), heading.line))
            .collect();
        assert_eq!(found, [(2, "NAME", 2), (3, "The Form", 4)]);
    }
}
