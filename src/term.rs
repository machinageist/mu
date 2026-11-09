// Author: machinageist
// Date: 2026-11-09
// Description: Terminal size lookup through the TIOCGWINSZ ioctl
// Notes: Probes stdout, then stderr, then stdin so `mdread x.md | less -R` still sees the real width

// File descriptors to ask, in order of preference
const PROBE_FDS: [libc::c_int; 3] = [libc::STDOUT_FILENO, libc::STDERR_FILENO, libc::STDIN_FILENO];

// Find the column count of whichever standard stream is a terminal
pub fn columns() -> Option<usize> {
    PROBE_FDS.into_iter().find_map(fd_columns)
}

// Ask one file descriptor for its window size
fn fd_columns(fd: libc::c_int) -> Option<usize> {
    let mut size = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCGWINSZ writes one winsize into the pointer, which points at a live local of that type;
    // on a non-terminal fd the call fails with ENOTTY and writes nothing
    let status = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) };
    (status == 0 && size.ws_col > 0).then_some(usize::from(size.ws_col))
}
