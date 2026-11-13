// Author: machinageist
// Description: HTML link fallback and namespace-aware EPUB navigation at public boundaries

mod common;

use common::strip_sgr;
use mu::{Format, Options, extract, present_outlined};

// Stored ZIP records keep the extraction boundary real without another dependency.
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
        (
            "OEBPS/picture.svg",
            include_bytes!("fixtures/images/wide.svg").as_slice(),
        ),
    ];
    files.extend(
        chapters
            .iter()
            .zip(&paths)
            .map(|((_, chapter), path)| (path.as_str(), chapter.as_bytes())),
    );
    extract(&zip(&files), Format::Epub).expect("actual EPUB extracts")
}

#[test]
fn prefixed_xhtml_chapter_links_retain_blocks_and_exact_destinations() {
    let html = chapters_html(&[(
        "chapter.xhtml",
        "<h:html xmlns:h='http://www.w3.org/1999/xhtml'><h:body><h:p><h:a href='#dest'>PREFIX_LINK</h:a></h:p><h:p id='dest'>DESTINATION</h:p></h:body></h:html>",
    )]);
    for width in [24, 60] {
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
                assert_eq!(strip_sgr(&rendered.text), "PREFIX_LINK\n\nDESTINATION\n");
                assert_eq!(rendered.links.len(), 1);
                let link = &rendered.links[0];
                assert_eq!(link.label, "PREFIX_LINK");
                let anchor = rendered
                    .anchors
                    .iter()
                    .find(|anchor| anchor.id == link.target)
                    .expect("prefixed XHTML link resolves to its own chapter destination");
                assert_eq!(
                    strip_sgr(rendered.text.lines().nth(anchor.line).unwrap()),
                    "DESTINATION"
                );
                assert_ne!(anchor.id, "dest", "chapter IDs remain scoped");
            }
        }
    }
}

#[test]
fn namespace_bindings_are_scoped_and_foreign_names_do_not_become_html_links_or_images() {
    let html = chapters_html(&[
        (
            "one.xhtml",
            r##"<h:html xmlns:h="http://www.w3.org/1999/xhtml" xmlns:q="http://www.w3.org/1999/xhtml"><h:body>
<h:p><q:a xmlns:q="urn:foreign" href="#dest">FOREIGN_PREFIX</q:a>
<q:span xmlns:q="urn:foreign"/><q:a href="#dest">RESTORED_PREFIX</q:a><u:a href="#dest">UNBOUND_PREFIX</u:a>
<r:a xmlns:r="http://www.w3.org/1999/xhtml" href="two.xhtml#dest">LOCAL_DECLARATION</r:a></h:p>
<h:section xmlns:h="urn:foreign"><h:a href="https://foreign.example/">FOREIGN_WEB</h:a><h:img src="picture.svg" alt="FOREIGN_IMAGE"/></h:section>
<section xmlns="urn:foreign"><a href="#dest">FOREIGN_DEFAULT</a><body>FOREIGN_BODY</body></section>
<h:p><h:a href="#dest">AFTER_SHADOW</h:a><h:a href="two.xhtml#dest">OTHER_CHAPTER</h:a></h:p>
<h:p id="dest">FIRST_DESTINATION</h:p>
<h:p><h:img src="picture.svg" alt="REAL_IMAGE" id="picture"/></h:p>
<h:p><h:a href="#picture">PICTURE_LINK</h:a></h:p>
<h:p><h:a href="https://example.org/"></h:a></h:p>
</h:body></h:html>"##,
        ),
        (
            "two.xhtml",
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p id="dest">SECOND_DESTINATION</p></body></html>"#,
        ),
    ]);
    assert!(html.contains("src=\"data:image/svg+xml;base64,"));
    for width in [24, 60] {
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
                assert_eq!(
                    labels,
                    [
                        "RESTORED_PREFIX",
                        "LOCAL_DECLARATION",
                        "AFTER_SHADOW",
                        "OTHER_CHAPTER",
                        "PICTURE_LINK"
                    ]
                );
                for (label, expected) in [
                    ("RESTORED_PREFIX", "FIRST_DESTINATION"),
                    ("AFTER_SHADOW", "FIRST_DESTINATION"),
                    ("LOCAL_DECLARATION", "SECOND_DESTINATION"),
                    ("OTHER_CHAPTER", "SECOND_DESTINATION"),
                    ("PICTURE_LINK", "[image: REAL_IMAGE]"),
                ] {
                    let link = rendered
                        .links
                        .iter()
                        .find(|link| link.label == label)
                        .unwrap();
                    let anchor = rendered
                        .anchors
                        .iter()
                        .find(|anchor| anchor.id == link.target)
                        .unwrap();
                    assert_eq!(
                        strip_sgr(rendered.text.lines().nth(anchor.line).unwrap()),
                        expected
                    );
                }
                assert!(!rendered.text.contains("FOREIGN_IMAGE"));
                assert!(!rendered.text.contains("foreign.example"));
                assert!(
                    strip_sgr(&rendered.text)
                        .replace('\n', "")
                        .contains("FOREIGN_BODY")
                );
                assert!(strip_sgr(&rendered.text).contains("(https://example.org/)"));
            }
        }
    }
}

