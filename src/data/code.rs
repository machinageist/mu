// Author: machinageist
// Date: 2026-11-09
// Description: Source files shown verbatim as plain lines, plus a shebang sniff
// Notes: The plain lines used without color; `crate::highlight` styles the same lines

use super::StyledLine;
use crate::style::Style;

// A script's interpreter line
const SHEBANG: &str = "#!";
pub(crate) const BYTE_ORDER_MARK: char = '\u{feff}';

// True when the input starts with a shebang line
pub fn sniff(head: &str) -> bool {
    head.trim_start_matches(BYTE_ORDER_MARK)
        .starts_with(SHEBANG)
}

// Split source text into unstyled lines, kept exactly as written
pub fn lines(input: &str) -> Vec<StyledLine> {
    input
        .trim_start_matches(BYTE_ORDER_MARK)
        .lines()
        .map(|line| vec![(Style::PLAIN, line.to_owned())])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shebang_sniffs_and_prose_does_not() {
        assert!(sniff("#!/bin/sh\necho hi\n"));
        assert!(sniff("\u{feff}#!/usr/bin/env python3\n"));
        assert!(!sniff("# Title\n"));
        assert!(!sniff("Hello #!\n"));
    }

    #[test]
    fn lines_are_kept_verbatim() {
        let lines = lines("\u{feff}int *p;\r\n  # define X _y_\n");
        let text: Vec<&str> = lines.iter().map(|line| line[0].1.as_str()).collect();
        assert_eq!(text, ["int *p;", "  # define X _y_"]);
    }
}
