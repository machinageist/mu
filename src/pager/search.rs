// Author: machinageist
// Date: 2026-11-09
// Description: Pager search: smart-case literal matches over the text as shown, and their highlighting
// Notes: Lines are matched with SGR and OSC 8 removed, by char index, so hidden target bytes
// never match. Highlighting only inserts SGR; hyperlink targets and visible text stay intact

#[cfg(test)]
const ESC: char = '\x1b';
const SGR_RESET: &str = "\x1b[0m";
const SGR_RESET_PREFIX: &str = "0;";
const MATCH_ON: &str = "\x1b[7m";
const CURRENT_ON: &str = "\x1b[1;7m";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Forward,
    Backward,
}

impl Direction {
    // The other way
    pub(super) fn reverse(self) -> Direction {
        match self {
            Direction::Forward => Direction::Backward,
            Direction::Backward => Direction::Forward,
        }
    }
}

// One match: its line and its char range in the line with styling removed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Match {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

// Where a jump landed, and whether it went round the end of the document
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Hit {
    pub line: usize,
    pub wrapped: bool,
}

// A query, the direction it was typed in, and every match in the document
#[derive(Debug)]
pub(super) struct Search {
    pub query: String,
    pub direction: Direction,
    matches: Vec<Match>,
    current: Option<usize>,
}

impl Search {
    // Find every match of `query` in `lines`, which may carry SGR styling
    pub(super) fn new<'a>(
        query: &str,
        direction: Direction,
        lines: impl IntoIterator<Item = &'a str>,
    ) -> Search {
        let mut search = Search {
            query: query.to_owned(),
            direction,
            matches: Vec::new(),
            current: None,
        };
        search.rescan(lines);
        search
    }

    // Match again over new lines, as after a re-layout; the current match is forgotten
    pub(super) fn rescan<'a>(&mut self, lines: impl IntoIterator<Item = &'a str>) {
        let ignore_case = !self.query.chars().any(char::is_uppercase);
        let needle: Vec<char> = self.query.chars().map(|ch| fold(ch, ignore_case)).collect();
        self.matches.clear();
        self.current = None;
        if needle.is_empty() {
            return;
        }
        for (line, text) in lines.into_iter().enumerate() {
            let hay: Vec<char> = strip_sgr(text)
                .chars()
                .map(|ch| fold(ch, ignore_case))
                .collect();
            let mut at = 0;
            while at + needle.len() <= hay.len() {
                if hay[at..at + needle.len()] == needle[..] {
                    self.matches.push(Match {
                        line,
                        start: at,
                        end: at + needle.len(),
                    });
                    at += needle.len();
                } else {
                    at += 1;
                }
            }
        }
    }

    // Count all matches
    pub(super) fn count(&self) -> usize {
        self.matches.len()
    }

    // The current match, for scrolling it into view sideways
    pub(super) fn current_match(&self) -> Option<Match> {
        self.matches.get(self.current?).copied()
    }

    // 1-based position of the current match, for the status bar
    pub(super) fn position(&self) -> Option<usize> {
        self.current.map(|index| index + 1)
    }

    // Jump to the first match from `top` in `direction`: at or below it going forward, above it going back
    pub(super) fn first_from(&mut self, top: usize, direction: Direction) -> Option<Hit> {
        let below = self.matches.partition_point(|found| found.line < top);
        let (index, wrapped) = match direction {
            Direction::Forward if below < self.matches.len() => (below, false),
            Direction::Forward => (0, true),
            Direction::Backward if below > 0 => (below - 1, false),
            Direction::Backward => (self.matches.len().checked_sub(1)?, true),
        };
        self.select(index, wrapped)
    }

    // Step to the next match in `direction` from the current one, going round the ends
    pub(super) fn step(&mut self, direction: Direction, top: usize) -> Option<Hit> {
        let Some(current) = self.current else {
            return self.first_from(top, direction);
        };
        let last = self.matches.len().checked_sub(1)?;
        let (index, wrapped) = match direction {
            Direction::Forward if current < last => (current + 1, false),
            Direction::Forward => (0, true),
            Direction::Backward if current > 0 => (current - 1, false),
            Direction::Backward => (last, true),
        };
        self.select(index, wrapped)
    }

    // Make a match current and report where it is
    fn select(&mut self, index: usize, wrapped: bool) -> Option<Hit> {
        let found = self.matches.get(index)?;
        self.current = Some(index);
        Some(Hit {
            line: found.line,
            wrapped,
        })
    }

    // Matches on one line, each with whether it is the current one
    pub(super) fn on_line(&self, line: usize) -> Vec<(Match, bool)> {
        let first = self.matches.partition_point(|found| found.line < line);
        self.matches[first..]
            .iter()
            .take_while(|found| found.line == line)
            .enumerate()
            .map(|(offset, found)| (*found, self.current == Some(first + offset)))
            .collect()
    }
}

