// Author: machinageist
// Date: 2026-11-09
// Description: mu CLI, reads markup or data from a file or stdin and prints it formatted for the terminal
// Notes: Renders into one buffer, then either pages it or writes it in a single call;
// a closed pipe (`| head`) is not an error

// Wired into the CLI in the next task
#[allow(dead_code)]
mod fetch;
mod pager;
mod term;

use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use mu::{CellSize, Format, ImageMode, ImageOptions, Options, Protocol, detect, present};

const USAGE: &str = "\
Usage: mu [OPTIONS] [FILE]

Present Markdown, HTML, reStructuredText, Org, AsciiDoc, JSON, YAML or TOML in the
terminal. Reads FILE, or stdin when FILE is - or absent. The format comes from the
file extension, else from the content, else Markdown.

Options:
  -f, --format <NAME>  Force a format: markdown, html, rst, org, asciidoc, json, yaml, toml
  -w, --width <N>      Wrap at N columns (default: terminal width, capped at 100)
      --color <WHEN>   auto, always or never (default: auto, honours NO_COLOR)
      --paging <WHEN>  auto, always or never (default: auto, pages when taller than the screen)
      --images <MODE>  auto, fetch or never (default: auto, local files and data: URIs only;
                       fetch also downloads http(s) images with curl)
      --base <DIR|URL> Resolve relative image paths here (default: the file's folder)
  -h, --help           Show this help
  -V, --version        Show the version

Pager keys:
  j k  Down Up  Enter   scroll a line        space b  PgDn PgUp   scroll a page
  d u                   scroll half a page   g G  Home End        jump to top or bottom
  q  Ctrl-C             quit";

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROGRAM: &str = "mu";
const STDIN_PATH: &str = "-";
const STDIN_TITLE: &str = "stdin";
const ENV_NO_COLOR: &str = "NO_COLOR";

// Terminals that speak a native graphics protocol, read from the environment
const ENV_TERM: &str = "TERM";
const ENV_TERM_PROGRAM: &str = "TERM_PROGRAM";
const ENV_KITTY_WINDOW: &str = "KITTY_WINDOW_ID";
const ENV_TMUX: &str = "TMUX";
const KITTY_TERMS: [&str; 2] = ["xterm-kitty", "xterm-ghostty"];
const KITTY_PROGRAMS: [&str; 1] = ["ghostty"];
const ITERM_PROGRAMS: [&str; 2] = ["iTerm.app", "WezTerm"];

// Rows kept free under the tallest image so it fits on one screen with the prompt
const IMAGE_ROW_MARGIN: usize = 2;

// Width defaults
const FALLBACK_WIDTH: usize = 80;
const MAX_AUTO_WIDTH: usize = 100;
const MIN_WIDTH: usize = 20;

// Exit codes
const EXIT_FAILURE: u8 = 1;
const EXIT_USAGE: u8 = 2;

enum When {
    Auto,
    Always,
    Never,
}

struct Args {
    path: Option<String>,
    format: Option<Format>,
    width: Option<usize>,
    color: When,
    paging: When,
    images: ImageMode,
    base: Option<String>,
}

enum Command {
    Render(Args),
    Help,
    Version,
}

// Run the CLI and map failures to exit codes
fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(Command::Render(args)) => args,
        Ok(Command::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("{PROGRAM} {VERSION}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{PROGRAM}: {message}\n\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    // Nothing to read: a bare `mu` at a prompt should explain itself, not hang
    if args.path.is_none() && io::stdin().is_terminal() {
        eprintln!("{USAGE}");
        return ExitCode::from(EXIT_USAGE);
    }

    let file = args.path.as_deref().filter(|path| *path != STDIN_PATH);
    let input = match read_input(file) {
        Ok(input) => input,
        Err(error) => {
            let source = args.path.as_deref().unwrap_or(STDIN_PATH);
            eprintln!("{PROGRAM}: {source}: {error}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };

    let options = Options {
        images: resolve_images(&args, file),
        ..Options::new(resolve_width(args.width), resolve_color(&args.color))
    };
    let format = args.format.unwrap_or_else(|| detect(file, &input));
    // Half-blocks first: they work in the pager, and decide whether paging is needed
    let output = present(&input, format, &options);

    // Page when wanted and the terminal can be taken over, otherwise fall through to a plain write
    if should_page(&args.paging, &output)
        && let Ok(session) = pager::Session::open()
    {
        let title = file.unwrap_or(STDIN_TITLE);
        let mut width = options.width;
        let mut relayout = |columns: usize| {
            let next = auto_width(columns);
            (args.width.is_none() && next != width).then(|| {
                width = next;
                present(
                    &input,
                    format,
                    &Options {
                        width,
                        ..options.clone()
                    },
                )
            })
        };
        return match session.run(title, output, &mut relayout) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{PROGRAM}: {error}");
                ExitCode::from(EXIT_FAILURE)
            }
        };
    }

    // Straight to the terminal, a native graphics protocol draws sharper pictures
    let protocol = native_protocol();
    let output = if protocol != Protocol::HalfBlock && options.images.mode != ImageMode::Never {
        let mut native = options.clone();
        native.images.protocol = protocol;
        present(&input, format, &native)
    } else {
        output
    };

    let mut stdout = io::stdout().lock();
    match stdout
        .write_all(output.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{PROGRAM}: {error}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

// Parse command-line arguments into a command
fn parse_args(mut raw: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut args = Args {
        path: None,
        format: None,
        width: None,
        color: When::Auto,
        paging: When::Auto,
        images: ImageMode::Auto,
        base: None,
    };
    while let Some(arg) = raw.next() {
        let (flag, inline_value) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (arg.clone(), None),
        };
        match flag.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "-f" | "--format" => {
                let value = inline_value
                    .or_else(|| raw.next())
                    .ok_or("--format needs a value")?;
                let format =
                    Format::from_name(&value).ok_or(format!("unknown format '{value}'"))?;
                args.format = Some(format);
            }
            "-w" | "--width" => {
                let value = inline_value
                    .or_else(|| raw.next())
                    .ok_or("--width needs a value")?;
                let width = value
                    .parse()
                    .map_err(|_| format!("invalid width '{value}'"))?;
                args.width = Some(width);
            }
            "--color" => {
                let value = inline_value
                    .or_else(|| raw.next())
                    .ok_or("--color needs a value")?;
                args.color = parse_when(&value).ok_or(format!("invalid color mode '{value}'"))?;
            }
            "--paging" => {
                let value = inline_value
                    .or_else(|| raw.next())
                    .ok_or("--paging needs a value")?;
                args.paging = parse_when(&value).ok_or(format!("invalid paging mode '{value}'"))?;
            }
            "--images" => {
                let value = inline_value
                    .or_else(|| raw.next())
                    .ok_or("--images needs a value")?;
                args.images =
                    parse_image_mode(&value).ok_or(format!("invalid images mode '{value}'"))?;
            }
            "--base" => {
                let value = inline_value
                    .or_else(|| raw.next())
                    .ok_or("--base needs a value")?;
                args.base = Some(value);
            }
            _ if arg != STDIN_PATH && arg.starts_with('-') => {
                return Err(format!("unknown option '{arg}'"));
            }
            _ if args.path.is_some() => return Err(format!("unexpected argument '{arg}'")),
            _ => args.path = Some(arg),
        }
    }
    Ok(Command::Render(args))
}

// Parse an --images value
fn parse_image_mode(value: &str) -> Option<ImageMode> {
    match value {
        "auto" => Some(ImageMode::Auto),
        "fetch" => Some(ImageMode::Fetch),
        "never" => Some(ImageMode::Never),
        _ => None,
    }
}

// Build image settings from the flags and the terminal's cell geometry, half-blocks by default
fn resolve_images(args: &Args, file: Option<&str>) -> ImageOptions {
    let size = term::size();
    let cell = size
        .and_then(|size| {
            let (width, height) = size.pixels?;
            let cell = CellSize {
                width: u32::try_from(width / size.columns).ok()?,
                height: u32::try_from(height / size.rows).ok()?,
            };
            (cell.width > 0 && cell.height > 0).then_some(cell)
        })
        .unwrap_or_default();
    ImageOptions {
        mode: args.images,
        // A file path is fine as a base: relative images resolve against its folder
        base: args.base.clone().or_else(|| file.map(str::to_owned)),
        cell,
        protocol: Protocol::HalfBlock,
        max_rows: size.map(|size| size.rows.saturating_sub(IMAGE_ROW_MARGIN).max(1)),
    }
}

// Pick a native graphics protocol when stdout is a terminal known to support one
fn native_protocol() -> Protocol {
    let env = |key: &str| std::env::var(key).unwrap_or_default();
    // Inside tmux the escapes would need passthrough wrapping, so stay with half-blocks
    if !io::stdout().is_terminal() || std::env::var_os(ENV_TMUX).is_some() {
        return Protocol::HalfBlock;
    }
    let term = env(ENV_TERM);
    let program = env(ENV_TERM_PROGRAM);
    if std::env::var_os(ENV_KITTY_WINDOW).is_some()
        || KITTY_TERMS.contains(&term.as_str())
        || KITTY_PROGRAMS.contains(&program.as_str())
    {
        Protocol::Kitty
    } else if ITERM_PROGRAMS.contains(&program.as_str()) {
        Protocol::Iterm
    } else {
        Protocol::HalfBlock
    }
}

// Parse an auto/always/never value
fn parse_when(value: &str) -> Option<When> {
    match value {
        "auto" => Some(When::Auto),
        "always" => Some(When::Always),
        "never" => Some(When::Never),
        _ => None,
    }
}

// Read the whole document from a file, or stdin when no path is given
fn read_input(path: Option<&str>) -> io::Result<String> {
    let bytes = match path {
        Some(path) => fs::read(path)?,
        None => {
            let mut bytes = Vec::new();
            io::stdin().lock().read_to_end(&mut bytes)?;
            bytes
        }
    };
    // Tolerate stray invalid bytes rather than refusing the file
    Ok(String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned()))
}

// Choose the wrap width from the flag or the terminal
fn resolve_width(requested: Option<usize>) -> usize {
    match requested {
        Some(width) => width.max(MIN_WIDTH),
        None => term::size().map_or(FALLBACK_WIDTH, |size| auto_width(size.columns)),
    }
}

// Fit the default width to a terminal: full width up to a readable cap
fn auto_width(columns: usize) -> usize {
    columns.clamp(MIN_WIDTH, MAX_AUTO_WIDTH)
}

// Decide whether output goes through the built-in pager
fn should_page(mode: &When, output: &str) -> bool {
    if !io::stdout().is_terminal() {
        return false;
    }
    match mode {
        When::Never => false,
        When::Always => true,
        // Like `less -F`: only page when the output would push the prompt off screen
        When::Auto => term::size()
            .is_some_and(|size| output.lines().nth(size.rows.saturating_sub(1)).is_some()),
    }
}

// Decide whether to emit ANSI styling
fn resolve_color(mode: &When) -> bool {
    match mode {
        When::Always => true,
        When::Never => false,
        When::Auto => {
            let no_color = std::env::var_os(ENV_NO_COLOR).is_some_and(|value| !value.is_empty());
            !no_color && io::stdout().is_terminal()
        }
    }
}
