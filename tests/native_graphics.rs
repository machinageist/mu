// Author: machinageist
// Description: Standalone tests exercise the real pager graphics module without mocks.
// Run: rustc --edition=2024 --test tests/native_graphics.rs -D warnings -o <scratch>/tests

#[allow(dead_code)]
#[path = "../src/pager/graphics.rs"]
mod graphics;

use graphics::{Image, Manager, Viewport};
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

const APC: &str = "\x1b_G";
const ST: &str = "\x1b\\";
const RESET: &str = "\x1b[0m";
const PLACEHOLDER: char = '\u{10eeee}';
const MAX_BYTES: usize = 32 * 1024 * 1024;

// Independent primary-protocol fixture, NOT imported from the implementation.
// rowcolumn-diacritics.txt, also verified against Ghostty v1.4.0 graphics_unicode.zig.
const DIACRITIC_HEX: &str = "0305 030D 030E 0310 0312 033D 033E 033F 0346 034A 034B 034C 0350 0351 0352 0357 035B 0363 0364 0365 0366 0367 0368 0369 036A 036B 036C 036D 036E 036F 0483 0484 0485 0486 0487 0592 0593 0594 0595 0597 0598 0599 059C 059D 059E 059F 05A0 05A1 05A8 05A9 05AB 05AC 05AF 05C4 0610 0611 0612 0613 0614 0615 0616 0617 0657 0658 0659 065A 065B 065D 065E 06D6 06D7 06D8 06D9 06DA 06DB 06DC 06DF 06E0 06E1 06E2 06E4 06E7 06E8 06EB 06EC 0730 0732 0733 0735 0736 073A 073D 073F 0740 0741 0743 0745 0747 0749 074A 07EB 07EC 07ED 07EE 07EF 07F0 07F1 07F3 0816 0817 0818 0819 081B 081C 081D 081E 081F 0820 0821 0822 0823 0825 0826 0827 0829 082A 082B 082C 082D 0951 0953 0954 0F82 0F83 0F86 0F87 135D 135E 135F 17DD 193A 1A17 1A75 1A76 1A77 1A78 1A79 1A7A 1A7B 1A7C 1B6B 1B6D 1B6E 1B6F 1B70 1B71 1B72 1B73 1CD0 1CD1 1CD2 1CDA 1CDB 1CE0 1DC0 1DC1 1DC3 1DC4 1DC5 1DC6 1DC7 1DC8 1DC9 1DCB 1DCC 1DD1 1DD2 1DD3 1DD4 1DD5 1DD6 1DD7 1DD8 1DD9 1DDA 1DDB 1DDC 1DDD 1DDE 1DDF 1DE0 1DE1 1DE2 1DE3 1DE4 1DE5 1DE6 1DFE 20D0 20D1 20D4 20D5 20D6 20D7 20DB 20DC 20E1 20E7 20E9 20F0 2CEF 2CF0 2CF1 2DE0 2DE1 2DE2 2DE3 2DE4 2DE5 2DE6 2DE7 2DE8 2DE9 2DEA 2DEB 2DEC 2DED 2DEE 2DEF 2DF0 2DF1 2DF2 2DF3 2DF4 2DF5 2DF6 2DF7 2DF8 2DF9 2DFA 2DFB 2DFC 2DFD 2DFE 2DFF A66F A67C A67D A6F0 A6F1 A8E0 A8E1 A8E2 A8E3 A8E4 A8E5 A8E6 A8E7 A8E8 A8E9 A8EA A8EB A8EC A8ED A8EE A8EF A8F0 A8F1 AAB0 AAB2 AAB3 AAB7 AAB8 AABE AABF AAC1 FE20 FE21 FE22 FE23 FE24 FE25 FE26 10A0F 10A38 1D185 1D186 1D187 1D188 1D189 1D1AA 1D1AB 1D1AC 1D1AD 1D242 1D243 1D244";

fn marks() -> &'static [char] {
    static MARKS: OnceLock<Vec<char>> = OnceLock::new();
    MARKS.get_or_init(|| {
        DIACRITIC_HEX
            .split_whitespace()
            .map(|hex| char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap())
            .collect()
    })
}

fn image(value: u8) -> Image {
    Image {
        line: 0,
        column: 0,
        columns: 4,
        rows: 3,
        width: 1,
        height: 1,
        pixels: Arc::from([value, value.rotate_left(2), 255 - value, value]),
    }
}

fn viewport() -> Viewport {
    Viewport {
        top: 0,
        left: 0,
        rows: 20,
        columns: 80,
    }
}

fn controls(output: &str) -> Vec<(&str, &str)> {
    output
        .split(APC)
        .skip(1)
        .map(|piece| {
            let body = piece.split_once(ST).expect("terminated APC").0;
            body.split_once(';').expect("control/payload delimiter")
        })
        .collect()
}

fn value(control: &str, key: &str) -> Option<u32> {
    control.split(',').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.parse().ok()).flatten()
    })
}

fn actions(output: &str, action: &str) -> Vec<u32> {
    controls(output)
        .into_iter()
        .filter(|(control, _)| control.split(',').any(|part| part == action))
        .map(|(control, _)| value(control, "i").unwrap())
        .collect()
}

fn decode(text: &str) -> Vec<u8> {
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = Vec::new();
    let mut n = 0u32;
    let mut bits = 0;
    for ch in text.chars().filter(|&ch| ch != '=') {
        n = (n << 6) | alphabet.find(ch).expect("base64 alphabet") as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((n >> bits) as u8);
        }
    }
    result
}

