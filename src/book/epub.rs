// Author: machinageist
// Date: 2026-11-10
// Description: Turn an EPUB into one HTML document: metadata, then each spine chapter's body
// Notes: Images are inlined as data: URIs so the image pipeline draws them without the zip,
// and links inside the book lose their href so no internal path is printed after the text

use std::collections::HashMap;

use super::zip::Archive;
use crate::image::base64;
use crate::image::source::percent_decode;

// Package files
const CONTAINER_PATH: &str = "META-INF/container.xml";
const ROOTFILE: &str = "rootfile";
const FULL_PATH: &str = "full-path";

// Package document elements and attributes, matched by local name
const ITEM: &str = "item";
const ITEMREF: &str = "itemref";
const ID: &str = "id";
const IDREF: &str = "idref";
const HREF: &str = "href";
const MEDIA_TYPE: &str = "media-type";
const TITLE: &str = "title";
const CREATOR: &str = "creator";
const CHAPTER_TYPES: [&str; 2] = ["application/xhtml+xml", "text/html"];

// Chapter elements that need rewriting
const BODY: &str = "body";
const IMG: &str = "img";
const IMAGE: &str = "image";
const SVG: &str = "svg";
const A: &str = "a";
const SRC: &str = "src";
const ALT: &str = "alt";
const XLINK_HREF: &str = "xlink:href";
const EXTERNAL_SCHEMES: [&str; 3] = ["http://", "https://", "mailto:"];

// Image media types by extension, for manifests that leave them out
const IMAGE_TYPES: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];
const IMAGE_PREFIX: &str = "image/";

// Markup skipped while scanning for tags
const SKIPPED: [(&str, &str); 4] = [
    ("<!--", "-->"),
    ("<![CDATA[", "]]>"),
    ("<?", "?>"),
    ("<!", ">"),
];

// Path pieces
const SEPARATOR: &str = "/";
const PARENT: &str = "..";
const CURRENT: &str = ".";
const QUERY_OR_FRAGMENT: [char; 2] = ['?', '#'];
const SCHEME_MARK: char = ':';
const NAMESPACE_MARK: char = ':';

// Prefix on every error, as the CLI shows it
const ERROR_PREFIX: &str = "not a readable EPUB: ";

// One manifest entry
struct Item {
    path: String,
    media_type: String,
}

// A tag found while scanning markup
struct Tag<'a> {
    name: &'a str,
    end: bool,
    attrs: Vec<(&'a str, &'a str)>,
    start: usize,
    stop: usize,
}

impl Tag<'_> {
    // The name without a namespace prefix, lowercased
    fn local(&self) -> String {
        local_name(self.name)
    }

    // Look up an attribute by its exact name, ignoring case
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| *value)
    }
}

// Convert an EPUB's bytes into one HTML document
pub fn html(bytes: &[u8]) -> Result<String, String> {
    convert(bytes).map_err(|reason| format!("{ERROR_PREFIX}{reason}"))
}

// Read the package and join the spine's chapters
fn convert(bytes: &[u8]) -> Result<String, String> {
    let archive = Archive::open(bytes)?;
    let container = text(&archive.read(CONTAINER_PATH)?);
    let package_path = tags(&container)
        .find(|tag| !tag.end && tag.local() == ROOTFILE)
        .and_then(|tag| tag.attr(FULL_PATH).map(str::to_owned))
        .ok_or("no package document")?;
    let package = text(&archive.read(&package_path)?);
    let mut manifest = HashMap::new();
    let mut spine = Vec::new();
    for tag in tags(&package).filter(|tag| !tag.end) {
        match tag.local().as_str() {
            ITEM => {
                if let (Some(id), Some(href)) = (tag.attr(ID), tag.attr(HREF)) {
                    let item = Item {
                        path: resolve(&package_path, href).unwrap_or_default(),
                        media_type: tag.attr(MEDIA_TYPE).unwrap_or_default().to_owned(),
                    };
                    manifest.insert(id.to_owned(), item);
                }
            }
            ITEMREF => spine.extend(tag.attr(IDREF).map(str::to_owned)),
            _ => {}
        }
    }
    if spine.is_empty() {
        return Err("the spine is empty".to_owned());
    }
    let media_types: HashMap<&str, &str> = manifest
        .values()
        .map(|item| (item.path.as_str(), item.media_type.as_str()))
        .collect();
    let mut out = String::from("<html><body>\n");
    out.push_str(&front_matter(&package));
    for id in &spine {
        let Some(item) = manifest.get(id) else {
            continue;
        };
        if !CHAPTER_TYPES.contains(&item.media_type.to_ascii_lowercase().as_str()) {
            continue;
        }
        let Ok(chapter) = archive.read(&item.path) else {
            continue;
        };
        let chapter = text(&chapter);
        out.push_str(&rewrite(body(&chapter), &item.path, &archive, &media_types));
        out.push('\n');
    }
    out.push_str("</body></html>\n");
    Ok(out)
}

