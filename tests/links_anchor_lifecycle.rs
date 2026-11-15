// Author: machinageist
// Description: Real EPUB anchors bind forward to painted content, never block decoration

mod common;

use common::strip_sgr;
use mu::{Format, ImageMode, ImageOptions, Options, Protocol, Rendered, extract, present_outlined};
use unicode_width::UnicodeWidthStr;

// Stored ZIP entries exercise extraction and chapter scoping without binary fixtures.
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

fn epub(chapters: &[(&str, &str)]) -> String {
    epub_with_assets(chapters, &[])
}

fn epub_with_assets(chapters: &[(&str, &str)], assets: &[(&str, &[u8])]) -> String {
    let container =
        b"<container><rootfiles><rootfile full-path='OEBPS/book.opf'/></rootfiles></container>";
    let mut package = String::from("<package><manifest>");
    for (index, (path, _)) in chapters.iter().enumerate() {
        package.push_str(&format!(
            "<item id='c{index}' href='{path}' media-type='application/xhtml+xml'/>"
        ));
    }
    package.push_str("</manifest><spine>");
    for index in 0..chapters.len() {
        package.push_str(&format!("<itemref idref='c{index}'/>"));
    }
    package.push_str("</spine></package>");
    let paths: Vec<_> = chapters
        .iter()
        .map(|(path, _)| format!("OEBPS/{path}"))
        .collect();
    let mut files = vec![
        ("mimetype", b"application/epub+zip".as_slice()),
        ("META-INF/container.xml", container.as_slice()),
        ("OEBPS/book.opf", package.as_bytes()),
    ];
    files.extend(
        chapters
            .iter()
            .zip(&paths)
            .map(|((_, chapter), path)| (path.as_str(), chapter.as_bytes())),
    );
    files.extend_from_slice(assets);
    extract(&zip(&files), Format::Epub).expect("actual EPUB extracts")
}

fn each_mode(html: &str, mut check: impl FnMut(&Rendered)) {
    for width in [20, 60] {
        for color in [false, true] {
            for links in [false, true] {
                let rendered = present_outlined(
                    html,
                    Format::Epub,
                    &Options {
                        links,
                        ..Options::new(width, color)
                    },
                );
                check(&rendered);
            }
        }
    }
}

fn assert_destination(rendered: &Rendered, label: &str, token: &str) {
    let link = rendered
        .links
        .iter()
        .find(|link| link.label == label)
        .unwrap_or_else(|| panic!("missing link {label}: {:?}", rendered.links));
    let anchor = rendered
        .anchors
        .iter()
        .find(|anchor| anchor.id == link.target)
        .expect("retained link has an anchor");
    let text = strip_sgr(&rendered.text);
    let (line, content) = text
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(token))
        .unwrap_or_else(|| panic!("missing token {token}: {text:?}"));
    let column = content[..content.find(token).unwrap()].width();
    assert_eq!(
        (anchor.line, anchor.column),
        (line, column),
        "{label} must point to {token}, not preceding text, padding or borders: {text:?}"
    );
}

fn assert_forward_lifetime(source: &str) {
    for following in ["<p>FORWARD_TARGET</p>", ""] {
        let first = format!(
            "<html><body><p><a href='#tail'>LOCAL_TAIL</a></p>{source}{following}</body></html>"
        );
        let html = epub(&[
            ("first.xhtml", &first),
            (
                "second.xhtml",
                "<html><body><p>UNRELATED_NEXT</p><p><a href='first.xhtml#tail'>CROSS_TAIL</a></p></body></html>",
            ),
        ]);
        each_mode(&html, |rendered| {
            if following.is_empty() {
                assert!(rendered.links.is_empty(), "{source}: {:?}", rendered.links);
            } else {
                assert_eq!(rendered.links.len(), 2, "{source}: {:?}", rendered.links);
                for label in ["LOCAL_TAIL", "CROSS_TAIL"] {
                    assert_destination(rendered, label, "FORWARD_TARGET");
                }
            }
            let text = strip_sgr(&rendered.text);
            for label in ["LOCAL_TAIL", "CROSS_TAIL", "UNRELATED_NEXT"] {
                assert!(text.contains(label), "missing readable {label}: {text:?}");
            }
        });
    }
}

