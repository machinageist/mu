// Author: machinageist
// Date: 2026-11-09
// Description: Table layout, fitting columns into the available width and drawing box borders
// Notes: Columns shrink widest-first so narrow columns keep their natural size,
// and cells wrap inside their column instead of overflowing the terminal

use std::ops::Range;

use pulldown_cmark::Alignment;

use crate::inline::{Inline, Line, measure, wrap};
use crate::style::{STYLE_FRAME, Style, paint};

// Narrowest a column may be squeezed to before the table is allowed to overflow
pub const MIN_COLUMN_WIDTH: usize = 3;

// One space of padding on each side of a cell
const CELL_PADDING: usize = 2;

// Box drawing pieces as left edge, column junction, right edge
const BORDER_TOP: [&str; 3] = ["┌", "┬", "┐"];
const BORDER_HEAD: [&str; 3] = ["├", "┼", "┤"];
const BORDER_BOTTOM: [&str; 3] = ["└", "┴", "┘"];
const BORDER_FILL: &str = "─";
const BORDER_SIDE: &str = "│";

pub struct Cell {
    pub atoms: Range<usize>,
    pub base: Style,
}

pub struct Row {
    pub cells: Vec<Cell>,
    pub head: bool,
}

pub struct Table {
    pub aligns: Vec<Alignment>,
    pub rows: Vec<Row>,
    pub cells: Vec<Cell>,
    pub cell_start: usize,
    pub in_head: bool,
}

impl Table {
    // Start an empty table with the given column alignments
    pub fn new(aligns: Vec<Alignment>) -> Table {
        Table {
            aligns,
            rows: Vec::new(),
            cells: Vec::new(),
            cell_start: 0,
            in_head: false,
        }
    }

    // Close the row being collected
    pub fn end_row(&mut self) {
        let cells = std::mem::take(&mut self.cells);
        self.rows.push(Row {
            cells,
            head: self.in_head,
        });
    }
}

// Lay out and draw the table as finished lines, without container gutters
// Notes: columns shrink no narrower than `min_column`, or their natural width when that is less
pub fn render(
    table: &Table,
    inline: &Inline,
    limit: usize,
    min_column: usize,
    color: bool,
) -> Vec<String> {
    let longest_row = table
        .rows
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(0);
    let columns = longest_row.max(table.aligns.len());
    if columns == 0 {
        return Vec::new();
    }

    // Natural column widths, then squeeze to fit
    let mut widths = vec![0; columns];
    for row in &table.rows {
        for (column, cell) in row.cells.iter().enumerate() {
            let natural = measure(&inline.atoms[cell.atoms.clone()]);
            widths[column] = widths[column].max(natural);
        }
    }
    let chrome = columns * (CELL_PADDING + 1) + 1;
    fit_columns(&mut widths, limit.saturating_sub(chrome), min_column);

    // Borders around the header and the body
    let mut lines = vec![border(&widths, BORDER_TOP, color)];
    for (index, row) in table.rows.iter().enumerate() {
        draw_row(&mut lines, row, &widths, &table.aligns, inline, color);
        if row.head && index + 1 < table.rows.len() {
            lines.push(border(&widths, BORDER_HEAD, color));
        }
    }
    lines.push(border(&widths, BORDER_BOTTOM, color));
    lines
}

// Shrink the widest column one step at a time until the total fits the budget
fn fit_columns(widths: &mut [usize], budget: usize, min_column: usize) {
    for width in widths.iter_mut() {
        *width = (*width).max(1);
    }
    let mut total: usize = widths.iter().sum();
    while total > budget {
        let Some((widest, &width)) = widths.iter().enumerate().max_by_key(|&(_, width)| *width)
        else {
            break;
        };
        if width <= min_column {
            break;
        }
        widths[widest] -= 1;
        total -= 1;
    }
}

// Draw a horizontal border with junctions between columns
fn border(widths: &[usize], pieces: [&str; 3], color: bool) -> String {
    let mut plain = String::from(pieces[0]);
    for (column, width) in widths.iter().enumerate() {
        if column > 0 {
            plain.push_str(pieces[1]);
        }
        for _ in 0..width + CELL_PADDING {
            plain.push_str(BORDER_FILL);
        }
    }
    plain.push_str(pieces[2]);
    let mut line = String::new();
    paint(&mut line, &plain, STYLE_FRAME, color);
    line
}

// Wrap every cell in a row and draw the row as one or more lines
fn draw_row(
    lines: &mut Vec<String>,
    row: &Row,
    widths: &[usize],
    aligns: &[Alignment],
    inline: &Inline,
    color: bool,
) {
    let wrapped: Vec<Vec<Line>> = widths
        .iter()
        .enumerate()
        .map(|(column, &width)| match row.cells.get(column) {
            Some(cell) => wrap(
                &inline.atoms[cell.atoms.clone()],
                &inline.arena,
                width,
                cell.base,
                color,
            ),
            None => Vec::new(),
        })
        .collect();
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(0).max(1);

    for line_index in 0..height {
        let mut line = String::new();
        paint(&mut line, BORDER_SIDE, STYLE_FRAME, color);
        for (column, cell_lines) in wrapped.iter().enumerate() {
            let (text, width) = cell_lines.get(line_index).map_or(("", 0), |cell_line| {
                (cell_line.text.as_str(), cell_line.width)
            });
            let gap = widths[column].saturating_sub(width);
            let (left, right) = match aligns.get(column) {
                Some(Alignment::Right) => (gap, 0),
                Some(Alignment::Center) => (gap / 2, gap - gap / 2),
                _ => (0, gap),
            };
            line.push(' ');
            push_spaces(&mut line, left);
            line.push_str(text);
            push_spaces(&mut line, right);
            line.push(' ');
            paint(&mut line, BORDER_SIDE, STYLE_FRAME, color);
        }
        lines.push(line);
    }
}

// Append a run of spaces
fn push_spaces(out: &mut String, count: usize) {
    for _ in 0..count {
        out.push(' ');
    }
}
