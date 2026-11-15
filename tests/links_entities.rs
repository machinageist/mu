// Author: machinageist
// Description: EPUB entity spellings retain exact identities at extraction/rendering boundaries

mod common;

use common::strip_sgr;
use mu::{Format, Options, extract, present_outlined};

// Stored ZIP records exercise public extraction without adding a fixture dependency.
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        let checksum = !data.iter().fold(!0u32, |crc, byte| {
            (0..8).fold(crc ^ u32::from(*byte), |crc, _| {
                (crc >> 1) ^ if crc & 1 == 0 { 0 } else { 0xedb8_8320 }
            })
        });
        let header = |central: bool| {
            let mut record = Vec::new();
            let signature: u32 = if central { 0x0201_4b50 } else { 0x0403_4b50 };
            record.extend_from_slice(&signature.to_le_bytes());
            if central {
                record.extend_from_slice(&20u16.to_le_bytes());
            }
            record.extend_from_slice(&[20, 0, 0, 0, 0, 0]);
            record.extend_from_slice(&[0; 4]);
            record.extend_from_slice(&checksum.to_le_bytes());
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
        out.extend(header(false));
        out.extend_from_slice(data);
        directory.extend(header(true));
    }
    let directory_offset = out.len() as u32;
    out.extend(&directory);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

fn book_html(
    container: &str,
    package_path: &str,
    package: &str,
    chapters: &[(&str, &str)],
) -> String {
    let mut files = vec![
        ("mimetype", b"application/epub+zip".as_slice()),
        ("META-INF/container.xml", container.as_bytes()),
        (package_path, package.as_bytes()),
    ];
    files.extend(
        chapters
            .iter()
            .map(|(path, source)| (*path, source.as_bytes())),
    );
    extract(&zip(&files), Format::Epub).expect("actual EPUB extracts")
}

fn chapter_html(body: &str) -> String {
    let chapter = format!(
        "<?xml version='1.0' encoding='UTF-8'?>\n<!DOCTYPE html PUBLIC '-//W3C//DTD XHTML 1.1//EN' 'http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd'>\n<html xmlns='http://www.w3.org/1999/xhtml'><head><title>Entities</title></head><body>{body}</body></html>"
    );
    book_html(
        "<container><rootfiles><rootfile full-path='book.opf'/></rootfiles></container>",
        "book.opf",
        "<package><manifest><item id='chapter' href='chapter.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='chapter'/></spine></package>",
        &[("chapter.xhtml", &chapter)],
    )
}

fn assert_destinations(html: &str, expected: &[(&str, &str)]) {
    for width in [24, 60] {
        for links in [false, true] {
            let mut plain = None;
            for color in [false, true] {
                let rendered = present_outlined(
                    html,
                    Format::Epub,
                    &Options {
                        links,
                        ..Options::new(width, color)
                    },
                );
                assert_eq!(
                    rendered
                        .links
                        .iter()
                        .map(|link| link.label.as_str())
                        .collect::<Vec<_>>(),
                    expected.iter().map(|(label, _)| *label).collect::<Vec<_>>(),
                    "width {width}, links {links}, color {color}"
                );
                for (link, (_, destination)) in rendered.links.iter().zip(expected) {
                    let anchor = rendered
                        .anchors
                        .iter()
                        .find(|anchor| anchor.id == link.target)
                        .expect("navigation resolves to a rendered, chapter-scoped anchor");
                    assert_eq!(
                        strip_sgr(rendered.text.lines().nth(anchor.line).unwrap()),
                        *destination,
                        "destination for {:?}",
                        link.label
                    );
                    assert!(
                        strip_sgr(rendered.text.lines().nth(link.line).unwrap())
                            .contains(&link.label)
                    );
                    assert!(anchor.id.starts_with("epub-"));
                }
                let visible = strip_sgr(&rendered.text);
                assert!(
                    !visible.contains('\x1b'),
                    "internal navigation emits no OSC 8"
                );
                if color {
                    assert_eq!(plain.as_ref(), Some(&visible));
                } else {
                    plain = Some(visible);
                }
            }
        }
    }
}

