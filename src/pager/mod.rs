// Author: machinageist
// Date: 2026-11-09
// Description: Built-in full-screen pager for rendered Markdown
// Notes: Keys come from /dev/tty so piped documents still page. Wrapping depends on width, so a
// resize re-renders the document instead of reflowing old lines. Every rendered line closes its
// own style, which is what lets any window of lines be drawn on its own

mod keys;
mod outline;
mod search;

use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use mu::{Anchor, Heading, NavigationLink, Rendered, outline_entries};

use crate::term::{self, Size};

#[cfg(test)]
use keys::parse_keys;
use keys::{Action, CTRL_C, CTRL_U, Decoder, Key, normal_action};
use outline::Outline;
use search::{Direction, Hit, Search, Token, highlight, strip_sgr, tokens};

const TTY_PATH: &str = "/dev/tty";

// Screen setup: alternate screen, hidden cursor, no autowrap, mouse wheel sent as arrow keys
const SCREEN_ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[?1007h";
const SCREEN_LEAVE: &str = "\x1b[?1007l\x1b[?7h\x1b[?25h\x1b[?1049l";

// Frame drawing
const SYNC_BEGIN: &str = "\x1b[?2026h";
const SYNC_END: &str = "\x1b[?2026l";
const CURSOR_HOME: &str = "\x1b[H";
const CLEAR_SCREEN: &str = "\x1b[2J";
const CLEAR_TO_EOL: &str = "\x1b[K";
const ROW_END: &str = "\r\n";
const STATUS_STYLE: &str = "\x1b[0;7m";
const SGR_RESET: &str = "\x1b[0m";
const STATUS_HINT: &str = "/ search  o outline  q quit";
const OUTLINE_TITLE: &str = "outline";
const OUTLINE_HINT: &str = "Enter jump  o close";
const NO_HEADINGS: &str = "no headings";
const SELECTED_STYLE: &str = "\x1b[7m";
const COLUMN_PREFIX: &str = "col ";
// A search jump that scrolls sideways leaves the match this far into the window
const MATCH_LEAD_DIVISOR: usize = 4;
const OSC8_CLOSE: &str = "\x1b]8;;\x1b\\";
// History is session-local and bounded even when cycling around a book.
const MAX_LINK_HISTORY: usize = 128;
const PROMPT_CURSOR: char = '█';
const NOT_FOUND_PREFIX: &str = "not found: ";
const WRAPPED_NOTE: &str = " wrapped";
const ELLIPSIS: char = '…';

// Used when the terminal will not report its size
const FALLBACK_SIZE: Size = Size {
    columns: 80,
    rows: 24,
    pixels: None,
};

// Bytes read from the terminal at once
const KEY_BUFFER_SIZE: usize = 64;
// Allow split terminal key sequences while keeping a lone Escape responsive
const KEY_SEQUENCE_WAIT: Duration = Duration::from_millis(100);

// Terminal settings to put back if a panic ends the process mid-session
static ACTIVE_MODE: Mutex<Option<(RawFd, libc::termios, bool)>> = Mutex::new(None);

// Write end of the resize pipe, read by the signal handler
static RESIZE_WAKE_FD: AtomicI32 = AtomicI32::new(-1);

// Scroll position over a document of `total` lines shown `rows` at a time
#[derive(Debug, PartialEq, Eq)]
struct View {
    top: usize,
    total: usize,
    rows: usize,
}

impl View {
    // Furthest the top line can scroll
    fn max_top(&self) -> usize {
        self.total.saturating_sub(self.rows)
    }

    // Move the window for one scroll action, never past either end
    fn apply(&mut self, action: Action) {
        let half = (self.rows / 2).max(1);
        let target = match action {
            Action::LineDown => self.top + 1,
            Action::LineUp => self.top.saturating_sub(1),
            Action::PageDown => self.top + self.rows,
            Action::PageUp => self.top.saturating_sub(self.rows),
            Action::HalfDown => self.top + half,
            Action::HalfUp => self.top.saturating_sub(half),
            Action::Top => 0,
            Action::Bottom => self.max_top(),
            _ => self.top,
        };
        self.top = target.min(self.max_top());
    }

    // Bring a line into view, putting it at the top unless it is already on screen
    fn show(&mut self, line: usize) {
        if line < self.top || line >= self.top + self.rows {
            self.top = line.min(self.max_top());
        }
    }

    // Adopt a new document length or window height, keeping the reader's place proportionally
    fn resize(&mut self, total: usize, rows: usize) {
        if let Some(top) = (self.top * total).checked_div(self.total) {
            self.top = top;
        }
        self.total = total;
        self.rows = rows.max(1);
        self.top = self.top.min(self.max_top());
    }
}

// Rendered text plus where each line starts
struct Document {
    text: String,
    starts: Vec<usize>,
    // Display width of the widest line, for the sideways scroll limit
    widest: usize,
}

impl Document {
    // Index the lines of rendered output
    fn new(text: String) -> Document {
        let mut starts = Vec::new();
        if !text.is_empty() {
            starts.push(0);
        }
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' && index + 1 < text.len() {
                starts.push(index + 1);
            }
        }
        let mut document = Document {
            text,
            starts,
            widest: 0,
        };
        document.widest = (0..document.len())
            .filter_map(|index| document.line(index))
            .map(|line| strip_sgr(line).width())
            .max()
            .unwrap_or(0);
        document
    }

    // Count the lines
    fn len(&self) -> usize {
        self.starts.len()
    }

    // Fetch one line without its newline
    fn line(&self, index: usize) -> Option<&str> {
        let start = *self.starts.get(index)?;
        let end = self
            .starts
            .get(index + 1)
            .map_or(self.text.len(), |next| next - 1);
        let line = &self.text[start..end];
        Some(line.strip_suffix('\n').unwrap_or(line))
    }
}

