// Author: machinageist
// Date: 2026-11-09
// Description: Fetch a URL through the system curl with curl-like options, and rewrite page URLs to raw ones
// Notes: No shell is involved and the URL is always one argument after `--`; response headers come
// back on stdout ahead of the body (`-D -`) and the final URL on stderr through `--write-out`

use std::process::{Command, Stdio};

// curl invocation shared by every request
const CURL_PROGRAM: &str = "curl";
const CURL_BASE_ARGS: [&str; 11] = [
    "-sS",
    "-L",
    "--proto",
    "=http,https",
    "--proto-redir",
    "=http,https",
    "--connect-timeout",
    "10",
    "--compressed",
    "-D",
    "-",
];
const WRITE_OUT_FLAG: &str = "-w";
const WRITE_OUT_MARKER: &str = "mu-final-url:";
const WRITE_OUT_FORMAT: &str = "%{stderr}mu-final-url:%{url_effective}\\n";
const END_OF_OPTIONS: &str = "--";

// curl flags mu exposes natively
const FLAG_USER_AGENT: &str = "-A";
const FLAG_HEADER: &str = "-H";
const FLAG_REQUEST: &str = "-X";
const FLAG_DATA: &str = "--data";
const FLAG_USER: &str = "-u";
const FLAG_INSECURE: &str = "-k";
const FLAG_HEAD: &str = "-I";
// With -I curl also writes the headers as its output, which would repeat the -D - copy
const FLAG_OUTPUT: &str = "-o";
const DISCARD_PATH: &str = "/dev/null";
const FLAG_MAX_TIME: &str = "--max-time";
const DEFAULT_USER_AGENT_PREFIX: &str = "mu/";

// Response parsing
const STATUS_PREFIX: &[u8] = b"HTTP/";
const BLOCK_ENDS: [&[u8]; 2] = [b"\r\n\r\n", b"\n\n"];
const LOCATION_HEADER: &str = "location";
const CONTENT_TYPE_HEADER: &str = "content-type";
const PROXY_ESTABLISHED: &str = "connection established";
const STATUS_INFORMATIONAL: u16 = 100;
const STATUS_SUCCESS: u16 = 200;
const STATUS_REDIRECT: u16 = 300;
const STATUS_CLIENT_ERROR: u16 = 400;

// URL schemes mu opens
const URL_SCHEMES: [&str; 2] = ["http://", "https://"];
const HTTPS_SCHEME: &str = "https://";
const SCHEME_SEPARATOR: &str = "://";

// Raw rewrites for code hosts
const GITHUB_HOSTS: [&str; 2] = ["github.com", "www.github.com"];
const GITHUB_RAW_HOST: &str = "raw.githubusercontent.com";
const GITHUB_BLOB: &str = "blob";
const GITLAB_HOST: &str = "gitlab.com";
const GITLAB_BLOB: &str = "/-/blob/";
const GITLAB_RAW: &str = "/-/raw/";
const GIST_HOST: &str = "gist.github.com";
const GIST_RAW_HOST: &str = "gist.githubusercontent.com";
const GIST_RAW_SUFFIX: &str = "/raw";

// Header table text, written as Markdown for the shared renderer
const TABLE_HEAD: &str = "| Header | Value |\n|---|---|\n";

#[derive(Clone, Debug, Default)]
pub struct Request {
    pub url: String,
    pub headers: Vec<String>,
    pub method: Option<String>,
    pub data: Vec<String>,
    pub user_agent: Option<String>,
    pub user: Option<String>,
    pub insecure: bool,
    pub head: bool,
    pub timeout: Option<String>,
    // Extra arguments passed to curl verbatim
    pub extra: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub status_line: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub final_url: String,
}

impl Response {
    // Look up a header of the final response, case-insensitively
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    // The response's Content-Type, if any
    pub fn content_type(&self) -> Option<&str> {
        self.header(CONTENT_TYPE_HEADER)
    }