#[test]
fn named_xhtml_identifier_matches_its_percent_encoded_fragment() {
    for id in ["caf&eacute;", "caf&#233;", "caf&#xE9;", "café"] {
        let html = chapter_html(&format!(
            "<p><a href='#caf&eacute;'>NAMED</a></p>\
<p><a href='#caf&#233;'>DECIMAL</a></p>\
<p><a href='#caf&#xE9;'>HEX</a></p>\
<p><a href='#café'>LITERAL</a></p>\
<p><a href='#caf%C3%A9'>PERCENT</a></p>\
<h2 id='{id}'>CAFE_TARGET</h2>"
        ));
        assert_destinations(
            &html,
            &[
                ("NAMED", "CAFE_TARGET"),
                ("DECIMAL", "CAFE_TARGET"),
                ("HEX", "CAFE_TARGET"),
                ("LITERAL", "CAFE_TARGET"),
                ("PERCENT", "CAFE_TARGET"),
            ],
        );
    }
}

#[test]
fn nbsp_and_encoded_spaces_are_exact_identity_bytes_not_trimmed_or_collapsed() {
    let html = chapter_html(
        "<p><a href='#part&nbsp;'>NAMED_NBSP</a></p>\
<p><a href='#part&#160;'>NUMERIC_NBSP</a></p>\
<p><a href='#part\u{a0}'>LITERAL_NBSP</a></p>\
<p><a href='#part%C2%A0'>PERCENT_NBSP</a></p>\
<p><a href='#part&#32;'>NUMERIC_SPACE</a></p>\
<p><a href='#part%20'>PERCENT_SPACE</a></p>\
<p><a href='#part'>NO_SPACE</a></p>\
<p><a href='#a&nbsp;&nbsp;b'>DOUBLE_NBSP</a></p>\
<p><a href='#&nbsp;part&nbsp;'>PADDED_NBSP</a></p>\
<h2 id='part&nbsp;'>NBSP_TARGET</h2>\
<h2 id='part '>SPACE_TARGET</h2>\
<h2 id='part'>BARE_TARGET</h2>\
<h2 id='a&#160;&#160;b'>DOUBLE_TARGET</h2>\
<h2 id='&#160;part&#160;'>PADDED_TARGET</h2>",
    );
    assert_destinations(
        &html,
        &[
            ("NAMED_NBSP", "NBSP_TARGET"),
            ("NUMERIC_NBSP", "NBSP_TARGET"),
            ("LITERAL_NBSP", "NBSP_TARGET"),
            ("PERCENT_NBSP", "NBSP_TARGET"),
            ("NUMERIC_SPACE", "SPACE_TARGET"),
            ("PERCENT_SPACE", "SPACE_TARGET"),
            ("NO_SPACE", "BARE_TARGET"),
            ("DOUBLE_NBSP", "DOUBLE_TARGET"),
            ("PADDED_NBSP", "PADDED_TARGET"),
        ],
    );
}