// Decode bytes as UTF-8, replacing anything invalid
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// The title as a heading and the creators as a byline, still escaped as in the package
fn front_matter(package: &str) -> String {
    let mut out = String::new();
    if let Some(title) = elements_text(package, TITLE).into_iter().next() {
        out.push_str(&format!("<h1>{title}</h1>\n"));
    }
    let creators = elements_text(package, CREATOR);
    if !creators.is_empty() {
        out.push_str(&format!("<p><em>{}</em></p>\n", creators.join(", ")));
    }
    out
}

// The trimmed, non-empty text of every element with this local name
fn elements_text(markup: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut scan = tags(markup).peekable();
    while let Some(tag) = scan.next() {
        if tag.end || tag.local() != name {
            continue;
        }
        let stop = scan.peek().map_or(markup.len(), |next| next.start);
        let value = markup[tag.stop..stop].trim();
        if !value.is_empty() {
            out.push(value.to_owned());
        }
    }
    out
}

// The inside of a chapter's body element, or the whole chapter without one
fn body(chapter: &str) -> &str {
    let mut start = None;
    let mut stop = chapter.len();
    for tag in tags(chapter).filter(|tag| tag.local() == BODY) {
        if tag.end {
            stop = tag.start;
        } else if start.is_none() {
            start = Some(tag.stop);
        }
    }
    let start = start.unwrap_or(0);
    chapter.get(start..stop.max(start)).unwrap_or(chapter)
}

// Copy a chapter body, inlining images and dropping internal link targets
fn rewrite(
    body: &str,
    chapter_path: &str,
    archive: &Archive,
    media_types: &HashMap<&str, &str>,
) -> String {
    let mut out = String::with_capacity(body.len());
    let mut copied = 0;
    for tag in tags(body) {
        let replacement = match (tag.local().as_str(), tag.end) {
            (SVG, _) => Some(String::new()),
            (IMG, false) => Some(image(
                tag.attr(SRC),
                tag.attr(ALT).unwrap_or_default(),
                chapter_path,
                archive,
                media_types,
            )),
            (IMAGE, false) => Some(image(
                tag.attr(HREF).or_else(|| tag.attr(XLINK_HREF)),
                "",
                chapter_path,
                archive,
                media_types,
            )),
            (A, false) if tag.attr(HREF).is_some_and(|href| !is_external(href)) => {
                Some(format!("<{}>", tag.name))
            }
            _ => None,
        };
        if let Some(replacement) = replacement {
            out.push_str(&body[copied..tag.start]);
            out.push_str(&replacement);
            copied = tag.stop;
        }
    }
    out.push_str(&body[copied..]);
    out
}

// An image tag with its picture inlined, or the alt text when the picture is missing
fn image(
    src: Option<&str>,
    alt: &str,
    chapter_path: &str,
    archive: &Archive,
    media_types: &HashMap<&str, &str>,
) -> String {
    let alt = alt.replace('"', "&quot;");
    let inlined = src
        .and_then(|src| resolve(chapter_path, src))
        .and_then(|path| {
            let media_type = media_types
                .get(path.as_str())
                .copied()
                .filter(|media_type| media_type.starts_with(IMAGE_PREFIX))
                .or_else(|| image_type(&path))?;
            let bytes = archive.read(&path).ok()?;
            Some(format!(
                "data:{media_type};base64,{}",
                base64::encode(&bytes)
            ))
        });
    match inlined {
        Some(uri) => format!("<img src=\"{uri}\" alt=\"{alt}\">"),
        None => alt,
    }
}