#[test]
fn trailing_heading_and_inline_code_ids_keep_their_forward_lifetime() {
    for source in [
        "<h2>BEFORE<span id='tail'/></h2>",
        "<p><code>BEFORE<span id='tail'/></code></p>",
        "<p>BEFORE<strong><code><span id='tail'/></code></strong></p>",
    ] {
        assert_forward_lifetime(source);
    }
}

#[test]
fn raw_code_ids_with_only_trailing_whitespace_still_wait_for_content() {
    for source in [
        "<pre class='language-rust'>let before = 1;<span id='tail'/></pre>",
        "<pre><code>BEFORE<span id='tail'/>\n \t\n</code></pre>",
        "<pre id='tail'><code/></pre>",
    ] {
        assert_forward_lifetime(source);
    }
}

#[test]
fn deferred_ids_skip_empty_container_markers_and_table_padding() {
    for source in [
        "<p>BEFORE<span id='tail'/></p><ul><li/></ul>",
        "<ul><li id='tail'/></ul>",
        "<p>BEFORE<span id='tail'/></p><blockquote><ul><li/></ul></blockquote>",
        "<p>BEFORE<span id='tail'/></p><pre/><table><tr><td/></tr></table>",
        "<table><tr><td>BEFORE<span id='tail'/></td></tr></table>",
        "<table><tr><td>BEFORE</td><td id='tail'/></tr></table>",
        "<table><tr><td>BEFORE</td></tr><tbody id='tail'/></table><table><tr><td/></tr></table>",
    ] {
        assert_forward_lifetime(source);
    }
}

#[test]
fn unicode_whitespace_cannot_consume_deferred_ids() {
    for whitespace in ["\u{a0}", "\u{2003}", "\u{202f}", "&#8195;"] {
        for source in [
            format!("<p>BEFORE<span id='tail'/>{whitespace}</p>"),
            format!("<p>BEFORE<span id='tail'/></p><p>{whitespace}</p>"),
            format!("<p>BEFORE<span id='tail'/></p><h2>{whitespace}</h2>"),
            format!("<table><tr><td>BEFORE<span id='tail'/>{whitespace}</td></tr></table>"),
        ] {
            assert_forward_lifetime(&source);
        }
    }
}

#[test]
fn unicode_whitespace_prefixes_bind_ids_at_the_following_glyph_column() {
    for (source_whitespace, painted_whitespace) in [
        ("\u{a0}", "\u{a0}"),
        ("\u{2003}", "\u{2003}"),
        ("\u{202f}", "\u{202f}"),
        ("\u{3000}", "\u{3000}"),
        ("&#8195;", "\u{2003}"),
    ] {
        for destination in [
            format!("<p><span id='tail'/>{source_whitespace}FORWARD_TARGET</p>"),
            format!("<p id='tail'>{source_whitespace}<strong>FORWARD_TARGET</strong></p>"),
            format!("<p>BEFORE<span id='tail'/></p><p>{source_whitespace}FORWARD_TARGET</p>"),
            format!("<h2 id='tail'>{source_whitespace}FORWARD_TARGET</h2>"),
            format!(
                "<p>BEFORE<span id='tail'/></p><blockquote><ul><li>{source_whitespace}FORWARD_TARGET</li></ul></blockquote>"
            ),
            format!("<table id='tail'><tr><td>{source_whitespace}FORWARD_TARGET</td></tr></table>"),
            format!("<table><tr><td id='tail'>{source_whitespace}FORWARD_TARGET</td></tr></table>"),
        ] {
            let first = format!(
                "<html><body><p><a href='#tail'>PREFIX_LINK</a></p>{destination}</body></html>"
            );
            let html = epub(&[("first.xhtml", &first)]);
            each_mode(&html, |rendered| {
                assert_eq!(rendered.links.len(), 1);
                assert_destination(rendered, "PREFIX_LINK", "FORWARD_TARGET");
                let text = strip_sgr(&rendered.text);
                assert!(
                    text.contains(&format!("{painted_whitespace}FORWARD_TARGET")),
                    "Unicode whitespace must retain its original rendering: {text:?}"
                );
            });
        }
    }
}