#[test]
fn package_identifiers_paths_and_cross_chapter_hrefs_share_entity_decoding() {
    for (id, idref, href) in [
        ("caf&eacute;", "caf&#233;", "text/caf&eacute;.xhtml"),
        ("caf&#xE9;", "caf&eacute;", "text/caf&#233;.xhtml"),
        ("café", "caf&eacute;", "text/café.xhtml"),
        ("caf&eacute;", "café", "text/caf%C3%A9.xhtml"),
    ] {
        let package = format!(
            "<package><manifest>\
<item id='from' href='text/from.xhtml' media-type='application/xhtml+xml'/>\
<item id='{id}' href='{href}' media-type='application/xhtml+xml'/>\
</manifest><spine><itemref idref='from'/><itemref idref='{idref}'/></spine></package>"
        );
        let html = book_html(
            "<container><rootfiles><rootfile full-path='OEBPS/pac&kappa;age.opf'/></rootfiles></container>",
            "OEBPS/pacκage.opf",
            &package,
            &[
                (
                    "OEBPS/text/from.xhtml",
                    "<html><body>\
<p><a href='caf&eacute;.xhtml#caf&eacute;'>NAMED_PATH</a></p>\
<p><a href='caf&#233;.xhtml#caf&#xE9;'>NUMERIC_PATH</a></p>\
<p><a href='café.xhtml#café'>LITERAL_PATH</a></p>\
<p><a href='caf%C3%A9.xhtml#caf%C3%A9'>PERCENT_PATH</a></p>\
<h2 id='café'>FIRST_CHAPTER_DECOY</h2></body></html>",
                ),
                (
                    "OEBPS/text/café.xhtml",
                    "<html><body><h2 id='caf&eacute;'>SECOND_CHAPTER_TARGET</h2></body></html>",
                ),
            ],
        );
        assert_destinations(
            &html,
            &[
                ("NAMED_PATH", "SECOND_CHAPTER_TARGET"),
                ("NUMERIC_PATH", "SECOND_CHAPTER_TARGET"),
                ("LITERAL_PATH", "SECOND_CHAPTER_TARGET"),
                ("PERCENT_PATH", "SECOND_CHAPTER_TARGET"),
            ],
        );
    }
}

#[test]
fn package_and_chapter_paths_keep_terminal_entity_whitespace() {
    let html = book_html(
        "<container><rootfiles><rootfile full-path='OEBPS/book.opf&nbsp;'/></rootfiles></container>",
        "OEBPS/book.opf\u{a0}",
        "<package><manifest>\
<item id='first' href='first.xhtml' media-type='application/xhtml+xml'/>\
<item id='&nbsp;second&nbsp;' href='second.xhtml&nbsp;' media-type='application/xhtml+xml'/>\
<item id='third' href='third.xhtml&#32;' media-type='application/xhtml+xml'/>\
</manifest><spine><itemref idref='first'/><itemref idref='&#160;second&#160;'/><itemref idref='third'/></spine></package>",
        &[
            (
                "OEBPS/first.xhtml",
                "<html><body>\
<p><a href='second.xhtml&nbsp;#caf&eacute;'>NAMED_PATH</a></p>\
<p><a href='second.xhtml%C2%A0'>PERCENT_PATH</a></p>\
<p><a href='third.xhtml&#32;'>SPACE_PATH</a></p></body></html>",
            ),
            (
                "OEBPS/second.xhtml\u{a0}",
                "<html><body><h2 id='café'>NBSP_PATH_TARGET</h2></body></html>",
            ),
            (
                "OEBPS/third.xhtml ",
                "<html><body><h2>SPACE_PATH_TARGET</h2></body></html>",
            ),
        ],
    );
    assert_destinations(
        &html,
        &[
            ("NAMED_PATH", "NBSP_PATH_TARGET"),
            ("PERCENT_PATH", "NBSP_PATH_TARGET"),
            ("SPACE_PATH", "SPACE_PATH_TARGET"),
        ],
    );
}

#[test]
fn entity_lookup_never_reparses_identifier_or_path_punctuation_as_markdown() {
    let html = book_html(
        "<container><rootfiles><rootfile full-path='book.opf'/></rootfiles></container>",
        "book.opf",
        "<package><manifest><item id='c' href='*[caf&eacute;]_(chapter).xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='c'/></spine></package>",
        &[(
            "*[café]_(chapter).xhtml",
            "<html><body>\
<p><a href='#*[caf&eacute;]_(id)'>PUNCTUATION</a></p>\
<p><a href='#&CounterClockwiseContourIntegral;'>LONG_NAME</a></p>\
<p><a href='#%E2%89%82%CC%B8'>MULTI_SCALAR</a></p>\
<p><a href='#x&ast;&num;&lsqb;&rsqb;&sol;&lt;&gt;&quot;&apos;&amp;'>DELIMITERS</a></p>\
<h2 id='*[café]_(id)'>PUNCTUATION_TARGET</h2>\
<h2 id='&#x2233;'>LONG_NAME_TARGET</h2>\
<h2 id='&NotEqualTilde;'>MULTI_SCALAR_TARGET</h2>\
<h2 id='x*#[]/&lt;&gt;&quot;&apos;&amp;'>DELIMITERS_TARGET</h2></body></html>",
        )],
    );
    assert_destinations(
        &html,
        &[
            ("PUNCTUATION", "PUNCTUATION_TARGET"),
            ("LONG_NAME", "LONG_NAME_TARGET"),
            ("MULTI_SCALAR", "MULTI_SCALAR_TARGET"),
            ("DELIMITERS", "DELIMITERS_TARGET"),
        ],
    );
}