fn acknowledge(manager: &mut Manager, output: &str) {
    manager.written();
    for (header, _) in controls(output) {
        if header.starts_with("a=t,") || header.starts_with("a=p,") {
            assert!(manager.reply(
                value(header, "i").unwrap(),
                value(header, "p"),
                true,
                Instant::now()
            ));
        }
    }
}

fn ready(manager: &mut Manager, view: Viewport) -> String {
    let upload = manager.frame(view, true, Instant::now());
    assert!(!upload.contains(PLACEHOLDER));
    acknowledge(manager, &upload);
    let placement = manager.frame(view, true, Instant::now());
    assert!(!placement.contains(PLACEHOLDER));
    acknowledge(manager, &placement);
    let overlay = manager.frame(view, true, Instant::now());
    manager.written();
    overlay
}

#[derive(Debug, PartialEq, Eq)]
struct Fragment {
    row: usize,
    column: usize,
    image: u32,
    placement: u32,
    cells: Vec<(usize, usize, usize)>,
}

fn number_list(input: &str) -> Vec<usize> {
    input.split(';').map(|n| n.parse().unwrap()).collect()
}

fn rgb<'a>(input: &'a str, prefix: &str) -> (u32, &'a str) {
    let (color, rest) = input
        .strip_prefix(prefix)
        .expect("truecolor SGR")
        .split_once('m')
        .unwrap();
    let color = number_list(color);
    assert_eq!(color.len(), 3);
    assert!(color.iter().all(|&channel| channel <= 255));
    (((color[0] << 16) | (color[1] << 8) | color[2]) as u32, rest)
}

// Parse ONLY the public byte stream. Each fragment must explicitly set CUP,
// clear fallback/search background, set foreground+underline IDs, and reset SGR.
fn fragments(mut input: &str) -> Vec<Fragment> {
    let mut result = Vec::new();
    while !input.is_empty() {
        let (cup, rest) = input
            .strip_prefix("\x1b[")
            .expect("explicit CUP")
            .split_once('H')
            .unwrap();
        let cup = number_list(cup);
        assert_eq!(cup.len(), 2);
        let rest = rest
            .strip_prefix(RESET)
            .expect("reset stale fallback/search attributes");
        let (image, rest) = rgb(rest, "\x1b[38;2;");
        let (placement, rest) = rgb(rest, "\x1b[58;2;");
        let (cells, rest) = rest
            .split_once(RESET)
            .expect("restore SGR after each fragment");
        let chars: Vec<_> = cells.chars().collect();
        assert_eq!(chars.len() % 4, 0);
        let cells = chars
            .chunks(4)
            .map(|cell| {
                assert_eq!(cell[0], PLACEHOLDER);
                let index = |ch| {
                    marks()
                        .iter()
                        .position(|&mark| mark == ch)
                        .expect("protocol diacritic")
                };
                (index(cell[1]), index(cell[2]), index(cell[3]))
            })
            .collect();
        result.push(Fragment {
            row: cup[0],
            column: cup[1],
            image,
            placement,
            cells,
        });
        input = rest;
    }
    result
}

#[test]
fn rgba_upload_roundtrips_all_channels_at_chunk_boundaries() {
    for pixels in [1, 767, 768, 769, 1600] {
        let mut source = image(7);
        source.width = pixels;
        source.pixels = Arc::from(
            (0..pixels as usize * 4)
                .map(|i| (i % 251) as u8)
                .collect::<Vec<_>>(),
        );
        let mut manager = Manager::new(vec![source.clone()]);
        assert!(manager.owned_ids().is_empty());
        let frame = manager.frame(viewport(), true, Instant::now());
        let chunks = controls(&frame);
        assert_eq!(chunks.len(), source.pixels.len().div_ceil(3072));
        let id = manager.owned_ids()[0];
        assert_ne!(id, 0);
        let header = chunks[0].0;
        for (key, expected) in [("f", 32), ("s", pixels), ("v", 1), ("i", id), ("q", 0)] {
            assert_eq!(value(header, key), Some(expected));
        }
        assert!(header.starts_with("a=t,t=d,"));
        let payload: String = chunks.iter().map(|(_, data)| *data).collect();
        assert_eq!(decode(&payload), source.pixels.as_ref());
        for (index, (header, data)) in chunks.iter().enumerate() {
            assert!(data.len() <= 4096);
            assert_eq!(data.len() % 4, 0);
            let more = u32::from(index + 1 < chunks.len());
            assert_eq!(value(header, "m"), Some(more));
            if more == 1 {
                assert_eq!(data.len(), 4096);
            }
            if index > 0 {
                assert_eq!(*header, format!("m={more}"));
            }
        }
        assert!(!frame.contains(PLACEHOLDER));
    }
}

#[test]
fn separate_assets_have_contiguous_chunks_and_unique_registered_ids() {
    let sources: Vec<_> = (0..3)
        .map(|value| {
            let mut source = image(value);
            source.width = 800;
            source.pixels = Arc::from(vec![value; 3200]);
            source
        })
        .collect();
    let mut manager = Manager::new(sources);
    let output = manager.frame(viewport(), true, Instant::now());
    let chunks = controls(&output);
    assert_eq!(chunks.len(), 6);
    assert_eq!(manager.owned_ids().iter().collect::<HashSet<_>>().len(), 3);
    for chunk in chunks.chunks(2) {
        assert!(
            manager
                .owned_ids()
                .contains(&value(chunk[0].0, "i").unwrap())
        );
        assert_eq!(value(chunk[0].0, "m"), Some(1));
        assert_eq!(chunk[1].0, "m=0");
    }
}

