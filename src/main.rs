// Author: machinageist
// Date: 2026-11-09
// Description: mu CLI, reads markup or data from a file, stdin or a URL and prints it formatted for the terminal
// Notes: Renders into one buffer, then either pages it or writes it in a single call;
// a closed pipe (`| head`) is not an error

mod fetch;
mod pager;
mod term;

use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use mu::{CellSize, Format, ImageMode, ImageOptions, Options, Protocol, detect, present, sniff};

const USAGE: &str = "\
Usage: mu [OPTIONS] [FILE | URL]

Present Markdown, HTML, reStructuredText, Org, AsciiDoc, JSON, YAML or TOML in the
terminal. Reads FILE, or stdin when FILE is - or absent, or fetches an http(s) URL
with curl. The format comes from the file extension, else from the content, else
Markdown; for a URL the response Content-Type is tried first.

Options:
  -f, --format <NAME>  Force a format: markdown, html, rst, org, asciidoc, json, yaml, toml
  -w, --width <N>      Wrap at N columns (default: terminal width, capped at 100)
      --color <WHEN>   auto, always or never (default: auto, honours NO_COLOR)
      --paging <WHEN>  auto, always or never (default: auto, pages when taller than the screen)
      --images <MODE>  auto, fetch or never (default: auto, local files and data: URIs only;
                       fetch also downloads http(s) images with curl)
                       (default for a URL: fetch)
      --base <DIR|URL> Resolve relative image paths here (default: the file's folder or the URL)
  -h, --help           Show this help
  -V, --version        Show the version

URL options (curl-like, only valid with a URL):
  -H, --header <H>     Add a request header (repeatable)
  -X, --request <M>    Use another request method
  -d, --data <D>       Send data in the request body (repeatable)
  -A, --user-agent <UA> Set the User-Agent (default: mu/VERSION)
  -u, --user <U:P>     Send credentials
  -k, --insecure       Skip TLS certificate checks
  -i, --include        Show the response status and headers, then the body
  -I, --head           Show the response status and headers only
      --timeout <SECS> Give up after this long
      --curl <ARG>     Pass ARG straight to curl (repeatable)
      --no-rewrite     Fetch the URL as typed, not the raw file behind a
                       GitHub, GitLab or Gist page

Pager keys:
  j k  Down Up  Enter   scroll a line        space b  PgDn PgUp   scroll a page
  d u                   scroll half a page   g G  Home End        jump to top or bottom
  q  Ctrl-C             quit";

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROGRAM: &str = "mu";
const STDIN_PATH: &str = "-";
const STDIN_TITLE: &str = "stdin";
const ENV_NO_COLOR: &str = "NO_COLOR";
const NEEDS_CURL: &str = "opening URLs needs curl";

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
    // None until --images is given, so a URL can default to fetching
    images: Option<ImageMode>,
    base: Option<String>,
    request: fetch::Request,
    include: bool,
    no_rewrite: bool,
    // The first URL-only flag seen, named in the error when there is no URL
    url_flag: Option<String>,
}

// Input ready to render: one or more parts, each in its own format
struct Source {
    parts: Vec<(String, Format)>,
    title: String,
    base: Option<String>,
    images: ImageMode,
    // Set for an HTTP error status, reported after the body is shown
    failure: Option<String>,
}

enum Command {
    Render(Box<Args>),
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

