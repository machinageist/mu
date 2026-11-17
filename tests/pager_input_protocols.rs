// Standalone, dependency-free tests of the real pager input and search modules.
// Run with: rustc --edition=2024 --test tests/pager_input_protocols.rs -o <scratch-binary>
#![allow(dead_code)]

#[path = "../src/pager/keys.rs"]
mod keys;
#[path = "../src/pager/search.rs"]
mod search;

use keys::{Decoder, GraphicsReply, Key, MouseEvent, parse_keys};

fn assert_all_splits(bytes: &[u8], expected: &[Key]) {
    assert_eq!(parse_keys(bytes), expected, "complete read: {bytes:?}");
    for split in 0..=bytes.len() {
        let mut decoder = Decoder::new();
        let mut keys = decoder.feed(&bytes[..split]);
        keys.extend(decoder.feed(&bytes[split..]));
        keys.extend(decoder.flush());
        assert_eq!(keys, expected, "split {split}: {bytes:?}");
    }
    let mut decoder = Decoder::new();
    let mut keys = Vec::new();
    for byte in bytes {
        keys.extend(decoder.feed(std::slice::from_ref(byte)));
    }
    keys.extend(decoder.flush());
    assert_eq!(keys, expected, "byte-at-a-time: {bytes:?}");
}

#[test]
fn sgr_mouse_reports_are_checked_and_zero_based() {
    assert_all_splits(
        b"\x1b[<0;1;1M\x1b[<0;81;23M\x1b[<64;81;23M\x1b[<65;1;1Mj",
        &[
            Key::Mouse(MouseEvent::LeftPress { column: 0, row: 0 }),
            Key::Mouse(MouseEvent::LeftPress {
                column: 80,
                row: 22,
            }),
            Key::Mouse(MouseEvent::WheelUp),
            Key::Mouse(MouseEvent::WheelDown),
            Key::Char('j'),
        ],
    );
}

#[test]
fn mouse_prefixed_sequences_never_become_navigation() {
    assert_all_splits(b"\x1b[<0;1;1A\x1b[<64;2;3B\x1b[<;5;6Dq", &[Key::Char('q')]);
}

#[test]
fn unsupported_and_malformed_mouse_reports_are_atomic() {
    for report in [
        b"\x1b[<0;1;1m".as_slice(), // release
        b"\x1b[<1;1;1M",            // middle button
        b"\x1b[<2;1;1M",            // right button
        b"\x1b[<3;1;1M",            // legacy release button
        b"\x1b[<4;1;1M",            // shift-left
        b"\x1b[<8;1;1M",            // alt-left
        b"\x1b[<16;1;1M",           // control-left
        b"\x1b[<32;1;1M",           // motion
        b"\x1b[<35;1;1M",           // motion, no button
        b"\x1b[<66;1;1M",           // horizontal wheel
        b"\x1b[<68;1;1M",           // modified wheel
        b"\x1b[<128;1;1M",          // extended button
        b"\x1b[<0;0;1M",
        b"\x1b[<0;1;0M",
        b"\x1b[<64;0;1M",
        b"\x1b[<0;;1M",
        b"\x1b[<;1;1M",
        b"\x1b[<0;1M",
        b"\x1b[<0;1;1;2M",
        b"\x1b[<0;-1;1M",
        b"\x1b[<0;+1;1M",
        b"\x1b[<0;1.5;1M",
        b"\x1b[<0;1:1;1M",
        b"\x1b[<0;1;1/M",  // intermediate bytes are not a mouse report
        b"\x1b[<0;1;1/2M", // parameters after an intermediate byte
        b"\x1b[<0;1;\x001M",
        b"\x1b[<0;1;\xff1M",
        b"\x1b[0;<1;1M",
        b"\x1bO<0;1;1M", // SS3 is not SGR mouse
        b"\x1b[<184467440737095516160;1;1M",
        b"\x1b[<0;184467440737095516160;1M",
        b"\x1b[<0;1;184467440737095516160M",
    ] {
        let mut bytes = report.to_vec();
        bytes.extend_from_slice(b"q/j");
        assert_all_splits(&bytes, &[Key::Char('q'), Key::Char('/'), Key::Char('j')]);
    }
}

