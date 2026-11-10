// Author: machinageist
// Date: 2026-11-10
// Description: PDF text through poppler's pdftotext, printed page by page with a rule between pages
// Notes: The PDF goes in on stdin and the text comes back on stdout, so no shell or temporary
// file is involved; the layout came from the page, so lines are printed as written

use std::io::{ErrorKind, Write};
use std::process::{Command, Stdio};

use crate::data::{self, StyledLine};
use crate::render::{Heading, Options, Rendered};
use crate::style::{STYLE_FRAME, Style};

// The extractor and its arguments: keep the layout, write UTF-8, read stdin, write stdout
const PDFTOTEXT: &str = "pdftotext";
const PDFTOTEXT_ARGS: [&str; 5] = ["-layout", "-enc", "UTF-8", "-", "-"];
const MISSING_TOOL: &str = "reading PDF needs pdftotext from poppler: `brew install poppler` or `apt install poppler-utils`";
const FAILED: &str = "pdftotext failed";

// pdftotext ends every page with a form feed
const PAGE_BREAK: char = '\u{c}';
// The rule between pages and the outline entry for each
const RULE: char = '─';
const RULE_LEAD: usize = 3;
const PAGE_LABEL: &str = "page";
const PAGE_HEADING: &str = "Page";
const PAGE_LEVEL: u8 = 1;

// Run pdftotext over a PDF's bytes and return its text
pub fn text(bytes: &[u8]) -> Result<String, String> {
    let mut child = Command::new(PDFTOTEXT)
        .args(PDFTOTEXT_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => MISSING_TOOL.to_owned(),
            _ => format!("{FAILED}: {error}"),
        })?;
    let mut stdin = child.stdin.take().ok_or(FAILED)?;
    // Write on another thread so a full stdout pipe cannot deadlock the two processes
    let output = std::thread::scope(|scope| {
        scope.spawn(move || {
            // A tool that stops reading early reports its own error below
            let _ = stdin.write_all(bytes);
        });
        child.wait_with_output()
    })
    .map_err(|error| format!("{FAILED}: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.trim();
        return Err(if reason.is_empty() {
            format!("{FAILED} ({})", output.status)
        } else {
            format!("{FAILED}: {reason}")
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// Print page text with a labelled rule between pages and a heading per page
pub fn present(text: &str, options: &Options) -> Rendered {
    let mut pages: Vec<&str> = text.split(PAGE_BREAK).collect();
    // The last page's form feed leaves an empty piece behind
    if pages.len() > 1 && pages.last().is_some_and(|page| page.trim().is_empty()) {
        pages.pop();
    }
    let mut lines: Vec<StyledLine> = Vec::new();
    let mut page_starts = Vec::with_capacity(pages.len());
    for (index, page) in pages.iter().enumerate() {
        let number = index + 1;
        if index == 0 {
            page_starts.push(0);
        } else {
            lines.push(Vec::new());
            page_starts.push(lines.len());
            lines.push(rule(number, options.width));
            lines.push(Vec::new());
        }
        let page_lines: Vec<&str> = page.lines().collect();
        let kept = page_lines
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .map_or(0, |last| last + 1);
        lines.extend(
            page_lines[..kept]
                .iter()
                .map(|line| vec![(Style::PLAIN, (*line).to_owned())]),
        );
    }
    let (text, starts) = data::print_counted(&lines, options);
    let headings = page_starts
        .iter()
        .enumerate()
        .map(|(index, start)| Heading {
            level: PAGE_LEVEL,
            text: format!("{PAGE_HEADING} {}", index + 1),
            line: starts.get(*start).copied().unwrap_or_default(),
        })
        .collect();
    Rendered { text, headings }
}

// A dim rule labelled with the page number, as wide as the output
fn rule(number: usize, width: usize) -> StyledLine {
    let lead: String = std::iter::repeat_n(RULE, RULE_LEAD).collect();
    let label = format!("{lead} {PAGE_LABEL} {number} ");
    let tail: String =
        std::iter::repeat_n(RULE, width.saturating_sub(label.chars().count())).collect();
    vec![(STYLE_FRAME, format!("{label}{tail}"))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_split_on_form_feeds_with_rules_and_headings() {
        let text = "Title\n\n  body\n\n\n\u{c}Second\n\u{c}";
        let rendered = present(text, &Options::new(20, false));
        assert_eq!(
            rendered.text,
            "Title\n\n  body\n\n─── page 2 ─────────\n\nSecond\n"
        );
        let headings: Vec<_> = rendered
            .headings
            .iter()
            .map(|heading| (heading.text.as_str(), heading.line))
            .collect();
        assert_eq!(headings, [("Page 1", 0), ("Page 2", 4)]);
    }

    #[test]
    fn text_without_form_feeds_is_one_page() {
        let rendered = present("only\n", &Options::new(20, false));
        assert_eq!(rendered.text, "only\n");
        assert_eq!(rendered.headings.len(), 1);
    }
}
