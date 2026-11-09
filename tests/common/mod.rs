// Author: machinageist
// Date: 2026-11-09
// Description: Shared helpers for per-format tests: rendering at a width and the output invariants
// Notes: Every format must pass `assert_invariants` over its fixture

#![allow(dead_code)]

use mu::{Format, Options, present};
use unicode_width::UnicodeWidthStr;

pub const ESC: char = '\x1b';
const SGR_RESET: &str = "\x1b[0m";
const SWEEP_WIDTHS: std::ops::RangeInclusive<usize> = 30..=120;
const COLOR_WIDTHS: [usize; 5] = [30, 47, 60, 80, 100];

// Render with color off at a fixed width
pub fn plain(input: &str, format: Format, width: usize) -> String {
    present(
        input,
        format,
        &Options {
            width,
            color: false,
        },
    )
}

// Render with color on at a fixed width
pub fn colored(input: &str, format: Format, width: usize) -> String {
    present(input, format, &Options { width, color: true })
}

// Remove SGR sequences of the form ESC [ ... m
pub fn strip_sgr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == ESC {
            for inner in chars.by_ref() {
                if inner == 'm' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

// Check width, color/plain equivalence, per-line resets and control scrubbing
pub fn assert_invariants(input: &str, format: Format) {
    for width in SWEEP_WIDTHS {
        for line in plain(input, format, width).lines() {
            assert!(
                line.width() <= width,
                "{format:?} width {width}: {line:?} is {} wide",
                line.width()
            );
        }
    }
    for width in COLOR_WIDTHS {
        let color = colored(input, format, width);
        assert_eq!(
            strip_sgr(&color),
            plain(input, format, width),
            "{format:?} width {width}"
        );
        for line in color.lines() {
            if let Some(last) = line.rfind(ESC) {
                assert!(
                    line[last..].starts_with(SGR_RESET),
                    "style left open: {line:?}"
                );
            }
        }
    }
    let hostile = format!("{input}\n\x1b]52;c;ZXZpbA==\x07 x\x1b[2J\n");
    for out in [
        plain(&hostile, format, 60),
        strip_sgr(&colored(&hostile, format, 60)),
    ] {
        assert!(!out.contains(ESC), "{format:?}: escape leaked");
        assert!(!out.contains('\x07'), "{format:?}: bell leaked");
    }
}