// A return position and the source link that locates it again after reflow.
struct Visit {
    top: usize,
    left: usize,
    source: usize,
}

// Everything the pager shows and remembers between keys, apart from the terminal itself
struct Pager {
    title: String,
    document: Document,
    headings: Vec<Heading>,
    anchors: HashMap<String, (usize, usize)>,
    links: Vec<NavigationLink>,
    selected_link: Option<usize>,
    history: VecDeque<Visit>,
    view: View,
    // Window width, and how many columns are scrolled off to the left
    columns: usize,
    left: usize,
    outline: Option<Outline>,
    // The open search prompt: its direction and what has been typed
    prompt: Option<(Direction, String)>,
    search: Option<Search>,
    last_query: Option<String>,
    // One-key messages: a note replacing the title, and whether the last jump wrapped
    note: Option<String>,
    wrapped: bool,
}

impl Pager {
    // Start at the top of a document shown `rows` lines at a time
    fn new(title: &str, rendered: Rendered, rows: usize) -> Pager {
        let document = Document::new(rendered.text);
        let view = View {
            top: 0,
            total: document.len(),
            rows,
        };
        let mut pager = Pager {
            title: title.chars().filter(|ch| !ch.is_control()).collect(),
            document,
            headings: rendered.headings,
            anchors: HashMap::new(),
            links: Vec::new(),
            selected_link: None,
            history: VecDeque::new(),
            view,
            columns: FALLBACK_SIZE.columns,
            left: 0,
            outline: None,
            prompt: None,
            search: None,
            last_query: None,
            note: None,
            wrapped: false,
        };
        pager.set_navigation(rendered.anchors, rendered.links);
        pager
    }

    // Act on one key; true means quit
    fn handle(&mut self, key: Key) -> bool {
        self.note = None;
        self.wrapped = false;
        if let Some((_, typed)) = &mut self.prompt {
            match key {
                Key::Char(ch) => typed.push(ch),
                Key::Backspace if typed.pop().is_none() => self.prompt = None,
                Key::Ctrl(CTRL_U) => typed.clear(),
                Key::Enter => {
                    if let Some((direction, typed)) = self.prompt.take() {
                        self.run_search(direction, typed);
                    }
                }
                Key::Esc | Key::Ctrl(CTRL_C) => self.prompt = None,
                _ => {}
            }
            return false;
        }
        if let Some(outline) = &mut self.outline {
            match key {
                Key::Enter => {
                    let line = self
                        .headings
                        .get(outline.selected)
                        .map(|heading| heading.line);
                    self.outline = None;
                    if let Some(line) = line {
                        self.view.top = line.min(self.view.max_top());
                        self.left = 0;
                    }
                }
                Key::Char('o' | 'q' | 'Q') | Key::Esc | Key::Ctrl(CTRL_C) => self.outline = None,
                _ => {
                    if let Some(action) = normal_action(key) {
                        outline.apply(action, self.headings.len(), self.view.rows);
                    }
                }
            }
            return false;
        }
        if key == Key::Enter && self.selected_link.is_some() {
            self.follow_link();
            return false;
        }
        let action = normal_action(key);
        if action.is_some() && !matches!(action, Some(Action::NextLink | Action::PrevLink)) {
            self.selected_link = None;
        }
        match action {
            Some(Action::Quit) => return true,
            Some(Action::Outline) if self.headings.is_empty() => {
                self.note = Some(NO_HEADINGS.to_owned());
            }
            Some(Action::Outline) => {
                let starts: Vec<usize> = self.headings.iter().map(|heading| heading.line).collect();
                self.outline = Some(Outline::open(&starts, self.view.top, self.view.rows));
            }
            Some(Action::Search(direction)) => self.prompt = Some((direction, String::new())),
            Some(Action::NextMatch) => self.step(false),
            Some(Action::PrevMatch) => self.step(true),
            Some(Action::NextLink) => self.select_link(false),
            Some(Action::PrevLink) => self.select_link(true),
            Some(Action::Back) => {
                if let Some(visit) = self.history.pop_back() {
                    self.view.top = visit.top.min(self.view.max_top());
                    self.left = visit.left.min(self.max_left());
                }
            }
            Some(Action::ClearSearch) => self.search = None,
            Some(Action::ScrollRight) => {
                let step = (self.columns / 2).max(1);
                self.left = (self.left + step).min(self.max_left());
            }
            Some(Action::ScrollLeft) => {
                self.left = self.left.saturating_sub((self.columns / 2).max(1));
            }
            Some(action) => self.view.apply(action),
            None => {}
        }
        false
    }

    // Only resolvable internal links enter the selection cycle. Reflow uses fresh metadata;
    // return positions follow their source link, or are dropped if that identity changed.
    fn set_navigation(&mut self, anchors: Vec<Anchor>, mut links: Vec<NavigationLink>) {
        self.anchors.clear();
        for anchor in anchors {
            if anchor.line < self.document.len() && !anchor.id.is_empty() {
                self.anchors
                    .entry(anchor.id)
                    .or_insert((anchor.line, anchor.column));
            }
        }
        links.retain(|link| {
            link.line < self.document.len() && self.anchors.contains_key(&link.target)
        });
        links.sort_by_key(|link| link.line);
        for link in &mut links {
            let plain: String = link.label.chars().filter(|ch| !ch.is_control()).collect();
            link.label = plain.split_whitespace().collect::<Vec<_>>().join(" ");
            if link.label.is_empty() {
                link.label = "link".to_owned();
            }
        }
        let max_top = self.view.max_top();
        let max_left = self.max_left();
        self.history.retain_mut(|visit| {
            let Some(old) = self.links.get(visit.source) else {
                return false;
            };
            let Some(new) = links.get(visit.source) else {
                return false;
            };
            if old.target != new.target || old.label != new.label {
                return false;
            }
            visit.top = visit
                .top
                .saturating_add(new.line)
                .saturating_sub(old.line)
                .min(max_top);
            visit.left = visit
                .left
                .saturating_add(new.column)
                .saturating_sub(old.column)
                .min(max_left);
            true
        });
        self.links = links;
        self.selected_link = None;
    }

