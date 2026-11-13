// Author: machinageist
// Date: 2026-11-09
// Description: Drive the real pager: search, clipping, resize, OSC 8 and EPUB chapter navigation
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
    after: usize,
}

impl Pty {
    // Start mu paging the fixture in a pty of the given size
    fn spawn(rows: u16, columns: u16) -> Pty {
        Pty::spawn_file(FIXTURE, rows, columns)
    }

    // Start mu paging a file in a pty of the given size
    fn spawn_file(path: &str, rows: u16, columns: u16) -> Pty {
        Pty::spawn_options(path, rows, columns, &[], &[])
    }

    // Explicit policies and a deterministic terminal environment for link regressions
    fn spawn_options(
        path: &str,
        rows: u16,
        columns: u16,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Pty {
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
            .args(["--paging", "always"])
            .args(args)
            .arg(path)
            .env("TERM", "xterm-256color")
            .env_remove("NO_COLOR")
            .env_remove("TERM_PROGRAM")
            .env_remove("TERM_PROGRAM_VERSION")
            .env_remove("KITTY_WINDOW_ID")
            .env_remove("VTE_VERSION")
            .env_remove("WT_SESSION")
            .env_remove("TMUX")
            .env_remove("STY")
            .envs(env.iter().copied())
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
            after: 0,
        }
    }

    // Type bytes at the pager
    fn send(&mut self, bytes: &[u8]) {
        self.after = self.output.len();
        self.master.write_all(bytes).expect("write to pty");
    }