    // Check for a 4xx or 5xx status
    pub fn is_error(&self) -> bool {
        self.status >= STATUS_CLIENT_ERROR
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FetchError {
    // curl is not installed
    Missing,
    // curl ran and failed, with its message
    Failed(String),
}

// Check whether a command-line argument is an http(s) URL
pub fn is_url(arg: &str) -> bool {
    URL_SCHEMES.iter().any(|scheme| {
        arg.get(..scheme.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
    })
}

// Build the curl argument list for a request
pub fn curl_args(request: &Request, version: &str) -> Vec<String> {
    let mut args: Vec<String> = CURL_BASE_ARGS.iter().map(|arg| (*arg).to_owned()).collect();
    args.extend([WRITE_OUT_FLAG.to_owned(), WRITE_OUT_FORMAT.to_owned()]);
    let user_agent = request
        .user_agent
        .clone()
        .unwrap_or_else(|| format!("{DEFAULT_USER_AGENT_PREFIX}{version}"));
    args.extend([FLAG_USER_AGENT.to_owned(), user_agent]);
    for header in &request.headers {
        args.extend([FLAG_HEADER.to_owned(), header.clone()]);
    }
    if let Some(method) = &request.method {
        args.extend([FLAG_REQUEST.to_owned(), method.clone()]);
    }
    for data in &request.data {
        args.extend([FLAG_DATA.to_owned(), data.clone()]);
    }
    if let Some(user) = &request.user {
        args.extend([FLAG_USER.to_owned(), user.clone()]);
    }
    if request.insecure {
        args.push(FLAG_INSECURE.to_owned());
    }
    if request.head {
        args.extend([
            FLAG_HEAD.to_owned(),
            FLAG_OUTPUT.to_owned(),
            DISCARD_PATH.to_owned(),
        ]);
    }
    if let Some(timeout) = &request.timeout {
        args.extend([FLAG_MAX_TIME.to_owned(), timeout.clone()]);
    }
    args.extend(request.extra.iter().cloned());
    args.extend([END_OF_OPTIONS.to_owned(), request.url.clone()]);
    args
}

// Run curl for a request and split its output into the final response
pub fn fetch(request: &Request, version: &str) -> Result<Response, FetchError> {
    let output = Command::new(CURL_PROGRAM)
        .args(curl_args(request, version))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => FetchError::Missing,
            _ => FetchError::Failed(error.to_string()),
        })?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut final_url = request.url.clone();
    let mut messages = Vec::new();
    for line in stderr.lines() {
        match line.strip_prefix(WRITE_OUT_MARKER) {
            Some(url) if !url.is_empty() => final_url = url.to_owned(),
            Some(_) => {}
            None => messages.push(line),
        }
    }
    if !output.status.success() {
        return Err(FetchError::Failed(messages.join("\n")));
    }
    let mut response = parse_response(&output.stdout);
    response.final_url = final_url;
    Ok(response)
}

// Split curl's `-D -` output into the final header block and the body
pub fn parse_response(stdout: &[u8]) -> Response {
    let mut rest = stdout;
    let mut response = Response {
        status: 0,
        status_line: String::new(),
        headers: Vec::new(),
        body: Vec::new(),
        final_url: String::new(),
    };
    while rest.starts_with(STATUS_PREFIX) {
        let Some((block, after)) = split_block(rest) else {
            break;
        };
        rest = after;
        let (status_line, status, headers) = parse_block(block);
        let skipped = is_interim(&status_line, status, &headers);
        response.status_line = status_line;
        response.status = status;
        response.headers = headers;
        if !skipped {
            break;
        }
    }
    response.body = rest.to_vec();
    response
}

// Cut one header block off the front, at the first blank line
fn split_block(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    BLOCK_ENDS
        .iter()
        .filter_map(|end| {
            bytes
                .windows(end.len())
                .position(|window| window == *end)
                .map(|at| (at, end.len()))
        })
        .min()
        .map(|(at, len)| (&bytes[..at], &bytes[at + len..]))
}

// Read a status line and its headers
fn parse_block(block: &[u8]) -> (String, u16, Vec<(String, String)>) {
    let text = String::from_utf8_lossy(block);
    let mut lines = text.lines();
    let status_line = lines.next().unwrap_or_default().trim().to_owned();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect();
    (status_line, status, headers)
}

// Detect blocks curl prints before the real response: 1xx, followed redirects, proxy tunnels
fn is_interim(status_line: &str, status: u16, headers: &[(String, String)]) -> bool {
    let informational = (STATUS_INFORMATIONAL..STATUS_SUCCESS).contains(&status);
    let redirect = (STATUS_REDIRECT..STATUS_CLIENT_ERROR).contains(&status)
        && headers
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case(LOCATION_HEADER));
    let tunnel = status_line.to_ascii_lowercase().contains(PROXY_ESTABLISHED);
    informational || redirect || tunnel
}

// Rewrite a code-host page URL to the raw file it shows, or None to fetch it as typed
pub fn raw_url(url: &str) -> Option<String> {
    if !is_url(url) {
        return None;
    }
    let (_, rest) = url.split_once(SCHEME_SEPARATOR)?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

    // github.com/OWNER/REPO/blob/REF/PATH
    if GITHUB_HOSTS.contains(&host.as_str()) {
        let [owner, repo, kind, reference, file @ ..] = segments.as_slice() else {
            return None;
        };
        if *kind != GITHUB_BLOB || file.is_empty() {
            return None;
        }
        return Some(format!(
            "{HTTPS_SCHEME}{GITHUB_RAW_HOST}/{owner}/{repo}/{reference}/{}",
            file.join("/")
        ));
    }

    // gitlab.com/NAMESPACE/.../-/blob/REF/PATH
    if host == GITLAB_HOST {
        let path = format!("/{path}");
        return path.contains(GITLAB_BLOB).then(|| {
            format!(
                "{HTTPS_SCHEME}{GITLAB_HOST}{}",
                path.replacen(GITLAB_BLOB, GITLAB_RAW, 1)
            )
        });
    }

    // gist.github.com/USER/ID
    if host == GIST_HOST {
        let [user, id] = segments.as_slice() else {
            return None;
        };
        return Some(format!(
            "{HTTPS_SCHEME}{GIST_RAW_HOST}/{user}/{id}{GIST_RAW_SUFFIX}"
        ));
    }
    None
}

