// Author: machinageist
// Description: EPUB destinations cannot escape their chapter through deferred anchors

mod common;

use common::strip_sgr;
use mu::{Format, Options, Rendered, extract, present_outlined};

// Stored ZIP entries exercise the public extraction boundary without fixture binaries.
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

fn chapters_html(chapters: &[(&str, &str)]) -> String {
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
    extract(&zip(&files), Format::Epub).expect("actual EPUB extracts")
}

fn destination<'a>(rendered: &'a Rendered, label: &str) -> &'a mu::Anchor {
    let link = rendered
        .links
        .iter()
        .find(|link| link.label == label)
        .unwrap_or_else(|| panic!("missing link {label:?}: {:?}", rendered.links));
    rendered
        .anchors
        .iter()
        .find(|anchor| anchor.id == link.target)
        .expect("retained navigation has a destination")
}

#[test]
fn hidden_only_chapter_does_not_resolve_to_the_next_chapters_heading() {
    let html = chapters_html(&[
        (
            "first.xhtml",
            "<html><body><p><a href='empty.xhtml'>EMPTY_LINK</a> <a href='third.xhtml'>THIRD_LINK</a></p></body></html>",
        ),
        (
            "empty.xhtml",
            "<html><body><script>HIDDEN</script></body></html>",
        ),
        (
            "third.xhtml",
            "<html><body><h1>UNRELATED_THIRD_CHAPTER</h1></body></html>",
        ),
    ]);
    for width in [20, 60] {
        for links in [false, true] {
            for color in [false, true] {
                let rendered = present_outlined(
                    &html,
                    Format::Epub,
                    &Options {
                        links,
                        ..Options::new(width, color)
                    },
                );
                let labels: Vec<_> = rendered
                    .links
                    .iter()
                    .map(|link| link.label.as_str())
                    .collect();
                assert_eq!(labels, ["THIRD_LINK"], "width {width}, color {color}");
                let visible = strip_sgr(&rendered.text);
                assert!(visible.contains("EMPTY_LINK"));
                assert!(!visible.contains("HIDDEN"));
                let heading = rendered
                    .headings
                    .iter()
                    .find(|heading| heading.text == "UNRELATED_THIRD_CHAPTER")
                    .expect("the unrelated chapter still renders");
                assert_eq!(destination(&rendered, "THIRD_LINK").line, heading.line);
            }
        }
    }
}

#[test]
fn chapter_end_discards_unpainted_ids_instead_of_borrowing_a_sibling_destination() {
    let html = chapters_html(&[
        (
            "first.xhtml",
            r##"<html><body><p>FIRST_CONTENT</p>
<p><a href="#tail">LOCAL_END</a> <a href="#legacy">LOCAL_NAME</a>
<a href="#empty-block">EMPTY_BLOCK</a> <a href="#empty-heading">EMPTY_HEADING</a>
<a href="#empty-code">EMPTY_CODE</a> <a href="#empty-row">EMPTY_ROW</a></p>
<a id="tail" name="legacy"></a><p id="empty-block"></p><h2 id="empty-heading"></h2>
<p><code id="empty-code"></code></p><table><tbody><tr id="empty-row"></tr></tbody></table>
</body></html>"##,
        ),
        (
            "second.xhtml",
            "<html><body><h1>SECOND_CONTENT</h1><p><a href='first.xhtml#tail'>CROSS_END</a> <a href='first.xhtml#legacy'>CROSS_NAME</a></p></body></html>",
        ),
    ]);
    for width in [20, 60] {
        for links in [false, true] {
            for color in [false, true] {
                let rendered = present_outlined(
                    &html,
                    Format::Epub,
                    &Options {
                        links,
                        ..Options::new(width, color)
                    },
                );
                assert!(rendered.links.is_empty(), "{:?}", rendered.links);
                let visible = strip_sgr(&rendered.text);
                for label in [
                    "FIRST_CONTENT",
                    "SECOND_CONTENT",
                    "LOCAL_END",
                    "LOCAL_NAME",
                    "EMPTY_BLOCK",
                    "EMPTY_HEADING",
                    "EMPTY_CODE",
                    "EMPTY_ROW",
                    "CROSS_END",
                    "CROSS_NAME",
                ] {
                    assert!(visible.contains(label), "missing readable label {label}");
                }
                assert_eq!(rendered.headings.len(), 1);
                assert_eq!(rendered.headings[0].text, "SECOND_CONTENT");
                assert_eq!(rendered.anchors.len(), 2, "only nonempty chapter tops bind");
            }
        }
    }
}

