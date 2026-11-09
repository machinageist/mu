// Author: machinageist
// Date: 2026-11-09
// Description: Terminal size lookup through the TIOCGWINSZ ioctl
// Notes: Probes stdout, then stderr, then stdin so `mu x.md | less -R` still sees the real width

// File descriptors to ask, in order of preference
const PROBE_FDS: [libc::c_int; 3] = [libc::STDOUT_FILENO, libc::STDERR_FILENO, libc::STDIN_FILENO];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub columns: usize,
    pub rows: usize,
    // Window size in pixels, when the terminal reports it
    pub pixels: Option<(usize, usize)>,
}

// Find the size of whichever standard stream is a terminal
pub fn size() -> Option<Size> {
    PROBE_FDS.into_iter().find_map(fd_size)
}

// Ask one file descriptor for its window size
pub fn fd_size(fd: libc::c_int) -> Option<Size> {
    let mut size = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCGWINSZ writes one winsize into the pointer, which points at a live local of that type;
    // on a non-terminal fd the call fails with ENOTTY and writes nothing
    let status = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) };
    (status == 0 && size.ws_col > 0 && size.ws_row > 0).then_some(Size {
        columns: usize::from(size.ws_col),
        rows: usize::from(size.ws_row),
        pixels: (size.ws_xpixel > 0 && size.ws_ypixel > 0)
            .then(|| (usize::from(size.ws_xpixel), usize::from(size.ws_ypixel))),
    })
}