#[test]
fn prefixed_hidden_subtrees_cannot_supply_headings_anchors_or_namespace_changes() {
    let html = chapters_html(&[(
        "chapter.xhtml",
        r##"<h:html xmlns:h="http://www.w3.org/1999/xhtml" class="language-python" id="root"><h:head><h:title>HIDDEN HEAD</h:title></h:head><h:body id="body">
<h:h2 id="topic">Clean<h:a class="headerlink" href="#topic">¶</h:a><h:template><h:h1>HIDDEN HEADING</h:h1><h:p id="hidden">HIDDEN TEMPLATE</h:p></h:template></h:h2>
<h:p><h:a href="#root">ROOT</h:a> <h:a href="#body">BODY</h:a> <h:a href="#hidden">MISSING_TARGET</h:a> <h:a href="#dest">DEST_LINK</h:a></h:p>
<h:script><![CDATA[var value = "</h:script><h:h1>HIDDEN CDATA</h:h1>";]]></h:script>
<h:script>var value = "<h:body xmlns:h='urn:foreign'><h:p id='fake'>HIDDEN SCRIPT</h:p>";</h:script>
<h:pre><h:code>def visible(): return None</h:code></h:pre><h:p id="dest">VISIBLE_DESTINATION</h:p>
</h:body></h:html>"##,
    )]);
    for width in [30, 60] {
        for links in [false, true] {
            let mut plain = None;
            for color in [false, true] {
                let rendered = present_outlined(
                    &html,
                    Format::Epub,
                    &Options {
                        links,
                        ..Options::new(width, color)
                    },
                );
                let visible = strip_sgr(&rendered.text);
                assert!(!visible.contains("HIDDEN "), "{visible:?}");
                assert!(!visible.contains('¶'));
                assert_eq!(rendered.headings.len(), 1);
                assert_eq!(rendered.headings[0].text, "Clean");
                let labels: Vec<_> = rendered
                    .links
                    .iter()
                    .map(|link| link.label.as_str())
                    .collect();
                assert_eq!(labels, ["ROOT", "BODY", "DEST_LINK"]);
                for link in &rendered.links {
                    let anchor = rendered
                        .anchors
                        .iter()
                        .find(|anchor| anchor.id == link.target)
                        .unwrap();
                    let expected = if link.label == "DEST_LINK" {
                        "VISIBLE_DESTINATION"
                    } else {
                        "Clean"
                    };
                    assert_eq!(
                        strip_sgr(rendered.text.lines().nth(anchor.line).unwrap()),
                        expected
                    );
                }
                if color {
                    assert!(rendered.text.contains("\x1b[0;1;35mdef\x1b[0m"));
                    assert_eq!(plain.as_ref(), Some(&visible));
                } else {
                    plain = Some(visible);
                }
            }
        }
    }
}

