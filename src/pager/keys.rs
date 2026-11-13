// Author: machinageist
// Date: 2026-11-09
// Description: Decode raw terminal input into keys, and map keys to pager actions
// Notes: Decoding is the same in every mode; what a key does depends on whether the search prompt
// is open. The incremental decoder holds partial input until another read or an input deadline;
// parse_keys keeps the original one-read behavior for callers that already have complete input

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
    Outline,
    ScrollLeft,
    ScrollRight,
    NextLink,
    PrevLink,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Char(char),
    Ctrl(u8),
    Enter,
    Backspace,
    Tab,
    Esc,
    // A navigation key sent as an escape sequence: arrows, Home, End, PgUp, PgDn
    Nav(Action),
}

// Retained input is bounded even if a terminal sends a sequence with no final byte.
const MAX_PENDING_BYTES: usize = 256;

// Decode terminal reads without making read boundaries into key boundaries.
#[derive(Default)]
pub(super) struct Decoder {
    pending: Vec<u8>,
    // After an invalid or oversized sequence, consume its suffix through the final byte
    // instead of interpreting that suffix as ordinary pager commands.
    discard_sequence: bool,
}

impl Decoder {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn feed(&mut self, bytes: &[u8]) -> Vec<Key> {
        let mut keys = Vec::new();
        for &byte in bytes {
            if self.discard_sequence {
                if byte == ESC {
                    self.discard_sequence = false;
                    self.pending.push(ESC);
                } else if (0x40..=0x7e).contains(&byte) {
                    self.discard_sequence = false;
                }
                continue;
            }
            match self.pending.as_slice() {
                [ESC] if matches!(byte, b'[' | b'O') => {
                    self.pending.push(byte);
                    continue;
                }
                [ESC] => {
                    keys.push(Key::Esc);
                    self.pending.clear();
                    // The following byte is a separate key, possibly another ESC.
                }
                [ESC, ..] => {
                    if byte == ESC {
                        // A fresh escape introducer also recovers from a broken sequence.
                        self.pending.truncate(1);
                    } else if (0x40..=0x7e).contains(&byte) {
                        let params = &self.pending[2..];
                        // Intermediate bytes form other controls, not navigation keys.
                        if self.pending.len() < MAX_PENDING_BYTES
                            && params.iter().all(|byte| (0x30..=0x3f).contains(byte))
                        {
                            self.pending.push(byte);
                            keys.extend(parse_keys(&self.pending));
                        }
                        self.pending.clear();
                    } else if (0x20..=0x3f).contains(&byte)
                        && self.pending.len() < MAX_PENDING_BYTES
                        && !(byte >= 0x30
                            && self
                                .pending
                                .last()
                                .is_some_and(|last| (0x20..=0x2f).contains(last)))
                    {
                        self.pending.push(byte);
                    } else {
                        self.pending.clear();
                        self.discard_sequence = true;
                    }
                    continue;
                }
                [] => {}
                _ => {
                    if (0x80..=0xbf).contains(&byte) {
                        self.pending.push(byte);
                        match std::str::from_utf8(&self.pending) {
                            Ok(text) => {
                                keys.extend(text.chars().next().map(Key::Char));
                                self.pending.clear();
                            }
                            Err(error) if error.error_len().is_some() => self.pending.clear(),
                            Err(_) => {}
                        }
                        continue;
                    }
                    // An invalid UTF-8 prefix must not swallow the next independent key.
                    self.pending.clear();
                }
            }
            match byte {
                ESC | 0xc2..=0xf4 => self.pending.push(byte),
                b'\r' | b'\n' => keys.push(Key::Enter),
                DEL | BACKSPACE => keys.push(Key::Backspace),
                b'\t' => keys.push(Key::Tab),
                0x00..=0x1f => keys.push(Key::Ctrl(byte)),
                0x20..=0x7e => keys.push(Key::Char(char::from(byte))),
                _ => {}
            }
        }
        keys
    }

    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || self.discard_sequence
    }

    // Called at the input deadline: only a lone ESC becomes a key. Other partial input
    // is dropped as a unit so parameter bytes cannot become pager commands.
    pub(super) fn flush(&mut self) -> Vec<Key> {
        let keys = if self.pending.as_slice() == [ESC] {
            vec![Key::Esc]
        } else {
            Vec::new()
        };
        self.pending.clear();
        self.discard_sequence = false;
        keys
    }
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
            b'\t' => (Some(Key::Tab), 1),
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
        (b'C', _) => Some(Action::ScrollRight),
        (b'D', _) => Some(Action::ScrollLeft),
        (b'H', _) | (b'~', b"1" | b"7") => Some(Action::Top),
        (b'F', _) | (b'~', b"4" | b"8") => Some(Action::Bottom),
        (b'~', b"5") => Some(Action::PageUp),
        (b'~', b"6") => Some(Action::PageDown),
        (b'Z', b"") => Some(Action::PrevLink),
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
        Key::Char('o') => Some(Action::Outline),
        Key::Char('h') => Some(Action::ScrollLeft),
        Key::Char('l') => Some(Action::ScrollRight),
        Key::Tab => Some(Action::NextLink),
        Key::Backspace => Some(Action::Back),
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
            actions(b"/?nNo"),
            [
                Search(Direction::Forward),
                Search(Direction::Backward),
                NextMatch,
                PrevMatch,
                Outline
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
        assert_eq!(
            actions(b"\x1b[C\x1b[Dhl"),
            [ScrollRight, ScrollLeft, ScrollLeft, ScrollRight]
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