#[test]
fn two_ack_stages_gate_every_placeholder() {
    let mut manager = Manager::new(vec![image(1)]);
    let upload = manager.frame(viewport(), true, Instant::now());
    let id = manager.owned_ids()[0];
    assert_eq!(actions(&upload, "a=t"), [id]);
    assert!(actions(&upload, "a=p").is_empty());
    assert!(!upload.contains(PLACEHOLDER));
    assert!(!manager.reply(id, Some(1), true, Instant::now()));
    acknowledge(&mut manager, &upload);
    let placement = manager.frame(viewport(), true, Instant::now());
    let headers = controls(&placement);
    assert_eq!(headers.len(), 1);
    for (key, expected) in [("U", 1), ("p", 1), ("c", 4), ("r", 3), ("C", 1), ("q", 0)] {
        assert_eq!(value(headers[0].0, key), Some(expected));
    }
    assert!(headers[0].1.is_empty());
    assert!(!placement.contains(PLACEHOLDER));
    acknowledge(&mut manager, &placement);
    assert!(manager.deadline().is_none());
    assert_eq!(
        fragments(&manager.frame(viewport(), true, Instant::now())).len(),
        3
    );
}

#[test]
fn upload_ack_deadline_begins_after_successful_flush_not_frame_generation() {
    let mut manager = Manager::new(vec![image(1)]);
    let old = Instant::now() - Duration::from_secs(30);
    let upload = manager.frame(viewport(), true, old);
    let id = manager.owned_ids()[0];
    assert!(manager.deadline().is_none(), "nothing has been flushed yet");
    assert!(
        !manager.reply(id, None, true, Instant::now()),
        "unflushed command is not ACK eligible"
    );
    assert!(manager.frame(viewport(), true, Instant::now()).is_empty());
    let before = Instant::now();
    manager.written();
    let after = Instant::now();
    let deadline = manager.deadline().unwrap();
    assert!(deadline >= before + Duration::from_secs(1));
    assert!(deadline <= after + Duration::from_secs(1));
    assert!(manager.reply(id, None, true, after));
    assert_eq!(actions(&upload, "a=t"), [id]);
}

#[test]
fn placement_ack_deadline_also_begins_after_flush() {
    let mut manager = Manager::new(vec![image(1)]);
    let upload = manager.frame(viewport(), true, Instant::now());
    acknowledge(&mut manager, &upload);
    let id = manager.owned_ids()[0];
    let output = manager.frame(viewport(), true, Instant::now() - Duration::from_secs(30));
    assert_eq!(actions(&output, "a=p"), [id]);
    assert!(manager.deadline().is_none());
    assert!(!manager.reply(id, Some(1), true, Instant::now()));
    manager.written();
    assert!(manager.reply(id, Some(1), true, Instant::now()));
}

#[test]
fn redraw_and_repeated_written_do_not_retransmit_or_extend_deadlines() {
    let mut manager = Manager::new(vec![image(1)]);
    let upload = manager.frame(viewport(), true, Instant::now());
    manager.written();
    for stage in 0..2 {
        let deadline = manager.deadline().unwrap();
        for _ in 0..128 {
            assert!(
                manager
                    .frame(viewport(), true, deadline - Duration::from_millis(1))
                    .is_empty()
            );
            manager.written();
            assert_eq!(manager.deadline(), Some(deadline));
        }
        let id = manager.owned_ids()[0];
        assert!(manager.reply(
            id,
            (stage == 1).then_some(1),
            true,
            deadline - Duration::from_millis(1)
        ));
        if stage == 0 {
            let placement = manager.frame(viewport(), true, Instant::now());
            assert_eq!(actions(&placement, "a=p"), [id]);
            manager.written();
        }
    }
    assert_eq!(actions(&upload, "a=t").len(), 1);
    for _ in 0..128 {
        let output = manager.frame(viewport(), true, Instant::now());
        assert!(controls(&output).is_empty());
        assert_eq!(fragments(&output).len(), 3);
        manager.written();
        assert!(manager.deadline().is_none());
    }
}

#[test]
fn wrong_unknown_and_duplicate_replies_cannot_advance_or_disable() {
    let mut manager = Manager::new(vec![image(1)]);
    let upload = manager.frame(viewport(), true, Instant::now());
    manager.written();
    let id = manager.owned_ids()[0];
    let deadline = manager.deadline();
    assert!(!manager.reply(0, None, false, Instant::now()));
    assert!(!manager.reply(id, Some(7), false, Instant::now()));
    assert_eq!(manager.deadline(), deadline);
    assert!(manager.reply(id, Some(0), true, Instant::now()));
    assert!(!manager.reply(id, None, false, Instant::now()));
    let placement = manager.frame(viewport(), true, Instant::now());
    manager.written();
    assert!(!manager.reply(id, None, false, Instant::now()));
    assert!(!manager.reply(id, Some(2), true, Instant::now()));
    assert!(manager.reply(id, Some(1), true, Instant::now()));
    assert!(!manager.reply(id, Some(1), false, Instant::now()));
    assert_eq!(actions(&upload, "a=t"), [id]);
    assert_eq!(actions(&placement, "a=p"), [id]);
    assert!(
        manager
            .frame(viewport(), true, Instant::now())
            .contains(PLACEHOLDER)
    );
}