    // Cycle through source links in document order, starting at the current viewport.
    fn select_link(&mut self, reverse: bool) {
        let count = self.links.len();
        if count == 0 {
            return;
        }
        let index = match self.selected_link {
            Some(index) if reverse => (index + count - 1) % count,
            Some(index) => (index + 1) % count,
            None if reverse => self
                .links
                .iter()
                .rposition(|link| link.line < self.view.top)
                .unwrap_or(count - 1),
            None => self
                .links
                .iter()
                .position(|link| link.line >= self.view.top)
                .unwrap_or(0),
        };
        self.selected_link = Some(index);
        self.view.show(self.links[index].line);
        let link = &self.links[index];
        let available = self.document.line(link.line).map_or(0, |line| {
            strip_sgr(line).width().saturating_sub(link.column)
        });
        self.reveal_column(link.column, link.label.width().min(available));
    }

    // Reveal a navigation position without losing selection to a manual scroll action
    fn reveal_column(&mut self, column: usize, span: usize) {
        let span = span.clamp(1, self.columns.max(1));
        let end = column.saturating_add(span);
        if column < self.left || end > self.left.saturating_add(self.columns) {
            self.left = end
                .saturating_sub(self.columns)
                .max(column.saturating_sub(self.columns / MATCH_LEAD_DIVISOR))
                .min(self.max_left());
        }
    }

    fn follow_link(&mut self) {
        let Some(index) = self.selected_link.take() else {
            return;
        };
        let Some((line, column)) = self
            .links
            .get(index)
            .and_then(|link| self.anchors.get(&link.target))
            .copied()
        else {
            return;
        };
        self.history.push_back(Visit {
            top: self.view.top,
            left: self.left,
            source: index,
        });
        if self.history.len() > MAX_LINK_HISTORY {
            self.history.pop_front();
        }
        self.view.top = line.min(self.view.max_top());
        self.reveal_column(column, 1);
    }

    // Search for a query, or repeat the last one when the prompt was left empty
    fn run_search(&mut self, direction: Direction, typed: String) {
        let query = if typed.is_empty() {
            match &self.last_query {
                Some(last) => last.clone(),
                None => return,
            }
        } else {
            typed
        };
        let document = &self.document;
        let lines = (0..document.len()).filter_map(|index| document.line(index));
        let mut search = Search::new(&query, direction, lines);
        self.last_query = Some(query);
        match search.first_from(self.view.top, direction) {
            Some(hit) => {
                self.search = Some(search);
                self.jump(hit);
            }
            None => {
                self.note = Some(format!("{NOT_FOUND_PREFIX}{}", search.query));
                self.search = None;
            }
        }
    }

    // Move to the next match, or the previous one when `reverse`, relative to the typed direction
    fn step(&mut self, reverse: bool) {
        let top = self.view.top;
        let hit = self.search.as_mut().and_then(|search| {
            let direction = if reverse {
                search.direction.reverse()
            } else {
                search.direction
            };
            search.step(direction, top)
        });
        if let Some(hit) = hit {
            self.jump(hit);
        }
    }

    // Scroll to a match and note when the search went round the end
    fn jump(&mut self, hit: Hit) {
        self.wrapped = hit.wrapped;
        self.view.show(hit.line);
        let Some(found) = self.search.as_ref().and_then(Search::current_match) else {
            return;
        };
        let Some(line) = self.document.line(found.line) else {
            return;
        };
        let shown = strip_sgr(line);
        let width_of = |chars: usize| shown.chars().take(chars).collect::<String>().width();
        let (start, end) = (width_of(found.start), width_of(found.end));
        if start < self.left || end > self.left + self.columns {
            self.left = start
                .saturating_sub(self.columns / MATCH_LEAD_DIVISOR)
                .min(self.max_left());
        }
    }

    // Furthest the view can scroll right: the widest line's end at the window's right edge
    fn max_left(&self) -> usize {
        self.document.widest.saturating_sub(self.columns)
    }

    // Take a re-rendered document, or just a new height, keeping the place and the search
    fn relayout(&mut self, rendered: Option<Rendered>, rows: usize) {
        let navigation = rendered.map(|rendered| {
            self.document = Document::new(rendered.text);
            self.headings = rendered.headings;
            if let Some(search) = &mut self.search {
                let document = &self.document;
                search.rescan((0..document.len()).filter_map(|index| document.line(index)));
            }
            (rendered.anchors, rendered.links)
        });
        self.view.resize(self.document.len(), rows);
        if let Some((anchors, links)) = navigation {
            self.set_navigation(anchors, links);
        }
        self.selected_link = None;
        let max_top = self.view.max_top();
        for visit in &mut self.history {
            visit.top = visit.top.min(max_top);
        }
        self.left = self.left.min(self.max_left());
        let top = self.view.top;
        if let Some(search) = &mut self.search {
            search.first_from(top, Direction::Forward);
        }
        if let Some(outline) = &mut self.outline {
            outline.follow(self.headings.len(), self.view.rows);
        }
    }

    // Paint one full frame: the visible lines with any matches, then the status bar on the last row
    fn draw(&self, frame: &mut String, columns: usize, clear: bool) {
        frame.clear();
        frame.push_str(SYNC_BEGIN);
        if clear {
            frame.push_str(CLEAR_SCREEN);
        }
        frame.push_str(CURSOR_HOME);
        if let Some(outline) = &self.outline {
            self.draw_outline(frame, outline, columns);
            frame.push_str(SYNC_END);
            return;
        }
        for row in 0..self.view.rows {
            let index = self.view.top + row;
            if let Some(line) = self.document.line(index) {
                let shown = match &self.search {
                    Some(search) => highlight(line, &search.on_line(index)),
                    None => line.to_owned(),
                };
                frame.push_str(&cut_left(&shown, self.left));
                if shown.contains("\x1b]8;") {
                    frame.push_str(OSC8_CLOSE);
                }
            }
            frame.push_str(CLEAR_TO_EOL);
            frame.push_str(ROW_END);
        }
        frame.push_str(&self.status_line(columns));
        frame.push_str(SYNC_END);
    }

