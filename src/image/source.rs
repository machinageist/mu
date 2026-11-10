// Author: machinageist
// Date: 2026-11-09
// Description: Resolve an image source to bytes: local path, data: URI, or http(s) in fetch mode
// Notes: Remote images go through the system curl, never a shell, and only in ImageMode::Fetch;
// every result is capped at MAX_SOURCE_BYTES

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{ImageMode, ImageOptions, MAX_SOURCE_BYTES};

// Source prefixes, matched without regard to case
const DATA_PREFIX: &str = "data:";
const FILE_PREFIX: &str = "file://";
const HTTP_PREFIX: &str = "http://";
const HTTPS_PREFIX: &str = "https://";
const PROTOCOL_RELATIVE_PREFIX: &str = "//";
const DEFAULT_SCHEME: &str = "https";
const SCHEME_SEPARATOR: &str = "://";

// data: URI pieces
const IMAGE_MEDIA_PREFIX: &str = "image/";
const BASE64_SUFFIX: &str = ";base64";
const DATA_SEPARATOR: char = ',';

// URL and path pieces
const PATH_SEPARATOR: &str = "/";
const QUERY_OR_FRAGMENT: [char; 2] = ['?', '#'];
const PARENT_SEGMENT: &str = "..";
const CURRENT_SEGMENT: &str = ".";
const PERCENT: u8 = b'%';
const HEX_RADIX: u32 = 16;
const OPTION_DASH: char = '-';

// curl invocation, the URL follows as its own argument after the end-of-options marker
const CURL_PROGRAM: &str = "curl";
// Redirects are limited to http(s) too, so a page cannot bounce curl to ftp or file
const CURL_FLAGS: &[&str] = &[
    "-fsSL",
    "--proto",
    "=http,https",
    "--proto-redir",
    "=http,https",
    "--max-time",
    "10",
];
const CURL_MAX_FILESIZE: &str = "--max-filesize";
const CURL_END_OF_OPTIONS: &str = "--";

// Read the bytes an image source points at, honouring the image mode and size cap
pub(crate) fn fetch(src: &str, options: &ImageOptions) -> Option<Vec<u8>> {
    if options.mode == ImageMode::Never {
        return None;
    }
    let src = src.trim();
    if has_prefix(src, DATA_PREFIX) {
        return data_uri(&src[DATA_PREFIX.len()..]);
    }
    if is_url(src) {
        return remote(src, options.mode);
    }
    if src.starts_with(PROTOCOL_RELATIVE_PREFIX) {
        return remote(&format!("{DEFAULT_SCHEME}:{src}"), options.mode);
    }
    if has_prefix(src, FILE_PREFIX) {
        return local(Path::new(""), &src[FILE_PREFIX.len()..]);
    }
    match options.base.as_deref() {
        Some(base) if is_url(base) => remote(&join_url(base, src)?, options.mode),
        Some(base) => local(&base_dir(Path::new(base)), src),
        None => local(Path::new(""), src),
    }
}

// Check for a prefix ignoring ASCII case
fn has_prefix(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

// Check whether text is an http(s) URL
fn is_url(text: &str) -> bool {
    has_prefix(text, HTTP_PREFIX) || has_prefix(text, HTTPS_PREFIX)
}

// Keep bytes only when they fit the source cap
fn capped(bytes: Vec<u8>) -> Option<Vec<u8>> {
    (bytes.len() <= MAX_SOURCE_BYTES).then_some(bytes)
}

// Decode the part of a data: URI after the scheme, images only
fn data_uri(rest: &str) -> Option<Vec<u8>> {
    let (meta, payload) = rest.split_once(DATA_SEPARATOR)?;
    if !has_prefix(meta, IMAGE_MEDIA_PREFIX) {
        return None;
    }
    let is_base64 = meta.len() >= BASE64_SUFFIX.len()
        && meta[meta.len() - BASE64_SUFFIX.len()..].eq_ignore_ascii_case(BASE64_SUFFIX);
    let bytes = if is_base64 {
        super::base64::decode(payload)?
    } else {
        percent_decode(payload)
    };
    capped(bytes)
}

// Decode %XX escapes, leaving malformed ones as they are
pub(crate) fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == PERCENT)
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, HEX_RADIX).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    out
}

// Cut a ?query or #fragment off the end
fn strip_query(text: &str) -> &str {
    text.split(QUERY_OR_FRAGMENT).next().unwrap_or(text)
}

// Use a base path as a directory, or its parent when it names a file
fn base_dir(base: &Path) -> PathBuf {
    match base.parent() {
        Some(parent) if base.is_file() => parent.to_path_buf(),
        _ => base.to_path_buf(),
    }
}