fn rejection(placement: bool) {
    let mut manager = Manager::new(vec![image(1)]);
    let upload = manager.frame(viewport(), true, Instant::now());
    let id = manager.owned_ids()[0];
    if placement {
        acknowledge(&mut manager, &upload);
        manager.frame(viewport(), true, Instant::now());
    }
    manager.written();
    assert!(manager.reply(id, placement.then_some(1), false, Instant::now()));
    assert!(manager.deadline().is_none());
    assert_eq!(manager.owned_ids(), [id]);
    assert!(!manager.reply(id, placement.then_some(1), true, Instant::now()));
    let output = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&output, "a=d"), [id]);
    assert!(!output.contains(PLACEHOLDER));
    manager.written();
    assert!(manager.owned_ids().is_empty());
    manager.reset(vec![image(2)]);
    assert!(manager.frame(viewport(), true, Instant::now()).is_empty());
}

#[test]
fn upload_rejection_permanently_falls_back_and_cleans_owned_asset() {
    rejection(false);
}
#[test]
fn placement_rejection_permanently_falls_back_and_cleans_owned_asset() {
    rejection(true);
}

fn timeout(placement: bool, late_reply: bool) {
    let mut manager = Manager::new(vec![image(1)]);
    let upload = manager.frame(viewport(), true, Instant::now());
    let id = manager.owned_ids()[0];
    if placement {
        acknowledge(&mut manager, &upload);
        manager.frame(viewport(), true, Instant::now());
    }
    manager.written();
    let deadline = manager.deadline().unwrap();
    if late_reply {
        assert!(manager.reply(id, placement.then_some(1), true, deadline));
    } else {
        assert!(
            manager
                .frame(viewport(), false, deadline - Duration::from_nanos(1))
                .is_empty()
        );
    }
    let output = manager.frame(viewport(), false, deadline);
    assert_eq!(actions(&output, "a=d"), [id]);
    assert!(!manager.reply(id, placement.then_some(1), true, deadline));
    manager.written();
    assert!(manager.frame(viewport(), true, deadline).is_empty());
    assert!(manager.deadline().is_none());
}

#[test]
fn upload_timeout_is_exact_bounded_and_applies_while_outline_hidden() {
    timeout(false, false);
}
#[test]
fn placement_timeout_is_exact_bounded_and_applies_while_outline_hidden() {
    timeout(true, false);
}
#[test]
fn upload_ack_at_deadline_cannot_revive_asset() {
    timeout(false, true);
}
#[test]
fn placement_ack_at_deadline_cannot_revive_asset() {
    timeout(true, true);
}

#[test]
fn clipping_all_four_edges_uses_source_cell_coordinates_and_body_cup() {
    let mut source = image(1);
    source.line = 5;
    source.column = 7;
    source.rows = 6;
    source.columns = 8;
    let cases = [
        Viewport {
            top: 0,
            left: 0,
            rows: 9,
            columns: 12,
        },
        Viewport {
            top: 7,
            left: 9,
            rows: 20,
            columns: 80,
        },
        Viewport {
            top: 7,
            left: 9,
            rows: 2,
            columns: 3,
        },
        Viewport {
            top: 0,
            left: 9,
            rows: 20,
            columns: 2,
        },
        Viewport {
            top: 7,
            left: 0,
            rows: 1,
            columns: 80,
        },
    ];
    for view in cases {
        let mut manager = Manager::new(vec![source.clone()]);
        let output = ready(&mut manager, view);
        let id = manager.owned_ids()[0];
        let fragments = fragments(&output);
        let top = source.line.max(view.top);
        let bottom = (source.line + source.rows).min(view.top + view.rows);
        let left = source.column.max(view.left);
        let right = (source.column + source.columns).min(view.left + view.columns);
        assert_eq!(fragments.len(), bottom - top);
        for (offset, fragment) in fragments.iter().enumerate() {
            assert_eq!(
                (fragment.row, fragment.column),
                (top + offset - view.top + 1, left - view.left + 1)
            );
            assert_eq!(fragment.image, id & 0x00ff_ffff);
            assert_eq!(fragment.placement, 1);
            assert_eq!(
                fragment.cells,
                (left..right)
                    .map(|col| (
                        top + offset - source.line,
                        col - source.column,
                        (id >> 24) as usize
                    ))
                    .collect::<Vec<_>>()
            );
            assert!(fragment.row <= view.rows);
            assert!(fragment.column + fragment.cells.len() - 1 <= view.columns);
        }
    }
}

#[test]
fn native_fragments_never_paint_leading_gutters_caption_or_status() {
    let mut source = image(1);
    source.line = 2;
    source.column = 6;
    source.rows = 4;
    source.columns = 5;
    let view = Viewport {
        top: 0,
        left: 0,
        rows: 5,
        columns: 12,
    };
    let mut manager = Manager::new(vec![source]);
    let output = ready(&mut manager, view);
    let fragments = fragments(&output);
    assert_eq!(fragments.len(), 3);
    for (offset, fragment) in fragments.iter().enumerate() {
        assert_eq!((fragment.row, fragment.column), (3 + offset, 7));
        assert_eq!(fragment.cells.len(), 5);
        assert!(fragment.row <= view.rows); // status occupies row 6, excluded by body rectangle.
    }
    assert!(!output.contains('\n'));
    assert!(!output.contains('\r'));
    assert!(!output.contains("\x1b]"));
}

#[test]
fn every_protocol_diacritic_index_and_full_32bit_color_id_roundtrip() {
    assert_eq!(marks().len(), 297);
    assert_eq!(marks().iter().collect::<HashSet<_>>().len(), 297);
    let mut source = image(1);
    source.rows = 297;
    source.columns = 297;
    let view = Viewport {
        rows: 297,
        columns: 297,
        ..viewport()
    };
    let mut manager = Manager::new(vec![source]);
    let output = ready(&mut manager, view);
    let id = manager.owned_ids()[0];
    let fragments = fragments(&output);
    assert_eq!(fragments.len(), 297);
    for (row, fragment) in fragments.iter().enumerate() {
        assert_eq!(fragment.cells.len(), 297);
        for (column, &(r, c, high)) in fragment.cells.iter().enumerate() {
            assert_eq!((r, c), (row, column));
            assert_eq!(fragment.image | ((high as u32) << 24), id);
        }
    }
}