    // Paint the heading list over the document, the selected entry in reverse video
    fn draw_outline(&self, frame: &mut String, outline: &Outline, columns: usize) {
        let entries = outline_entries(&self.headings, columns.saturating_sub(1).max(1));
        for row in 0..self.view.rows {
            let index = outline.top + row;
            if let Some(entry) = entries.get(index) {
                if index == outline.selected {
                    frame.push_str(SELECTED_STYLE);
                    frame.push_str(entry);
                    frame.push_str(SGR_RESET);
                } else {
                    frame.push_str(entry);
                }
            }
            frame.push_str(CLEAR_TO_EOL);
            frame.push_str(ROW_END);
        }
        let place = format!("{}/{}", outline.selected + 1, self.headings.len());
        let right = fit_left(
            &format!("{place}  {OUTLINE_HINT} "),
            columns.saturating_sub(2),
        );
        let left = fit_left(OUTLINE_TITLE, columns.saturating_sub(right.width() + 2));
        frame.push_str(&bar(&left, &right, columns));
    }

    // Build the reverse-video status bar: the prompt, or a title or note with position and hints
    fn status_line(&self, columns: usize) -> String {
        if let Some((direction, typed)) = &self.prompt {
            let prompt = format!("{}{typed}{PROMPT_CURSOR}", direction_mark(*direction));
            let prompt = fit_left(&prompt, columns.saturating_sub(2));
            return bar(&prompt, "", columns);
        }
        let view = &self.view;
        let first = (view.top + 1).min(view.total);
        let last = (view.top + view.rows).min(view.total);
        let percent = (last * 100).checked_div(view.total).unwrap_or(100);
        let found = self.search.as_ref().map_or(String::new(), |search| {
            let position = search
                .position()
                .map_or("-".to_owned(), |position| position.to_string());
            let wrapped = if self.wrapped { WRAPPED_NOTE } else { "" };
            format!(
                "{}{} {position}/{}{wrapped}  ",
                direction_mark(search.direction),
                search.query,
                search.count()
            )
        });
        let column = if self.left > 0 {
            format!("{COLUMN_PREFIX}{}  ", self.left + 1)
        } else {
            String::new()
        };
        let place = format!("{column}{first}-{last}/{} {percent}%", view.total);
        let back = if self.history.is_empty() {
            ""
        } else {
            "  Backspace back"
        };
        if let Some(link) = self.selected_link.and_then(|index| self.links.get(index)) {
            let label = format!("{}  Enter follow", link.label);
            let right = format!("{place}{back} ");
            if label.width() + right.width() + 2 > columns {
                // The semantic label matters more than position/history on narrow screens.
                let right = fit_left("Enter follow ", columns.saturating_sub(2));
                let left = fit_left(&link.label, columns.saturating_sub(right.width() + 2));
                return bar(&left, &right, columns);
            }
            let left = fit_left(&label, columns.saturating_sub(right.width() + 2));
            return bar(&left, &right, columns);
        }
        let links = if self.links.is_empty() {
            ""
        } else {
            "Tab links  "
        };
        let mut right = format!("{found}{place}{back}  {links}{STATUS_HINT} ");
        if right.width() + 2 > columns {
            right = format!("{found}{place}{back} ");
        }
        let right = fit_left(&right, columns.saturating_sub(2));
        let left = self.note.as_deref().unwrap_or(&self.title);
        let left = fit_left(left, columns.saturating_sub(right.width() + 2));
        bar(&left, &right, columns)
    }
}

// Drop the first `left` display columns without splitting SGR or OSC 8 controls.
// A wide character cut in half becomes spaces for its visible half.
fn cut_left(line: &str, left: usize) -> String {
    if left == 0 {
        return line.to_owned();
    }
    let mut out = String::with_capacity(line.len());
    let mut column = 0;
    for token in tokens(line) {
        let ch = match token {
            Token::Sgr(sequence) | Token::Osc8(sequence) => {
                out.push_str(sequence);
                continue;
            }
            Token::Char(ch) => ch,
        };
        let width = ch.width().unwrap_or(0);
        if column >= left {
            out.push(ch);
        } else if column + width > left {
            out.push_str(&" ".repeat(column + width - left));
        }
        column += width;
    }
    out
}

// The key that starts a search in this direction
fn direction_mark(direction: Direction) -> char {
    match direction {
        Direction::Forward => '/',
        Direction::Backward => '?',
    }
}

// Lay out a reverse-video bar exactly `columns` wide, left text after one space, right text flush
fn bar(left: &str, right: &str, columns: usize) -> String {
    let gap = columns.saturating_sub(1 + left.width() + right.width());
    format!("{STATUS_STYLE} {left}{}{right}{SGR_RESET}", " ".repeat(gap))
}

// Raw-mode, alternate-screen session on the controlling terminal, undone on drop
pub struct Session {
    tty: File,
    original: libc::termios,
    hyperlinks: bool,
}