// Guess an image media type from a path's extension
fn image_type(path: &str) -> Option<&'static str> {
    let (_, extension) = path.rsplit_once('.')?;
    IMAGE_TYPES
        .iter()
        .find(|(known, _)| extension.eq_ignore_ascii_case(known))
        .map(|(_, media_type)| *media_type)
}

// Check whether a link leaves the book
fn is_external(href: &str) -> bool {
    EXTERNAL_SCHEMES.iter().any(|scheme| {
        href.len() >= scheme.len() && href[..scheme.len()].eq_ignore_ascii_case(scheme)
    })
}

// Resolve an href against the file it appears in, giving a path inside the zip
fn resolve(from: &str, href: &str) -> Option<String> {
    let href = href
        .trim()
        .split(QUERY_OR_FRAGMENT)
        .next()
        .unwrap_or_default();
    let before_slash = href.split(SEPARATOR).next().unwrap_or_default();
    if href.is_empty() || before_slash.contains(SCHEME_MARK) {
        return None;
    }
    let href = String::from_utf8(percent_decode(href)).ok()?;
    let mut parts: Vec<&str> = if href.starts_with(SEPARATOR) {
        Vec::new()
    } else {
        let mut dir: Vec<&str> = from.split(SEPARATOR).collect();
        dir.pop();
        dir
    };
    for part in href.split(SEPARATOR) {
        match part {
            "" | CURRENT => {}
            PARENT => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join(SEPARATOR))
}

// Lowercase a tag or attribute name and drop its namespace prefix
fn local_name(name: &str) -> String {
    name.rsplit(NAMESPACE_MARK)
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
}

// Scan markup for tags, skipping comments, CDATA, declarations and processing instructions
fn tags(markup: &str) -> impl Iterator<Item = Tag<'_>> {
    let mut at = 0;
    std::iter::from_fn(move || {
        loop {
            let lt = at + markup.get(at..)?.find('<')?;
            let rest = &markup[lt..];
            if let Some((_, close)) = SKIPPED.iter().find(|(open, _)| rest.starts_with(open)) {
                at = rest
                    .find(close)
                    .map_or(markup.len(), |end| lt + end + close.len());
                continue;
            }
            match tag_at(markup, lt) {
                Some(tag) => {
                    at = tag.stop;
                    return Some(tag);
                }
                None => at = lt + 1,
            }
        }
    })
}

// Parse the tag that starts at `lt`, if it is one
fn tag_at(markup: &str, lt: usize) -> Option<Tag<'_>> {
    let bytes = markup.as_bytes();
    let mut at = lt + 1;
    let end = bytes.get(at) == Some(&b'/');
    if end {
        at += 1;
    }
    let name_start = at;
    while bytes.get(at).is_some_and(|byte| is_name_byte(*byte)) {
        at += 1;
    }
    if at == name_start || !bytes[name_start].is_ascii_alphabetic() {
        return None;
    }
    let name = &markup[name_start..at];
    let mut attrs = Vec::new();
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        match bytes.get(at)? {
            b'>' => break,
            b'/' => at += 1,
            _ => {
                let key_start = at;
                while bytes.get(at).is_some_and(|byte| is_name_byte(*byte)) {
                    at += 1;
                }
                if at == key_start {
                    at += 1;
                    continue;
                }
                let key = &markup[key_start..at];
                while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                    at += 1;
                }
                if bytes.get(at) != Some(&b'=') {
                    attrs.push((key, ""));
                    continue;
                }
                at += 1;
                while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                    at += 1;
                }
                let value = match bytes.get(at)? {
                    quote @ (b'"' | b'\'') => {
                        let close = at + 1 + markup[at + 1..].find(char::from(*quote))?;
                        let value = &markup[at + 1..close];
                        at = close + 1;
                        value
                    }
                    _ => {
                        let value_start = at;
                        while bytes
                            .get(at)
                            .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'>')
                        {
                            at += 1;
                        }
                        &markup[value_start..at]
                    }
                };
                attrs.push((key, value));
            }
        }
    }
    Some(Tag {
        name,
        end,
        attrs,
        start: lt,
        stop: at + 1,
    })
}

