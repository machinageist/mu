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
pub(super) enum MouseEvent {
    LeftPress { column: usize, row: usize },
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GraphicsReply {
    pub image: u32,
    pub placement: Option<u32>,
    pub ok: bool,
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
    Mouse(MouseEvent),
    Graphics(GraphicsReply),
}

// Retained input is bounded even if a terminal sends a sequence with no final byte.
const MAX_PENDING_BYTES: usize = 256;

// An APC is terminated only by ST (ESC backslash), never by an embedded key/CSI.
#[derive(Default)]
struct Apc {
    discarded: bool,
    escape: bool,
}

enum DiscardSequence {
    // A deadline can split ESC [ from M and its three opaque legacy payload bytes.
    CsiStart,
    Other,
}

// Decode terminal reads without making read boundaries into key boundaries.
#[derive(Default)]
pub(super) struct Decoder {
    pending: Vec<u8>,
    legacy_remaining: u8,
    legacy_timed_out: bool,
    apc: Option<Apc>,
    // After an invalid or oversized sequence, consume its suffix through the final byte
    // instead of interpreting that suffix as ordinary pager commands.
    discard_sequence: Option<DiscardSequence>,
}

impl Decoder {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn feed(&mut self, bytes: &[u8]) -> Vec<Key> {
        let mut keys = Vec::new();
        for &byte in bytes {
            if self.legacy_remaining > 0 {
                self.legacy_remaining -= 1;
                continue;
            }
            // Ctrl-C remains an explicit way to quit an unterminated control. A legacy
            // mouse packet is different: its next three bytes are always opaque payload.
            if byte == CTRL_C
                && (self.apc.is_some()
                    || self.discard_sequence.is_some()
                    || self.pending.starts_with(&[ESC, b'['])
                    || self.pending.starts_with(&[ESC, b'O']))
            {
                self.pending.clear();
                self.apc = None;
                self.discard_sequence = None;
                keys.push(Key::Ctrl(CTRL_C));
                continue;
            }
            if self.apc.is_some() {
                self.apc_byte(byte, &mut keys);
                continue;
            }
            if let Some(discard) = self.discard_sequence.take() {
                if byte == ESC {
                    self.pending.push(ESC);
                } else if (0x40..=0x7e).contains(&byte) {
                    if matches!(discard, DiscardSequence::CsiStart) && byte == b'M' {
                        self.legacy_remaining = 3;
                        self.legacy_timed_out = true;
                    }
                } else {
                    self.discard_sequence = Some(DiscardSequence::Other);
                }
                continue;
            }
            match self.pending.as_slice() {
                [ESC] if byte == b'_' => {
                    self.pending.clear();
                    self.apc = Some(Apc::default());
                    continue;
                }
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
                            if self.pending.as_slice() == [ESC, b'['] && byte == b'M' {
                                self.legacy_remaining = 3;
                                self.legacy_timed_out = false;
                            } else {
                                keys.extend(escape_key(self.pending[1], params, byte));
                            }
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
                        self.discard_sequence = Some(DiscardSequence::Other);
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

    fn apc_byte(&mut self, byte: u8, keys: &mut Vec<Key>) {
        let apc = self.apc.as_mut().expect("active APC");
        if apc.escape && byte == b'\\' {
            if !apc.discarded {
                keys.extend(graphics_reply(&self.pending).map(Key::Graphics));
            }
            self.pending.clear();
            self.apc = None;
            return;
        }
        if apc.escape || (byte != ESC && self.pending.len() == MAX_PENDING_BYTES) {
            apc.discarded = true;
            self.pending.clear();
        }
        apc.escape = byte == ESC;
        if !apc.discarded && !apc.escape {
            self.pending.push(byte);
        }
    }

    pub(super) fn has_pending(&self) -> bool {
        // Quarantine needs no further deadline: it holds no bytes and only waits for
        // a terminator (or Ctrl-C). Reporting it here would repeatedly arm idle timers.
        if let Some(apc) = &self.apc {
            return !apc.discarded;
        }
        !self.pending.is_empty() || (self.legacy_remaining > 0 && !self.legacy_timed_out)
    }

    // Called at the input deadline: only a lone ESC becomes a key. Drop retained
    // bytes, but quarantine partial controls through their boundary so a late suffix
    // cannot become pager commands. Incomplete UTF-8 is simply dropped.
    pub(super) fn flush(&mut self) -> Vec<Key> {
        if let Some(apc) = &mut self.apc {
            apc.discarded = true;
            self.pending.clear();
            return Vec::new();
        }
        if self.pending.starts_with(&[ESC, b'[']) || self.pending.starts_with(&[ESC, b'O']) {
            self.discard_sequence = Some(if self.pending.as_slice() == [ESC, b'['] {
                DiscardSequence::CsiStart
            } else {
                DiscardSequence::Other
            });
        }
        self.legacy_timed_out |= self.legacy_remaining > 0;
        let keys = if self.pending.as_slice() == [ESC] {
            vec![Key::Esc]
        } else {
            Vec::new()
        };
        self.pending.clear();
        keys
    }
}

// Test convenience wrapper over the same parser used for incremental input.
#[cfg(test)]
pub(super) fn parse_keys(bytes: &[u8]) -> Vec<Key> {
    let mut decoder = Decoder::new();
    let mut keys = decoder.feed(bytes);
    keys.extend(decoder.flush());
    keys
}

// Decode reply identity only; the pager decides whether these IDs are owned/in flight.
fn graphics_reply(bytes: &[u8]) -> Option<GraphicsReply> {
    let body = bytes.strip_prefix(b"Gi=")?;
    let separator = body.iter().position(|&byte| byte == b';')?;
    let (identity, status) = body.split_at(separator);
    let status = &status[1..];
    if status.is_empty() || !status.iter().all(|byte| (0x20..=0x7e).contains(byte)) {
        return None;
    }
    let mut fields = identity.split(|&byte| byte == b',');
    let image = u32::try_from(decimal(fields.next()?)?).ok()?;
    let placement = match fields.next() {
        Some(field) => Some(u32::try_from(decimal(field.strip_prefix(b"p=")?)?).ok()?),
        None => None,
    };
    if fields.next().is_some() {
        return None;
    }
    Some(GraphicsReply {
        image,
        placement,
        ok: status == b"OK",
    })
}

// Mouse-marked controls must never reach the deliberately permissive arrow parser.
fn escape_key(introducer: u8, params: &[u8], final_byte: u8) -> Option<Key> {
    if params.contains(&b'<') {
        return (introducer == b'[')
            .then(|| mouse_event(params, final_byte))
            .flatten()
            .map(Key::Mouse);
    }
    escape_action(params, final_byte).map(Key::Nav)
}

fn mouse_event(params: &[u8], final_byte: u8) -> Option<MouseEvent> {
    if final_byte != b'M' {
        return None;
    }
    let mut fields = params.strip_prefix(b"<")?.split(|&byte| byte == b';');
    let button = decimal(fields.next()?)?;
    // SGR coordinates are positive 1-based values. Convert exactly once here.
    let column = decimal(fields.next()?)?.checked_sub(1)?;
    let row = decimal(fields.next()?)?.checked_sub(1)?;
    if fields.next().is_some() {
        return None;
    }
    match button {
        0 => Some(MouseEvent::LeftPress { column, row }),
        64 => Some(MouseEvent::WheelUp),
        65 => Some(MouseEvent::WheelDown),
        _ => None,
    }
}

// Empty, signed, nondecimal and overflowing fields are not numbers.
fn decimal(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }
    bytes.iter().try_fold(0usize, |value, &byte| {
        byte.is_ascii_digit()
            .then_some(())
            .and_then(|()| value.checked_mul(10)?.checked_add(usize::from(byte - b'0')))
    })
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