#[test]
fn offscreen_and_zero_sized_viewports_never_upload_or_allocate() {
    let mut source = image(1);
    source.line = 50;
    source.column = 50;
    let mut manager = Manager::new(vec![source]);
    for view in [
        viewport(),
        Viewport {
            rows: 0,
            ..viewport()
        },
        Viewport {
            columns: 0,
            ..viewport()
        },
        Viewport {
            top: usize::MAX,
            left: usize::MAX,
            ..viewport()
        },
    ] {
        assert!(manager.frame(view, true, Instant::now()).is_empty());
        assert!(manager.owned_ids().is_empty());
    }
}

#[test]
fn exact_nonintersecting_edges_do_not_emit_placeholders() {
    let source = image(1);
    let mut manager = Manager::new(vec![source.clone()]);
    ready(&mut manager, viewport());
    for view in [
        Viewport {
            top: source.rows,
            ..viewport()
        },
        Viewport {
            left: source.columns,
            ..viewport()
        },
    ] {
        assert!(manager.frame(view, true, Instant::now()).is_empty());
    }
    assert_eq!(manager.owned_ids().len(), 1);
}

#[test]
fn scrolling_and_outline_hide_reuse_assets_without_retransmission() {
    let mut source = image(1);
    source.line = 10;
    let mut manager = Manager::new(vec![source]);
    ready(&mut manager, viewport());
    let ids = manager.owned_ids();
    assert!(manager.frame(viewport(), false, Instant::now()).is_empty());
    assert!(
        manager
            .frame(
                Viewport {
                    top: 100,
                    ..viewport()
                },
                true,
                Instant::now()
            )
            .is_empty()
    );
    let view = Viewport {
        top: 11,
        left: 2,
        rows: 1,
        columns: 1,
    };
    let output = manager.frame(view, true, Instant::now());
    assert!(controls(&output).is_empty());
    let fragments = fragments(&output);
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].cells[0].0, 1);
    assert_eq!(fragments[0].cells[0].1, 2);
    assert_eq!(manager.owned_ids(), ids);
}

#[test]
fn eight_asset_admission_is_stable_across_redraws_with_more_visible_assets() {
    let mut manager = Manager::new((0..12).map(image).collect());
    let upload = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&upload, "a=t").len(), 8);
    acknowledge(&mut manager, &upload);
    let placements = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&placements, "a=p").len(), 8);
    acknowledge(&mut manager, &placements);
    let ids = manager.owned_ids();
    for _ in 0..20 {
        let output = manager.frame(viewport(), true, Instant::now());
        assert!(controls(&output).is_empty());
        assert_eq!(
            output.chars().filter(|&ch| ch == PLACEHOLDER).count(),
            8 * 12
        );
        assert_eq!(manager.owned_ids(), ids);
        manager.written();
    }
}

#[test]
fn eviction_deletes_before_upload_and_retains_ids_until_written() {
    let sources: Vec<_> = (0..16)
        .map(|n| {
            let mut source = image(n);
            source.line = usize::from(n) * 4;
            source
        })
        .collect();
    let view = Viewport {
        rows: 32,
        ..viewport()
    };
    let mut manager = Manager::new(sources);
    ready(&mut manager, view);
    let old: HashSet<_> = manager.owned_ids().into_iter().collect();
    let output = manager.frame(Viewport { top: 32, ..view }, true, Instant::now());
    let deletes = actions(&output, "a=d");
    let uploads = actions(&output, "a=t");
    assert_eq!(deletes.iter().copied().collect::<HashSet<_>>(), old);
    assert_eq!(uploads.len(), 8);
    assert!(uploads.iter().all(|id| !old.contains(id)));
    let chunks = controls(&output);
    assert!(
        chunks[..8]
            .iter()
            .all(|(header, _)| header.starts_with("a=d,d=I,"))
    );
    assert!(
        chunks[8..]
            .iter()
            .all(|(header, _)| header.starts_with("a=t,"))
    );
    assert_eq!(manager.owned_ids().len(), 16);
    assert!(
        old.iter()
            .all(|&id| !manager.reply(id, None, true, Instant::now()))
    );
    manager.written();
    assert_eq!(manager.owned_ids(), uploads);
}

#[test]
fn retirement_backpressure_bounds_all_owned_ids_even_without_successful_writes() {
    let mut manager = Manager::new(Vec::new());
    for batch in 0..100 {
        manager.reset((0..8).map(|n| image(batch + n)).collect());
        let output = manager.frame(viewport(), true, Instant::now());
        assert!(manager.owned_ids().len() <= 16);
        assert!(actions(&output, "a=t").len() <= 8);
    }
    let output = manager.cleanup();
    let ids = manager.owned_ids();
    assert_eq!(ids.len(), 16);
    assert_eq!(
        actions(&output, "a=d")
            .iter()
            .copied()
            .collect::<HashSet<_>>(),
        ids.into_iter().collect()
    );
    manager.written();
    assert!(manager.owned_ids().is_empty());
}