// Fold a char for comparison: its first lowercase char when ignoring case, so indexes stay 1:1
fn fold(ch: char, ignore_case: bool) -> char {
    if ignore_case {
        ch.to_lowercase().next().unwrap_or(ch)
    } else {
        ch
    }
}

// SGR and generated OSC 8 controls are indivisible, zero-width tokens. Keep this scanner
// shared by search, highlighting and horizontal clipping so URI bytes never become text.
pub(super) enum Token<'a> {
    Sgr(&'a str),
    Osc8(&'a str),
    Char(char),
}

pub(super) fn tokens(text: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        if let Some(csi) = rest.strip_prefix("\x1b[")
            && let Some(end) = csi.bytes().position(|byte| (0x40..=0x7e).contains(&byte))
            && csi.as_bytes()[end] == b'm'
        {
            let (sequence, tail) = rest.split_at(end + 3);
            rest = tail;
            return Some(Token::Sgr(sequence));
        }
        if let Some(osc) = rest.strip_prefix("\x1b]8;")
            && let Some(end) = osc.find("\x1b\\")
            && osc[..end].contains(';')
        {
            let (sequence, tail) = rest.split_at(end + 6);
            rest = tail;
            return Some(Token::Osc8(sequence));
        }
        let ch = rest.chars().next()?;
        rest = &rest[ch.len_utf8()..];
        Some(Token::Char(ch))
    })
}

// Historical name: remove both SGR and OSC 8, leaving only the visible text.
pub(super) fn strip_sgr(text: &str) -> String {
    tokens(text)
        .filter_map(|token| match token {
            Token::Char(ch) => Some(ch),
            Token::Sgr(_) | Token::Osc8(_) => None,
        })
        .collect()
}

// Re-draw a styled line with its matches in reverse video, the current one also bold
// Notes: the line's own SGR state is replayed after each match, and restated inside one when the
// line changes style mid-match, so colors outside matches are exactly as before
pub(super) fn highlight(line: &str, matches: &[(Match, bool)]) -> String {
    if matches.is_empty() {
        return line.to_owned();
    }
    let mut out = String::with_capacity(line.len() + matches.len() * 16);
    // SGR sequences in force since the line's last reset
    let mut state = String::new();
    let mut index = 0;
    let mut active: Option<(Match, bool)> = None;
    let mut pending = matches.iter().copied().peekable();
    for token in tokens(line) {
        let ch = match token {
            Token::Osc8(sequence) => {
                // SGR resets do not affect hyperlink state. Never replay or style URI bytes.
                out.push_str(sequence);
                continue;
            }
            Token::Sgr(sequence) => {
                out.push_str(sequence);
                track_state(&mut state, sequence);
                if let Some((_, current)) = active {
                    out.push_str(match_style(current));
                }
                continue;
            }
            Token::Char(ch) => ch,
        };
        if let Some((found, _)) = active
            && index == found.end
        {
            out.push_str(SGR_RESET);
            out.push_str(&state);
            active = None;
        }
        if active.is_none()
            && let Some(&(found, current)) = pending.peek()
            && index == found.start
        {
            pending.next();
            out.push_str(match_style(current));
            active = Some((found, current));
        }
        out.push(ch);
        index += 1;
    }
    if active.is_some() {
        out.push_str(SGR_RESET);
        out.push_str(&state);
    }
    out
}

// Pick the highlight escape for a match
fn match_style(current: bool) -> &'static str {
    if current { CURRENT_ON } else { MATCH_ON }
}