// Read a local file relative to a directory, with the size checked before reading
fn local(dir: &Path, src: &str) -> Option<Vec<u8>> {
    let decoded = String::from_utf8(percent_decode(strip_query(src))).ok()?;
    let path = dir.join(decoded);
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES as u64 {
        return None;
    }
    capped(std::fs::read(&path).ok()?)
}

// Resolve a relative reference against an http(s) base URL, for the common cases
fn join_url(base: &str, relative: &str) -> Option<String> {
    if is_url(relative) {
        return Some(relative.to_string());
    }
    if relative.starts_with(PROTOCOL_RELATIVE_PREFIX) {
        let scheme = base.split(SCHEME_SEPARATOR).next()?;
        return Some(format!("{scheme}:{relative}"));
    }
    let host_start = base.find(SCHEME_SEPARATOR)? + SCHEME_SEPARATOR.len();
    let base = strip_query(base);
    let path_start = base[host_start..]
        .find(PATH_SEPARATOR)
        .map_or(base.len(), |offset| host_start + offset);
    let origin = &base[..path_start];
    if relative.starts_with(PATH_SEPARATOR) {
        return Some(format!("{origin}{relative}"));
    }

    // Walk the relative segments from the base's directory
    let base_path = &base[path_start..];
    let base_dir = base_path
        .rsplit_once(PATH_SEPARATOR)
        .map_or("", |(dir, _)| dir);
    let mut segments: Vec<&str> = base_dir
        .split(PATH_SEPARATOR)
        .filter(|segment| !segment.is_empty())
        .collect();
    let path_end = relative.find(QUERY_OR_FRAGMENT).unwrap_or(relative.len());
    let (relative_path, suffix) = relative.split_at(path_end);
    let relative_segments: Vec<&str> = relative_path.split(PATH_SEPARATOR).collect();
    for segment in &relative_segments {
        match *segment {
            PARENT_SEGMENT => {
                segments.pop();
            }
            CURRENT_SEGMENT | "" => {}
            other => segments.push(other),
        }
    }

    // Keep a trailing slash when the reference names a directory
    let trailing = relative_segments
        .last()
        .is_some_and(|last| [PARENT_SEGMENT, CURRENT_SEGMENT, ""].contains(last))
        && !segments.is_empty();
    let slash = if trailing { PATH_SEPARATOR } else { "" };
    let path = segments.join(PATH_SEPARATOR);
    Some(format!("{origin}{PATH_SEPARATOR}{path}{slash}{suffix}"))
}

// Check that a URL is safe to hand to curl as a single argument
fn is_safe_url(url: &str) -> bool {
    !url.is_empty()
        && !url.starts_with(OPTION_DASH)
        && !url
            .chars()
            .any(|symbol| symbol.is_whitespace() || symbol.is_control())
}