#[test]
fn mouse_parameter_tail_cannot_quit_or_start_search() {
    for prefix in [
        b"\x1b[<0;1;".as_slice(),
        b"\x1b[<0;1;1/2",  // invalid intermediate ordering
        b"\x1b[<0;1;\x00", // invalid byte
    ] {
        for deadline in [false, true] {
            let mut decoder = Decoder::new();
            assert_eq!(decoder.feed(prefix), []);
            if deadline {
                assert_eq!(decoder.flush(), []);
                assert!(!decoder.has_pending());
            }
            // Slash is an intermediate byte and q is the final byte, not a key.
            assert_eq!(decoder.feed(b"/q"), []);
            assert_eq!(decoder.feed(b"n"), [Key::Char('n')]);
            assert!(!decoder.has_pending());
        }
    }
}

#[test]
fn kitty_replies_are_events_not_keyboard_input() {
    assert_all_splits(
        b"\x1b_Gi=42;OK\x1b\\\x1b_Gi=7,p=19;OK\x1b\\\x1b_Gi=9,p=2;ENOENT:no image\x1b\\q",
        &[
            Key::Graphics(GraphicsReply {
                image: 42,
                placement: None,
                ok: true,
            }),
            Key::Graphics(GraphicsReply {
                image: 7,
                placement: Some(19),
                ok: true,
            }),
            Key::Graphics(GraphicsReply {
                image: 9,
                placement: Some(2),
                ok: false,
            }),
            Key::Char('q'),
        ],
    );
}

#[test]
fn unknown_and_malformed_apc_is_atomic() {
    for apc in [
        b"unknown q/j\x1b[A".as_slice(),
        b"Gq/j",
        b"Gi=;OK",
        b"Gi=-1;OK",
        b"Gi=+1;OK",
        b"Gi=1,p=;OK",
        b"Gi=1,p=-2;OK",
        b"Gi=1,p=2,;OK",
        b"Gi=1,q=2;OK",
        b"Gi=1,p=2,p=3;OK",
        b"Gi=4294967296;OK",
        b"Gi=1,p=4294967296;OK",
        b"Gi=1;",
        b"Gi=1;OK\x00q/j",
        b"Gi=1;OK\x7fq/j",
        b"Gi=1;OK\xffq/j",
    ] {
        let mut bytes = b"\x1b_".to_vec();
        bytes.extend_from_slice(apc);
        bytes.extend_from_slice(b"\x1b\\n");
        assert_all_splits(&bytes, &[Key::Char('n')]);
    }
}