// Characters allowed in tag and attribute names
fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::zip::build;

    const CONTAINER: &str = r#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

    #[test]
    fn paths_resolve_inside_the_zip() {
        assert_eq!(
            resolve("OEBPS/text/ch1.xhtml", "../images/a%20b.png#x").unwrap(),
            "OEBPS/images/a b.png"
        );
        assert_eq!(
            resolve("OEBPS/content.opf", "./ch1.xhtml").unwrap(),
            "OEBPS/ch1.xhtml"
        );
        assert_eq!(resolve("content.opf", "/root.png").unwrap(), "root.png");
        assert!(resolve("a.xhtml", "https://example.com/x.png").is_none());
        assert!(resolve("a.xhtml", "#note").is_none());
    }

    #[test]
    fn tags_skip_comments_and_read_attributes() {
        let markup = r#"<!-- <img src="no"> --><p class='x' hidden data-a=b>t</p>"#;
        let found: Vec<_> = tags(markup).collect();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "p");
        assert_eq!(found[0].attr("class"), Some("x"));
        assert_eq!(found[0].attr("hidden"), Some(""));
        assert_eq!(found[0].attr("data-a"), Some("b"));
        assert!(found[1].end);
    }

    #[test]
    fn spine_order_metadata_images_and_links() {
        let package = r#"<package><metadata>
  <dc:title>A &amp; B</dc:title><dc:creator>Ann</dc:creator><dc:creator>Bo</dc:creator>
</metadata><manifest>
  <item id="c2" href="text/two.xhtml" media-type="application/xhtml+xml"/>
  <item id="c1" href="text/one.xhtml" media-type="application/xhtml+xml"/>
  <item id="pic" href="images/p.png" media-type="image/png"/>
  <item id="css" href="style.css" media-type="text/css"/>
</manifest><spine><itemref idref="c1"/><itemref idref="css"/><itemref idref="gone"/><itemref idref="c2"/></spine></package>"#;
        let one = r#"<html><head><title>x</title></head><body class="c">
<h1>One</h1><p><img src="../images/p.png" alt="Pic"/> <a href="two.xhtml#t">next</a> <a href="https://x.org">web</a></p>
</body></html>"#;
        let two = r#"<html><body><h1>Two</h1><img src="missing.png" alt="Lost"/>
<svg><image xlink:href="../images/p.png"/></svg></body></html>"#;
        let bytes = build::zip(
            &[
                ("mimetype", b"application/epub+zip"),
                ("META-INF/container.xml", CONTAINER.as_bytes()),
                ("OEBPS/content.opf", package.as_bytes()),
                ("OEBPS/text/one.xhtml", one.as_bytes()),
                ("OEBPS/text/two.xhtml", two.as_bytes()),
                ("OEBPS/images/p.png", b"PNG"),
            ],
            true,
        );
        let html = html(&bytes).unwrap();
        let uri = format!("data:image/png;base64,{}", base64::encode(b"PNG"));
        assert_eq!(
            html,
            format!(
                "<html><body>\n<h1>A &amp; B</h1>\n<p><em>Ann, Bo</em></p>\n\n\
<h1>One</h1><p><img src=\"{uri}\" alt=\"Pic\"> <a>next</a> <a href=\"https://x.org\">web</a></p>\n\n\
<h1>Two</h1>Lost\n<img src=\"{uri}\" alt=\"\">\n</body></html>\n"
            )
        );
    }

    #[test]
    fn broken_books_are_errors() {
        assert!(html(b"not a zip").unwrap_err().starts_with(ERROR_PREFIX));
        let no_container = build::zip(&[("mimetype", b"application/epub+zip")], false);
        assert!(
            html(&no_container)
                .unwrap_err()
                .contains("META-INF/container.xml")
        );
        let empty = build::zip(
            &[
                ("META-INF/container.xml", CONTAINER.as_bytes()),
                ("OEBPS/content.opf", b"<package><spine/></package>"),
            ],
            false,
        );
        assert!(html(&empty).unwrap_err().contains("spine is empty"));
    }
}