#[test]
fn prefixed_templates_do_not_close_visible_ancestors_or_expose_hidden_headings() {
    let html = chapters_html(&[(
        "chapter.xhtml",
        "<h:html xmlns:h='http://www.w3.org/1999/xhtml'><h:body><h:h1>Visible<h:template></h:h1><h:h2>HIDDEN</h:h2></h:template> title</h:h1><h:p>Body</h:p></h:body></h:html>",
    )]);
    let rendered = present_outlined(&html, Format::Epub, &Options::new(60, false));
    assert_eq!(rendered.text, "Visible title\n═════════════\n\nBody\n");
    assert_eq!(rendered.headings.len(), 1);
    assert_eq!(rendered.headings[0].text, "Visible title");
}

#[test]
fn unprefixed_mixed_case_chapters_keep_legacy_html_hidden_content_behavior() {
    let html = chapters_html(&[(
        "chapter.xhtml",
        "<HTML><HEAD><TITLE>HIDDEN HEAD</title></head><BODY><SCRIPT>HIDDEN SCRIPT</script><TEMPLATE><H1>HIDDEN HEADING</h1></template><H2>Visible</h2><P>Body</p></body></html>",
    )]);
    let rendered = present_outlined(&html, Format::Epub, &Options::new(60, false));
    assert_eq!(rendered.text, "Visible\n───────\n\nBody\n");
    assert_eq!(rendered.headings.len(), 1);
    assert_eq!(rendered.headings[0].text, "Visible");
}

#[test]
fn empty_heading_and_decorative_links_do_not_create_outline_text() {
    let source = "<h2 id='topic'>Title<a href='#topic'></a><a name='empty'></a><a href='https://example.org/'><template>HIDDEN</template></a></h2><p>Body</p>";
    let epub = chapters_html(&[("chapter.xhtml", source)]);
    for (format, source) in [(Format::Html, source), (Format::Epub, epub.as_str())] {
        for links in [false, true] {
            for color in [false, true] {
                let rendered = present_outlined(
                    source,
                    format,
                    &Options {
                        links,
                        ..Options::new(60, color)
                    },
                );
                assert_eq!(strip_sgr(&rendered.text), "Title\n─────\n\nBody\n");
                assert_eq!(rendered.headings.len(), 1);
                assert_eq!(rendered.headings[0].text, "Title");
                assert!(rendered.links.is_empty());
            }
        }
    }
    for format in [Format::Html, Format::Epub] {
        assert!(
            present_outlined(
                "<a name='decorative'></a><a href='#decorative'></a><em></em>",
                format,
                &Options::new(60, false)
            )
            .text
            .is_empty()
        );
    }
}

#[test]
fn xml_literal_and_instruction_content_cannot_create_epub_actions() {
    for (declaration, content, visible_literal) in [
        (
            "",
            "<p><![CDATA[> <a href='https://example.org/LITERAL'>LITERAL &amp;</a>]]></p>",
            true,
        ),
        (
            "",
            "<?p > <a href='https://example.org/LITERAL'>LITERAL</a> ?>",
            false,
        ),
        (
            "<!DOCTYPE html [<!ENTITY hidden \"> <body><a href='https://example.org/LITERAL'>LITERAL</a></body>\">]>",
            "",
            false,
        ),
    ] {
        let chapter = format!(
            "{declaration}<html xmlns='http://www.w3.org/1999/xhtml'><body>{content}<p>Body</p></body></html>"
        );
        let html = chapters_html(&[("chapter.xhtml", &chapter)]);
        let rendered = present_outlined(
            &html,
            Format::Epub,
            &Options {
                links: true,
                ..Options::new(240, false)
            },
        );
        assert!(
            !rendered.text.contains('\x1b'),
            "XML literal/instruction became an action: {:?}",
            rendered.text
        );
        assert!(rendered.links.is_empty());
        assert!(rendered.text.contains("Body"));
        assert_eq!(rendered.text.contains("LITERAL"), visible_literal);
        if visible_literal {
            assert!(rendered.text.contains("<a href="));
            assert!(rendered.text.contains("&amp;"));
        }
    }
}