#[test]
fn empty_chapters_at_spine_edges_or_between_chapters_never_invent_destinations() {
    for empty in [
        "<html><body></body></html>",
        "<html><body> \n </body></html>",
        "<html id='root'><body id='body'><a id='empty' name='legacy'></a><p id='block'></p><h2></h2><code></code></body></html>",
    ] {
        for empty_at in [0, 1, 2] {
            let mut chapters = vec![
                (
                    "source.xhtml",
                    "<html><body><p><a href='empty.xhtml'>EMPTY_TOP</a> <a href='empty.xhtml#root'>EMPTY_ROOT</a> <a href='empty.xhtml#body'>EMPTY_BODY</a> <a href='empty.xhtml#empty'>EMPTY_ID</a> <a href='empty.xhtml#legacy'>EMPTY_NAME</a> <a href='empty.xhtml#block'>EMPTY_BLOCK</a> <a href='visible.xhtml'>VISIBLE_LINK</a></p></body></html>",
                ),
                (
                    "visible.xhtml",
                    "<html><body><h1>VISIBLE_CHAPTER</h1></body></html>",
                ),
            ];
            chapters.insert(empty_at, ("empty.xhtml", empty));
            let html = chapters_html(&chapters);
            for width in [20, 60] {
                for links in [false, true] {
                    for color in [false, true] {
                        let rendered = present_outlined(
                            &html,
                            Format::Epub,
                            &Options {
                                links,
                                ..Options::new(width, color)
                            },
                        );
                        let labels: Vec<_> = rendered
                            .links
                            .iter()
                            .map(|link| link.label.as_str())
                            .collect();
                        assert_eq!(labels, ["VISIBLE_LINK"], "empty spine index {empty_at}");
                        assert_eq!(rendered.anchors.len(), 2);
                        let visible = strip_sgr(&rendered.text);
                        assert!(visible.contains("EMPTY_TOP"));
                        assert!(visible.contains("EMPTY_ID"));
                        assert_eq!(
                            visible
                                .lines()
                                .nth(destination(&rendered, "VISIBLE_LINK").line),
                            Some("VISIBLE_CHAPTER")
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn recovered_unclosed_markup_cannot_swallow_following_chapter_boundaries() {
    for unclosed in [
        "<h2><div>",
        "<pre><div>",
        "<code><div>",
        "<table><tr><td><div>",
    ] {
        for ending in ["</body></html>", ""] {
            let first = format!(
                "<html><body><p><a href='empty.xhtml'>EMPTY</a> <a href='third.xhtml#dest'>VALID</a></p>{unclosed}{ending}"
            );
            let html = chapters_html(&[
                ("first.xhtml", &first),
                ("empty.xhtml", "<html><body></body></html>"),
                (
                    "third.xhtml",
                    "<html><body><h1 id='dest'>THIRD_DESTINATION</h1></body></html>",
                ),
            ]);
            for width in [20, 60] {
                let rendered = present_outlined(&html, Format::Epub, &Options::new(width, false));
                assert_eq!(
                    rendered
                        .links
                        .iter()
                        .map(|link| link.label.as_str())
                        .collect::<Vec<_>>(),
                    ["VALID"],
                    "{unclosed:?}, {ending:?}, width {width}"
                );
                assert!(rendered.text.contains("EMPTY"));
                assert_eq!(
                    rendered
                        .text
                        .lines()
                        .nth(destination(&rendered, "VALID").line),
                    Some("THIRD_DESTINATION")
                );
            }
        }
    }
}

#[test]
fn zero_width_ids_and_named_anchors_still_find_following_content_in_their_chapter() {
    let html = chapters_html(&[
        (
            "first.xhtml",
            r##"<html><body>
<p><a href="#inline">INLINE</a> <a href="#legacy">LEGACY</a>
<a href="#block">BLOCK</a> <a href="#through-div">THROUGH_DIV</a></p>
<p>prefix <a id="inline"></a>INLINE_TARGET</p>
<a name="legacy"></a><h2>LEGACY_TARGET</h2>
<p id="block"><span></span></p><p>BLOCK_TARGET</p>
<a id="through-div"></a><div id="epub-00-top"></div><p>DIV_TARGET</p>
</body></html>"##,
        ),
        (
            "second.xhtml",
            "<html><body><p>UNRELATED_SIBLING</p></body></html>",
        ),
    ]);
    for width in [20, 60] {
        for links in [false, true] {
            for color in [false, true] {
                let rendered = present_outlined(
                    &html,
                    Format::Epub,
                    &Options {
                        links,
                        ..Options::new(width, color)
                    },
                );
                assert_eq!(rendered.links.len(), 4);
                for (label, expected, column) in [
                    ("INLINE", "prefix INLINE_TARGET", 7),
                    ("LEGACY", "LEGACY_TARGET", 0),
                    ("BLOCK", "BLOCK_TARGET", 0),
                    ("THROUGH_DIV", "DIV_TARGET", 0),
                ] {
                    let anchor = destination(&rendered, label);
                    let line = strip_sgr(rendered.text.lines().nth(anchor.line).unwrap());
                    assert_eq!(line, expected, "{label}, width {width}, color {color}");
                    assert_eq!(anchor.column, column, "{label}, width {width}");
                }
            }
        }
    }
}

#[test]
fn only_reserved_epub_chapter_wrappers_end_the_pending_anchor_scope() {
    for (id, scoped) in [
        ("epub-00-top", true),
        ("epub--top", false),
        ("epub-a-top", false),
        ("epub-gg-top", false),
        ("epub-00-id-00-top", false),
        ("other-00-top", false),
    ] {
        let html = format!(
            "<p><a href='#before'>BEFORE</a> <a href='#inside'>INSIDE</a></p><a id='before'></a><div id='{id}'><a id='inside'></a></div><p>TARGET</p>"
        );
        let rendered = present_outlined(&html, Format::Epub, &Options::new(60, false));
        assert_eq!(rendered.text, "BEFORE INSIDE\n\nTARGET\n");
        if scoped {
            assert!(rendered.links.is_empty());
            assert!(rendered.anchors.is_empty());
        } else {
            assert_eq!(rendered.links.len(), 2, "ordinary identity {id}");
            for label in ["BEFORE", "INSIDE"] {
                let anchor = destination(&rendered, label);
                assert_eq!(rendered.text.lines().nth(anchor.line), Some("TARGET"));
            }
        }
        let plain_html = present_outlined(&html, Format::Html, &Options::new(60, false));
        assert_eq!(
            plain_html.text,
            "BEFORE (#before) INSIDE (#inside)\n\nTARGET\n"
        );
        assert!(plain_html.links.is_empty());
        assert!(plain_html.anchors.is_empty());
    }
}
