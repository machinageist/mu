// Author: machinageist
// Date: 2026-11-09
// Description: Drive the real pager in a pseudo-terminal: resize, search, outline and quit
// Notes: Unix only. The child gets the pty as its controlling terminal so /dev/tty and SIGWINCH
// behave as in a real terminal; the resize test guards the macOS poll()-on-/dev/tty regression

#![cfg(unix)]

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_mu");
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/kitchen-sink.md"
);
const FRAME_START: &str = "\x1b[?2026h";
const WAIT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

// A child mu attached to a pseudo-terminal, killed on drop
struct Pty {
    master: File,
    child: Child,
    output: Vec<u8>,
}

impl Pty {
    // Start mu paging the fixture in a pty of the given size
    fn spawn(rows: u16, columns: u16) -> Pty {
        let (mut master, mut slave) = (0, 0);
        // SAFETY: openpty writes two new descriptors; name, termios and winsize may be null
        let status = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, 0, "openpty failed");
        // SAFETY: both descriptors are fresh from openpty and owned by nothing else
        let (master, slave) = unsafe { (File::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        set_size(&master, rows, columns);
        // SAFETY: fcntl on a descriptor we own
        unsafe {
            libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK);
        }
        let stdio = |fd: &OwnedFd| Stdio::from(fd.try_clone().expect("clone slave"));
        let mut command = Command::new(BIN);
        command
            .args(["--paging", "always", FIXTURE])
            .env("TERM", "xterm-256color")
            .env_remove("NO_COLOR")
            .stdin(stdio(&slave))
            .stdout(stdio(&slave))
            .stderr(stdio(&slave));
        // SAFETY: setsid and ioctl are async-signal-safe, as pre_exec requires
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                libc::ioctl(0, libc::TIOCSCTTY as _, 0);
                Ok(())
            });
        }
        let child = command.spawn().expect("spawn mu");
        drop(slave);
        Pty {
            master,
            child,
            output: Vec::new(),
        }
    }

    // Type bytes at the pager
    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("write to pty");
    }

    // Read until the latest frame contains `needle`, failing after a few seconds
    fn wait_for(&mut self, needle: &str) {
        let deadline = Instant::now() + WAIT;
        loop {
            let mut chunk = [0; 65536];
            match self.master.read(&mut chunk) {
                Ok(count) if count > 0 => self.output.extend_from_slice(&chunk[..count]),
                _ => std::thread::sleep(POLL_INTERVAL),
            }
            if self.latest_frame().contains(needle) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {needle:?}; last frame: {:?}",
                self.latest_frame()
            );
        }
    }

    // The text drawn since the last frame began
    fn latest_frame(&self) -> String {
        let text = String::from_utf8_lossy(&self.output);
        text.rsplit(FRAME_START)
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    // Wait for the child to exit and return its code
    fn exit_code(&mut self) -> Option<i32> {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.child.try_wait() {
                return status.code();
            }
            let _ = self.master.read(&mut [0; 4096]);
            std::thread::sleep(POLL_INTERVAL);
        }
        None
    }
}

impl Drop for Pty {
    // Never leave a pager running after a failed test
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// Set the pty's window size; the kernel then sends SIGWINCH to the pager
fn set_size(master: &File, rows: u16, columns: u16) {
    let size = libc::winsize {
        ws_row: rows,
        ws_col: columns,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCSWINSZ reads a winsize that lives for the call
    let status = unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ as _, &size) };
    assert_eq!(status, 0, "TIOCSWINSZ failed");
}

#[test]
fn a_resize_re_renders_without_waiting_for_a_key() {
    let mut pty = Pty::spawn(24, 80);
    pty.wait_for("1-23/81");
    set_size(&pty.master, 30, 60);
    pty.wait_for("1-29/83");
}

#[test]
fn search_outline_and_quit_in_a_real_terminal() {
    let mut pty = Pty::spawn(24, 80);
    pty.wait_for("o outline");
    pty.send(b"/table\r");
    pty.wait_for("/table 1/1");
    assert!(
        pty.latest_frame().contains("\x1b[1;7m"),
        "current match not highlighted"
    );
    pty.send(b"/zzzz\r");
    pty.wait_for("not found: zzzz");
    pty.send(b"o");
    pty.wait_for("Enter jump");
    pty.send(b"G\r");
    pty.wait_for("100%");
    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
}
