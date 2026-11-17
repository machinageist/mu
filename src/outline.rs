// Author: machinageist
// Date: 2026-11-09
// Description: A document's outline: its headings, indented by level and cut to the width
// Notes: Shared by `--toc` and the pager's outline overlay

use crate::display;
use crate::render::{Heading, Options};
use crate::style::{STYLE_HEADINGS, paint};

const INDENT_PER_LEVEL: usize = 2;
const ELLIPSIS: char = '…';

// One plain line per heading, indented two spaces per level below the shallowest present
pub fn outline_entries(headings: &[Heading], width: usize) -> Vec<String> {
    let shallowest = headings
        .iter()
        .map(|heading| heading.level)
        .min()
        .unwrap_or(1);
    headings
        .iter()
        .map(|heading| {
            let indent = usize::from(heading.level - shallowest) * INDENT_PER_LEVEL;
            let indent = indent.min(width.saturating_sub(1));
            let text = fit_right(&heading.text, width - indent);
            format!("{}{text}", " ".repeat(indent))
        })
        .collect()
}

// Print the outline, each heading in its level's style when color is on
pub fn outline(headings: &[Heading], options: &Options) -> String {
    let mut out = String::new();
    for (heading, entry) in headings
        .iter()
        .zip(outline_entries(headings, options.width))
    {
        let text = entry.trim_start();
        out.push_str(&entry[..entry.len() - text.len()]);
        let style = STYLE_HEADINGS[usize::from(heading.level.clamp(1, 6)) - 1];
        paint(&mut out, text, style, options.color);
        out.push('\n');
    }
    out
}

// Cut text to `room` columns, marking the cut with an ellipsis
fn fit_right(text: &str, room: usize) -> String {
    if room == 0 {
        return String::new();
    }
    if display::width(text) <= room {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for (unit, width) in display::units(text) {
        if used + width + 1 > room {
            break;
        }
        used += width;
        out.push_str(unit);
    }
    // Never leave a space hanging before the ellipsis
    let mut out = out.trim_end().to_owned();
    out.push(ELLIPSIS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a heading at a level
    fn heading(level: u8, text: &str) -> Heading {
        Heading {
            level,
            text: text.to_owned(),
            line: 0,
        }
    }

    #[test]
    fn entries_indent_from_the_shallowest_level() {
        let headings = [heading(2, "Intro"), heading(3, "Detail"), heading(2, "End")];
        assert_eq!(outline_entries(&headings, 40), ["Intro", "  Detail", "End"]);
    }

    #[test]
    fn long_headings_are_cut_to_the_width() {
        let headings = [heading(1, "Top"), heading(2, "A rather long 見出し")];
        let entries = outline_entries(&headings, 12);
        assert_eq!(entries[1], "  A rather…");
        assert!(entries.iter().all(|entry| display::width(entry) <= 12));
    }

    #[test]
    fn colored_outline_styles_each_level() {
        let options = Options::new(40, true);
        let text = outline(&[heading(1, "Top"), heading(2, "Sub")], &options);
        assert!(text.starts_with('\x1b'));
        assert!(text.contains("  \x1b"));
        assert_eq!(outline(&[], &options), "");
    }

    #[test]
    fn clipping_uses_indivisible_graphemes_even_at_zero_width() {
        let headings = [heading(1, "é👩‍💻中🇺🇸XYZ")];
        assert_eq!(outline_entries(&headings, 0), [""]);
        assert_eq!(outline_entries(&headings, 3), ["é…"]);
        assert_eq!(outline_entries(&headings, 4), ["é👩‍💻…"]);
        assert_eq!(outline_entries(&headings, 6), ["é👩‍💻中…"]);
    }
}
