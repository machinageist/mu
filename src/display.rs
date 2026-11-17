//! Shared terminal display units for layout, clipping and hit testing.
//!
//! Units are Unicode extended grapheme clusters (UAX #29), measured individually
//! using `unicode-width`'s non-CJK width. Ambiguous characters occupy one column;
//! combining sequences and emoji ZWJ sequences remain indivisible. Width is the
//! sum of unit widths, not scalar widths. Zero-width units are retained. Tabs and
//! controls are not expanded or sanitized here: callers must handle source text
//! controls before layout. ANSI controls are not parsed; strip generated SGR/OSC
//! before measuring, or process controls separately while retaining grapheme runs.
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Iterate borrowed, indivisible extended grapheme clusters and their columns.
pub fn units(text: &str) -> impl Iterator<Item = (&str, usize)> {
    text.graphemes(true)
        .map(|unit| (unit, UnicodeWidthStr::width(unit)))
}

/// Measure text with exactly the same unit model used by the wrapper.
pub fn width(text: &str) -> usize {
    units(text).map(|(_, width)| width).sum()
}

// Segment visible text across highlighter style boundaries, then replay the
// original fragments. SGR changes must not split an extended grapheme cluster.
pub(crate) fn styled_units(
    line: &[crate::data::Span],
    mut visit: impl FnMut(&str, usize, &[(crate::style::Style, &str)]),
) {
    let text: String = line.iter().map(|(_, text)| text.as_str()).collect();
    let (mut index, mut offset) = (0, 0);
    let mut fragments = Vec::new();
    for (unit, width) in units(&text) {
        fragments.clear();
        let mut remaining = unit.len();
        while remaining > 0 {
            let (style, source) = &line[index];
            let take = (source.len() - offset).min(remaining);
            if take > 0 {
                fragments.push((*style, &source[offset..offset + take]));
            }
            offset += take;
            remaining -= take;
            if offset == source.len() {
                index += 1;
                offset = 0;
            }
        }
        visit(unit, width, &fragments);
    }
}