#[test]
fn package_metadata_literals_and_instructions_cannot_create_links() {
    let html = extract(&zip(&[
        ("META-INF/container.xml", b"<container><rootfiles><rootfile full-path='book.opf'/></rootfiles></container>"),
        ("book.opf", b"<package><metadata><title><![CDATA[> <a href='https://example.org/LITERAL'>LITERAL &amp;</a>]]></title><creator><?p > <a href='https://example.org/INSTRUCTION'>INSTRUCTION</a> ?></creator></metadata><manifest><item id='a' href='chapter.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='a'/></spine></package>"),
        ("chapter.xhtml", b"<html><body><p>Body</p></body></html>"),
    ]), Format::Epub).unwrap();
    let rendered = present_outlined(
        &html,
        Format::Epub,
        &Options {
            links: true,
            ..Options::new(240, false)
        },
    );
    assert!(
        !rendered.text.contains('\x1b'),
        "literal metadata became an action: {:?}",
        rendered.text
    );
    assert!(rendered.links.is_empty());
    assert!(rendered.text.contains("LITERAL &amp;"));
    assert!(!rendered.text.contains("INSTRUCTION"));
    assert!(rendered.text.contains("Body"));
}

#[test]
fn missing_image_alt_text_stays_literal_instead_of_creating_markup_actions() {
    for image in [
        "<img src='missing.png' alt=\"<a href='https://example.org/ALT'>LITERAL &amp;</a>\"/>",
        "<svg><image href='missing.png' alt=\"<a href='https://example.org/ALT'>LITERAL &amp;</a>\"/></svg>",
    ] {
        for linked in [false, true] {
            let content = if linked {
                format!("<a href='https://example.org/OUTER'>{image}</a>")
            } else {
                image.to_owned()
            };
            let chapter = format!("<html><body><p>{content}</p><p>Body</p></body></html>");
            let html = chapters_html(&[("chapter.xhtml", &chapter)]);
            let rendered = present_outlined(
                &html,
                Format::Epub,
                &Options {
                    links: true,
                    ..Options::new(240, false)
                },
            );
            assert!(
                !rendered.text.contains("\x1b]8;;https://example.org/ALT"),
                "literal alt text became an action: {:?}",
                rendered.text
            );
            assert_eq!(
                rendered.text.contains("\x1b]8;;https://example.org/OUTER"),
                linked
            );
            assert!(
                rendered
                    .text
                    .contains("<a href='https://example.org/ALT'>LITERAL &</a>")
            );
            assert!(rendered.links.is_empty());
        }
    }
}

#[test]
fn hidden_foreign_element_ids_cannot_resolve_to_unrelated_visible_text() {
    for hidden in [
        "<f:script xmlns:f='urn:foreign' id='hidden'>SECRET</f:script>",
        "<svg xmlns='http://www.w3.org/2000/svg'><script id='hidden'>SECRET</script></svg>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML' id='hidden'>SECRET</math>",
    ] {
        let chapter = format!(
            "<html><body><p><a href='#hidden'>HIDDEN_TARGET</a> <a href='#visible'>VISIBLE_TARGET</a></p>{hidden}<p id='visible'>Visible destination</p></body></html>"
        );
        let html = chapters_html(&[("chapter.xhtml", &chapter)]);
        let rendered = present_outlined(&html, Format::Epub, &Options::new(60, false));
        assert!(!rendered.text.contains("SECRET"));
        assert!(rendered.text.contains("HIDDEN_TARGET"));
        assert_eq!(
            rendered
                .links
                .iter()
                .map(|link| link.label.as_str())
                .collect::<Vec<_>>(),
            ["VISIBLE_TARGET"]
        );
        let anchor = rendered
            .anchors
            .iter()
            .find(|anchor| anchor.id == rendered.links[0].target)
            .unwrap();
        assert_eq!(
            rendered.text.lines().nth(anchor.line).unwrap(),
            "Visible destination"
        );
    }
}

