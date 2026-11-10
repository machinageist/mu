// Author: machinageist
// Date: 2026-11-09
// Description: `mu <url>` fetches through curl and presents the response
// Notes: A local TcpListener serves canned responses, so no test touches the network;
// each URL's output is compared with the same body piped in with an explicit format

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::thread;

const BIN: &str = env!("CARGO_BIN_EXE_mu");
const EXIT_FAILURE: i32 = 1;
const EXIT_USAGE: i32 = 2;
const COMMON_ARGS: [&str; 4] = ["-w", "60", "--color", "never"];

const HTML_BODY: &str = "<h1>Hello</h1><p>From the <b>server</b>.</p>";
const JSON_BODY: &str = "{\"name\":\"mu\",\"tags\":[1,2]}";
const MISSING_BODY: &str = "# Not here\n";
const OUTLINE_BODY: &str = "# Guide\n\nIntro.\n\n## Install\n\n## Use\n";
// The hand-written fixture PDF is plain ASCII, so it can be served as text
const PDF_BODY: &str = include_str!("fixtures/sample.pdf");
const PDF_FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.pdf");

// Answer a request path with (status line, content type, body)
fn route(path: &str) -> (&'static str, &'static str, &'static str) {
    match path {
        "/page" => ("200 OK", "text/html; charset=utf-8", HTML_BODY),
        "/data" => ("200 OK", "application/json", JSON_BODY),
        "/guide" => ("200 OK", "text/markdown", OUTLINE_BODY),
        "/paper" => ("200 OK", "application/pdf", PDF_BODY),
        "/old" => ("301 Moved Permanently", "text/plain", ""),
        _ => ("404 Not Found", "text/markdown", MISSING_BODY),
    }
}

// Serve one connection: read the request head, write a canned response
fn answer(stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("request line");
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let (status, content_type, body) = route(path);
    let location = if path == "/old" {
        "Location: /page\r\n"
    } else {
        ""
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{location}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = stream;
    let _ = stream.write_all(response.as_bytes());
}

// Start a server on a free port and return its base URL
fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            answer(stream);
        }
    });
    format!("http://{address}")
}

// Run mu and return (exit code, stdout, stderr)
fn run(args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(BIN)
        .args(COMMON_ARGS)
        .args(args)
        .env("no_proxy", "*")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mu");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for mu");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn html_is_chosen_by_content_type() {
    let url = format!("{}/page", serve());
    let (code, out, _) = run(&[&url], "");
    let (_, expected, _) = run(&["-f", "html"], HTML_BODY);
    assert_eq!(code, 0);
    assert_eq!(out, expected);
    assert!(out.contains("Hello"));
}

#[test]
fn json_is_chosen_by_content_type() {
    let url = format!("{}/data", serve());
    let (code, out, _) = run(&[&url], "");
    let (_, expected, _) = run(&["-f", "json"], JSON_BODY);
    assert_eq!(code, 0);
    assert_eq!(out, expected);
}

#[test]
fn redirects_are_followed() {
    let url = format!("{}/old", serve());
    let (code, out, _) = run(&[&url], "");
    let (_, expected, _) = run(&["-f", "html"], HTML_BODY);
    assert_eq!(code, 0);
    assert_eq!(out, expected);
}

#[test]
fn http_errors_render_the_body_and_exit_1() {
    let url = format!("{}/nope", serve());
    let (code, out, err) = run(&[&url], "");
    assert_eq!(code, EXIT_FAILURE);
    assert!(out.contains("Not here"));
    assert!(err.contains("HTTP 404"), "{err}");
}

#[test]
fn head_shows_status_and_headers_only() {
    let url = format!("{}/page", serve());
    let (code, out, _) = run(&["-I", &url], "");
    assert_eq!(code, 0);
    assert!(out.contains("HTTP/1.1 200 OK"), "{out}");
    assert!(out.contains("Content-Type"), "{out}");
    assert!(!out.contains("Hello"), "{out}");
    assert_eq!(out.matches("Content-Type").count(), 1, "{out}");
}

#[test]
fn include_shows_headers_then_body() {
    let url = format!("{}/page", serve());
    let (code, out, _) = run(&["-i", &url], "");
    assert_eq!(code, 0);
    let headers = out.find("Content-Type").expect("headers shown");
    let body = out.find("Hello").expect("body shown");
    assert!(headers < body);
}

#[test]
fn transport_failure_exits_1_with_a_message() {
    // Bind and drop a listener so the port is very likely closed
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port")
        .port();
    let (code, out, err) = run(&[&format!("http://127.0.0.1:{port}/")], "");
    assert_eq!(code, EXIT_FAILURE);
    assert!(out.is_empty());
    assert!(err.starts_with("mu: http://127.0.0.1"), "{err}");
}

#[test]
fn url_flags_without_a_url_are_a_usage_error() {
    let (code, _, err) = run(&["-I"], "# x\n");
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("-I only works with a URL"), "{err}");
    let (code, _, _) = run(&["-H", "A: b", "README.md"], "");
    assert_eq!(code, EXIT_USAGE);
}

#[test]
fn toc_lists_the_headings_of_a_fetched_page() {
    let url = format!("{}/guide", serve());
    let (code, out, _) = run(&["--toc", &url], "");
    assert_eq!(code, 0);
    assert_eq!(out, "Guide\n  Install\n  Use\n");
}

#[test]
fn a_pdf_url_matches_the_file() {
    if Command::new("pdftotext")
        .arg("-v")
        .stderr(Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("skipped: pdftotext is not installed");
        return;
    }
    let url = format!("{}/paper", serve());
    let from_url = run(&[&url], "");
    assert_eq!(from_url.0, 0, "{}", from_url.2);
    assert_eq!(from_url, run(&[PDF_FIXTURE], ""));
}