    fn resize(&mut self, rows: u16, columns: u16) {
        self.after = self.output.len();
        set_size(&self.master, rows, columns);
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
            let fresh = self.output[self.after..]
                .windows(FRAME_START.len())
                .any(|part| part == FRAME_START.as_bytes());
            if fresh
                && self.latest_frame().contains("\x1b[?2026l")
                && self.latest_frame().contains(needle)
            {
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
            self.drain();
            if let Ok(Some(status)) = self.child.try_wait() {
                self.drain();
                return status.code();
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        None
    }

    fn drain(&mut self) {
        let mut chunk = [0; 65536];
        while let Ok(count) = self.master.read(&mut chunk) {
            if count == 0 {
                break;
            }
            self.output.extend_from_slice(&chunk[..count]);
        }
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
    pty.resize(30, 60);
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

#[test]
fn a_wide_csv_scrolls_sideways() {
    let header: Vec<String> = (1..=12)
        .map(|column| format!("column_{column:02}"))
        .collect();
    let row: Vec<String> = (1..=12)
        .map(|column| format!("value-{column:02}-abcdef"))
        .collect();
    let csv = format!(
        "{}\n{}\n{}\n",
        header.join(","),
        row.join(","),
        row.join(",")
    );
    let path = format!("{}/wide.csv", env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(&path, csv).expect("write wide csv");

    let mut pty = Pty::spawn_file(&path, 24, 80);
    // Cells wrap at the 10-column minimum and the table overflows the 80-column window
    pty.wait_for("wide.csv 1-8/8");
    pty.send(b"l");
    pty.wait_for("wide.csv col 41  1-8/8");
    pty.send(b"\x1b[D");
    pty.wait_for("wide.csv 1-8/8");
    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
}

// Tests create source fixtures without changing checked-in fixture line counts.
struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new(name: &str, bytes: &[u8]) -> Fixture {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("mu-pager-{}-{serial}-{name}", std::process::id()));
        std::fs::write(&path, bytes).expect("write pager fixture");
        Fixture(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().expect("fixture path")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

const URI: &str = "https://example.org/hidden-destination-token?x=1&y=2";
const OSC_CLOSE: &str = "\x1b]8;;\x1b\\";

// Inspect protocol independently of the pager's scanner; return visible text.
fn visible_and_balanced(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    let mut linked = false;
    while !rest.is_empty() {
        if let Some(osc) = rest.strip_prefix("\x1b]8;;") {
            let end = osc.find("\x1b\\").expect("OSC 8 must use ST");
            let target = &osc[..end];
            assert!(
                target.is_empty() || target == URI,
                "hyperlink target corrupted: {target:?}"
            );
            assert!(
                !linked || target.is_empty(),
                "link reopened without a close"
            );
            linked = !target.is_empty();
            rest = &osc[end + 2..];
        } else if let Some(csi) = rest.strip_prefix("\x1b[") {
            let end = csi
                .bytes()
                .position(|byte| (0x40..=0x7e).contains(&byte))
                .expect("complete CSI");
            let sequence = &csi[..=end];
            if matches!(sequence, "K" | "0;7m" | "?2026l" | "?1049l") {
                assert!(!linked, "link leaked into terminal chrome: {sequence:?}");
            }
            rest = &csi[end + 1..];
        } else {
            let ch = rest.chars().next().expect("remaining character");
            if matches!(ch, '\r' | '\n') {
                assert!(!linked, "link leaked across a line ending");
            }
            assert_ne!(ch, '\x1b', "unexpected terminal control: {rest:?}");
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    assert!(!linked, "link left open at end of output");
    out
}

#[test]
fn forced_osc8_survives_search_sideways_resize_and_exit() {
    let source = format!(
        "{}[Clickable **wide** label 全角 ending {}]({URI}) tail\n\nLast line.\n",
        "padding ".repeat(18),
        "tailword ".repeat(18)
    );
    let fixture = Fixture::new("links.md", source.as_bytes());
    let mut pty = Pty::spawn_options(
        fixture.path(),
        10,
        80,
        &["--links", "always", "--width", "240"],
        &[],
    );
    pty.wait_for("o outline");
    assert!(pty.latest_frame().contains(&format!("\x1b]8;;{URI}\x1b\\")));
    assert!(visible_and_balanced(&pty.latest_frame()).contains("Clickable wide label 全角 ending"));

    pty.send(b"/Clickable wide label\r");
    pty.wait_for("/Clickable wide label 1/1");
    assert!(pty.latest_frame().contains("\x1b[1;7m"));
    visible_and_balanced(&pty.latest_frame());

    pty.send(b"/hidden\r");
    pty.wait_for("not found: hidden");
    visible_and_balanced(&pty.latest_frame());

    pty.send(b"gll");
    pty.wait_for("col ");
    let shown = visible_and_balanced(&pty.latest_frame());
    assert!(
        shown.contains("全角 ending") && !shown.contains("Clickable"),
        "{shown:?}"
    );
    pty.resize(12, 60);
    pty.wait_for("\x1b[2J");
    visible_and_balanced(&pty.latest_frame());

    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
    let output = String::from_utf8_lossy(&pty.output);
    assert!(
        output.contains("\x1b[?1049l"),
        "alternate screen not restored"
    );
    assert!(output.contains(OSC_CLOSE));
    visible_and_balanced(&output);
}

// A stored ZIP keeps the EPUB fixture textual and exercises the production extractor.
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = !data.iter().fold(!0u32, |crc, byte| {
            let mut crc = crc ^ u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
            crc
        });
        let fields = |central: bool| {
            let mut record = Vec::new();
            let signature: u32 = if central { 0x0201_4b50 } else { 0x0403_4b50 };
            record.extend_from_slice(&signature.to_le_bytes());
            if central {
                record.extend_from_slice(&20u16.to_le_bytes());
            }
            record.extend_from_slice(&[20, 0, 0, 0, 0, 0]);
            record.extend_from_slice(&[0; 4]);
            record.extend_from_slice(&crc.to_le_bytes());
            record.extend_from_slice(&(data.len() as u32).to_le_bytes());
            record.extend_from_slice(&(data.len() as u32).to_le_bytes());
            record.extend_from_slice(&(name.len() as u16).to_le_bytes());
            record.extend_from_slice(&[0, 0]);
            if central {
                record.extend_from_slice(&[0; 10]);
                record.extend_from_slice(&offset.to_le_bytes());
            }
            record.extend_from_slice(name.as_bytes());
            record
        };
        out.extend(fields(false));
        out.extend_from_slice(data);
        directory.extend(fields(true));
    }
    let offset = out.len() as u32;
    out.extend(&directory);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

fn navigation_book() -> Fixture {
    let container = br#"<container><rootfiles><rootfile full-path="book/package.opf"/></rootfiles></container>"#;
    let package = br#"<package><manifest>
      <item id="first" href="one.xhtml" media-type="application/xhtml+xml"/>
      <item id="second" href="two.xhtml" media-type="application/xhtml+xml"/>
      </manifest><spine><itemref idref="first"/><itemref idref="second"/></spine></package>"#;
    let padding: String = (0..24)
        .map(|index| {
            format!(
                "<p>Filler paragraph {index} has enough words to wrap after resizing the book.</p>"
            )
        })
        .collect();
    let first = format!(
        r##"<html><body><h1 id="shared">Chapter One</h1>
        <p><a href="two.xhtml#absent">Broken fragment</a>
        <a href="missing.xhtml#shared">Broken chapter</a></p>
        <p><a href="two.xhtml#shared">Visit Chapter Two</a></p>
        <p><a href="#shared">Local Chapter One</a></p>{padding}</body></html>"##
    );
    let second = format!(
        r#"<html><body><h1 id="shared">Chapter Two</h1>
        <p><a href="one.xhtml#shared">Visit Chapter One</a></p>{padding}</body></html>"#
    );
    Fixture::new(
        "navigation.epub",
        &zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container),
            ("book/package.opf", package),
            ("book/one.xhtml", first.as_bytes()),
            ("book/two.xhtml", second.as_bytes()),
        ]),
    )
}

fn body(frame: &str) -> String {
    let visible = visible_and_balanced(frame);
    visible
        .rsplit_once('\n')
        .map_or(visible.clone(), |(body, _)| body.to_owned())
}

fn status(frame: &str) -> String {
    visible_and_balanced(frame)
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn epub_chapter_links_jump_and_return_with_duplicate_ids_and_no_osc8() {
    let fixture = navigation_book();
    let mut pty = Pty::spawn_options(
        fixture.path(),
        12,
        100,
        &["--links", "never", "--color", "never"],
        &[],
    );
    pty.wait_for("1-11/");
    let original = body(&pty.latest_frame());
    assert!(original.starts_with("Chapter One"), "{original:?}");
    assert!(original.contains("Broken fragment"));
    assert!(!pty.latest_frame().contains("\x1b]8;;"));

    pty.send(b"\t");
    pty.wait_for("Enter follow");
    assert!(status(&pty.latest_frame()).contains("Visit Chapter Two"));
    assert!(!status(&pty.latest_frame()).contains("shared"));
    pty.send(b"\x1b[Z");
    pty.wait_for("Visit Chapter One  Enter follow");
    pty.send(b"\t");
    pty.wait_for("Visit Chapter Two  Enter follow");
    let departure = body(&pty.latest_frame());
    pty.send(b"\r");
    pty.wait_for("Backspace back");
    assert!(body(&pty.latest_frame()).starts_with("Chapter Two"));

    pty.send(b"\t\r");
    pty.wait_for("1-11/");
    assert!(body(&pty.latest_frame()).starts_with("Chapter One"));
    pty.send(b"\x7f");
    pty.wait_for("Chapter Two");
    assert!(body(&pty.latest_frame()).starts_with("Chapter Two"));
    pty.send(b"\x08");
    pty.wait_for("Tab links");
    assert_eq!(body(&pty.latest_frame()), departure);
    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
    assert!(!String::from_utf8_lossy(&pty.output).contains("\x1b]8;"));
}

#[test]
fn epub_selection_clears_for_search_outline_scrolling_and_reflow() {
    let fixture = navigation_book();
    let mut pty = Pty::spawn_options(
        fixture.path(),
        14,
        100,
        &["--links", "never", "--color", "never"],
        &[],
    );
    pty.wait_for("Tab links");
    pty.send(b"\t/Filler paragraph 20\r");
    pty.wait_for("/Filler paragraph 20 1/2");
    assert!(!status(&pty.latest_frame()).contains("Enter follow"));
    let searched = body(&pty.latest_frame());
    pty.send(b"\r");
    pty.wait_for("/Filler paragraph 20 1/2");
    assert_ne!(
        body(&pty.latest_frame()),
        searched,
        "Enter must still scroll"
    );

    pty.send(b"\x1bg\to");
    pty.wait_for("Enter jump");
    pty.send(b"\r");
    pty.wait_for("Tab links");
    assert!(body(&pty.latest_frame()).starts_with("Chapter One"));
    assert!(!status(&pty.latest_frame()).contains("Enter follow"));

    pty.send(b"\tj");
    pty.wait_for("Tab links");
    assert!(!status(&pty.latest_frame()).contains("Enter follow"));
    pty.send(b"g\t");
    pty.wait_for("Enter follow");
    pty.resize(14, 40);
    pty.wait_for("\x1b[2J");
    assert!(!status(&pty.latest_frame()).contains("Enter follow"));
    pty.send(b"\t");
    pty.wait_for("Enter follow");
    assert!(
        status(&pty.latest_frame()).contains("Visit Chapter Two"),
        "a narrow selection bar must prioritize the readable label"
    );
    pty.send(b"\x1bg\t\r");
    pty.wait_for("Backspace back");
    assert!(body(&pty.latest_frame()).starts_with("Chapter Two"));
    pty.resize(16, 60);
    pty.wait_for("\x1b[2J");
    pty.send(b"\x7f");
    pty.wait_for("%");
    assert!(body(&pty.latest_frame()).contains("Visit Chapter Two"));
    assert!(!status(&pty.latest_frame()).contains("Backspace back"));
    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
}

#[test]
fn external_links_reflow_with_plain_color_and_active_search() {
    let words = "wrapped label ".repeat(12);
    let source =
        format!("[First **styled** 全角 {words}Lastneedle]({URI})\n\nAfter linked label.\n");
    let expected = format!("First styled 全角 {words}Lastneedle After linked label.");
    let normalize = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let fixture = Fixture::new("reflow.md", source.as_bytes());
    let mut pty = Pty::spawn_options(
        fixture.path(),
        24,
        80,
        &["--links", "always", "--color", "never"],
        &[],
    );
    pty.wait_for("q quit");
    assert_eq!(normalize(&body(&pty.latest_frame())), normalize(&expected));
    pty.send(b"/Lastneedle\r");
    pty.wait_for("/Lastneedle 1/1");
    assert!(pty.latest_frame().contains("\x1b[1;7mLastneedle"));
    let before = body(&pty.latest_frame());
    pty.resize(24, 40);
    pty.wait_for("\x1b[2J");
    assert!(status(&pty.latest_frame()).contains("/Lastneedle 1/1"));
    assert!(pty.latest_frame().contains("\x1b[1;7mLastneedle"));
    let after = body(&pty.latest_frame());
    assert_ne!(after, before, "resize must reflow the linked label");
    assert_eq!(normalize(&after), normalize(&expected));
    pty.send(b"\x03");
    assert_eq!(pty.exit_code(), Some(0));
    let output = String::from_utf8_lossy(&pty.output);
    assert!(
        output.contains(&format!("{OSC_CLOSE}\x1b[?1007l")),
        "exit must close OSC 8 before restoring the terminal"
    );
    visible_and_balanced(&output);
}

#[test]
fn terminal_link_policy_is_observable_in_real_pager_sessions() {
    let fixture = Fixture::new("policy.md", format!("[Policy label]({URI})\n").as_bytes());
    type PolicyCase<'a> = (&'a [&'a str], &'a [(&'a str, &'a str)], bool);
    let cases: &[PolicyCase<'_>] = &[
        (&[], &[("TERM", "xterm-kitty")], true),
        (&["--links", "auto"], &[("TERM_PROGRAM", "WezTerm")], true),
        (&["--links", "auto"], &[], false),
        (
            &[],
            &[("TERM", "xterm-kitty"), ("TMUX", "/fake/session")],
            false,
        ),
        (&[], &[("TERM", "xterm-kitty"), ("NO_COLOR", "1")], false),
        (&["--color", "never"], &[("TERM", "xterm-kitty")], false),
        (
            &["--links", "always", "--color", "never"],
            &[("NO_COLOR", "1")],
            true,
        ),
        (&["--links", "never"], &[("TERM", "xterm-kitty")], false),
    ];
    for &(args, env, enabled) in cases {
        let mut pty = Pty::spawn_options(fixture.path(), 10, 100, args, env);
        pty.wait_for("q quit");
        let frame = pty.latest_frame();
        assert_eq!(frame.contains("\x1b]8;"), enabled, "{args:?} {env:?}");
        let shown = visible_and_balanced(&frame);
        assert!(shown.contains("Policy label"));
        assert_eq!(
            shown.contains(URI),
            !enabled,
            "printed fallback: {args:?} {env:?}"
        );
        if enabled {
            pty.resize(10, 40);
            pty.wait_for("\x1b[2J");
            pty.send(b"l");
            pty.wait_for("%");
            assert!(
                !status(&pty.latest_frame()).contains("col "),
                "hidden URI bytes must not create horizontal overflow"
            );
        }
        pty.send(b"q");
        assert_eq!(pty.exit_code(), Some(0));
        let output = String::from_utf8_lossy(&pty.output);
        assert_eq!(output.contains("\x1b]8;"), enabled);
        for mode in ["?1000h", "?1002h", "?1003h", "?1006h"] {
            assert!(!output.contains(mode), "mouse reporting enabled: {mode}");
        }
        visible_and_balanced(&output);
    }
}

#[test]
fn epub_return_history_is_bounded_in_a_live_session() {
    let fixture = navigation_book();
    let mut pty = Pty::spawn_options(
        fixture.path(),
        12,
        100,
        &["--links", "never", "--color", "never"],
        &[],
    );
    pty.wait_for("Tab links");
    // Prompt sentinels prove the full burst was consumed, not just its first frame.
    let mut jumps = b"g\t\r".repeat(140);
    jumps.extend_from_slice(b"/historyfilled");
    pty.send(&jumps);
    pty.wait_for("/historyfilled█");
    pty.send(b"\x1b");
    pty.wait_for("Backspace back");
    let mut returns = vec![0x7f; 128];
    returns.extend_from_slice(b"/historyempty");
    pty.send(&returns);
    pty.wait_for("/historyempty█");
    pty.send(b"\x1b");
    pty.wait_for("Tab links");
    assert!(!status(&pty.latest_frame()).contains("Backspace back"));
    let returned = body(&pty.latest_frame());
    assert!(returned.starts_with("Chapter One"));
    pty.send(b"\x7f");
    pty.wait_for("Tab links");
    assert_eq!(
        body(&pty.latest_frame()),
        returned,
        "empty history is a no-op"
    );
    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
}

#[test]
fn epub_links_and_destinations_in_wide_tables_scroll_into_view() {
    let container =
        br#"<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>"#;
    let package = br#"<package><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/></spine></package>"#;
    let cells = "<td>padding cell</td>".repeat(10);
    let header = "<th>column title</th>".repeat(11);
    let padding = "<p>filler paragraph.</p>".repeat(40);
    let chapter = format!(
        "<html><body><h1>Wide</h1><table><tr>{header}</tr><tr>{cells}<td><a href='#edge'>RIGHT_LINK</a></td></tr></table>{padding}<table><tr>{header}</tr><tr>{cells}<td id='edge'>DESTINATION</td></tr></table>{padding}</body></html>"
    );
    let fixture = Fixture::new(
        "wide-navigation.epub",
        &zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container),
            ("book.opf", package),
            ("one.xhtml", chapter.as_bytes()),
        ]),
    );
    let mut pty = Pty::spawn_options(
        fixture.path(),
        12,
        50,
        &[
            "--links=never",
            "--color=never",
            "--images=never",
            "--width=240",
        ],
        &[],
    );
    let viewport = |frame: &str| {
        body(frame)
            .lines()
            .map(|line| line.chars().take(50).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };
    pty.wait_for("1-11/");
    assert!(!viewport(&pty.latest_frame()).contains("RIGHT_LINK"));
    pty.send(b"\t");
    pty.wait_for("Enter follow");
    let departure = viewport(&pty.latest_frame());
    assert!(
        departure.contains("RIGHT_LINK"),
        "selected source is clipped: {departure:?}"
    );
    pty.send(b"\r");
    pty.wait_for("DESTINATION");
    let target = viewport(&pty.latest_frame());
    assert!(
        target.contains("DESTINATION"),
        "destination is clipped: {target:?}"
    );
    pty.send(b"\x7f");
    pty.wait_for("RIGHT_LINK");
    assert_eq!(viewport(&pty.latest_frame()), departure);
    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
}

#[test]
fn fragmented_shift_tab_and_a_lone_escape_work_in_a_real_session() {
    let fixture = navigation_book();
    let mut pty = Pty::spawn_options(
        fixture.path(),
        12,
        100,
        &["--links=never", "--color=never"],
        &[],
    );
    pty.wait_for("Tab links");
    pty.send(b"\t");
    pty.wait_for("Visit Chapter Two  Enter follow");
    pty.send(b"\x1b[");
    pty.wait_for("Visit Chapter Two  Enter follow");
    pty.send(b"Z");
    pty.wait_for("Visit Chapter One  Enter follow");
    pty.send(b"/query");
    pty.wait_for("/query█");
    pty.send(b"\x1b");
    pty.wait_for("Tab links");
    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
}

#[test]
fn split_utf8_and_oversized_escape_sequences_do_not_become_wrong_keys() {
    let fixture = navigation_book();
    let mut pty = Pty::spawn_options(
        fixture.path(),
        12,
        100,
        &["--links=never", "--color=never"],
        &[],
    );
    pty.wait_for("Tab links");
    pty.send(b"/\xe5");
    pty.wait_for("/█");
    pty.send(b"\x85");
    pty.wait_for("/█");
    pty.send(b"\xa8");
    pty.wait_for("/全█");
    pty.send(b"\x15");
    pty.wait_for("/█");
    pty.send(b"\x1b");
    pty.wait_for("Tab links");
    let mut oversized = b"\x1b[".to_vec();
    oversized.extend(std::iter::repeat_n(b'1', 600));
    oversized.extend_from_slice(b"q/decoderalive");
    pty.send(&oversized);
    pty.wait_for("/decoderalive█");
    pty.send(b"\x1b");
    pty.wait_for("Tab links");
    pty.send(b"q");
    assert_eq!(pty.exit_code(), Some(0));
}