// Write the status line and headers as a Markdown table for the shared renderer
pub fn header_table(response: &Response) -> String {
    let mut out = format!("**{}**\n\n", escape_markdown(&response.status_line));
    if response.headers.is_empty() {
        return out;
    }
    out.push_str(TABLE_HEAD);
    for (key, value) in &response.headers {
        out.push_str(&format!(
            "| {} | {} |\n",
            escape_markdown(key),
            escape_markdown(value)
        ));
    }
    out
}

// Backslash-escape ASCII punctuation so header text renders literally, pipes included
fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = "9.9.9";

    #[test]
    fn urls_are_recognised_by_scheme_only() {
        assert!(is_url("https://example.com"));
        assert!(is_url("HTTP://example.com/x"));
        assert!(!is_url("example.com"));
        assert!(!is_url("./http://x"));
        assert!(!is_url("ftp://x"));
        assert!(!is_url("htt"));
    }

    #[test]
    fn curl_args_put_options_before_the_url() {
        let request = Request {
            url: "https://x.io/a".to_owned(),
            headers: vec!["Accept: text/html".to_owned()],
            method: Some("POST".to_owned()),
            data: vec!["a=1".to_owned()],
            user: Some("me:pw".to_owned()),
            insecure: true,
            head: true,
            timeout: Some("5".to_owned()),
            extra: vec!["--http1.1".to_owned()],
            ..Request::default()
        };
        let args = curl_args(&request, VERSION);
        let tail: Vec<&str> = args
            .iter()
            .map(String::as_str)
            .skip(CURL_BASE_ARGS.len())
            .collect();
        assert_eq!(
            tail,
            [
                "-w",
                WRITE_OUT_FORMAT,
                "-A",
                "mu/9.9.9",
                "-H",
                "Accept: text/html",
                "-X",
                "POST",
                "--data",
                "a=1",
                "-u",
                "me:pw",
                "-k",
                "-I",
                "-o",
                "/dev/null",
                "--max-time",
                "5",
                "--http1.1",
                "--",
                "https://x.io/a"
            ]
        );
    }

    #[test]
    fn redirect_chains_and_continues_are_skipped() {
        let raw = b"HTTP/1.1 301 Moved\r\nLocation: /b\r\n\r\nHTTP/1.1 100 Continue\r\n\r\nHTTP/2 200\r\ncontent-type: text/html\r\n\r\n<p>hi</p>";
        let response = parse_response(raw);
        assert_eq!(response.status, 200);
        assert_eq!(response.status_line, "HTTP/2 200");
        assert_eq!(response.content_type(), Some("text/html"));
        assert_eq!(response.body, b"<p>hi</p>");
    }

    #[test]
    fn a_body_that_looks_like_headers_stays_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nHTTP/1.1 is a protocol\r\n\r\nmore";
        let response = parse_response(raw);
        assert_eq!(response.body, b"HTTP/1.1 is a protocol\r\n\r\nmore");
    }

    #[test]
    fn proxy_tunnel_block_is_skipped_and_errors_are_flagged() {
        let raw = b"HTTP/1.1 200 Connection established\r\n\r\nHTTP/1.1 404 Not Found\r\n\r\ngone";
        let response = parse_response(raw);
        assert_eq!(response.status, 404);
        assert!(response.is_error());
        assert_eq!(response.body, b"gone");
    }

    #[test]
    fn head_requests_have_an_empty_body() {
        let response = parse_response(b"HTTP/2 200\nserver: x\n\n");
        assert_eq!(response.header("Server"), Some("x"));
        assert!(response.body.is_empty());
    }

    #[test]
    fn code_host_pages_rewrite_to_raw() {
        assert_eq!(
            raw_url("https://github.com/machinageist/mu/blob/main/README.md?plain=1#L3").as_deref(),
            Some("https://raw.githubusercontent.com/machinageist/mu/main/README.md")
        );
        assert_eq!(
            raw_url("https://gitlab.com/group/sub/proj/-/blob/v1/docs/a.adoc").as_deref(),
            Some("https://gitlab.com/group/sub/proj/-/raw/v1/docs/a.adoc")
        );
        assert_eq!(
            raw_url("https://gist.github.com/someone/abc123").as_deref(),
            Some("https://gist.githubusercontent.com/someone/abc123/raw")
        );
        assert_eq!(raw_url("https://github.com/machinageist/mu"), None);
        assert_eq!(raw_url("https://github.com/o/r/tree/main/src"), None);
        assert_eq!(raw_url("https://example.com/a/blob/b/c"), None);
    }

    #[test]
    fn header_table_escapes_markdown() {
        let response = parse_response(b"HTTP/2 200\r\nx-weird: a|b *c*\r\n\r\n");
        assert_eq!(
            header_table(&response),
            "**HTTP\\/2 200**\n\n| Header | Value |\n|---|---|\n| x\\-weird | a\\|b \\*c\\* |\n"
        );
    }
}
