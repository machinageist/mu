// Author: machinageist
// Date: 2026-11-09
// Description: Built-in full-screen pager for rendered Markdown
// Notes: Keys come from /dev/tty so piped documents still page. Wrapping depends on width, so a
// resize re-renders the document instead of reflowing old lines. Every rendered line closes its
// own style, which is what lets any window of lines be drawn on its own

mod keys;
mod outline;
mod search;

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, Ordering};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use mu::{Heading, Rendered, outline_entries};

use crate::term::{self, Size};

use keys::{Action, CTRL_C, CTRL_U, Key, normal_action, parse_keys};
use outline::Outline;
use search::{Direction, Hit, Search, highlight};

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

// Terminal settings to put back if a panic ends the process mid-session
static ACTIVE_MODE: Mutex<Option<(RawFd, libc::termios)>> = Mutex::new(None);

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
        Document { text, starts }
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

// Everything the pager shows and remembers between keys, apart from the terminal itself
struct Pager {
    title: String,
    document: Document,
    headings: Vec<Heading>,
    view: View,
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
        Pager {
            title: title.chars().filter(|ch| !ch.is_control()).collect(),
            document,
            headings: rendered.headings,
            view,
            outline: None,
            prompt: None,
            search: None,
            last_query: None,
            note: None,
            wrapped: false,
        }
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
        match normal_action(key) {
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
            Some(Action::ClearSearch) => self.search = None,
            Some(action) => self.view.apply(action),
            None => {}
        }
        false
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
    }

    // Take a re-rendered document, or just a new height, keeping the place and the search
    fn relayout(&mut self, rendered: Option<Rendered>, rows: usize) {
        if let Some(rendered) = rendered {
            self.document = Document::new(rendered.text);
            self.headings = rendered.headings;
            if let Some(search) = &mut self.search {
                let document = &self.document;
                search.rescan((0..document.len()).filter_map(|index| document.line(index)));
            }
        }
        self.view.resize(self.document.len(), rows);
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
                match &self.search {
                    Some(search) => frame.push_str(&highlight(line, &search.on_line(index))),
                    None => frame.push_str(line),
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
        let place = format!("{first}-{last}/{} {percent}%", view.total);
        let mut right = format!("{found}{place}  {STATUS_HINT} ");
        if right.width() + 2 > columns {
            right = format!("{found}{place} ");
        }
        let right = fit_left(&right, columns.saturating_sub(2));
        let left = self.note.as_deref().unwrap_or(&self.title);
        let left = fit_left(left, columns.saturating_sub(right.width() + 2));
        bar(&left, &right, columns)
    }
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
            *active = Some((fd, original));
        }
        install_panic_hook();
        let mut session = Session { tty, original };
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
        let mut pager = Pager::new(title, rendered, body_rows(size));
        let mut frame = String::new();
        let mut keys = [0; KEY_BUFFER_SIZE];
        let mut clear = false;
        loop {
            pager.draw(&mut frame, size.columns, clear);
            self.tty.write_all(frame.as_bytes())?;
            clear = false;

            let (input_ready, resized) =
                wait_for_input(self.tty.as_raw_fd(), resize.read.as_raw_fd())?;
            if resized {
                resize.drain();
                size = self.size();
                pager.relayout(relayout(size.columns), body_rows(size));
                clear = true;
            }
            if input_ready {
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
                for key in parse_keys(&keys[..count]) {
                    if pager.handle(key) {
                        return Ok(());
                    }
                }
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
        restore_terminal(self.tty.as_raw_fd(), &self.original);
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
fn wait_for_input(tty: RawFd, wake: RawFd) -> io::Result<(bool, bool)> {
    let mut fds = [
        libc::pollfd {
            fd: tty,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: wake,
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // SAFETY: fds is a live array of two pollfd structs for the whole call
        let status = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if status >= 0 {
            return Ok((fds[0].revents != 0, fds[1].revents & libc::POLLIN != 0));
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
fn restore_terminal(fd: RawFd, mode: &libc::termios) {
    // SAFETY: the buffer is a static string valid for its full length
    unsafe {
        libc::write(fd, SCREEN_LEAVE.as_ptr().cast(), SCREEN_LEAVE.len());
    }
    let _ = set_mode(fd, mode);
}

// Restore the terminal as soon as an uncaught panic is reported, before anything else prints
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut active) = ACTIVE_MODE.try_lock()
            && let Some((fd, mode)) = active.take()
        {
            restore_terminal(fd, &mode);
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
        Pager::new("doc.md", Rendered { text, headings }, rows)
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

    #[test]
    fn long_titles_keep_their_tail() {
        assert_eq!(fit_left("a/b/c/file.md", 8), "…file.md");
        assert_eq!(fit_left("short", 8), "short");
    }
}