#[test]
fn per_asset_virtual_placement_limit_is_eight_without_redraw_churn() {
    let source = image(1);
    let scenes: Vec<_> = (1..=12)
        .map(|columns| {
            let mut source = source.clone();
            source.columns = columns;
            source
        })
        .collect();
    let mut manager = Manager::new(scenes);
    let upload = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&upload, "a=t").len(), 1);
    acknowledge(&mut manager, &upload);
    let placements = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&placements, "a=p").len(), 8);
    let ids: Vec<_> = controls(&placements)
        .iter()
        .map(|(header, _)| value(header, "p").unwrap())
        .collect();
    assert_eq!(ids, (1..=8).collect::<Vec<_>>());
    acknowledge(&mut manager, &placements);
    for _ in 0..10 {
        let output = manager.frame(viewport(), true, Instant::now());
        assert!(controls(&output).is_empty());
        assert_eq!(fragments(&output).len(), 8 * 3);
    }
}

#[test]
fn distinct_source_rgba_capture_budget_is_32mib_and_omissions_keep_fallback() {
    let mut first = image(1);
    first.width = 1024;
    first.height = 8192;
    first.pixels = Arc::from(vec![1; MAX_BYTES]);
    let mut manager = Manager::new(vec![first.clone(), image(2)]);
    // Two references held by the manager (source and live cache), not another copy.
    let output = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&output, "a=t").len(), 1);
    assert_eq!(Arc::strong_count(&first.pixels), 3);
    let chunks = controls(&output);
    assert_eq!(
        chunks.iter().map(|(_, data)| data.len()).sum::<usize>(),
        MAX_BYTES.div_ceil(3) * 4
    );
    manager.cleanup();
    assert_eq!(Arc::strong_count(&first.pixels), 1);
}

#[test]
fn equal_distinct_arcs_deduplicate_within_the_pixel_budget() {
    let mut first = image(1);
    first.width = 1024;
    first.height = 4096;
    first.pixels = Arc::from(vec![1; MAX_BYTES / 2]);
    let mut duplicate = first.clone();
    duplicate.pixels = Arc::from(first.pixels.to_vec());
    let extra = image(3);
    let mut manager = Manager::new(vec![first.clone(), duplicate.clone(), extra]);
    let output = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&output, "a=t").len(), 2);
    assert_eq!(Arc::strong_count(&duplicate.pixels), 1);
    assert_eq!(Arc::strong_count(&first.pixels), 3);
}

#[test]
fn scene_capture_limit_is_4096_even_for_one_shared_raster() {
    let source = image(1);
    let mut scenes = vec![source.clone(); 4096];
    let mut beyond = image(2);
    beyond.line = 50;
    scenes.push(beyond);
    let mut manager = Manager::new(scenes);
    let output = manager.frame(
        Viewport {
            top: 50,
            ..viewport()
        },
        true,
        Instant::now(),
    );
    assert!(output.is_empty());
    assert!(manager.owned_ids().is_empty());
}

#[test]
fn overlapping_repeated_scenes_obey_the_total_placeholder_cell_budget() {
    let mut source = image(1);
    source.rows = 297;
    source.columns = 297;
    let mut manager = Manager::new(vec![source; 12]);
    let view = Viewport {
        rows: 297,
        columns: 297,
        ..viewport()
    };
    let output = ready(&mut manager, view);
    assert_eq!(
        output.chars().filter(|&ch| ch == PLACEHOLDER).count(),
        8 * 297 * 297
    );
    assert_eq!(manager.owned_ids().len(), 1);
}

#[test]
fn invalid_rasters_grids_and_overflowing_boxes_are_rejected_before_allocation() {
    let source = image(1);
    let mut invalid = Vec::new();
    for (width, height) in [(0, 1), (1, 0), (u32::MAX, u32::MAX), (1024, 8193)] {
        let mut candidate = source.clone();
        candidate.width = width;
        candidate.height = height;
        invalid.push(candidate);
    }
    for bytes in [0, 3, 5] {
        let mut candidate = source.clone();
        candidate.pixels = Arc::from(vec![0; bytes]);
        invalid.push(candidate);
    }
    for (columns, rows) in [(0, 1), (1, 0), (298, 1), (1, 298)] {
        let mut candidate = source.clone();
        candidate.columns = columns;
        candidate.rows = rows;
        invalid.push(candidate);
    }
    for (line, column) in [(usize::MAX, 0), (0, usize::MAX)] {
        let mut candidate = source.clone();
        candidate.line = line;
        candidate.column = column;
        invalid.push(candidate);
    }
    let mut manager = Manager::new(invalid);
    assert!(manager.frame(viewport(), true, Instant::now()).is_empty());
    assert!(manager.owned_ids().is_empty());
}

#[test]
fn equal_reallocated_rasters_rebind_and_reflow_coordinates_without_upload() {
    let source = image(1);
    let mut manager = Manager::new(vec![source.clone()]);
    ready(&mut manager, viewport());
    let ids = manager.owned_ids();
    let mut replacement = source.clone();
    replacement.pixels = Arc::from(source.pixels.to_vec());
    replacement.line = 4;
    replacement.column = 7;
    manager.reset(vec![replacement.clone()]);
    assert_eq!(Arc::strong_count(&source.pixels), 1);
    assert_eq!(Arc::strong_count(&replacement.pixels), 3);
    let output = manager.frame(viewport(), true, Instant::now());
    assert!(controls(&output).is_empty());
    assert_eq!(manager.owned_ids(), ids);
    let fragments = fragments(&output);
    assert_eq!((fragments[0].row, fragments[0].column), (5, 8));
}

