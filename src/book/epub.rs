// Author: machinageist
// Date: 2026-11-10
// Description: Turn an EPUB into one HTML document: metadata, then each spine chapter's body
// Notes: Images are inlined as data: URIs so the image pipeline draws them without the zip,
// and chapter-scoped anchors preserve navigation without printing internal ZIP paths

use std::collections::{HashMap, HashSet};

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
const CLASS: &str = "class";
const LANGUAGE_ATTRS: [&str; 3] = [CLASS, "data-lang", "data-language"];
const MEDIA_TYPE: &str = "media-type";
const TITLE: &str = "title";
const CREATOR: &str = "creator";
const CHAPTER_TYPES: [&str; 2] = ["application/xhtml+xml", "text/html"];

// Chapter elements that need rewriting
const HTML: &str = "html";
const BODY: &str = "body";
const IMG: &str = "img";
const IMAGE: &str = "image";
const SVG: &str = "svg";
const A: &str = "a";
const NAME: &str = "name";
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
        .and_then(|tag| tag.attr(FULL_PATH).and_then(|path| resolve("", path)))
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
                    manifest.insert(decode_entities(id), item);
                }
            }
            ITEMREF => spine.extend(tag.attr(IDREF).map(decode_entities)),
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
    // Only readable spine chapters can be destinations in the joined document.
    let mut chapters = Vec::new();
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
        chapters.push((item.path.as_str(), body(&normalize_xhtml(&text(&chapter)))));
    }
    let chapter_paths: HashSet<&str> = chapters.iter().map(|(path, _)| *path).collect();
    let mut out = String::from("<html><body>\n");
    out.push_str(&front_matter(&package));
    for (path, chapter) in &chapters {
        // Bound pending destinations so an empty chapter cannot point into its successor
        out.push_str(&format!("<div id=\"{}\">", anchor_id(path, None)));
        out.push_str(&rewrite(
            chapter,
            path,
            &archive,
            &media_types,
            &chapter_paths,
        ));
        out.push_str("</div>\n");
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
        let mut escaped = String::with_capacity(value.len());
        copy_xml_text(&mut escaped, value);
        if !escaped.trim().is_empty() {
            out.push(escaped);
        }
    }
    out
}

// Resolve chapter namespaces before body extraction discards root declarations. Only
// XHTML (and the SVG image vocabulary) becomes semantic HTML; foreign names stay inert.
struct NamespaceScope<'a> {
    name: &'a str,
    normalized: String,
    declarations: Vec<(&'a str, String)>,
    hidden: bool,
}

fn normalized_name(
    name: &str,
    declarations: &[(&str, String)],
    scopes: &[NamespaceScope<'_>],
) -> String {
    let (prefix, local) = name.split_once(':').unwrap_or(("", name));
    let namespace = declarations
        .iter()
        .rev()
        .chain(
            scopes
                .iter()
                .rev()
                .flat_map(|scope| scope.declarations.iter().rev()),
        )
        .find(|(declared, _)| *declared == prefix)
        .map(|(_, uri)| uri.as_str());
    match namespace {
        Some("http://www.w3.org/1999/xhtml") => local.to_ascii_lowercase(),
        Some("http://www.w3.org/2000/svg") if matches!(local, SVG | IMAGE) => local.to_owned(),
        None | Some("") if prefix.is_empty() => local.to_ascii_lowercase(),
        _ => format!("epub-foreign:{local}"),
    }
}

fn copy_tag(out: &mut String, chapter: &str, tag: &Tag<'_>, name: &str) {
    let start = tag.start + if tag.end { 2 } else { 1 };
    out.push_str(&chapter[tag.start..start]);
    out.push_str(name);
    out.push_str(&chapter[start + tag.name.len()..tag.stop]);
}

// XML-qualified names are case-sensitive. Retain tolerant HTML matching for
// legacy unprefixed chapters, including mixed-case script/template end tags.
fn same_element_name(left: &str, right: &str) -> bool {
    left == right || (!left.contains(':') && left.eq_ignore_ascii_case(right))
}

// Keep XML literals and instructions opaque to every later HTML scan
fn skipped_end(markup: &str, start: usize) -> Option<usize> {
    let rest = &markup[start..];
    let (open, close) = SKIPPED.iter().find(|(open, _)| rest.starts_with(open))?;
    if *open != "<!" {
        return Some(
            rest[open.len()..]
                .find(close)
                .map_or(markup.len(), |end| start + open.len() + end + close.len()),
        );
    }
    // A declaration may contain quoted `>` and a DOCTYPE internal subset
    let bytes = markup.as_bytes();
    let mut at = start + open.len();
    let mut quote = None;
    let mut brackets = 0usize;
    while at < bytes.len() {
        let byte = bytes[at];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
        } else if bytes[at..].starts_with(b"<!--") {
            at = markup[at + 4..]
                .find("-->")
                .map_or(markup.len(), |end| at + 4 + end + 3);
            continue;
        } else if bytes[at..].starts_with(b"<?") {
            at = markup[at + 2..]
                .find("?>")
                .map_or(markup.len(), |end| at + 2 + end + 2);
            continue;
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'>' if brackets == 0 => return Some(at + 1),
                _ => {}
            }
        }
        at += 1;
    }
    Some(markup.len())
}