impl Session {
    // Take over the controlling terminal for full-screen paging
    pub fn open() -> io::Result<Session> {
        let tty = OpenOptions::new().read(true).write(true).open(TTY_PATH)?;
        let fd = tty.as_raw_fd();
        let original = get_mode(fd)?;
        let mut raw = original;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);
        raw.c_iflag &= !(libc::IXON | libc::ICRNL);
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        set_mode(fd, &raw)?;
        if let Ok(mut active) = ACTIVE_MODE.lock() {
            *active = Some((fd, original, false));
        }
        install_panic_hook();
        let mut session = Session {
            tty,
            original,
            hyperlinks: false,
        };
        session.tty.write_all(SCREEN_ENTER.as_bytes())?;
        Ok(session)
    }

    // Page through `text` until the reader quits; `relayout` re-renders for a new column count
    pub fn run(
        mut self,
        title: &str,
        rendered: Rendered,
        relayout: &mut dyn FnMut(usize) -> Option<Rendered>,
    ) -> io::Result<()> {
        let resize = ResizeSignal::install()?;
        let mut size = self.size();
        self.track_links(&rendered);
        let mut pager = Pager::new(title, rendered, body_rows(size));
        pager.columns = size.columns;
        let mut frame = String::new();
        let mut keys = [0; KEY_BUFFER_SIZE];
        let mut decoder = Decoder::new();
        let mut input_deadline = None;
        let mut clear = false;
        loop {
            pager.draw(&mut frame, size.columns, clear);
            self.tty.write_all(frame.as_bytes())?;
            clear = false;

            let (input_ready, resized) = wait_for_input(
                self.tty.as_raw_fd(),
                resize.read.as_raw_fd(),
                input_deadline,
            )?;
            if resized {
                resize.drain();
                size = self.size();
                pager.columns = size.columns;
                let rendered = relayout(size.columns);
                if let Some(rendered) = &rendered {
                    self.track_links(rendered);
                }
                pager.relayout(rendered, body_rows(size));
                clear = true;
            }
            let decoded = if input_ready {
                let count = match self.tty.read(&mut keys) {
                    Ok(0) => return Ok(()),
                    Ok(count) => count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let decoded = decoder.feed(&keys[..count]);
                input_deadline = decoder
                    .has_pending()
                    .then(|| Instant::now() + KEY_SEQUENCE_WAIT);
                decoded
            } else if input_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                input_deadline = None;
                decoder.flush()
            } else {
                Vec::new()
            };
            for key in decoded {
                if pager.handle(key) {
                    return Ok(());
                }
            }
        }
    }

    // Cleanup closes OSC 8 only if this session could have emitted it; --links never
    // therefore remains entirely free of OSC 8 controls, including on normal exit.
    fn track_links(&mut self, rendered: &Rendered) {
        if !self.hyperlinks && rendered.text.contains("\x1b]8;") {
            self.hyperlinks = true;
            if let Ok(mut active) = ACTIVE_MODE.lock()
                && let Some((_, _, hyperlinks)) = active.as_mut()
            {
                *hyperlinks = true;
            }
        }
    }

    // Read the window size from the terminal itself
    fn size(&self) -> Size {
        term::fd_size(self.tty.as_raw_fd()).unwrap_or(FALLBACK_SIZE)
    }
}

impl Drop for Session {
    // Leave the alternate screen and restore the original terminal mode
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_MODE.lock() {
            *active = None;
        }
        restore_terminal(self.tty.as_raw_fd(), &self.original, self.hyperlinks);
    }
}

// Self-pipe the SIGWINCH handler writes to, so a resize wakes poll() without a race
struct ResizeSignal {
    read: File,
    _write: File,
    previous: libc::sigaction,
}

impl ResizeSignal {
    // Create the pipe and route SIGWINCH into it
    fn install() -> io::Result<ResizeSignal> {
        let mut fds = [0; 2];
        // SAFETY: pipe() writes two new descriptors into the two-element array
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        for fd in fds {
            // SAFETY: fd is a descriptor pipe() just returned; these flag changes cannot invalidate it
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
        // SAFETY: both descriptors are fresh and owned by nothing else, so each File becomes the sole owner
        let (read, write) = unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) };
        RESIZE_WAKE_FD.store(fds[1], Ordering::SeqCst);

        // SAFETY: an all-zero sigaction is a valid starting value; the handler only does an
        // async-signal-safe write(), and `previous` receives the old disposition for restoring
        let previous = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_resize as extern "C" fn(libc::c_int) as libc::sighandler_t;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            let mut previous: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(libc::SIGWINCH, &action, &mut previous) != 0 {
                return Err(io::Error::last_os_error());
            }
            previous
        };
        Ok(ResizeSignal {
            read,
            _write: write,
            previous,
        })
    }

    // Empty the pipe so the next resize wakes poll() again
    fn drain(&self) {
        let mut sink = [0; KEY_BUFFER_SIZE];
        while matches!((&self.read).read(&mut sink), Ok(count) if count > 0) {}
    }
}

impl Drop for ResizeSignal {
    // Put the previous SIGWINCH handler back before the pipe closes
    fn drop(&mut self) {
        RESIZE_WAKE_FD.store(-1, Ordering::SeqCst);
        // SAFETY: restores the disposition saved by install(); passing null discards the current one
        unsafe {
            libc::sigaction(libc::SIGWINCH, &self.previous, std::ptr::null_mut());
        }
    }
}

// Signal handler: poke the self-pipe
extern "C" fn on_resize(_signal: libc::c_int) {
    let fd = RESIZE_WAKE_FD.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte: u8 = 1;
        // SAFETY: write() is async-signal-safe and the one-byte buffer lives for the call;
        // the pipe is non-blocking, so a full pipe drops the byte instead of hanging
        unsafe {
            libc::write(fd, (&raw const byte).cast(), 1);
        }
    }
}