#[test]
fn obsolete_grid_retires_whole_image_and_requires_fresh_two_stage_ack() {
    let mut source = image(1);
    let mut manager = Manager::new(vec![source.clone()]);
    ready(&mut manager, viewport());
    let old = manager.owned_ids()[0];
    source.columns = 5;
    manager.reset(vec![source]);
    assert_eq!(manager.owned_ids(), [old]);
    manager.written();
    assert_eq!(
        manager.owned_ids(),
        [old],
        "unemitted deletion must survive written"
    );
    let output = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&output, "a=d"), [old]);
    let new = actions(&output, "a=t")[0];
    assert_ne!(new, old);
    assert!(!output.contains(PLACEHOLDER));
    assert!(!manager.reply(old, Some(1), true, Instant::now()));
    acknowledge(&mut manager, &output);
    assert_eq!(manager.owned_ids(), [new]);
    let placement = manager.frame(viewport(), true, Instant::now());
    assert_eq!(value(controls(&placement)[0].0, "c"), Some(5));
    acknowledge(&mut manager, &placement);
    assert_eq!(
        fragments(&manager.frame(viewport(), true, Instant::now()))[0]
            .cells
            .len(),
        5
    );
}

#[test]
fn source_pixel_or_dimension_changes_do_not_reuse_an_acknowledged_asset() {
    let first = image(1);
    let mut manager = Manager::new(vec![first.clone()]);
    ready(&mut manager, viewport());
    let old = manager.owned_ids()[0];
    let mut replacement = first;
    replacement.width = 2;
    replacement.pixels = Arc::from([1; 8]);
    manager.reset(vec![replacement]);
    let output = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&output, "a=d"), [old]);
    assert_ne!(actions(&output, "a=t"), [old]);
    assert!(!output.contains(PLACEHOLDER));
}

#[test]
fn reset_after_frame_keeps_new_unemitted_retirements_registered() {
    let first = image(1);
    let mut second = image(2);
    second.line = 5;
    let mut manager = Manager::new(vec![first.clone(), second.clone()]);
    ready(&mut manager, viewport());
    let ids = manager.owned_ids();
    manager.reset(vec![second]);
    let output = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&output, "a=d"), [ids[0]]);
    manager.reset(Vec::new());
    manager.written();
    assert_eq!(manager.owned_ids(), [ids[1]]);
    let output = manager.frame(viewport(), false, Instant::now());
    assert_eq!(actions(&output, "a=d"), [ids[1]]);
    assert_eq!(manager.owned_ids(), [ids[1]]);
    manager.written();
    assert!(manager.owned_ids().is_empty());
}

#[test]
fn cleanup_is_repeatable_id_scoped_and_covers_pending_and_ready_assets() {
    let mut manager = Manager::new(vec![image(1), image(2)]);
    let upload = manager.frame(viewport(), true, Instant::now());
    manager.written();
    let ids = manager.owned_ids();
    assert!(manager.reply(ids[0], None, true, Instant::now()));
    let placements = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&placements, "a=p"), [ids[0]]);
    let output = manager.cleanup();
    assert_eq!(manager.cleanup(), output);
    assert_eq!(
        manager.owned_ids().iter().copied().collect::<HashSet<_>>(),
        ids.iter().copied().collect()
    );
    for (header, payload) in controls(&output) {
        assert!(header.starts_with("a=d,d=I,i="));
        assert_eq!(value(header, "q"), Some(2));
        assert!(ids.contains(&value(header, "i").unwrap()));
        assert!(payload.is_empty());
    }
    assert_eq!(actions(&output, "a=d").len(), 2);
    assert_eq!(actions(&upload, "a=t").len(), 2);
    manager.written();
    assert!(manager.owned_ids().is_empty());
    assert!(manager.cleanup().is_empty());
    assert!(manager.frame(viewport(), true, Instant::now()).is_empty());
}

#[test]
fn lru_eviction_preserves_recently_used_inactive_assets() {
    let sources: Vec<_> = (0..9)
        .map(|n| {
            let mut source = image(n);
            source.line = usize::from(n) * 4;
            source
        })
        .collect();
    let mut manager = Manager::new(sources);
    let view = Viewport {
        rows: 3,
        ..viewport()
    };
    for n in 0..8 {
        ready(&mut manager, Viewport { top: n * 4, ..view });
    }
    let old = manager.owned_ids();
    assert_eq!(old.len(), 8);
    assert!(controls(&manager.frame(view, true, Instant::now())).is_empty());
    let output = manager.frame(Viewport { top: 32, ..view }, true, Instant::now());
    assert_eq!(actions(&output, "a=d"), [old[1]]);
    assert_eq!(actions(&output, "a=t").len(), 1);
    assert_eq!(manager.owned_ids().len(), 9);
    manager.written();
    let remaining = manager.owned_ids();
    assert_eq!(remaining.len(), 8);
    assert!(remaining.contains(&old[0]));
    assert!(!remaining.contains(&old[1]));
    assert!(old[2..].iter().all(|id| remaining.contains(id)));
}

#[test]
fn deadline_is_earliest_outstanding_stage_not_latest_upload() {
    let first = image(1);
    let mut second = image(2);
    second.line = 10;
    let mut manager = Manager::new(vec![first, second]);
    let view = Viewport {
        rows: 3,
        ..viewport()
    };
    let first = manager.frame(view, true, Instant::now());
    manager.written();
    let first_deadline = manager.deadline().unwrap();
    let second = manager.frame(Viewport { top: 10, ..view }, true, Instant::now());
    manager.written();
    assert_eq!(manager.deadline(), Some(first_deadline));
    assert!(manager.reply(actions(&first, "a=t")[0], None, true, Instant::now()));
    let second_deadline = manager.deadline().unwrap();
    assert!(second_deadline >= first_deadline);
    assert!(manager.reply(actions(&second, "a=t")[0], None, true, Instant::now()));
    assert!(manager.deadline().is_none());
}