#[test]
fn entity_and_percent_escapes_are_each_decoded_once_not_recursively() {
    let html = chapter_html(
        "<p><a href='#caf&eacute;'>UNICODE</a></p>\
<p><a href='#caf&amp;eacute;'>LITERAL_ENTITY</a></p>\
<p><a href='#caf&#38;eacute;'>NUMERIC_AMP</a></p>\
<p><a href='#caf%26eacute%3B'>PERCENT_ENTITY</a></p>\
<p><a href='#caf%25C3%25A9'>LITERAL_PERCENT</a></p>\
<p><a href='#caf%2526eacute%253B'>NESTED_PERCENT</a></p>\
<h2 id='café'>UNICODE_TARGET</h2>\
<h2 id='caf&amp;eacute;'>ENTITY_TARGET</h2>\
<h2 id='caf%C3%A9'>PERCENT_TARGET</h2>\
<h2 id='caf%26eacute%3B'>NESTED_TARGET</h2>",
    );
    assert_destinations(
        &html,
        &[
            ("UNICODE", "UNICODE_TARGET"),
            ("LITERAL_ENTITY", "ENTITY_TARGET"),
            ("NUMERIC_AMP", "ENTITY_TARGET"),
            ("PERCENT_ENTITY", "ENTITY_TARGET"),
            ("LITERAL_PERCENT", "PERCENT_TARGET"),
            ("NESTED_PERCENT", "NESTED_TARGET"),
        ],
    );
}

#[test]
fn package_and_chapter_paths_decode_entities_and_percent_escapes_only_once() {
    let html = book_html(
        "<container><rootfiles><rootfile full-path='OEBPS/book&amp;eacute;.opf'/></rootfiles></container>",
        "OEBPS/book&eacute;.opf",
        "<package><manifest>\
<item id='from' href='first.xhtml' media-type='application/xhtml+xml'/>\
<item id='caf&amp;eacute;' href='caf&amp;eacute;.xhtml' media-type='application/xhtml+xml'/>\
<item id='percent' href='caf%25C3%25A9.xhtml' media-type='application/xhtml+xml'/>\
<item id='unicode' href='café.xhtml' media-type='application/xhtml+xml'/>\
</manifest><spine><itemref idref='from'/><itemref idref='caf&#38;eacute;'/><itemref idref='percent'/><itemref idref='unicode'/></spine></package>",
        &[
            (
                "OEBPS/first.xhtml",
                "<html><body>\
<p><a href='caf&amp;eacute;.xhtml#caf&amp;eacute;'>LITERAL_ENTITY</a></p>\
<p><a href='caf%26eacute%3B.xhtml#caf%26eacute%3B'>PERCENT_ENTITY</a></p>\
<p><a href='caf%25C3%25A9.xhtml#caf%25C3%25A9'>LITERAL_PERCENT</a></p>\
<p><a href='caf%C3%A9.xhtml#caf%C3%A9'>UNICODE</a></p></body></html>",
            ),
            (
                "OEBPS/caf&eacute;.xhtml",
                "<html><body><h2 id='caf&amp;eacute;'>ENTITY_PATH_TARGET</h2></body></html>",
            ),
            (
                "OEBPS/caf%C3%A9.xhtml",
                "<html><body><h2 id='caf%C3%A9'>PERCENT_PATH_TARGET</h2></body></html>",
            ),
            (
                "OEBPS/café.xhtml",
                "<html><body><h2 id='café'>UNICODE_PATH_TARGET</h2></body></html>",
            ),
        ],
    );
    assert_destinations(
        &html,
        &[
            ("LITERAL_ENTITY", "ENTITY_PATH_TARGET"),
            ("PERCENT_ENTITY", "ENTITY_PATH_TARGET"),
            ("LITERAL_PERCENT", "PERCENT_PATH_TARGET"),
            ("UNICODE", "UNICODE_PATH_TARGET"),
        ],
    );
}