#[test]
fn wrapped_unicode_whitespace_keeps_ids_pending_until_the_actual_glyph() {
    for whitespace in ['\u{a0}', '\u{2003}', '\u{202f}', '\u{3000}'] {
        let prefix = whitespace.to_string().repeat(121);
        for destination in [
            format!("<p><span id='tail'/>{prefix}FORWARD_TARGET</p>"),
            format!("<p id='tail'>{prefix}FORWARD_TARGET</p>"),
            format!(
                "<p>BEFORE<span id='tail'/></p><h3>{prefix}<strong>FORWARD_TARGET</strong></h3>"
            ),
        ] {
            let first = format!(
                "<html><body><p><a href='#tail'>WRAPPED_LINK</a></p>{destination}</body></html>"
            );
            let html = epub(&[("first.xhtml", &first)]);
            each_mode(&html, |rendered| {
                assert_eq!(rendered.links.len(), 1);
                assert_destination(rendered, "WRAPPED_LINK", "FORWARD_TARGET");
                let text = strip_sgr(&rendered.text);
                assert_eq!(text.chars().filter(|ch| *ch == whitespace).count(), 121);
                assert!(
                    text.lines()
                        .filter(|line| !line.is_empty() && line.chars().all(char::is_whitespace))
                        .count()
                        >= 2,
                    "the fixture must wrap through whitespace-only lines: {text:?}"
                );
            });
        }
    }
}

#[test]
fn deferred_ids_skip_wrapped_unicode_whitespace_blocks_or_expire_with_the_chapter() {
    for whitespace in ["\u{a0}", "\u{2003}", "\u{202f}", "\u{3000}", "&#8195;"] {
        let run = whitespace.repeat(121);
        let blocks = [
            format!("<p>{run}</p>"),
            format!("<h2>{run}</h2>"),
            format!("<blockquote><ul><li>{run}</li></ul></blockquote>"),
            format!("<pre>{run}</pre>"),
            format!("<table><tr><td>{run}</td></tr></table>"),
        ];
        for following in blocks
            .iter()
            .cloned()
            .chain(std::iter::once(blocks.concat()))
        {
            assert_forward_lifetime(&format!("<p>BEFORE<span id='tail'/>{run}</p>{following}"));
        }
    }
}

#[test]
fn unicode_whitespace_anchor_placement_preserves_text_style_and_osc8_bytes() {
    let first = "<html><body><p><a href='#tail'>GO</a></p><p><span id='tail'/><strong>\u{a0}\u{2003}\u{3000}TARGET</strong></p><p><a href='https://example.org/x'>WEB</a></p></body></html>";
    let control = first
        .replace("<span id='tail'/>", "")
        .replace("TARGET", "<span id='tail'/>TARGET");
    let html = epub(&[("first.xhtml", first)]);
    let control_html = epub(&[("first.xhtml", &control)]);
    for width in [20, 60] {
        for color in [false, true] {
            for links in [false, true] {
                let options = Options {
                    links,
                    ..Options::new(width, color)
                };
                let rendered = present_outlined(&html, Format::Epub, &options);
                let expected = present_outlined(&control_html, Format::Epub, &options);
                assert_eq!(rendered, expected, "anchor placement is metadata only");
                assert!(rendered.text.contains("\u{a0}\u{2003}\u{3000}TARGET"));
                assert_eq!(rendered.text.contains("\x1b["), color);
                assert_eq!(
                    rendered
                        .text
                        .contains("\x1b]8;;https://example.org/x\x1b\\WEB\x1b]8;;\x1b\\"),
                    links && !color
                );
                assert_eq!(
                    rendered
                        .text
                        .contains("\x1b]8;;https://example.org/x\x1b\\"),
                    links
                );
            }
        }
    }
}

