// Author: machinageist
// Date: 2026-11-09
// Description: Built-in full-screen pager for rendered Markdown
// Notes: Keys come from /dev/tty so piped documents still page. Wrapping depends on width, so a
// resize re-renders the document instead of reflowing old lines. Every rendered line closes its
// own style, which is what lets any window of lines be drawn on its own

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, Ordering};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::term::{self, Size};

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
const STATUS_HINT: &str = "q quit";
const ELLIPSIS: char = '…';

// Used when the terminal will not report its size
const FALLBACK_SIZE: Size = Size {
    columns: 80,
    rows: 24,
};

// Key bytes
const ESC: u8 = 0x1b;
const CTRL_B: u8 = 0x02;
const CTRL_C: u8 = 0x03;
const CTRL_D: u8 = 0x04;
const CTRL_F: u8 = 0x06;
const CTRL_U: u8 = 0x15;
const KEY_BUFFER_SIZE: usize = 64;

// Terminal settings to put back if a panic aborts the process mid-session
static ACTIVE_MODE: Mutex<Option<(RawFd, libc::termios)>> = Mutex::new(None);

// Write end of the resize pipe, read by the signal handler
static RESIZE_WAKE_FD: AtomicI32 = AtomicI32::new(-1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Quit,
    LineDown,
    LineUp,
    PageDown,
    PageUp,
    HalfDown,
    HalfUp,
    Top,
    Bottom,
}

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
            Action::Quit => self.top,
        };
        self.top = target.min(self.max_top());
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
        text: String,
        relayout: &mut dyn FnMut(usize) -> Option<String>,
    ) -> io::Result<()> {
        let resize = ResizeSignal::install()?;
        let title: String = title.chars().filter(|ch| !ch.is_control()).collect();
        let mut document = Document::new(text);
        let mut size = self.size();
        let mut view = View {
            top: 0,
            total: document.len(),
            rows: body_rows(size),
        };
        let mut frame = String::new();
        let mut keys = [0; KEY_BUFFER_SIZE];
        let mut clear = false;
        loop {
            draw_frame(&mut frame, &document, &view, &title, size.columns, clear);
            self.tty.write_all(frame.as_bytes())?;
            clear = false;

            let (input_ready, resized) =
                wait_for_input(self.tty.as_raw_fd(), resize.read.as_raw_fd())?;
            if resized {
                resize.drain();
                size = self.size();
                if let Some(text) = relayout(size.columns) {
                    document = Document::new(text);
                }
                view.resize(document.len(), body_rows(size));
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
                for action in parse_keys(&keys[..count]) {
                    if action == Action::Quit {
                        return Ok(());
                    }
                    view.apply(action);
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

// Restore the terminal before a panic aborts the process, since abort skips Drop
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

// Paint one full frame: the visible lines, then the status bar on the last row
fn draw_frame(
    frame: &mut String,
    document: &Document,
    view: &View,
    title: &str,
    columns: usize,
    clear: bool,
) {
    frame.clear();
    frame.push_str(SYNC_BEGIN);
    if clear {
        frame.push_str(CLEAR_SCREEN);
    }
    frame.push_str(CURSOR_HOME);
    for row in 0..view.rows {
        if let Some(line) = document.line(view.top + row) {
            frame.push_str(line);
        }
        frame.push_str(CLEAR_TO_EOL);
        frame.push_str(ROW_END);
    }
    frame.push_str(&status_line(title, view, columns));
    frame.push_str(SYNC_END);
}

// Build the reverse-video status bar: title on the left, position and hint on the right
fn status_line(title: &str, view: &View, columns: usize) -> String {
    let first = (view.top + 1).min(view.total);
    let last = (view.top + view.rows).min(view.total);
    let percent = (last * 100).checked_div(view.total).unwrap_or(100);
    let right = format!("{first}-{last}/{} {percent}%  {STATUS_HINT} ", view.total);
    let title = fit_left(title, columns.saturating_sub(right.width() + 2));
    let gap = columns.saturating_sub(1 + title.width() + right.width());
    format!(
        "{STATUS_STYLE} {title}{}{right}{SGR_RESET}",
        " ".repeat(gap)
    )
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

// Translate raw key bytes into pager actions, ignoring keys with no binding
fn parse_keys(bytes: &[u8]) -> Vec<Action> {
    let mut actions = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let introducer = bytes.get(index + 1).copied();
        if bytes[index] == ESC && matches!(introducer, Some(b'[' | b'O')) {
            let params_start = index + 2;
            let final_offset = bytes[params_start..]
                .iter()
                .position(|byte| (0x40..=0x7E).contains(byte));
            let Some(offset) = final_offset else {
                break;
            };
            let end = params_start + offset;
            actions.extend(escape_action(&bytes[params_start..end], bytes[end]));
            index = end + 1;
        } else {
            actions.extend(key_action(bytes[index]));
            index += 1;
        }
    }
    actions
}

// Map a CSI or SS3 key sequence to an action
fn escape_action(params: &[u8], final_byte: u8) -> Option<Action> {
    match (final_byte, params) {
        (b'A', _) => Some(Action::LineUp),
        (b'B', _) => Some(Action::LineDown),
        (b'H', _) | (b'~', b"1" | b"7") => Some(Action::Top),
        (b'F', _) | (b'~', b"4" | b"8") => Some(Action::Bottom),
        (b'~', b"5") => Some(Action::PageUp),
        (b'~', b"6") => Some(Action::PageDown),
        _ => None,
    }
}

// Map a single key byte to an action
fn key_action(byte: u8) -> Option<Action> {
    match byte {
        b'q' | b'Q' | CTRL_C => Some(Action::Quit),
        b'j' | b'e' | b'\r' | b'\n' => Some(Action::LineDown),
        b'k' | b'y' => Some(Action::LineUp),
        b' ' | b'f' | CTRL_F => Some(Action::PageDown),
        b'b' | CTRL_B => Some(Action::PageUp),
        b'd' | CTRL_D => Some(Action::HalfDown),
        b'u' | CTRL_U => Some(Action::HalfUp),
        b'g' | b'<' => Some(Action::Top),
        b'G' | b'>' => Some(Action::Bottom),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a view for scroll tests
    fn view(top: usize, total: usize, rows: usize) -> View {
        View { top, total, rows }
    }

    #[test]
    fn letter_and_control_keys_map_to_actions() {
        let actions = parse_keys(b"jk bdugGq\x03\x06\x02");
        use Action::*;
        assert_eq!(
            actions,
            [
                LineDown, LineUp, PageDown, PageUp, HalfDown, HalfUp, Top, Bottom, Quit, Quit,
                PageDown, PageUp
            ]
        );
    }

    #[test]
    fn escape_sequences_map_to_actions() {
        let actions = parse_keys(b"\x1b[A\x1b[B\x1b[5~\x1b[6~\x1b[H\x1b[F\x1b[1~\x1b[4~\x1bOA");
        use Action::*;
        assert_eq!(
            actions,
            [
                LineUp, LineDown, PageUp, PageDown, Top, Bottom, Top, Bottom, LineUp
            ]
        );
    }

    #[test]
    fn wheel_burst_yields_one_action_per_arrow() {
        assert_eq!(parse_keys(b"\x1b[B\x1b[B\x1b[B"), [Action::LineDown; 3]);
    }

    #[test]
    fn unknown_and_incomplete_keys_are_ignored() {
        assert_eq!(parse_keys(b"x\x1b[2~z"), []);
        assert_eq!(parse_keys(b"j\x1b["), [Action::LineDown]);
        assert_eq!(parse_keys(b"\x1b"), []);
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

    #[test]
    fn status_line_fills_exactly_the_width() {
        let state = view(0, 300, 40);
        for columns in [30, 60, 120] {
            let line = status_line("docs/README.md", &state, columns);
            let visible = line
                .trim_start_matches(STATUS_STYLE)
                .trim_end_matches(SGR_RESET);
            assert_eq!(visible.width(), columns, "{visible:?}");
            assert!(visible.contains("1-40/300 13%"), "{visible:?}");
        }
    }

    #[test]
    fn long_titles_keep_their_tail() {
        assert_eq!(fit_left("a/b/c/file.md", 8), "…file.md");
        assert_eq!(fit_left("short", 8), "short");
    }
}