#[test]
fn root_body_legacy_anchor_and_image_identities_decode_entities_once() {
    let html = book_html(
        "<container><rootfiles><rootfile full-path='book.opf'/></rootfiles></container>",
        "book.opf",
        "<package><manifest><item id='c' href='chapter.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='c'/></spine></package>",
        &[(
            "chapter.xhtml",
            "<html id='r&eacute;'><body id='b&eacute;'>\
<h2>ROOT_AND_BODY_TARGET</h2>\
<p><a href='#r%C3%A9'>ROOT</a></p><p><a href='#b%C3%A9'>BODY</a></p>\
<p><a href='#l%C3%A9'>LEGACY</a></p><p><a href='#image%C2%A0'>IMAGE</a></p>\
<p><a href='#s%C3%A9'>SVG</a></p>\
<p><a name='l&eacute;'>LEGACY_TARGET</a></p>\
<p><img id='image&nbsp;' src='missing.png' alt='IMAGE_TARGET'/></p>\
<svg id='s&eacute;'><image href='missing.svg' alt='SVG_TARGET'/></svg>\
</body></html>",
        )],
    );
    assert_destinations(
        &html,
        &[
            ("ROOT", "ROOT_AND_BODY_TARGET"),
            ("BODY", "ROOT_AND_BODY_TARGET"),
            ("LEGACY", "LEGACY_TARGET"),
            ("IMAGE", "IMAGE_TARGET"),
            ("SVG", "SVG_TARGET"),
        ],
    );
}

#[test]
fn unknown_malformed_and_declared_entities_remain_literal_not_expanded_or_repaired() {
    let html = chapter_html(
        "<!DOCTYPE html [<!ENTITY planted 'café'><!ENTITY eacute 'wrong'>\
<!ENTITY remote SYSTEM 'file:///ENTITY_MUST_NOT_BE_READ'>]>\
<p><a href='#caf&NotAnEntity;'>UNKNOWN</a></p>\
<p><a href='#caf&amp;NotAnEntity;'>ESCAPED_UNKNOWN</a></p>\
<p><a href='#caf&eacute'>NO_SEMICOLON</a></p>\
<p><a href='#caf&EACUTE;'>CASE_SENSITIVE</a></p>\
<p><a href='#&planted;'>DECLARED</a></p>\
<p><a href='#&remote;'>EXTERNAL_ENTITY</a></p>\
<p><a href='#caf&eacute;'>STANDARD_NAME</a></p>\
<h2 id='caf&amp;NotAnEntity;'>UNKNOWN_TARGET</h2>\
<h2 id='caf&amp;eacute'>NO_SEMICOLON_TARGET</h2>\
<h2 id='caf&amp;EACUTE;'>CASE_TARGET</h2>\
<h2 id='&amp;planted;'>DECLARED_LITERAL</h2>\
<h2 id='café'>STANDARD_TARGET</h2>",
    );
    assert_destinations(
        &html,
        &[
            ("UNKNOWN", "UNKNOWN_TARGET"),
            ("ESCAPED_UNKNOWN", "UNKNOWN_TARGET"),
            ("NO_SEMICOLON", "NO_SEMICOLON_TARGET"),
            ("CASE_SENSITIVE", "CASE_TARGET"),
            ("DECLARED", "DECLARED_LITERAL"),
            ("STANDARD_NAME", "STANDARD_TARGET"),
        ],
    );
    let rendered = present_outlined(&html, Format::Epub, &Options::new(60, false));
    assert!(rendered.text.contains("EXTERNAL_ENTITY"));
    assert!(!rendered.text.contains("ENTITY_MUST_NOT_BE_READ"));
}