#[test]
fn repaired_or_replacement_character_internal_targets_remain_readable_but_inactive() {
    for href in [
        b"#bad\xef\xbf\xbd".as_slice(),
        b"#bad\xff",
        b"#bad%EF%BF%BD",
    ] {
        let chapter = [
            b"<html><body><p><a href='".as_slice(), href,
            b"'>AMBIGUOUS</a> <a href='#safe'>SAFE</a></p><p id='bad\xef\xbf\xbd'>Wrong destination</p><p id='safe'>Safe destination</p></body></html>",
        ].concat();
        let html = extract(&zip(&[
            ("META-INF/container.xml", b"<container><rootfiles><rootfile full-path='book.opf'/></rootfiles></container>"),
            ("book.opf", b"<package><manifest><item id='a' href='chapter.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='a'/></spine></package>"),
            ("chapter.xhtml", &chapter),
        ]), Format::Epub).unwrap();
        let rendered = present_outlined(&html, Format::Epub, &Options::new(60, false));
        assert!(rendered.text.contains("AMBIGUOUS"));
        assert_eq!(
            rendered
                .links
                .iter()
                .map(|link| link.label.as_str())
                .collect::<Vec<_>>(),
            ["SAFE"]
        );
    }
}

#[test]
fn foreign_raw_and_hidden_namespaces_do_not_expose_script_content_or_actions() {
    for chapter in [
        "<html><body><svg xmlns='http://www.w3.org/2000/svg'><script>HIDDEN SVG SCRIPT</script></svg><p>Body</p></body></html>",
        "<html><body><math xmlns='http://www.w3.org/1998/Math/MathML'><mtext>HIDDEN MATH</mtext></math><p>Body</p></body></html>",
        "<html><body><f:script xmlns:f='urn:foreign'><a xmlns='http://www.w3.org/1999/xhtml' href='https://example.org/'>HIDDEN LINK</a></f:script><p>Body</p></body></html>",
        "<html><body><f:xmp xmlns:f='urn:foreign'><a xmlns='http://www.w3.org/1999/xhtml' href='https://example.org/'>LITERAL</a></f:xmp><p>Body</p></body></html>",
    ] {
        let html = chapters_html(&[("chapter.xhtml", chapter)]);
        let rendered = present_outlined(
            &html,
            Format::Epub,
            &Options {
                links: true,
                ..Options::new(200, false)
            },
        );
        assert!(!rendered.text.contains("HIDDEN"), "{:?}", rendered.text);
        assert!(
            !rendered.text.contains('\x1b'),
            "foreign raw text became a hyperlink"
        );
        assert!(rendered.text.contains("Body"));
        assert!(rendered.links.is_empty());
    }
}

#[test]
fn standalone_empty_external_links_expose_the_destination_in_html_and_extracted_epub() {
    for anchor in [
        "<a href='https://example.org/'></a>",
        "<p><a href='https://example.org/'> \n <em></em></a></p>",
        "<p><a href='https://example.org/'><template>HIDDEN</template><script>HIDDEN</script></a></p>",
    ] {
        let epub = chapters_html(&[("chapter.xhtml", anchor)]);
        for (format, source) in [(Format::Html, anchor), (Format::Epub, epub.as_str())] {
            for links in [false, true] {
                for color in [false, true] {
                    let options = Options {
                        links,
                        ..Options::new(80, color)
                    };
                    let rendered = present_outlined(source, format, &options);
                    assert_eq!(
                        strip_sgr(&rendered.text),
                        "(https://example.org/)\n",
                        "{format:?}, links {links}, color {color}, {anchor}"
                    );
                    assert!(rendered.headings.is_empty());
                    assert!(rendered.links.is_empty());
                }
            }
        }
    }
}