#[test]
fn reset_reorders_source_indices_without_confusing_asset_identity() {
    let first = image(1);
    let mut second = image(2);
    second.line = 5;
    let mut manager = Manager::new(vec![first.clone(), second.clone()]);
    ready(&mut manager, viewport());
    let ids = manager.owned_ids();
    let mut first = first;
    first.line = 5;
    second.line = 0;
    manager.reset(vec![second, first]);
    let output = manager.frame(viewport(), true, Instant::now());
    assert!(controls(&output).is_empty());
    let fragments = fragments(&output);
    assert_eq!(fragments[0].image, ids[1] & 0x00ff_ffff);
    assert_eq!(fragments[3].image, ids[0] & 0x00ff_ffff);
    assert_eq!(manager.owned_ids(), ids);
}

#[test]
fn matching_raw_bytes_but_different_dimensions_do_not_alias() {
    let mut source = image(1);
    source.width = 2;
    source.pixels = Arc::from([1; 8]);
    let mut manager = Manager::new(vec![source.clone()]);
    ready(&mut manager, viewport());
    let id = manager.owned_ids()[0];
    source.width = 1;
    source.height = 2;
    manager.reset(vec![source]);
    let output = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&output, "a=d"), [id]);
    let upload = controls(&output)
        .into_iter()
        .find(|(header, _)| header.starts_with("a=t,"))
        .unwrap();
    assert_eq!(value(upload.0, "s"), Some(1));
    assert_eq!(value(upload.0, "v"), Some(2));
    assert_ne!(value(upload.0, "i"), Some(id));
}

#[test]
fn changed_pixels_with_unchanged_dimensions_require_new_upload() {
    let mut manager = Manager::new(vec![image(1)]);
    ready(&mut manager, viewport());
    let id = manager.owned_ids()[0];
    let replacement = image(2);
    manager.reset(vec![replacement.clone()]);
    let output = manager.frame(viewport(), true, Instant::now());
    assert_eq!(actions(&output, "a=d"), [id]);
    let upload = controls(&output)
        .into_iter()
        .find(|(header, _)| header.starts_with("a=t,"))
        .unwrap();
    assert_eq!(decode(upload.1), replacement.pixels.as_ref());
    assert_ne!(value(upload.0, "i"), Some(id));
}

#[test]
fn near_usize_max_document_offsets_clip_without_overflow() {
    let mut source = image(1);
    source.line = usize::MAX - 3;
    source.column = usize::MAX - 4;
    let view = Viewport {
        top: usize::MAX - 2,
        left: usize::MAX - 2,
        rows: 20,
        columns: 80,
    };
    let mut manager = Manager::new(vec![source]);
    let output = ready(&mut manager, view);
    let fragments = fragments(&output);
    assert_eq!(fragments.len(), 2);
    assert_eq!((fragments[0].row, fragments[0].column), (1, 1));
    assert_eq!((fragments[1].row, fragments[1].column), (2, 1));
    assert_eq!(fragments[0].cells.len(), 2);
    assert_eq!((fragments[0].cells[0].0, fragments[0].cells[0].1), (1, 2));
    assert_eq!((fragments[1].cells[1].0, fragments[1].cells[1].1), (2, 3));
}

#[test]
fn outline_before_first_frame_does_not_allocate_upload_or_create_deadlines() {
    let mut manager = Manager::new(vec![image(1)]);
    for _ in 0..32 {
        assert!(manager.frame(viewport(), false, Instant::now()).is_empty());
        manager.written();
        assert!(manager.deadline().is_none());
        assert!(manager.owned_ids().is_empty());
    }
    assert_eq!(
        actions(&manager.frame(viewport(), true, Instant::now()), "a=t").len(),
        1
    );
}

#[test]
fn one_rejection_retires_all_live_assets_and_releases_all_source_arcs() {
    let sources = vec![image(1), image(2), image(3)];
    let mut manager = Manager::new(sources.clone());
    manager.frame(viewport(), true, Instant::now());
    manager.written();
    let ids = manager.owned_ids();
    assert!(manager.reply(ids[0], None, true, Instant::now()));
    assert!(manager.reply(ids[1], None, false, Instant::now()));
    assert!(manager.deadline().is_none());
    assert!(
        sources
            .iter()
            .all(|source| Arc::strong_count(&source.pixels) == 1)
    );
    let output = manager.frame(viewport(), false, Instant::now());
    assert_eq!(
        actions(&output, "a=d").into_iter().collect::<HashSet<_>>(),
        ids.iter().copied().collect()
    );
    assert!(
        ids.iter()
            .all(|&id| !manager.reply(id, None, true, Instant::now()))
    );
    assert_eq!(manager.owned_ids().len(), 3);
    manager.written();
    assert!(manager.owned_ids().is_empty());
}

#[test]
fn owned_ids_are_unique_processwide_even_across_managers_and_threads() {
    let threads: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                let mut ids = Vec::new();
                for _ in 0..64 {
                    let mut manager = Manager::new(vec![image(1)]);
                    manager.frame(viewport(), true, Instant::now());
                    ids.extend(manager.owned_ids());
                }
                ids
            })
        })
        .collect();
    let ids: Vec<_> = threads
        .into_iter()
        .flat_map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(ids.len(), 512);
    assert!(!ids.contains(&0));
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
}
