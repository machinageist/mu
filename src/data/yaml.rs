// Author: machinageist
// Date: 2026-11-09
// Description: YAML syntax highlighting into styled lines
// Notes: Stub until implemented; lines pass through unstyled

use super::{StyledLine, plain_lines};

// Check whether the head of an input looks like YAML
pub fn sniff(_head: &str) -> bool {
    false
}

// Classify YAML text into styled lines
pub fn highlight(input: &str) -> Vec<StyledLine> {
    plain_lines(input)
}