    let source = match args.path.as_deref().filter(|path| fetch::is_url(path)) {
        Some(url) => load_url(&args, url),
        None => load_file(&args),
    };
    let source = match source {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{PROGRAM}: {message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    let code = show(&args, &source);
    match &source.failure {
        Some(message) => {
            eprintln!("{PROGRAM}: {message}");
            ExitCode::from(EXIT_FAILURE)
        }
        None => code,
    }
}

// Read a file or stdin, detecting its format
fn load_file(args: &Args) -> Result<Source, String> {
    let file = args.path.as_deref().filter(|path| *path != STDIN_PATH);
    let input = read_input(file).map_err(|error| {
        let source = args.path.as_deref().unwrap_or(STDIN_PATH);
        format!("{source}: {error}")
    })?;
    let format = args.format.unwrap_or_else(|| detect(file, &input));
    Ok(Source {
        parts: vec![(input, format)],
        title: file.unwrap_or(STDIN_TITLE).to_owned(),
        // A file path is fine as a base: relative images resolve against its folder
        base: args.base.clone().or_else(|| file.map(str::to_owned)),
        images: args.images.unwrap_or(ImageMode::Auto),
        failure: None,
    })
}

// Fetch a URL with curl, choosing the body's format from the response
fn load_url(args: &Args, url: &str) -> Result<Source, String> {
    let target = (!args.no_rewrite)
        .then(|| fetch::raw_url(url))
        .flatten()
        .unwrap_or_else(|| url.to_owned());
    let request = fetch::Request {
        url: target,
        ..args.request.clone()
    };
    let response = fetch::fetch(&request, VERSION).map_err(|error| match error {
        fetch::FetchError::Missing => NEEDS_CURL.to_owned(),
        fetch::FetchError::Failed(message) => format!("{url}: {message}"),
    })?;

    let mut parts = Vec::new();
    if args.include || request.head {
        parts.push((fetch::header_table(&response), Format::Markdown));
    }
    if !request.head {
        let body = String::from_utf8(response.body.clone())
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());
        // text/plain and other unknown types fall through to the path and the content
        let format = args
            .format
            .or_else(|| response.content_type().and_then(Format::from_media_type))
            .or_else(|| Format::from_url_path(&response.final_url))
            .unwrap_or_else(|| sniff(&body));
        parts.push((body, format));
    }
    Ok(Source {
        parts,
        title: url.to_owned(),
        base: Some(args.base.clone().unwrap_or(response.final_url.clone())),
        images: args.images.unwrap_or(ImageMode::Fetch),
        failure: response
            .is_error()
            .then(|| format!("{url}: HTTP {}", response.status)),
    })
}

// Render every part of a source and join them
fn present_all(source: &Source, options: &Options) -> String {
    source
        .parts
        .iter()
        .map(|(input, format)| present(input, *format, options))
        .filter(|output| !output.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

// Render a source and page it or write it out
fn show(args: &Args, source: &Source) -> ExitCode {
    let options = Options {
        images: resolve_images(source.images, source.base.clone()),
        ..Options::new(resolve_width(args.width), resolve_color(&args.color))
    };
    // Half-blocks first: they work in the pager, and decide whether paging is needed
    let output = present_all(source, &options);

    // Page when wanted and the terminal can be taken over, otherwise fall through to a plain write
    if should_page(&args.paging, &output)
        && let Ok(session) = pager::Session::open()
    {
        let mut width = options.width;
        let mut relayout = |columns: usize| {
            let next = auto_width(columns);
            (args.width.is_none() && next != width).then(|| {
                width = next;
                present_all(
                    source,
                    &Options {
                        width,
                        ..options.clone()
                    },
                )
            })
        };
        return match session.run(&source.title, output, &mut relayout) {
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
        present_all(source, &native)
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
        images: None,
        base: None,
        request: fetch::Request::default(),
        include: false,
        no_rewrite: false,
        url_flag: None,
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
                    Some(parse_image_mode(&value).ok_or(format!("invalid images mode '{value}'"))?);
            }
            "--base" => {
                let value = inline_value
                    .or_else(|| raw.next())
                    .ok_or("--base needs a value")?;
                args.base = Some(value);
            }
            "-H" | "--header" | "-X" | "--request" | "-d" | "--data" | "-A" | "--user-agent"
            | "-u" | "--user" | "--timeout" | "--curl" => {
                let value = inline_value
                    .or_else(|| raw.next())
                    .ok_or(format!("{flag} needs a value"))?;
                let request = &mut args.request;
                match flag.as_str() {
                    "-H" | "--header" => request.headers.push(value),
                    "-X" | "--request" => request.method = Some(value),
                    "-d" | "--data" => request.data.push(value),
                    "-A" | "--user-agent" => request.user_agent = Some(value),
                    "-u" | "--user" => request.user = Some(value),
                    "--timeout" => request.timeout = Some(value),
                    _ => request.extra.push(value),
                }
                args.url_flag.get_or_insert(flag);
            }
            "-k" | "--insecure" | "-i" | "--include" | "-I" | "--head" | "--no-rewrite" => {
                match flag.as_str() {
                    "-k" | "--insecure" => args.request.insecure = true,
                    "-i" | "--include" => args.include = true,
                    "-I" | "--head" => args.request.head = true,
                    _ => args.no_rewrite = true,
                }
                args.url_flag.get_or_insert(flag);
            }
            _ if arg != STDIN_PATH && arg.starts_with('-') => {
                return Err(format!("unknown option '{arg}'"));
            }
            _ if args.path.is_some() => return Err(format!("unexpected argument '{arg}'")),
            _ => args.path = Some(arg),
        }
    }
    let has_url = args.path.as_deref().is_some_and(fetch::is_url);
    if let (Some(flag), false) = (&args.url_flag, has_url) {
        return Err(format!("{flag} only works with a URL"));
    }
    Ok(Command::Render(Box::new(args)))
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

// Build image settings from the mode, base and the terminal's cell geometry, half-blocks by default
fn resolve_images(mode: ImageMode, base: Option<String>) -> ImageOptions {
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
        mode,
        base,
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
