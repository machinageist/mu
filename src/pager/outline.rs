// Author: machinageist
// Date: 2026-11-09
// Description: The pager's outline overlay: which heading is selected and which part of the list shows
// Notes: Pure state over a heading count and a window height, so it is tested without a terminal

use super::keys::Action;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Outline {
    pub selected: usize,
    pub top: usize,
}

impl Outline {
    // Open on the heading whose section holds `line`: the last one starting at or above it
    pub(super) fn open(starts: &[usize], line: usize, rows: usize) -> Outline {
        let selected = starts
            .partition_point(|start| *start <= line)
            .saturating_sub(1);
        let mut outline = Outline { selected, top: 0 };
        outline.follow(starts.len(), rows);
        outline
    }

    // Move the selection for a scroll action, never past either end
    pub(super) fn apply(&mut self, action: Action, count: usize, rows: usize) {
        let last = count.saturating_sub(1);
        let rows = rows.max(1);
        let half = (rows / 2).max(1);
        self.selected = match action {
            Action::LineDown => self.selected + 1,
            Action::LineUp => self.selected.saturating_sub(1),
            Action::PageDown => self.selected + rows,
            Action::PageUp => self.selected.saturating_sub(rows),
            Action::HalfDown => self.selected + half,
            Action::HalfUp => self.selected.saturating_sub(half),
            Action::Top => 0,
            Action::Bottom => last,
            _ => self.selected,
        }
        .min(last);
        self.follow(count, rows);
    }

    // Keep the selection on screen and the list from scrolling past its end
    pub(super) fn follow(&mut self, count: usize, rows: usize) {
        let rows = rows.max(1);
        self.selected = self.selected.min(count.saturating_sub(1));
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + rows {
            self.top = self.selected + 1 - rows;
        }
        self.top = self.top.min(count.saturating_sub(rows));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STARTS: [usize; 6] = [0, 10, 20, 30, 40, 50];

    #[test]
    fn opens_on_the_section_being_read() {
        assert_eq!(Outline::open(&STARTS, 0, 3).selected, 0);
        assert_eq!(Outline::open(&STARTS, 25, 3).selected, 2);
        assert_eq!(Outline::open(&STARTS, 30, 3).selected, 3);
        assert_eq!(Outline::open(&[5, 9], 2, 3).selected, 0);
        assert_eq!(
            Outline::open(&STARTS, 99, 3),
            Outline {
                selected: 5,
                top: 3
            }
        );
    }

    #[test]
    fn moves_clamp_and_scroll_the_list() {
        let mut outline = Outline::open(&STARTS, 0, 3);
        outline.apply(Action::LineUp, 6, 3);
        assert_eq!(
            outline,
            Outline {
                selected: 0,
                top: 0
            }
        );
        outline.apply(Action::PageDown, 6, 3);
        assert_eq!(
            outline,
            Outline {
                selected: 3,
                top: 1
            }
        );
        outline.apply(Action::Bottom, 6, 3);
        assert_eq!(
            outline,
            Outline {
                selected: 5,
                top: 3
            }
        );
        outline.apply(Action::LineDown, 6, 3);
        assert_eq!(outline.selected, 5);
        outline.apply(Action::HalfUp, 6, 3);
        assert_eq!(
            outline,
            Outline {
                selected: 4,
                top: 3
            }
        );
        outline.apply(Action::Top, 6, 3);
        assert_eq!(
            outline,
            Outline {
                selected: 0,
                top: 0
            }
        );
    }

    #[test]
    fn a_taller_window_shows_the_whole_list() {
        let mut outline = Outline::open(&STARTS, 99, 3);
        outline.follow(6, 40);
        assert_eq!(
            outline,
            Outline {
                selected: 5,
                top: 0
            }
        );
    }
}
