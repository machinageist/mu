// Author: machinageist
// Date: 2026-11-09
// Description: Decode raw terminal input into keys, and map keys to pager actions
// Notes: Decoding is the same in every mode; what a key does depends on whether the search prompt
// is open. A lone ESC counts as the Esc key only when no `[` or `O` follows it in the same read

use super::search::Direction;

const ESC: u8 = 0x1b;
const DEL: u8 = 0x7f;
const BACKSPACE: u8 = 0x08;
pub(super) const CTRL_B: u8 = 0x02;
pub(super) const CTRL_C: u8 = 0x03;
pub(super) const CTRL_D: u8 = 0x04;
pub(super) const CTRL_F: u8 = 0x06;
pub(super) const CTRL_U: u8 = 0x15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Quit,
    LineDown,
    LineUp,
    PageDown,
    PageUp,
    HalfDown,
    HalfUp,
    Top,
    Bottom,
    Search(Direction),
    NextMatch,
    PrevMatch,
    ClearSearch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Char(char),
    Ctrl(u8),
    Enter,
    Backspace,
    Esc,
    // A navigation key sent as an escape sequence: arrows, Home, End, PgUp, PgDn
    Nav(Action),
}

// Decode a read's worth of bytes into keys, skipping sequences with no meaning here
pub(super) fn parse_keys(bytes: &[u8]) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let introducer = bytes.get(index + 1).copied();
        if byte == ESC && matches!(introducer, Some(b'[' | b'O')) {
            let params_start = index + 2;
            let final_offset = bytes[params_start..]
                .iter()
                .position(|byte| (0x40..=0x7E).contains(byte));
            let Some(offset) = final_offset else {
                break;
            };
            let end = params_start + offset;
            keys.extend(escape_action(&bytes[params_start..end], bytes[end]).map(Key::Nav));
            index = end + 1;
            continue;
        }
        let (key, used) = match byte {
            ESC => (Some(Key::Esc), 1),
            b'\r' | b'\n' => (Some(Key::Enter), 1),
            DEL | BACKSPACE => (Some(Key::Backspace), 1),
            0x00..=0x1f => (Some(Key::Ctrl(byte)), 1),
            0x20..=0x7e => (Some(Key::Char(char::from(byte))), 1),
            _ => decode_utf8(&bytes[index..]),
        };
        keys.extend(key);
        index += used;
    }
    keys
}

// Decode one UTF-8 character, or skip one byte when it is not valid
fn decode_utf8(bytes: &[u8]) -> (Option<Key>, usize) {
    let length = match bytes[0] {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return (None, 1),
    };
    bytes
        .get(..length)
        .and_then(|encoded| std::str::from_utf8(encoded).ok())
        .and_then(|text| text.chars().next())
        .map_or((None, 1), |ch| (Some(Key::Char(ch)), length))
}

// Map a CSI or SS3 key sequence to an action
fn escape_action(params: &[u8], final_byte: u8) -> Option<Action> {
    match (final_byte, params) {
        (b'A', _) => Some(Action::LineUp),
        (b'B', _) => Some(Action::LineDown),
        (b'H', _) | (b'~', b"1" | b"7") => Some(Action::Top),
        (b'F', _) | (b'~', b"4" | b"8") => Some(Action::Bottom),
        (b'~', b"5") => Some(Action::PageUp),
        (b'~', b"6") => Some(Action::PageDown),
        _ => None,
    }
}

// What a key does while reading, with no prompt open
pub(super) fn normal_action(key: Key) -> Option<Action> {
    match key {
        Key::Char('q' | 'Q') | Key::Ctrl(CTRL_C) => Some(Action::Quit),
        Key::Char('j' | 'e') | Key::Enter => Some(Action::LineDown),
        Key::Char('k' | 'y') => Some(Action::LineUp),
        Key::Char(' ' | 'f') | Key::Ctrl(CTRL_F) => Some(Action::PageDown),
        Key::Char('b') | Key::Ctrl(CTRL_B) => Some(Action::PageUp),
        Key::Char('d') | Key::Ctrl(CTRL_D) => Some(Action::HalfDown),
        Key::Char('u') | Key::Ctrl(CTRL_U) => Some(Action::HalfUp),
        Key::Char('g' | '<') => Some(Action::Top),
        Key::Char('G' | '>') => Some(Action::Bottom),
        Key::Char('/') => Some(Action::Search(Direction::Forward)),
        Key::Char('?') => Some(Action::Search(Direction::Backward)),
        Key::Char('n') => Some(Action::NextMatch),
        Key::Char('N') => Some(Action::PrevMatch),
        Key::Esc => Some(Action::ClearSearch),
        Key::Nav(action) => Some(action),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Decode bytes and map them as in normal mode
    fn actions(bytes: &[u8]) -> Vec<Action> {
        parse_keys(bytes)
            .into_iter()
            .filter_map(normal_action)
            .collect()
    }

    #[test]
    fn letter_and_control_keys_map_to_actions() {
        use Action::*;
        assert_eq!(
            actions(b"jk bdugGq\x03\x06\x02"),
            [
                LineDown, LineUp, PageDown, PageUp, HalfDown, HalfUp, Top, Bottom, Quit, Quit,
                PageDown, PageUp
            ]
        );
    }

    #[test]
    fn search_keys_map_to_actions() {
        use Action::*;
        assert_eq!(
            actions(b"/?nN"),
            [
                Search(Direction::Forward),
                Search(Direction::Backward),
                NextMatch,
                PrevMatch
            ]
        );
    }

    #[test]
    fn escape_sequences_map_to_actions() {
        use Action::*;
        assert_eq!(
            actions(b"\x1b[A\x1b[B\x1b[5~\x1b[6~\x1b[H\x1b[F\x1b[1~\x1b[4~\x1bOA"),
            [
                LineUp, LineDown, PageUp, PageDown, Top, Bottom, Top, Bottom, LineUp
            ]
        );
    }

    #[test]
    fn wheel_burst_yields_one_action_per_arrow() {
        assert_eq!(actions(b"\x1b[B\x1b[B\x1b[B"), [Action::LineDown; 3]);
    }

    #[test]
    fn unknown_and_incomplete_sequences_are_ignored() {
        assert_eq!(actions(b"x\x1b[2~z"), []);
        assert_eq!(actions(b"j\x1b["), [Action::LineDown]);
    }

    #[test]
    fn a_lone_escape_is_the_esc_key() {
        assert_eq!(parse_keys(b"\x1b"), [Key::Esc]);
        assert_eq!(parse_keys(b"\x1bx"), [Key::Esc, Key::Char('x')]);
    }

    #[test]
    fn utf8_and_editing_keys_decode() {
        assert_eq!(
            parse_keys("é全\x7f\x08\r\x15".as_bytes()),
            [
                Key::Char('é'),
                Key::Char('全'),
                Key::Backspace,
                Key::Backspace,
                Key::Enter,
                Key::Ctrl(CTRL_U)
            ]
        );
        assert_eq!(parse_keys(b"\xff\xc3"), []);
    }
}