// Download a URL with the system curl, only in fetch mode
fn remote(url: &str, mode: ImageMode) -> Option<Vec<u8>> {
    if mode != ImageMode::Fetch || !is_url(url) || !is_safe_url(url) {
        return None;
    }
    let output = Command::new(CURL_PROGRAM)
        .args(CURL_FLAGS)
        .arg(CURL_MAX_FILESIZE)
        .arg(MAX_SOURCE_BYTES.to_string())
        .arg(CURL_END_OF_OPTIONS)
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    capped(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const PNG_MAGIC: &[u8] = b"\x89PNG";
    const PNG_DATA_URI: &str = "data:image/png;base64,iVBORw==";
    const SVG_DATA_URI: &str = "data:image/svg+xml,%3Csvg%20width%3D%221%22%2F%3E";
    const SVG_TEXT: &[u8] = b"<svg width=\"1\"/>";
    const TEXT_DATA_URI: &str = "data:text/plain;base64,aGk=";
    const FILE_BYTES: &[u8] = b"pixels";

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    // Temporary directory removed on drop
    struct TempDir(PathBuf);

    impl TempDir {
        // Create a unique empty directory under the system temp dir
        fn new() -> TempDir {
            let unique = format!(
                "mu-source-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        // Write a file inside the directory
        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            path
        }

        // Base option pointing at this directory
        fn base(&self) -> Option<String> {
            Some(self.0.to_string_lossy().into_owned())
        }
    }

    impl Drop for TempDir {
        // Remove the directory and everything in it
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // Options with a mode and base
    fn options(mode: ImageMode, base: Option<String>) -> ImageOptions {
        ImageOptions {
            mode,
            base,
            ..ImageOptions::default()
        }
    }

    #[test]
    fn never_mode_loads_nothing() {
        let never = options(ImageMode::Never, None);
        assert_eq!(fetch(PNG_DATA_URI, &never), None);
    }

    #[test]
    fn decodes_base64_and_percent_data_uris() {
        let auto = options(ImageMode::Auto, None);
        assert_eq!(fetch(PNG_DATA_URI, &auto).as_deref(), Some(PNG_MAGIC));
        assert_eq!(fetch(SVG_DATA_URI, &auto).as_deref(), Some(SVG_TEXT));
    }

    #[test]
    fn rejects_non_image_data_uri() {
        let auto = options(ImageMode::Auto, None);
        assert_eq!(fetch(TEXT_DATA_URI, &auto), None);
    }

    #[test]
    fn resolves_relative_path_against_base_dir() {
        let dir = TempDir::new();
        dir.write("pic.png", FILE_BYTES);
        let auto = options(ImageMode::Auto, dir.base());
        assert_eq!(fetch("pic.png", &auto).as_deref(), Some(FILE_BYTES));
        assert_eq!(fetch("missing.png", &auto), None);
    }

    #[test]
    fn file_base_uses_its_parent() {
        let dir = TempDir::new();
        dir.write("pic.png", FILE_BYTES);
        let page = dir.write("page.html", b"<img>");
        let auto = options(ImageMode::Auto, Some(page.to_string_lossy().into_owned()));
        assert_eq!(fetch("pic.png", &auto).as_deref(), Some(FILE_BYTES));
    }

    #[test]
    fn strips_query_and_fragment() {
        let dir = TempDir::new();
        dir.write("pic.png", FILE_BYTES);
        let auto = options(ImageMode::Auto, dir.base());
        assert_eq!(fetch("pic.png?v=2", &auto).as_deref(), Some(FILE_BYTES));
        assert_eq!(fetch("pic.png#top", &auto).as_deref(), Some(FILE_BYTES));
    }

    #[test]
    fn percent_decodes_file_names_and_file_urls() {
        let dir = TempDir::new();
        let path = dir.write("my pic.png", FILE_BYTES);
        let auto = options(ImageMode::Auto, dir.base());
        assert_eq!(fetch("my%20pic.png", &auto).as_deref(), Some(FILE_BYTES));
        let url = format!("file://{}", path.to_string_lossy().replace(' ', "%20"));
        let no_base = options(ImageMode::Auto, None);
        assert_eq!(fetch(&url, &no_base).as_deref(), Some(FILE_BYTES));
    }

    #[test]
    fn rejects_oversize_file() {
        let dir = TempDir::new();
        let file = File::create(dir.0.join("huge.png")).unwrap();
        file.set_len(MAX_SOURCE_BYTES as u64 + 1).unwrap();
        let auto = options(ImageMode::Auto, dir.base());
        assert_eq!(fetch("huge.png", &auto), None);
    }

    #[test]
    fn rejects_directory() {
        let dir = TempDir::new();
        fs::create_dir(dir.0.join("sub")).unwrap();
        let auto = options(ImageMode::Auto, dir.base());
        assert_eq!(fetch("sub", &auto), None);
    }

    #[test]
    fn remote_needs_fetch_mode() {
        let auto = options(ImageMode::Auto, None);
        assert_eq!(fetch("https://example.invalid/a.png", &auto), None);
        assert_eq!(fetch("//example.invalid/a.png", &auto), None);
        let url_base = options(ImageMode::Auto, Some("https://example.invalid/".into()));
        assert_eq!(fetch("a.png", &url_base), None);
    }

    #[test]
    fn rejects_unsafe_urls() {
        assert!(is_safe_url("https://example.com/a.png"));
        assert!(!is_safe_url("-K/etc/passwd"));
        assert!(!is_safe_url("https://example.com/a b.png"));
        assert!(!is_safe_url("https://example.com/a\n.png"));
        let fetch_mode = options(ImageMode::Fetch, None);
        assert_eq!(fetch("https://example.invalid/a b.png", &fetch_mode), None);
    }

    #[test]
    fn joins_urls_like_a_browser() {
        let base = "https://example.com/docs/guide/page.html?x=1";
        let cases = [
            ("pic.png", "https://example.com/docs/guide/pic.png"),
            (
                "./img/pic.png",
                "https://example.com/docs/guide/img/pic.png",
            ),
            ("../pic.png", "https://example.com/docs/pic.png"),
            ("../../../pic.png", "https://example.com/pic.png"),
            ("/root.png", "https://example.com/root.png"),
            ("pic.png?v=2", "https://example.com/docs/guide/pic.png?v=2"),
            ("//cdn.example.com/a.png", "https://cdn.example.com/a.png"),
            ("http://other.com/b.png", "http://other.com/b.png"),
        ];
        for (relative, expected) in cases {
            assert_eq!(join_url(base, relative).as_deref(), Some(expected));
        }
        assert_eq!(
            join_url("https://example.com", "pic.png").as_deref(),
            Some("https://example.com/pic.png")
        );
        assert_eq!(
            join_url("https://example.com/docs/", "pic.png").as_deref(),
            Some("https://example.com/docs/pic.png")
        );
    }
}