// Block until a key arrives or the window is resized
// Notes: select(), not poll(): macOS poll() rejects /dev/tty with POLLNVAL straight away, which
// once made a resize wait for the next key because the loop sat in a blocking read
fn wait_for_input(tty: RawFd, wake: RawFd, deadline: Option<Instant>) -> io::Result<(bool, bool)> {
    let highest = tty.max(wake);
    // FD_SETSIZE's integer type differs by platform, so compare both as u64
    if u64::try_from(highest).map_or(true, |fd| fd >= libc::FD_SETSIZE as u64) {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    loop {
        // SAFETY: an all-zero fd_set is a valid empty set, FD_SET only touches descriptors
        // checked above to be under FD_SETSIZE, and the set outlives the select() call
        let (status, ready) = unsafe {
            let mut ready: libc::fd_set = std::mem::zeroed();
            libc::FD_ZERO(&mut ready);
            libc::FD_SET(tty, &mut ready);
            libc::FD_SET(wake, &mut ready);
            // The live timeout is at most 100ms, fitting suseconds_t on supported Unix platforms
            let mut timeout = deadline.map(|deadline| libc::timeval {
                tv_sec: 0,
                tv_usec: deadline
                    .saturating_duration_since(Instant::now())
                    .as_micros()
                    .min(KEY_SEQUENCE_WAIT.as_micros())
                    as libc::suseconds_t,
            });
            let status = libc::select(
                highest + 1,
                &mut ready,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                timeout
                    .as_mut()
                    .map_or(std::ptr::null_mut(), std::ptr::from_mut),
            );
            (status, ready)
        };
        if status >= 0 {
            // SAFETY: `ready` is the set select() just filled, and both descriptors are in range
            return Ok(unsafe { (libc::FD_ISSET(tty, &ready), libc::FD_ISSET(wake, &ready)) });
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

// Read the terminal's current line settings
fn get_mode(fd: RawFd) -> io::Result<libc::termios> {
    let mut mode = MaybeUninit::<libc::termios>::uninit();
    // SAFETY: tcgetattr fills the whole termios on success, and it is only read after that check
    unsafe {
        if libc::tcgetattr(fd, mode.as_mut_ptr()) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(mode.assume_init())
    }
}

// Apply line settings once pending output has drained, keeping typed-ahead keys
fn set_mode(fd: RawFd, mode: &libc::termios) -> io::Result<()> {
    // SAFETY: mode points at a valid termios for the duration of the call
    if unsafe { libc::tcsetattr(fd, libc::TCSADRAIN, mode) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// Leave the alternate screen and put the line settings back
fn restore_terminal(fd: RawFd, mode: &libc::termios, hyperlinks: bool) {
    // SAFETY: the buffer is a static string valid for its full length
    unsafe {
        if hyperlinks {
            libc::write(fd, OSC8_CLOSE.as_ptr().cast(), OSC8_CLOSE.len());
        }
        libc::write(fd, SCREEN_LEAVE.as_ptr().cast(), SCREEN_LEAVE.len());
    }
    let _ = set_mode(fd, mode);
}

// Restore the terminal as soon as an uncaught panic is reported, before anything else prints
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut active) = ACTIVE_MODE.try_lock()
            && let Some((fd, mode, hyperlinks)) = active.take()
        {
            restore_terminal(fd, &mode, hyperlinks);
        }
        previous(info);
    }));
}

// Rows left for content under the status bar
fn body_rows(size: Size) -> usize {
    size.rows.saturating_sub(1).max(1)
}

// Trim text from the left to fit `room` columns, marking the cut with an ellipsis
fn fit_left(text: &str, room: usize) -> String {
    if text.width() <= room {
        return text.to_owned();
    }
    let mut kept = 0;
    let mut start = text.len();
    for (index, ch) in text.char_indices().rev() {
        let width = ch.width().unwrap_or(0);
        if kept + width + 1 > room {
            break;
        }
        kept += width;
        start = index;
    }
    format!("{ELLIPSIS}{}", &text[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a view for scroll tests
    fn view(top: usize, total: usize, rows: usize) -> View {
        View { top, total, rows }
    }

    #[test]
    fn scrolling_clamps_to_both_ends() {
        let mut state = view(0, 100, 20);
        state.apply(Action::LineUp);
        assert_eq!(state.top, 0);
        state.apply(Action::PageDown);
        assert_eq!(state.top, 20);
        state.apply(Action::HalfDown);
        assert_eq!(state.top, 30);
        state.apply(Action::Bottom);
        assert_eq!(state.top, 80);
        state.apply(Action::PageDown);
        assert_eq!(state.top, 80);
        state.apply(Action::Top);
        assert_eq!(state.top, 0);
    }

    #[test]
    fn short_document_never_scrolls() {
        let mut state = view(0, 5, 20);
        state.apply(Action::Bottom);
        state.apply(Action::LineDown);
        assert_eq!(state.top, 0);
    }

    #[test]
    fn resize_keeps_place_proportionally() {
        let mut state = view(50, 200, 20);
        state.resize(100, 20);
        assert_eq!(state, view(25, 100, 20));
        state.resize(100, 90);
        assert_eq!(state.top, 10);
    }

    #[test]
    fn document_splits_lines_without_newlines() {
        let document = Document::new("a\n\nb\n".to_owned());
        assert_eq!(document.len(), 3);
        assert_eq!(document.line(0), Some("a"));
        assert_eq!(document.line(1), Some(""));
        assert_eq!(document.line(2), Some("b"));
        assert_eq!(document.line(3), None);
        assert_eq!(Document::new(String::new()).len(), 0);
    }

    // A pager over `total` numbered lines with "match" on the given lines
    fn pager(total: usize, matching: &[usize], rows: usize) -> Pager {
        let text: String = (0..total)
            .map(|line| {
                if matching.contains(&line) {
                    format!("line {line} has a \x1b[1mMatch\x1b[0m\n")
                } else {
                    format!("line {line}\n")
                }
            })
            .collect();
        Pager::new(
            "doc.md",
            Rendered {
                text,
                headings: Vec::new(),
                ..Rendered::default()
            },
            rows,
        )
    }

    // Feed raw key bytes, returning true if the pager asked to quit
    fn feed(pager: &mut Pager, bytes: &[u8]) -> bool {
        parse_keys(bytes).into_iter().any(|key| pager.handle(key))
    }

    // The status bar without its styling
    fn status(pager: &Pager, columns: usize) -> String {
        pager
            .status_line(columns)
            .trim_start_matches(STATUS_STYLE)
            .trim_end_matches(SGR_RESET)
            .to_owned()
    }

    #[test]
    fn search_jumps_counts_and_wraps() {
        let mut state = pager(100, &[10, 50, 90], 5);
        feed(&mut state, b"/match\r");
        assert_eq!(state.view.top, 10);
        assert!(
            status(&state, 80).contains("/match 1/3"),
            "{}",
            status(&state, 80)
        );
        feed(&mut state, b"n");
        assert_eq!(state.view.top, 50);
        feed(&mut state, b"n");
        assert_eq!(state.view.top, 90);
        feed(&mut state, b"n");
        assert_eq!(state.view.top, 10);
        assert!(status(&state, 80).contains("/match 1/3 wrapped"));
        feed(&mut state, b"j");
        assert!(!status(&state, 80).contains("wrapped"));
    }

    #[test]
    fn backward_search_and_reversed_steps() {
        let mut state = pager(100, &[10, 50, 90], 20);
        state.view.top = 40;
        feed(&mut state, b"?match\r");
        assert_eq!(state.view.top, 10);
        assert!(status(&state, 80).contains("?match 1/3"));
        feed(&mut state, b"N");
        assert_eq!(state.view.top, 50);
        feed(&mut state, b"n");
        assert_eq!(state.view.top, 10);
    }

    #[test]
    fn a_match_already_on_screen_does_not_scroll() {
        let mut state = pager(100, &[5, 50], 20);
        feed(&mut state, b"/match\r");
        assert_eq!(state.view.top, 0);
    }

    #[test]
    fn empty_enter_repeats_the_last_search_in_the_new_direction() {
        let mut state = pager(100, &[10, 50, 90], 20);
        feed(&mut state, b"/match\r");
        feed(&mut state, b"\x1b");
        assert!(state.search.is_none());
        state.view.top = 60;
        feed(&mut state, b"?\r");
        assert_eq!(state.view.top, 50);
        assert_eq!(state.last_query.as_deref(), Some("match"));
    }

    #[test]
    fn not_found_keeps_the_view_until_the_next_key() {
        let mut state = pager(100, &[10], 20);
        state.view.top = 30;
        feed(&mut state, b"/zeta\r");
        assert_eq!(state.view.top, 30);
        assert!(state.search.is_none());
        assert!(status(&state, 80).starts_with(" not found: zeta"));
        feed(&mut state, b"k");
        assert!(status(&state, 80).starts_with(" doc.md"));
    }

    #[test]
    fn prompt_editing_keys() {
        let mut state = pager(10, &[], 5);
        feed(&mut state, "/ab\x7fcé".as_bytes());
        assert_eq!(state.prompt, Some((Direction::Forward, "acé".to_owned())));
        assert!(status(&state, 40).starts_with(" /acé█"));
        feed(&mut state, b"\x15x");
        assert_eq!(state.prompt, Some((Direction::Forward, "x".to_owned())));
        feed(&mut state, b"\x1b");
        assert_eq!(state.prompt, None);
        feed(&mut state, b"?\x7f");
        assert_eq!(state.prompt, None);
        feed(&mut state, b"/q\x03");
        assert_eq!(state.prompt, None);
        assert!(
            !feed(&mut state, b"/q"),
            "q types into the prompt rather than quitting"
        );
        assert!(feed(&mut state, b"\x1bq"));
    }

    #[test]
    fn relayout_keeps_the_search() {
        let mut state = pager(100, &[10, 50, 90], 20);
        feed(&mut state, b"/match\r");
        state.relayout(
            Some(Rendered {
                text: "match\nnone\nmatch\n".to_owned(),
                headings: Vec::new(),
                ..Rendered::default()
            }),
            20,
        );
        let search = state.search.as_ref().expect("search kept");
        assert_eq!(search.count(), 2);
        assert_eq!(search.position(), Some(1));
    }

    #[test]
    fn highlighted_frame_marks_visible_matches() {
        let mut state = pager(30, &[3], 10);
        feed(&mut state, b"/match\r");
        let mut frame = String::new();
        state.draw(&mut frame, 40, false);
        assert!(frame.contains("\x1b[1;7mMatch"), "{frame:?}");
    }

    #[test]
    fn status_line_fills_exactly_the_width_in_every_mode() {
        let mut state = pager(300, &[100], 40);
        for setup in [&b""[..], b"/match\r", b"/zeta\r", b"/typing"] {
            feed(&mut state, b"\x1b\x1b");
            feed(&mut state, setup);
            for columns in [30, 60, 120] {
                let visible = status(&state, columns);
                assert_eq!(visible.width(), columns, "{setup:?}: {visible:?}");
            }
        }
        feed(&mut state, b"\x1b\x1bg");
        assert!(status(&state, 120).contains("1-40/300 13%"));
    }

    // A pager over 100 lines with headings at 0, 30 and 60
    fn outlined(rows: usize) -> Pager {
        let text: String = (0..100).map(|line| format!("line {line}\n")).collect();
        let headings = [(1, "Top", 0), (2, "Middle", 30), (2, "End", 60)]
            .into_iter()
            .map(|(level, text, line)| Heading {
                level,
                text: text.to_owned(),
                line,
            })
            .collect();
        Pager::new(
            "doc.md",
            Rendered {
                text,
                headings,
                ..Rendered::default()
            },
            rows,
        )
    }

    #[test]
    fn outline_opens_on_the_current_section_and_jumps() {
        let mut state = outlined(10);
        state.view.top = 40;
        feed(&mut state, b"o");
        assert_eq!(
            state.outline.as_ref().map(|outline| outline.selected),
            Some(1)
        );
        feed(&mut state, b"j\r");
        assert!(state.outline.is_none());
        assert_eq!(state.view.top, 60);
        feed(&mut state, b"ok\x1b[A\r");
        assert_eq!(state.view.top, 0);
    }

    #[test]
    fn outline_closes_without_moving() {
        let mut state = outlined(10);
        state.view.top = 45;
        for close in [&b"o"[..], b"q", b"\x1b", b"\x03"] {
            feed(&mut state, b"oj");
            assert!(!feed(&mut state, close), "{close:?} must not quit");
            assert!(state.outline.is_none(), "{close:?}");
            assert_eq!(state.view.top, 45);
        }
    }

    #[test]
    fn a_jump_near_the_end_is_clamped() {
        let mut state = outlined(50);
        feed(&mut state, b"oG\r");
        assert_eq!(state.view.top, 50);
    }

    #[test]
    fn no_headings_is_noted_until_the_next_key() {
        let mut state = pager(10, &[], 5);
        feed(&mut state, b"o");
        assert!(state.outline.is_none());
        assert!(status(&state, 40).starts_with(" no headings"));
        feed(&mut state, b"j");
        assert!(status(&state, 40).starts_with(" doc.md"));
    }

    #[test]
    fn outline_frame_lists_headings_and_fills_the_status_bar() {
        let mut state = outlined(10);
        feed(&mut state, b"o");
        for columns in [20, 60] {
            let mut frame = String::new();
            state.draw(&mut frame, columns, false);
            assert!(frame.contains("\x1b[7mTop\x1b[0m"), "{frame:?}");
            assert!(frame.contains("  Middle"));
            let bar = frame.rsplit(ROW_END).next().unwrap_or_default();
            let visible = bar
                .trim_end_matches(SYNC_END)
                .trim_start_matches(STATUS_STYLE)
                .trim_end_matches(SGR_RESET);
            assert_eq!(visible.width(), columns, "{visible:?}");
        }
    }

    #[test]
    fn relayout_replaces_the_headings() {
        let mut state = outlined(10);
        feed(&mut state, b"oG");
        state.relayout(
            Some(Rendered {
                text: "a\nb\n".to_owned(),
                headings: vec![Heading {
                    level: 1,
                    text: "Only".to_owned(),
                    line: 1,
                }],
                ..Rendered::default()
            }),
            10,
        );
        assert_eq!(
            state.outline.as_ref().map(|outline| outline.selected),
            Some(0)
        );
        feed(&mut state, b"\r");
        assert_eq!(state.view.top, 0);
    }

    // A pager 80 columns wide over a short line, a 200-column line, and a match at column 150
    fn wide(rows: usize) -> Pager {
        let long = format!("{}needle{}", "x".repeat(150), "y".repeat(44));
        let text = format!("short\n{long}\nlast\n");
        let mut state = Pager::new(
            "wide.csv",
            Rendered {
                text,
                headings: vec![Heading {
                    level: 1,
                    text: "Top".to_owned(),
                    line: 0,
                }],
                ..Rendered::default()
            },
            rows,
        );
        state.columns = 80;
        state
    }

    #[test]
    fn cutting_keeps_styles_and_splits_wide_characters() {
        assert_eq!(cut_left("\x1b[1mabc\x1b[0mdef", 2), "\x1b[1mc\x1b[0mdef");
        assert_eq!(cut_left("\x1b[1mabc\x1b[0mdef", 0), "\x1b[1mabc\x1b[0mdef");
        assert_eq!(cut_left("全角x", 1), " 角x");
        assert_eq!(cut_left("全角x", 4), "x");
        assert_eq!(cut_left("\x1b[33mab\x1b[0m", 10), "\x1b[33m\x1b[0m");
        let line = "\x1b[34mone \x1b[1mtwo\x1b[0m three";
        for left in 0..16 {
            let expected: String = strip_sgr(line).chars().skip(left).collect();
            assert_eq!(strip_sgr(&cut_left(line, left)), expected, "{left}");
        }
    }

    #[test]
    fn sideways_scrolling_stops_at_the_widest_line() {
        let mut state = wide(10);
        assert_eq!(state.document.widest, 200);
        feed(&mut state, b"l");
        assert_eq!(state.left, 40);
        assert!(
            status(&state, 80).contains("col 41  1-3/3"),
            "{}",
            status(&state, 80)
        );
        feed(&mut state, b"\x1b[C\x1b[Cl");
        assert_eq!(state.left, 120);
        feed(&mut state, b"h");
        assert_eq!(state.left, 80);
        feed(&mut state, b"\x1b[D\x1b[D\x1b[D");
        assert_eq!(state.left, 0);
        assert!(!status(&state, 80).contains("col "));
    }

    #[test]
    fn narrow_documents_do_not_scroll_sideways() {
        let mut state = pager(10, &[], 5);
        state.columns = 80;
        feed(&mut state, b"ll");
        assert_eq!(state.left, 0);
    }

    #[test]
    fn a_search_scrolls_sideways_to_its_match() {
        let mut state = wide(10);
        feed(&mut state, b"/needle\r");
        assert_eq!(state.left, 120);
        let mut frame = String::new();
        state.draw(&mut frame, 80, false);
        assert!(frame.contains("\x1b[1;7mneedle"), "{frame:?}");
        feed(&mut state, b"\x1b");
        assert_eq!(state.left, 120, "Esc keeps the column");
    }

    #[test]
    fn outline_jumps_and_resizes_reset_or_clamp_the_column() {
        let mut state = wide(10);
        feed(&mut state, b"lll");
        feed(&mut state, b"o\r");
        assert_eq!(state.left, 0);
        feed(&mut state, b"lll");
        state.columns = 190;
        state.relayout(None, 10);
        assert_eq!(state.left, 10);
    }

    #[test]
    fn long_titles_keep_their_tail() {
        assert_eq!(fit_left("a/b/c/file.md", 8), "…file.md");
        assert_eq!(fit_left("short", 8), "short");
    }
}
