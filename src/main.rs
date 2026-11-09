// Author: machinageist
// Date: 2026-11-09
// Description: mdread CLI, reads Markdown from a file or stdin and prints it formatted for the terminal
// Notes: Renders into one buffer and writes it in a single call; a closed pipe (`| head`) is not an error

mod term;

use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use mdread::{Options, render};

const USAGE: &str = "\
Usage: mdread [OPTIONS] [FILE]

Render Markdown in the terminal. Reads FILE, or stdin when FILE is - or absent.

Options:
  -w, --width <N>      Wrap at N columns (default: terminal width, capped at 100)
      --color <WHEN>   auto, always or never (default: auto, honours NO_COLOR)
  -h, --help           Show this help
  -V, --version        Show the version";

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROGRAM: &str = "mdread";
const STDIN_PATH: &str = "-";
const ENV_NO_COLOR: &str = "NO_COLOR";

// Width defaults
const FALLBACK_WIDTH: usize = 80;
const MAX_AUTO_WIDTH: usize = 100;
const MIN_WIDTH: usize = 20;

// Exit codes
const EXIT_FAILURE: u8 = 1;
const EXIT_USAGE: u8 = 2;

enum ColorMode {
    Auto,
    Always,
    Never,
}

struct Args {
    path: Option<String>,
    width: Option<usize>,
    color: ColorMode,
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

    // Nothing to read: a bare `mdread` at a prompt should explain itself, not hang
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
        width: resolve_width(args.width),
        color: resolve_color(&args.color),
    };
    let output = render(&input, &options);

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
        width: None,
        color: ColorMode::Auto,
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
                args.color = match value.as_str() {
                    "auto" => ColorMode::Auto,
                    "always" => ColorMode::Always,
                    "never" => ColorMode::Never,
                    _ => return Err(format!("invalid color mode '{value}'")),
                };
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
    let width = requested.unwrap_or_else(|| {
        term::columns().map_or(FALLBACK_WIDTH, |columns| columns.min(MAX_AUTO_WIDTH))
    });
    width.max(MIN_WIDTH)
}

// Decide whether to emit ANSI styling
fn resolve_color(mode: &ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            let no_color = std::env::var_os(ENV_NO_COLOR).is_some_and(|value| !value.is_empty());
            !no_color && io::stdout().is_terminal()
        }
    }
}