#[test]
fn empty_inline_styles_and_zero_width_text_do_not_paint_destinations() {
    for source in [
        "<p>BEFORE<span id='tail'/><strong></strong>\u{200b}</p>",
        "<p>BEFORE<span id='tail'>\u{200b}</span></p>",
        "<p>BEFORE<span id='tail'/></p><p>\u{200b}</p>",
        "<p>BEFORE<span id='tail'/></p><h2>\u{200b}</h2>",
        "<p id='tail'>\u{200b}</p>",
    ] {
        assert_forward_lifetime(source);
    }
    for empty in ["<strong/>", "<em><strong/></em>", "<a href='#missing'/>"] {
        let first = format!(
            "<html><body><p><a href='#tail'>GAP_LINK</a></p><p>BEFORE<span id='tail'/>{empty} GAP_TARGET</p></body></html>"
        );
        let html = epub(&[("first.xhtml", &first)]);
        each_mode(&html, |rendered| {
            assert_eq!(rendered.links.len(), 1);
            assert_destination(rendered, "GAP_LINK", "GAP_TARGET");
        });
    }
}

#[test]
fn leading_named_raw_and_table_identities_still_bind_to_their_first_content() {
    let html = epub(&[(
        "first.xhtml",
        r##"<html><body><p>
<a href="#lead">LEADING</a> <a href="#legacy">NAMED</a> <a href="#code">INLINE_CODE</a>
<a href="#raw">RAW</a> <a href="#raw-inner">RAW_INNER</a>
<a href="#table">TABLE</a> <a href="#group">GROUP</a> <a href="#row">ROW</a>
<a href="#empty">EMPTY_CELL</a> <a href="#cell">CELL</a>
<a href="#empty-group">EMPTY_GROUP</a> <a href="#group2">GROUP_TWO</a>
<a href="#row2">ROW_TWO</a> <a href="#table-name">TABLE_NAME</a></p>
<p id="lead"><a name="legacy"/><strong>LEAD_TARGET</strong></p>
<p><code><span id="code"/>CODE_TARGET</code></p>
<pre id="raw" class="language-rust">
	<span id="raw-inner">RAW_TARGET</span></pre>
<table id="table"><tbody id="group"><tr id="row"><td id="empty"/><td id="cell">CELL_X</td></tr></tbody>
<tbody id="empty-group"/><tbody id="group2"><tr id="row2"><td><a name="table-name"/>ROW_X</td><td>OTHER</td></tr></tbody></table>
</body></html>"##,
    )]);
    each_mode(&html, |rendered| {
        assert_eq!(rendered.links.len(), 14);
        for (label, token) in [
            ("LEADING", "LEAD_TARGET"),
            ("NAMED", "LEAD_TARGET"),
            ("INLINE_CODE", "CODE_TARGET"),
            ("RAW", "RAW_TARGET"),
            ("RAW_INNER", "RAW_TARGET"),
            ("TABLE", "CELL_X"),
            ("GROUP", "CELL_X"),
            ("ROW", "CELL_X"),
            ("EMPTY_CELL", "CELL_X"),
            ("CELL", "CELL_X"),
            ("EMPTY_GROUP", "ROW_X"),
            ("GROUP_TWO", "ROW_X"),
            ("ROW_TWO", "ROW_X"),
            ("TABLE_NAME", "ROW_X"),
        ] {
            assert_destination(rendered, label, token);
        }
    });
}

#[test]
fn deferred_ids_bind_inside_following_sibling_content_not_its_chrome() {
    for following in [
        "<p>FORWARD_TARGET</p>",
        "<h2>FORWARD_TARGET</h2>",
        "<blockquote><ul><li>FORWARD_TARGET</li></ul></blockquote>",
        "<pre class='language-rust'>\n\tFORWARD_TARGET</pre>",
        "<table><tr><td/></tr><tr><td>FORWARD_TARGET</td></tr></table>",
        "<div><p/><div><p>FORWARD_TARGET</p></div></div>",
    ] {
        let first = format!(
            "<html><body><p><a href='#tail'>SIBLING</a></p><p>BEFORE<span id='tail'/></p>{following}</body></html>"
        );
        let html = epub(&[("first.xhtml", &first)]);
        each_mode(&html, |rendered| {
            assert_eq!(rendered.links.len(), 1);
            assert_destination(rendered, "SIBLING", "FORWARD_TARGET");
        });
    }
}