// XML literal text must not turn into markup in the HTML presenter
fn copy_literal(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            _ => out.push(ch),
        }
    }
}

// Preserve text/entities, encode CDATA, and discard instructions/declarations
fn copy_xml_text(out: &mut String, mut text: &str) {
    while let Some(start) = text.find('<') {
        out.push_str(&text[..start]);
        text = &text[start..];
        if let Some(end) = skipped_end(text, 0) {
            if let Some(literal) = text[..end].strip_prefix("<![CDATA[") {
                copy_literal(out, literal.strip_suffix("]]>").unwrap_or(literal));
            }
            text = &text[end..];
        } else {
            out.push_str("&lt;");
            text = &text[1..];
        }
    }
    out.push_str(text);
}

fn raw_text_end<'a>(chapter: &'a str, mut at: usize, name: &str) -> Option<Tag<'a>> {
    while let Some(offset) = chapter[at..].find('<') {
        let start = at + offset;
        let rest = &chapter[start..];
        if let Some(end) = skipped_end(chapter, start) {
            at = end;
        } else if rest.starts_with("</")
            && let Some(tag) = tag_at(chapter, start)
            && same_element_name(tag.name, name)
        {
            return Some(tag);
        } else {
            at = start + 1;
        }
    }
    None
}

// Close recovered source elements before a chapter can enter the joined document
fn close_scopes(out: &mut String, scopes: &[NamespaceScope<'_>]) {
    for scope in scopes.iter().rev().filter(|scope| !scope.hidden) {
        out.push_str("</");
        out.push_str(&scope.normalized);
        out.push('>');
    }
}

fn normalize_xhtml(chapter: &str) -> String {
    const VOID: [&str; 15] = [
        "area", "base", "br", "col", "embed", "hr", IMG, "input", "link", "meta", "param",
        "source", "track", "wbr", "keygen",
    ];
    const RAW_TEXT: [&str; 6] = ["script", "style", "textarea", TITLE, "iframe", "xmp"];
    const HIDDEN: [&str; 12] = [
        "head", "script", "style", "textarea", TITLE, "iframe", "noscript", "template", "input",
        "select", "button", "math",
    ];
    let mut out = String::with_capacity(chapter.len());
    let mut scopes: Vec<NamespaceScope<'_>> = Vec::new();
    let mut at = 0;
    while let Some(mut tag) = tags(&chapter[at..]).next() {
        tag.start += at;
        tag.stop += at;
        let hidden = scopes.last().is_some_and(|scope| scope.hidden);
        if !hidden {
            copy_xml_text(&mut out, &chapter[at..tag.start]);
        }
        at = tag.stop;
        if tag.end {
            if let Some(index) = scopes
                .iter()
                .rposition(|scope| same_element_name(scope.name, tag.name))
            {
                // A malformed close inside hidden content cannot end a visible ancestor
                if hidden && !scopes[index].hidden {
                    continue;
                }
                close_scopes(&mut out, &scopes[index + 1..]);
                if !scopes[index].hidden {
                    copy_tag(&mut out, chapter, &tag, &scopes[index].normalized);
                }
                scopes.truncate(index);
            } else if !hidden {
                copy_tag(
                    &mut out,
                    chapter,
                    &tag,
                    &normalized_name(tag.name, &[], &scopes),
                );
            }
            continue;
        }
        let declarations: Vec<_> = tag
            .attrs
            .iter()
            .filter_map(|(name, uri)| {
                let prefix = if *name == "xmlns" {
                    ""
                } else {
                    name.strip_prefix("xmlns:")?
                };
                Some((prefix, decode_entities(uri)))
            })
            .collect();
        let name = normalized_name(tag.name, &declarations, &scopes);
        // Hidden elements must not leave their own IDs for the next visible sibling
        let local = local_name(tag.name);
        let hidden_content =
            hidden || HIDDEN.contains(&name.as_str()) || HIDDEN.contains(&local.as_str());
        if !hidden_content {
            copy_tag(&mut out, chapter, &tag, &name);
        }
        if chapter[tag.start..tag.stop].trim_end().ends_with("/>") || VOID.contains(&name.as_str())
        {
            continue;
        }
        // Opaque content stays inert even when its namespace is foreign (e.g. SVG script)
        if RAW_TEXT.contains(&name.as_str()) || RAW_TEXT.contains(&local.as_str()) {
            // Raw text is not XML markup: embedded fake tags cannot change namespace,
            // body or hidden-subtree scope. Match the original qualified closing name.
            let end = raw_text_end(chapter, at, tag.name);
            let stop = end.as_ref().map_or(chapter.len(), |end| end.start);
            if !hidden_content {
                // Keep raw visible text literal even to the later chapter rewriter.
                copy_literal(&mut out, &chapter[at..stop]);
            }
            if !hidden_content {
                out.push_str(&format!("</{name}>"));
            }
            at = end.map_or(chapter.len(), |end| end.stop);
            continue;
        }
        scopes.push(NamespaceScope {
            name: tag.name,
            normalized: name,
            declarations,
            hidden: hidden_content,
        });
    }
    if !scopes.last().is_some_and(|scope| scope.hidden) {
        copy_xml_text(&mut out, &chapter[at..]);
    }
    close_scopes(&mut out, &scopes);
    out
}

// Keep body content, IDs and language ancestors, without copying head or unsafe attributes.
fn body(chapter: &str) -> String {
    let mut root = None;
    let mut body = None;
    let mut stop = chapter.len();
    for tag in tags(chapter) {
        match (tag.name, tag.end) {
            (HTML, false) if root.is_none() && body.is_none() => root = Some(tag),
            (BODY, false) if body.is_none() => body = Some(tag),
            (BODY, true) => stop = tag.start,
            _ => {}
        }
    }
    let start = body.as_ref().map_or(0, |tag| tag.stop);
    let mut out = chapter
        .get(start..stop.max(start))
        .unwrap_or(chapter)
        .to_owned();
    if let Some(body) = body {
        // Separate, balanced wrappers retain nearest-ancestor precedence and chapter scope.
        for tag in std::iter::once(&body).chain(root.iter()) {
            let attributes: String = LANGUAGE_ATTRS
                .iter()
                .chain(std::iter::once(&ID))
                .filter_map(|name| tag.attr(name).map(|value| (name, value)))
                .map(|(name, value)| {
                    // Values are still entity-escaped source, so do not double-escape `&`.
                    let value = value
                        .replace('"', "&quot;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;");
                    format!(" {name}=\"{value}\"")
                })
                .collect();
            if !attributes.is_empty() {
                out = format!("<div{attributes}>{out}</div>");
            }
        }
    }
    out
}

// Copy a chapter body, inlining images and namespacing local destinations.
fn rewrite(
    body: &str,
    chapter_path: &str,
    archive: &Archive,
    media_types: &HashMap<&str, &str>,
    chapter_paths: &HashSet<&str>,
) -> String {
    let mut out = String::with_capacity(body.len());
    let mut copied = 0;
    for tag in tags(body) {
        // Names were namespace-resolved before body extraction; do not reinterpret
        // foreign qualified names by their local spelling here.
        let local = tag.name.to_ascii_lowercase();
        let self_closing = body[tag.start..tag.stop].trim_end().ends_with("/>");
        let replacement = match (local.as_str(), tag.end) {
            (SVG, true) => Some("</span>".to_owned()),
            (SVG, false) => Some(format!(
                "<span{}>{}",
                anchor_attributes(&tag, chapter_path),
                if self_closing { "</span>" } else { "" }
            )),
            (IMG | IMAGE, false) => {
                let picture = image(
                    if local == IMG {
                        tag.attr(SRC)
                    } else {
                        tag.attr(HREF).or_else(|| tag.attr(XLINK_HREF))
                    },
                    tag.attr(ALT).unwrap_or_default(),
                    chapter_path,
                    archive,
                    media_types,
                );
                let attrs = anchor_attributes(&tag, chapter_path);
                Some(if attrs.is_empty() {
                    picture
                } else {
                    format!("<span{attrs}>{picture}</span>")
                })
            }
            (_, false)
                if tag.attr(ID).is_some()
                    || (local == A && (tag.attr(NAME).is_some() || tag.attr(HREF).is_some())) =>
            {
                let mut replacement = format!("<{}", tag.name);
                for (name, value) in &tag.attrs {
                    let value = if name.eq_ignore_ascii_case(ID)
                        || (local == A && name.eq_ignore_ascii_case(NAME))
                    {
                        anchor_id(chapter_path, Some(&decode_entities(value)))
                    } else if local == A && name.eq_ignore_ascii_case(HREF) {
                        let href = decode_entities(value);
                        if is_external(href.trim()) {
                            replacement.push_str(&format!(" {name}=\"{}\"", escape_source(value)));
                            continue;
                        }
                        let Some(target) = internal_target(chapter_path, &href, chapter_paths)
                        else {
                            continue;
                        };
                        format!("#{target}")
                    } else {
                        replacement.push_str(&format!(" {name}=\"{}\"", escape_source(value)));
                        continue;
                    };
                    replacement.push_str(&format!(" {name}=\"{value}\""));
                }
                replacement.push_str(if self_closing { "/>" } else { ">" });
                Some(replacement)
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

// Collision-free chapter and fragment identities, including literal percent signs and Unicode.
fn anchor_id(chapter: &str, fragment: Option<&str>) -> String {
    fn hex(text: &str) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        text.bytes()
            .flat_map(|byte| {
                [
                    char::from(DIGITS[usize::from(byte >> 4)]),
                    char::from(DIGITS[usize::from(byte & 15)]),
                ]
            })
            .collect()
    }
    let chapter = hex(chapter);
    match fragment {
        Some(fragment) => format!("epub-{chapter}-id-{}", hex(fragment)),
        None => format!("epub-{chapter}-top"),
    }
}

// IDs on elements replaced by the image pipeline identify their visible replacement.
fn anchor_attributes(tag: &Tag<'_>, chapter: &str) -> String {
    tag.attr(ID).map_or(String::new(), |id| {
        format!(" id=\"{}\"", anchor_id(chapter, Some(&decode_entities(id))))
    })
}

// The input is entity-escaped source; escape delimiters without doubling ampersands.
fn escape_source(value: &str) -> String {
    value
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// Internal hrefs refer only to readable chapters, never a local/executable URI.
fn internal_target(from: &str, href: &str, chapters: &HashSet<&str>) -> Option<String> {
    if invalid_internal_text(href) {
        return None;
    }
    let href = href.trim();
    let (path, fragment) = href
        .split_once('#')
        .map_or((href, None), |(path, fragment)| (path, Some(fragment)));
    let path = if path.split('?').next().unwrap_or_default().is_empty() {
        from.to_owned()
    } else {
        resolve_decoded(from, path)?
    };
    if !chapters.contains(path.as_str()) || invalid_internal_text(&path) {
        return None;
    }
    let fragment = fragment
        .filter(|fragment| !fragment.is_empty())
        .map(|fragment| String::from_utf8(percent_decode(fragment)))
        .transpose()
        .ok()?;
    if fragment.as_deref().is_some_and(invalid_internal_text) {
        return None;
    }
    Some(anchor_id(&path, fragment.as_deref()))
}

// Refuse parser-repaired spellings before they can bind to a different chapter/ID
fn invalid_internal_text(text: &str) -> bool {
    text.chars().any(|ch| ch.is_control() || ch == '\u{fffd}')
}

// An image tag with its picture inlined, or the alt text when the picture is missing
fn image(
    src: Option<&str>,
    alt: &str,
    chapter_path: &str,
    archive: &Archive,
    media_types: &HashMap<&str, &str>,
) -> String {
    let alt = escape_source(alt);
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
        href.as_bytes()
            .get(..scheme.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme.as_bytes()))
    })
}

// Resolve an href against the file it appears in, giving a path inside the zip
fn resolve(from: &str, href: &str) -> Option<String> {
    resolve_decoded(from, &decode_entities(href))
}

// Hrefs are entity-decoded once, before separating their URI components.
fn resolve_decoded(from: &str, href: &str) -> Option<String> {
    let href = href
        .trim()
        .split(QUERY_OR_FRAGMENT)
        .next()
        .unwrap_or_default();
    let before_slash = href.split(SEPARATOR).next().unwrap_or_default();
    if href.is_empty() || href.starts_with("//") || before_slash.contains(SCHEME_MARK) {
        return None;
    }
    let href = String::from_utf8(percent_decode(href)).ok()?;
    if href.chars().any(char::is_control)
        || href.starts_with("//")
        || href.split(SEPARATOR).next()?.contains(SCHEME_MARK)
    {
        return None;
    }
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
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join(SEPARATOR))
}

// XML character references used in package paths and XHTML IDs/hrefs.
fn decode_entities(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest
            .bytes()
            .take(13)
            .position(|byte| byte == b';')
            .and_then(|end| {
                let name = &rest[1..end];
                let ch = match name {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ => {
                        let number = name.strip_prefix('#')?;
                        let code = match number.strip_prefix(['x', 'X']) {
                            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                            None => number.parse().ok()?,
                        };
                        char::from_u32(code)?
                    }
                };
                Some((ch, end + 1))
            });
        match decoded {
            Some((ch, length)) => {
                out.push(ch);
                rest = &rest[length..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
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
            if let Some(end) = skipped_end(markup, lt) {
                at = end;
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
        let one_top = anchor_id("OEBPS/text/one.xhtml", None);
        let two_top = anchor_id("OEBPS/text/two.xhtml", None);
        let target = anchor_id("OEBPS/text/two.xhtml", Some("t"));
        assert_eq!(
            html,
            format!(
                "<html><body>\n<h1>A &amp; B</h1>\n<p><em>Ann, Bo</em></p>\n<div id=\"{one_top}\"><div class=\"c\">\n\
<h1>One</h1><p><img src=\"{uri}\" alt=\"Pic\"> <a href=\"#{target}\">next</a> <a href=\"https://x.org\">web</a></p>\n</div></div>\n\
<div id=\"{two_top}\"><h1>Two</h1>Lost\n<span><img src=\"{uri}\" alt=\"\"></span></div>\n</body></html>\n"
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