#[test]
fn external_entity_decoding_preserves_source_scheme_and_rejection_provenance() {
    for (href, uri) in [
        (
            "https://example.org/caf&eacute;",
            "https://example.org/caf%C3%A9",
        ),
        (
            "https://example.org/caf&#233;",
            "https://example.org/caf%C3%A9",
        ),
        ("https://example.org/café", "https://example.org/caf%C3%A9"),
        (
            "https://example.org/caf%C3%A9",
            "https://example.org/caf%C3%A9",
        ),
        (
            "https://example.org/caf&amp;eacute;",
            "https://example.org/caf&eacute;",
        ),
        (
            "https://example.org/&NotAnEntity;",
            "https://example.org/&NotAnEntity;",
        ),
    ] {
        let html = chapter_html(&format!("<p><a href='{href}'>EXTERNAL</a></p>"));
        let rendered = present_outlined(
            &html,
            Format::Epub,
            &Options {
                links: true,
                ..Options::new(240, false)
            },
        );
        assert_eq!(
            rendered.text,
            format!("\x1b]8;;{uri}\x1b\\EXTERNAL\x1b]8;;\x1b\\\n")
        );
        assert!(rendered.links.is_empty());
    }
    for href in [
        "https://example.org/a&NewLine;b",
        "https://example.org/a&Tab;b",
        "https://example.org/a&#0;b",
        "https://example.org/a\u{fffd}b",
        "file&colon;https://example.org/",
        "file:https://example.org/",
        "file&colon;mailto:reader@example.org",
        "javascript&colon;https://example.org/",
    ] {
        let html = chapter_html(&format!("<p><a href='{href}'>INACTIVE</a></p>"));
        let plain = present_outlined(&html, Format::Epub, &Options::new(240, false));
        let enabled = present_outlined(
            &html,
            Format::Epub,
            &Options {
                links: true,
                ..Options::new(240, false)
            },
        );
        assert_eq!(enabled.text, plain.text, "invalid source target {href:?}");
        assert!(enabled.text.contains("INACTIVE"));
        assert!(!enabled.text.contains('\x1b'));
        assert!(enabled.links.is_empty());
    }
}

#[test]
fn unsafe_entity_expansions_and_repaired_targets_do_not_become_internal_actions() {
    for href in [
        "#caf&NewLine;",
        "#caf&Tab;",
        "#caf&#128;",
        "#caf&#0;",
        "#caf&#xFFFD;",
        "#caf%EF%BF%BD",
        "#caf\u{fffd}",
        "#caf\n",
        "#caf\t",
        "file&colon;///chapter.xhtml#safe",
        "javascript&colon;safe",
        "&sol;&sol;host/chapter.xhtml#safe",
        "..&sol;chapter.xhtml#safe",
    ] {
        let html = chapter_html(&format!(
            "<p><a href='{href}'>UNSAFE</a></p><p><a href='#safe'>SAFE</a></p>\
<h2 id='caf'>TRIMMED_DECOY</h2><h2 id='caf€'>HTML_REPAIR_DECOY</h2>\
<h2 id='caf&#10;'>NEWLINE_DECOY</h2><h2 id='caf&#9;'>TAB_DECOY</h2>\
<h2 id='caf&#128;'>CONTROL_DECOY</h2><h2 id='caf&#0;'>NULL_DECOY</h2>\
<h2 id='caf&#xFFFD;'>REPAIR_DECOY</h2><h2 id='safe'>SAFE_TARGET</h2>"
        ));
        assert_destinations(&html, &[("SAFE", "SAFE_TARGET")]);
        let rendered = present_outlined(&html, Format::Epub, &Options::new(60, false));
        assert!(
            rendered.text.contains("UNSAFE"),
            "fallback remains readable for {href:?}"
        );
        assert!(
            rendered
                .text
                .chars()
                .all(|ch| !ch.is_control() || ch == '\n')
        );
    }
}