#[test]
fn anchors_after_standalone_images_outlive_the_picture_and_caption() {
    for following in ["<p>AFTER_PICTURE</p>", ""] {
        let first = format!(
            "<html><body><p><a href='#picture'>PICTURE</a> <a href='#tail'>TAIL</a></p><p><img id='picture' src='wide.svg' alt='ART'/><span id='tail'/></p>{following}</body></html>"
        );
        let html = epub_with_assets(
            &[
                ("first.xhtml", &first),
                (
                    "second.xhtml",
                    "<html><body><p>UNRELATED_NEXT</p><p><a href='first.xhtml#tail'>CROSS_TAIL</a></p></body></html>",
                ),
            ],
            &[("OEBPS/wide.svg", include_bytes!("fixtures/images/wide.svg"))],
        );
        for width in [20, 60] {
            for color in [false, true] {
                for links in [false, true] {
                    let rendered = present_outlined(
                        &html,
                        Format::Epub,
                        &Options {
                            links,
                            images: ImageOptions {
                                mode: ImageMode::Auto,
                                protocol: Protocol::HalfBlock,
                                max_rows: Some(2),
                                ..ImageOptions::default()
                            },
                            ..Options::new(width, color)
                        },
                    );
                    assert_destination(&rendered, "PICTURE", if color { "▀" } else { "[image:" });
                    if following.is_empty() {
                        assert_eq!(rendered.links.len(), 1, "{:?}", rendered.links);
                    } else {
                        assert_eq!(rendered.links.len(), 3);
                        for label in ["TAIL", "CROSS_TAIL"] {
                            assert_destination(&rendered, label, "AFTER_PICTURE");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn inline_pagebreak_after_text_binds_forward_or_expires_at_chapter_end() {
    for following in ["<p>NEXT_PAGE_START</p>", ""] {
        let first = format!(
            "<html><body><p><a href='#page2'>PAGE_TWO</a></p><p>PREVIOUS_PAGE<span epub:type='pagebreak' id='page2'/></p>{following}</body></html>"
        );
        let html = epub(&[
            ("first.xhtml", &first),
            (
                "second.xhtml",
                "<html><body><p>UNRELATED_NEXT</p><p><a href='first.xhtml#page2'>CROSS_PAGE</a></p></body></html>",
            ),
        ]);
        each_mode(&html, |rendered| {
            if following.is_empty() {
                assert!(rendered.links.is_empty(), "{:?}", rendered.links);
            } else {
                assert_eq!(rendered.links.len(), 2);
                for label in ["PAGE_TWO", "CROSS_PAGE"] {
                    assert_destination(rendered, label, "NEXT_PAGE_START");
                }
            }
            let text = strip_sgr(&rendered.text);
            for token in ["PAGE_TWO", "PREVIOUS_PAGE", "UNRELATED_NEXT", "CROSS_PAGE"] {
                assert!(text.contains(token), "missing readable {token}: {text:?}");
            }
        });
    }
}

#[test]
fn unconsumed_trailing_table_groups_never_bind_back_to_table_borders() {
    for group in ["thead", "tbody", "tfoot"] {
        for following in ["<p>AFTER_TABLE</p>", ""] {
            let first = format!(
                "<html><body><p><a href='#empty'>EMPTY_GROUP</a></p><table><tbody><tr><td>ACTUAL_ROW</td></tr></tbody><{group} id='empty'/></table>{following}</body></html>"
            );
            let html = epub(&[
                ("first.xhtml", &first),
                (
                    "second.xhtml",
                    "<html><body><p>UNRELATED_NEXT</p><p><a href='first.xhtml#empty'>CROSS_GROUP</a></p></body></html>",
                ),
            ]);
            each_mode(&html, |rendered| {
                if following.is_empty() {
                    assert!(rendered.links.is_empty(), "{group}: {:?}", rendered.links);
                } else {
                    assert_eq!(rendered.links.len(), 2);
                    for label in ["EMPTY_GROUP", "CROSS_GROUP"] {
                        assert_destination(rendered, label, "AFTER_TABLE");
                    }
                }
                assert!(strip_sgr(&rendered.text).contains("ACTUAL_ROW"));
            });
        }
    }
}