#[test]
fn legacy_payload_stays_atomic_across_deadlines() {
    for count in 0..3 {
        let mut decoder = Decoder::new();
        let bytes = b"\x1b[Mq/j";
        assert_eq!(decoder.feed(&bytes[..3 + count]), []);
        assert!(decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        assert!(!decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        assert_eq!(decoder.feed(&bytes[3 + count..]), []);
        assert_eq!(decoder.feed(b"n"), [Key::Char('n')]);
    }
}

#[test]
fn timed_out_apc_is_quarantined_through_its_terminator() {
    for prefix in [b"\x1b_".as_slice(), b"\x1b_Gi=2;OK", b"\x1b_Gi=2;OK\x1b"] {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.feed(prefix), []);
        assert!(decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        assert!(!decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        let suffix = if prefix.ends_with(b"\x1b") {
            b"\\j".as_slice()
        } else {
            b"q/j\x1b\\j"
        };
        assert_eq!(decoder.feed(suffix), [Key::Char('j')]);
        assert!(!decoder.has_pending());
    }
}

#[test]
fn timed_out_csi_does_not_reinterpret_its_suffix() {
    for prefix in [b"\x1b[<0;12".as_slice(), b"\x1b[1;", b"\x1bO1;"] {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.feed(prefix), []);
        assert!(decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        assert!(!decoder.has_pending());
        assert_eq!(decoder.feed(b"3A/n"), [Key::Char('/'), Key::Char('n')]);
        assert_eq!(decoder.feed(b"\x1b[A"), [Key::Nav(keys::Action::LineUp)]);
    }
}

#[test]
fn ctrl_c_can_abort_an_unterminated_quarantined_control() {
    for prefix in [b"\x1b_Gi=2;".as_slice(), b"\x1b[<0;", b"\x1b[\x00"] {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.feed(prefix), []);
        assert_eq!(decoder.flush(), []);
        assert_eq!(
            decoder.feed(b"\x03/n"),
            [Key::Ctrl(keys::CTRL_C), Key::Char('/'), Key::Char('n')]
        );
        assert!(!decoder.has_pending());
    }
}

#[test]
fn timed_out_empty_csi_still_recognizes_legacy_payload() {
    let mut decoder = Decoder::new();
    assert_eq!(decoder.feed(b"\x1b["), []);
    assert_eq!(decoder.flush(), []);
    assert!(!decoder.has_pending());
    assert_eq!(decoder.feed(b"Mq/jn"), [Key::Char('n')]);
}

#[test]
fn apc_payload_limit_includes_exactly_256_bytes() {
    let mut body = b"Gi=8;".to_vec();
    body.resize(256, b'E');
    let mut bytes = b"\x1b_".to_vec();
    bytes.extend_from_slice(&body);
    bytes.extend_from_slice(b"\x1b\\n");
    assert_all_splits(
        &bytes,
        &[
            Key::Graphics(GraphicsReply {
                image: 8,
                placement: None,
                ok: false,
            }),
            Key::Char('n'),
        ],
    );
    // Keep the reply syntactically valid: only the payload size rejects it.
    bytes.insert(2 + body.len(), b'E');
    assert_all_splits(&bytes, &[Key::Char('n')]);
}

#[test]
fn every_control_payload_deadline_preserves_its_boundary() {
    for bytes in [
        b"\x1b[<0;81;23M".as_slice(),
        b"\x1b[1;5A",
        b"\x1bO1;5B",
        b"\x1b[Mq/j",
        b"\x1b_Gi=7,p=19;OK\x1b\\",
    ] {
        for split in 2..bytes.len() {
            let mut decoder = Decoder::new();
            assert_eq!(
                decoder.feed(&bytes[..split]),
                [],
                "prefix {split}: {bytes:?}"
            );
            assert!(decoder.has_pending(), "prefix {split}: {bytes:?}");
            for _ in 0..3 {
                assert_eq!(decoder.flush(), []);
                assert!(!decoder.has_pending());
                assert_eq!(decoder.feed(b""), []);
            }
            for byte in &bytes[split..] {
                assert_eq!(decoder.feed(std::slice::from_ref(byte)), []);
                // Late fragments do not repeatedly rearm a timed-out control.
                assert!(!decoder.has_pending());
                assert_eq!(decoder.flush(), []);
            }
            assert_eq!(
                decoder.feed(b"q/j"),
                [Key::Char('q'), Key::Char('/'), Key::Char('j')]
            );
        }
    }
}

#[test]
fn legacy_payload_including_escape_and_ctrl_c_is_opaque() {
    for payload in [b"q/j".as_slice(), b"\x1b[Q", b"\x03\x00\xff"] {
        let mut bytes = b"\x1b[M".to_vec();
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(b"n");
        assert_all_splits(&bytes, &[Key::Char('n')]);
        let mut decoder = Decoder::new();
        assert_eq!(decoder.feed(b"\x1b["), []);
        assert_eq!(decoder.flush(), []);
        assert_eq!(decoder.feed(b"M"), []);
        assert!(!decoder.has_pending());
        for byte in payload {
            assert_eq!(decoder.feed(std::slice::from_ref(byte)), []);
            assert_eq!(decoder.flush(), []);
            assert!(!decoder.has_pending());
        }
        assert_eq!(decoder.feed(b"n"), [Key::Char('n')]);
    }
}

#[test]
fn only_bare_csi_m_introduces_legacy_mouse() {
    for introducer in [b"\x1b[1M".as_slice(), b"\x1bOM", b"\x1b[ M"] {
        let mut bytes = introducer.to_vec();
        bytes.extend_from_slice(b"q/j");
        assert_all_splits(&bytes, &[Key::Char('q'), Key::Char('/'), Key::Char('j')]);
        let mut decoder = Decoder::new();
        assert_eq!(decoder.feed(&introducer[..2]), []);
        assert_eq!(decoder.flush(), []);
        assert_eq!(decoder.feed(&introducer[2..]), []);
        assert_eq!(
            decoder.feed(b"q/j"),
            [Key::Char('q'), Key::Char('/'), Key::Char('j')]
        );
    }
}

#[test]
fn invalid_apc_escape_and_bel_do_not_expose_payload() {
    for body in [
        b"Gi=1;OK\x07q/j".as_slice(),
        b"Gi=1;OK\x1b[Aq/j",
        b"Gi=1;OK\x1bq/j",
        b"Gi=1;OK\x1b\x1b",
    ] {
        let mut bytes = b"\x1b_".to_vec();
        bytes.extend_from_slice(body);
        bytes.extend_from_slice(b"\x1b\\n");
        assert_all_splits(&bytes, &[Key::Char('n')]);
    }
}

#[test]
fn oversized_controls_discard_streaming_suffix_without_idle_deadlines() {
    let payload = vec![b'1'; 1024 * 1024];
    for introducer in [b"\x1b[<".as_slice(), b"\x1bO", b"\x1b_Gi=1;"] {
        for chunk_size in [1, 7, 256, 1024] {
            let mut decoder = Decoder::new();
            assert_eq!(decoder.feed(introducer), []);
            for chunk in payload.chunks(chunk_size) {
                assert_eq!(decoder.feed(chunk), []);
            }
            assert!(!decoder.has_pending());
            assert_eq!(decoder.flush(), []);
            if introducer.starts_with(b"\x1b_") {
                assert_eq!(decoder.feed(b"q/j\x1b[A\x07"), []);
                assert_eq!(decoder.feed(b"\x1b"), []);
                assert!(!decoder.has_pending());
                assert_eq!(decoder.feed(b"\\"), []);
            } else {
                assert_eq!(decoder.feed(b"/q"), []);
            }
            assert!(!decoder.has_pending());
            assert_eq!(
                decoder.feed(b"q/j"),
                [Key::Char('q'), Key::Char('/'), Key::Char('j')]
            );
        }
    }
}

#[test]
fn csi_limit_counts_introducer_and_final_byte() {
    let mut bytes = b"\x1b[".to_vec();
    bytes.extend(std::iter::repeat_n(b'1', 253));
    bytes.extend_from_slice(b"An");
    assert_all_splits(&bytes, &[Key::Nav(keys::Action::LineUp), Key::Char('n')]);
    bytes.insert(2, b'1');
    assert_all_splits(&bytes, &[Key::Char('n')]);
}

#[test]
fn keyboard_navigation_modifiers_editing_and_utf8_keep_their_meanings() {
    use keys::Action::*;
    assert_all_splits(
        b"\x1b[A\x1b[1;5B\x1bOC\x1b[1;2D\x1b[H\x1bOF\x1b[7~\x1b[8~\x1b[5~\x1b[6~\x1b[Z",
        &[
            Key::Nav(LineUp),
            Key::Nav(LineDown),
            Key::Nav(ScrollRight),
            Key::Nav(ScrollLeft),
            Key::Nav(Top),
            Key::Nav(Bottom),
            Key::Nav(Top),
            Key::Nav(Bottom),
            Key::Nav(PageUp),
            Key::Nav(PageDown),
            Key::Nav(PrevLink),
        ],
    );
    assert_all_splits(
        "é全🦀\x7f\x08\t\r\n\x03\x15q/".as_bytes(),
        &[
            Key::Char('é'),
            Key::Char('全'),
            Key::Char('🦀'),
            Key::Backspace,
            Key::Backspace,
            Key::Tab,
            Key::Enter,
            Key::Enter,
            Key::Ctrl(keys::CTRL_C),
            Key::Ctrl(keys::CTRL_U),
            Key::Char('q'),
            Key::Char('/'),
        ],
    );
}

#[test]
fn lone_escape_and_partial_utf8_have_one_deadline_only() {
    let mut decoder = Decoder::new();
    assert_eq!(decoder.feed(b"\x1b"), []);
    assert!(decoder.has_pending());
    assert_eq!(decoder.flush(), [Key::Esc]);
    assert!(!decoder.has_pending());
    assert_eq!(decoder.flush(), []);
    assert_eq!(decoder.feed(b"q/"), [Key::Char('q'), Key::Char('/')]);
    // Once a lone Escape deadline fires, a late introducer is ordinary text.
    for introducer in *b"[_" {
        assert_eq!(decoder.feed(b"\x1b"), []);
        assert_eq!(decoder.flush(), [Key::Esc]);
        assert_eq!(
            decoder.feed(&[introducer, b'q']),
            [Key::Char(char::from(introducer)), Key::Char('q')]
        );
    }
    for prefix in [b"\xc3".as_slice(), b"\xe5\x85", b"\xf0\x9f\xa6"] {
        assert_eq!(decoder.feed(prefix), []);
        assert!(decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        assert!(!decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        assert_eq!(decoder.feed(b"\x80q/"), [Key::Char('q'), Key::Char('/')]);
    }
    assert_all_splits(
        b"\xc3q\xe5/\xf0\x9fj\xff",
        &[Key::Char('q'), Key::Char('/'), Key::Char('j')],
    );
    assert_all_splits(
        b"\x1bx\x1b\x1b",
        &[Key::Esc, Key::Char('x'), Key::Esc, Key::Esc],
    );
}

#[test]
fn a_fresh_escape_recovers_csi_but_is_opaque_inside_apc() {
    for prefix in [b"\x1b[<0;".as_slice(), b"\x1b[\x00", b"\x1bO1;"] {
        for deadline in [false, true] {
            let mut decoder = Decoder::new();
            assert_eq!(decoder.feed(prefix), []);
            if deadline {
                assert_eq!(decoder.flush(), []);
            }
            assert_eq!(decoder.feed(b"\x1b"), []);
            assert!(decoder.has_pending());
            assert_eq!(
                decoder.feed(b"[Aj"),
                [Key::Nav(keys::Action::LineUp), Key::Char('j')]
            );
            assert!(!decoder.has_pending());
        }
    }
    assert_all_splits(b"\x1b_Gi=1;\x1b[Aq/j\x1b\\n", &[Key::Char('n')]);
}

#[test]
fn every_graphics_reply_status_is_explicit_and_ids_fit_u32() {
    for (status, ok) in [
        (b"OK".as_slice(), true),
        (b"OKAY", false),
        (b"ok", false),
        (b"EIO:q/j", false),
    ] {
        let mut bytes = b"\x1b_Gi=4294967295,p=4294967295;".to_vec();
        bytes.extend_from_slice(status);
        bytes.extend_from_slice(b"\x1b\\n");
        assert_all_splits(
            &bytes,
            &[
                Key::Graphics(GraphicsReply {
                    image: u32::MAX,
                    placement: Some(u32::MAX),
                    ok,
                }),
                Key::Char('n'),
            ],
        );
    }
    // Identity ownership, including whether zero is usable, belongs to the pager.
    assert_all_splits(
        b"\x1b_Gi=0,p=0;OK\x1b\\",
        &[Key::Graphics(GraphicsReply {
            image: 0,
            placement: Some(0),
            ok: true,
        })],
    );
}

#[test]
fn a_finished_timeout_does_not_disable_the_next_controls_deadline() {
    let mut decoder = Decoder::new();
    for _ in 0..3 {
        assert_eq!(decoder.feed(b"\x1b[M"), []);
        assert!(decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        assert!(!decoder.has_pending());
        assert_eq!(decoder.feed(b"q/j"), []);
        assert_eq!(decoder.feed(b"\x1b_Gi=1;"), []);
        assert!(decoder.has_pending());
        assert_eq!(decoder.flush(), []);
        assert!(!decoder.has_pending());
        assert_eq!(decoder.feed(b"OK\x1b\\"), []);
        assert_eq!(decoder.feed(b"q"), [Key::Char('q')]);
    }
}

#[test]
fn legacy_mouse_payload_is_not_keyboard_input() {
    assert_all_splits(b"\x1b[Mq/jn", &[Key::Char('n')]);
}