// Follow the line's SGR state: a reset clears it, anything else adds to it
fn track_state(state: &mut String, sequence: &str) {
    let params = &sequence[2..sequence.len() - 1];
    if params.is_empty() || params == "0" {
        state.clear();
    } else if params.starts_with(SGR_RESET_PREFIX) {
        *state = sequence.to_owned();
    } else {
        state.push_str(sequence);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: [&str; 5] = [
        "Alpha beta",
        "\x1b[1;34mBeta\x1b[0m gamma",
        "nothing here",
        "beta beta",
        "BETA",
    ];

    // Search the sample lines
    fn search(query: &str, direction: Direction) -> Search {
        Search::new(query, direction, LINES)
    }

    #[test]
    fn lowercase_queries_ignore_case_and_capitals_do_not() {
        assert_eq!(search("beta", Direction::Forward).count(), 5);
        assert_eq!(search("Beta", Direction::Forward).count(), 1);
        assert_eq!(search("BETA", Direction::Forward).count(), 1);
        assert_eq!(search("zeta", Direction::Forward).count(), 0);
    }

    #[test]
    fn styling_never_splits_a_match() {
        let found = search("beta gamma", Direction::Forward);
        assert_eq!(found.count(), 1);
        assert_eq!(
            found.on_line(1)[0].0,
            Match {
                line: 1,
                start: 0,
                end: 10
            }
        );
    }

    #[test]
    fn forward_starts_at_the_top_line_and_wraps() {
        let mut found = search("beta", Direction::Forward);
        assert_eq!(
            found.first_from(2, Direction::Forward),
            Some(Hit {
                line: 3,
                wrapped: false
            })
        );
        assert_eq!(found.position(), Some(3));
        assert_eq!(
            found.step(Direction::Forward, 2).map(|hit| hit.line),
            Some(3)
        );
        assert_eq!(
            found.step(Direction::Forward, 2).map(|hit| hit.line),
            Some(4)
        );
        assert_eq!(
            found.step(Direction::Forward, 2),
            Some(Hit {
                line: 0,
                wrapped: true
            })
        );
    }

    #[test]
    fn backward_starts_above_the_top_line_and_wraps() {
        let mut found = search("beta", Direction::Backward);
        assert_eq!(
            found.first_from(1, Direction::Backward),
            Some(Hit {
                line: 0,
                wrapped: false
            })
        );
        assert_eq!(
            found.step(Direction::Backward, 1),
            Some(Hit {
                line: 4,
                wrapped: true
            })
        );
        // N after ? goes the other way
        assert_eq!(
            found.step(Direction::Backward.reverse(), 1),
            Some(Hit {
                line: 0,
                wrapped: true
            })
        );
    }

    #[test]
    fn no_match_means_no_jump() {
        let mut found = search("zeta", Direction::Forward);
        assert_eq!(found.first_from(0, Direction::Forward), None);
        assert_eq!(found.step(Direction::Backward, 0), None);
        assert_eq!(found.position(), None);
    }

    #[test]
    fn rescan_follows_a_new_layout() {
        let mut found = search("beta", Direction::Forward);
        found.first_from(0, Direction::Forward);
        found.rescan(["no", "Beta here"]);
        assert_eq!(found.count(), 1);
        assert_eq!(found.position(), None);
    }

    #[test]
    fn highlighting_only_adds_styling() {
        let lines = [
            "plain beta text",
            "\x1b[1;34mBeta\x1b[0m gamma",
            "\x1b[33mhalf\x1b[0m way beta",
            "全角beta全角",
            "beta",
        ];
        let found = Search::new("beta", Direction::Forward, lines);
        for (number, line) in lines.iter().enumerate() {
            let shown = highlight(line, &found.on_line(number));
            assert_eq!(strip_sgr(&shown), strip_sgr(line), "line {number}");
            assert!(shown.contains(MATCH_ON), "line {number}: {shown:?}");
            let tail = shown.rsplit(ESC).next().unwrap_or_default();
            assert!(
                !shown.contains(ESC) || tail.starts_with("[0m") || shown.ends_with(SGR_RESET),
                "line {number} does not end reset: {shown:?}"
            );
        }
    }

    #[test]
    fn style_inside_a_match_keeps_the_highlight_and_restores_after() {
        let line = "a\x1b[33mbc\x1b[0md e";
        let found = Search::new("abcd", Direction::Forward, [line]);
        let mut found = found;
        found.first_from(0, Direction::Forward);
        assert_eq!(
            highlight(line, &found.on_line(0)),
            "\x1b[1;7ma\x1b[33m\x1b[1;7mbc\x1b[0m\x1b[1;7md\x1b[0m e"
        );
    }

    #[test]
    fn colors_outside_a_match_are_replayed() {
        let line = "\x1b[34mone two\x1b[0m";
        let found = Search::new("one", Direction::Forward, [line]);
        assert_eq!(
            highlight(line, &found.on_line(0)),
            "\x1b[34m\x1b[7mone\x1b[0m\x1b[34m two\x1b[0m"
        );
    }
}
