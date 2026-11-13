// Author: machinageist
// Date: 2026-11-10
// Description: EPUB books read as one document: metadata, chapters in spine order, inlined images
// Notes: The book is zipped by the test from parts, so the fixture stays readable text

mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::{assert_invariants, strip_sgr};
use mu::{
    CellSize, Format, ImageMode, ImageOptions, Options, Protocol, extract, present,
    present_outlined, sniff_bytes,
};

const BIN: &str = env!("CARGO_BIN_EXE_mu");
const TMP_DIR: &str = env!("CARGO_TARGET_TMPDIR");
const EXIT_FAILURE: i32 = 1;
const WIDTH: usize = 60;
const PICTURE: &[u8] = include_bytes!("fixtures/images/wide.svg");
const HALF_BLOCKS: [char; 2] = ['▀', '▄'];

const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#;

const PACKAGE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:0</dc:identifier>
    <dc:title>The Sample Book</dc:title>
    <dc:creator>Ada Writer</dc:creator>
  </metadata>
  <manifest>
    <item id="two" href="text/two.xhtml" media-type="application/xhtml+xml"/>
    <item id="one" href="text/one.xhtml" media-type="application/xhtml+xml"/>
    <item id="art" href="images/wide.svg" media-type="image/svg+xml"/>
  </manifest>
  <spine>
    <itemref idref="one"/>
    <itemref idref="two"/>
  </spine>
</package>
"#;

const CHAPTER_ONE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter One</title></head>
<body>
  <h1>Beginnings</h1>
  <p>It was a <em>dark</em> night. See <a href="two.xhtml#end">the end</a>.</p>
  <p><img src="../images/wide.svg" alt="A wide picture"/></p>
</body>
</html>
"#;

const CHAPTER_TWO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter Two</title></head>
<body>
  <h1 id="end">Endings</h1>
  <h2>Coda</h2>
  <p><img src="../images/gone.png" alt="A lost picture"/></p>
  <p>More at <a href="https://example.org/">the site</a>.</p>
</body>
</html>
"#;

// Write a zip of stored entries, enough for the reader under test
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        let fields = |central: bool| {
            let mut record = Vec::new();
            let signature: u32 = if central { 0x0201_4b50 } else { 0x0403_4b50 };
            record.extend_from_slice(&signature.to_le_bytes());
            if central {
                record.extend_from_slice(&20u16.to_le_bytes());
            }
            record.extend_from_slice(&[20, 0, 0, 0, 0, 0]);
            record.extend_from_slice(&[0; 8]);
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
        out.extend(fields(false));
        out.extend_from_slice(data);
        directory.extend(fields(true));
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

// The sample book's bytes
fn book() -> Vec<u8> {
    zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", PACKAGE.as_bytes()),
        ("OEBPS/text/one.xhtml", CHAPTER_ONE.as_bytes()),
        ("OEBPS/text/two.xhtml", CHAPTER_TWO.as_bytes()),
        ("OEBPS/images/wide.svg", PICTURE),
    ])
}

// The book as the HTML document it becomes
fn book_html() -> String {
    extract(&book(), Format::Epub).expect("readable book")
}

// Build an actual EPUB from named chapters; manifest hrefs stay entity-escaped XML.
fn chapters_html(chapters: &[(&str, &str)]) -> String {
    let mut package = String::from("<package><manifest>");
    for (index, (path, _)) in chapters.iter().enumerate() {
        let href = path
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('&', "&amp;")
            .replace('"', "&quot;");
        package.push_str(&format!(
            "<item id=\"c{index}\" href=\"{href}\" media-type=\"application/xhtml+xml\"/>"
        ));
    }
    package.push_str("</manifest><spine>");
    for index in 0..chapters.len() {
        package.push_str(&format!("<itemref idref=\"c{index}\"/>"));
    }
    package.push_str("</spine></package>");
    let paths: Vec<_> = chapters
        .iter()
        .map(|(path, _)| format!("OEBPS/{path}"))
        .collect();
    let mut files = vec![
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", package.as_bytes()),
    ];
    files.extend(
        chapters
            .iter()
            .zip(&paths)
            .map(|((_, text), path)| (path.as_str(), text.as_bytes())),
    );
    extract(&zip(&files), Format::Epub).expect("readable book")
}

fn linked_anchor<'a>(rendered: &'a mu::Rendered, label: &str) -> &'a mu::Anchor {
    let link = rendered
        .links
        .iter()
        .find(|link| link.label == label)
        .unwrap_or_else(|| panic!("missing link {label:?}: {:?}", rendered.links));
    rendered
        .anchors
        .iter()
        .find(|anchor| anchor.id == link.target)
        .unwrap_or_else(|| panic!("missing destination for {label:?}"))
}

// Run mu on a path or on stdin and return (exit code, stdout, stderr)
fn run(args: &[&str], stdin: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(BIN)
        .args(["-w", "60", "--color", "never", "--images", "never"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mu");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for mu");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn magic_bytes_name_the_book() {
    assert_eq!(sniff_bytes(&book()), Some(Format::Epub));
    assert_eq!(Format::from_path("novel.EPUB"), Some(Format::Epub));
    assert_eq!(Format::from_name("epub"), Some(Format::Epub));
    assert_eq!(
        Format::from_media_type("application/epub+zip"),
        Some(Format::Epub)
    );
    assert!(Format::Epub.is_binary());
}

#[test]
fn chapters_render_in_spine_order() {
    let out = present(&book_html(), Format::Epub, &Options::new(WIDTH, false));
    assert_eq!(
        out,
        "The Sample Book\n═══════════════\n\nAda Writer\n\n\
Beginnings\n══════════\n\nIt was a dark night. See the end.\n\n[image: A wide picture]\n\n\
Endings\n═══════\n\nCoda\n────\n\nA lost picture\n\nMore at the site (https://example.org/).\n"
    );
}

#[test]
fn extracted_chapter_links_jump_to_their_rendered_target() {
    let html = book_html();
    assert!(!html.contains("href=\"two.xhtml#end\""));
    for width in [24, 60] {
        let rendered = present_outlined(&html, Format::Epub, &Options::new(width, false));
        let link = rendered
            .links
            .iter()
            .find(|link| link.label == "the end")
            .expect("extracted internal link remains navigable");
        let anchor = rendered
            .anchors
            .iter()
            .find(|anchor| anchor.id == link.target)
            .expect("chapter link resolves to an anchor");
        assert_ne!(anchor.id, "end", "chapter IDs must be scoped");
        assert_eq!(rendered.text.lines().nth(anchor.line), Some("Endings"));
        assert!(
            rendered
                .text
                .lines()
                .nth(link.line)
                .unwrap()
                .contains("end")
        );
        assert!(!rendered.text.contains("two.xhtml"));
    }
}

#[test]
fn zip_paths_fragments_and_chapter_tops_have_distinct_destinations() {
    let html = chapters_html(&[
        (
            "text/a_b.xhtml",
            r##"<html id="root"><body id="body"><h1 id="dup">First</h1>
<p><a href="#dup">same</a> <a href="a/b.xhtml#dup">other</a>
<a href="../other/a%26b%20c.xhtml#r%C3%A9sum%C3%A9%26tail">percent</a>
<a href="../other/a&amp;b%20c.xhtml#résumé&amp;tail">entity</a>
<a href="../other/literal%2520%26amp%3B.xhtml#rate%2520">literal</a>
<a href="../other/literal%2520%26amp%3B.xhtml#rate%20">space</a>
<a href="a/b.xhtml">chapter</a> <a href="empty.xhtml">empty</a>
<a href="#legacy%26name">legacy</a> <a href="#root">root</a> <a href="#body">body</a></p>
<p><a name="legacy&amp;name"></a>Legacy target</p></body></html>"##,
        ),
        (
            "text/a/b.xhtml",
            "<html><body><h1 id=\"dup\">Second</h1></body></html>",
        ),
        (
            "other/a&b c.xhtml",
            "<html><body><p id=\"r&#233;sum&#xE9;&amp;tail\">Escaped target</p></body></html>",
        ),
        (
            "other/literal%20&amp;.xhtml",
            "<html><body><p id=\"rate%20\">Literal target</p><p id=\"rate \">Space target</p></body></html>",
        ),
        ("text/empty.xhtml", "<html><body></body></html>"),
        (
            "text/after.xhtml",
            "<html><body><p>After empty</p></body></html>",
        ),
    ]);
    for width in [24, 60] {
        for links in [false, true] {
            let options = Options {
                links,
                ..Options::new(width, false)
            };
            let rendered = present_outlined(&html, Format::Epub, &options);
            for (label, expected) in [
                ("same", "First"),
                ("other", "Second"),
                ("chapter", "Second"),
                ("percent", "Escaped target"),
                ("entity", "Escaped target"),
                ("literal", "Literal target"),
                ("space", "Space target"),
                ("legacy", "Legacy target"),
                ("root", "First"),
                ("body", "First"),
            ] {
                let anchor = linked_anchor(&rendered, label);
                assert_eq!(
                    rendered.text.lines().nth(anchor.line),
                    Some(expected),
                    "{label}, width {width}"
                );
            }
            assert!(
                !rendered.links.iter().any(|link| link.label == "empty"),
                "an empty chapter must not borrow its successor's destination"
            );
            assert_ne!(
                linked_anchor(&rendered, "same").id,
                linked_anchor(&rendered, "other").id
            );
            assert_ne!(
                linked_anchor(&rendered, "chapter").id,
                linked_anchor(&rendered, "other").id
            );
            assert_eq!(
                linked_anchor(&rendered, "percent").id,
                linked_anchor(&rendered, "entity").id
            );
            assert!(!rendered.text.contains("xhtml"));
            assert!(!rendered.text.contains("epub-"));
        }
    }
}

#[test]
fn visible_html_identities_survive_structural_rendering_and_reflow() {
    let html = chapters_html(&[(
        "chapter.xhtml",
        r##"<html><body>
<p><a href="#inline">inline</a> <a href="#item">item</a>
<a href="#term">term</a> <a href="#definition">definition</a>
<a href="#caption">caption</a> <a href="#group">group</a> <a href="#row">row</a> <a href="#cell">cell</a>
<a href="#raw">raw</a> <a href="#span">span</a> <a href="#heading">heading</a></p>
<h2 id="heading">Clean heading<a class="headerlink" href="#heading">¶</a></h2>
<p>Before<br/><span id="inline">Inline target</span></p>
<ul><li>Earlier item</li><li id="item">Item target</li></ul>
<dl><dt id="term">Term target</dt><dd id="definition">Definition target</dd></dl>
<table><caption id="caption">Caption target</caption><thead><tr><th>Header</th></tr></thead>
<tbody id="group"><tr id="row"><td id="cell">Cell target</td></tr></tbody></table>
<pre><code>Earlier code
<span id="raw">Raw target</span></code></pre>
<p><code>Earlier <span id="span">Span target</span></code></p>
</body></html>"##,
    )]);
    for width in [24, 60] {
        let rendered = present_outlined(&html, Format::Epub, &Options::new(width, false));
        let mut misplaced = Vec::new();
        for (label, expected) in [
            ("inline", "Inline target"),
            ("item", "Item target"),
            ("term", "Term target"),
            ("definition", "Definition target"),
            ("caption", "Caption target"),
            ("group", "Cell target"),
            ("row", "Cell target"),
            ("cell", "Cell target"),
            ("raw", "Raw target"),
            ("span", "Span target"),
            ("heading", "Clean heading"),
        ] {
            let anchor = linked_anchor(&rendered, label);
            let line = rendered
                .text
                .lines()
                .nth(anchor.line)
                .expect("anchor refers to an output line");
            if !line.contains(expected) {
                misplaced.push(format!("{label}, width {width}: {line:?}"));
            }
        }
        assert!(misplaced.is_empty(), "{}", misplaced.join("\n"));
        assert!(!rendered.text.contains('¶'));
        assert_eq!(rendered.headings[0].text, "Clean heading");
        assert!(!rendered.links.iter().any(|link| link.label == "¶"));
    }
}

#[test]
fn hidden_missing_and_unsupported_targets_never_become_navigation() {
    let html = chapters_html(&[(
        "text/chapter.xhtml",
        r##"<html><head><title id="head">Excluded</title></head><body>
<h1 id="safe">Visible heading</h1>
<p><a href="#safe">safe</a> <a href="#hidden">hidden link</a>
<a href="#descendant">hidden descendant</a> <a href="#script">script link</a>
<a href="#head">head link</a> <a href="#missing">missing fragment</a>
<a href="&#10;#safe">control-bearing link</a>
<a href="gone.xhtml#safe">missing chapter</a> <a href="../../../OEBPS/text/chapter.xhtml#safe">escape root</a>
<a href="javascript:alert(1)">javascript link</a> <a href="file:///tmp/chapter.xhtml">file link</a>
<a href="data:text/html,no">data link</a> <a href="//example.org/chapter.xhtml">network link</a></p>
<template id="hidden"><a name="descendant">HIDDEN TEMPLATE</a><h1>HIDDEN HEADING</h1></template>
<script id="script">var x = '<p id="script-child">HIDDEN SCRIPT</p>';</script>
<noscript><p id="noscript">HIDDEN NOSCRIPT</p></noscript><p>After hidden</p>
</body></html>"##,
    )]);
    for links in [false, true] {
        let rendered = present_outlined(
            &html,
            Format::Epub,
            &Options {
                links,
                ..Options::new(80, false)
            },
        );
        let labels: Vec<_> = rendered
            .links
            .iter()
            .map(|link| link.label.as_str())
            .collect();
        assert_eq!(labels, ["safe"]);
        assert_eq!(
            rendered.anchors.len(),
            2,
            "only chapter top and visible heading remain"
        );
        assert!(rendered.text.contains("missing fragment"));
        assert!(rendered.text.contains("missing chapter"));
        assert!(rendered.text.contains("javascript link"));
        assert!(rendered.text.contains("After hidden"));
        for excluded in [
            "HIDDEN",
            "Excluded",
            "file:",
            "javascript:",
            "data:",
            "xhtml",
            "epub-",
            "\x1b]8;",
        ] {
            assert!(
                !rendered.text.contains(excluded),
                "{excluded}: {:?}",
                rendered.text
            );
        }
    }
}

#[test]
fn external_http_https_and_mailto_links_keep_their_original_destinations() {
    let html = chapters_html(&[(
        "chapter.xhtml",
        r#"<html><body><p>
<a href="HTTP://example.org/a?x=1&amp;y=2">web</a>
<a href="HTTPS://example.org/secure">secure</a>
<a href="MAILTO:reader@example.org">mail</a>
</p></body></html>"#,
    )]);
    assert!(html.contains("href=\"HTTP://example.org/a?x=1&amp;y=2\""));
    assert!(html.contains("href=\"HTTPS://example.org/secure\""));
    assert!(html.contains("href=\"MAILTO:reader@example.org\""));
    for links in [false, true] {
        let rendered = present_outlined(
            &html,
            Format::Epub,
            &Options {
                links,
                ..Options::new(120, false)
            },
        );
        assert!(
            rendered.links.is_empty(),
            "external links are not pager destinations"
        );
        for target in [
            "HTTP://example.org/a?x=1&y=2",
            "HTTPS://example.org/secure",
            "MAILTO:reader@example.org",
        ] {
            assert!(
                rendered.text.contains(target),
                "{target}: {:?}",
                rendered.text
            );
        }
        assert_eq!(rendered.text.contains("\x1b]8;;"), links);
    }
}

#[test]
fn outline_lists_the_title_and_chapter_headings() {
    let headings: Vec<_> =
        present_outlined(&book_html(), Format::Epub, &Options::new(WIDTH, false))
            .headings
            .into_iter()
            .map(|heading| (heading.level, heading.text))
            .collect();
    assert_eq!(
        headings,
        [
            (1, "The Sample Book".to_owned()),
            (1, "Beginnings".to_owned()),
            (1, "Endings".to_owned()),
            (2, "Coda".to_owned()),
        ]
    );
}

#[test]
fn extracted_chapters_keep_highlighting_and_clean_heading_labels() {
    let chapter = CHAPTER_ONE.replace(
        "<h1>Beginnings</h1>",
        "<h1>Beginnings<a class=\"headerlink\" href=\"#begin\">¶</a></h1><div class=\"language-python\"><pre><span><code>def f(): return None</code></span></pre></div>",
    );
    let bytes = zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", PACKAGE.as_bytes()),
        ("OEBPS/text/one.xhtml", chapter.as_bytes()),
        ("OEBPS/text/two.xhtml", CHAPTER_TWO.as_bytes()),
    ]);
    let html = extract(&bytes, Format::Epub).expect("readable book");
    for width in [30, 60, 80] {
        let out = present_outlined(&html, Format::Epub, &Options::new(width, true));
        assert!(
            out.text.contains("\x1b[0;1;35mdef\x1b[0m"),
            "{:?}",
            out.text
        );
        assert!(
            out.headings
                .iter()
                .any(|heading| heading.text == "Beginnings")
        );
        assert!(!out.text.contains('¶'));
        assert_eq!(
            strip_sgr(&out.text),
            present(&html, Format::Epub, &Options::new(width, false))
        );
    }
    assert_invariants(&html, Format::Epub);
}

#[test]
fn chapter_root_language_hints_survive_extraction_without_leaking() {
    let package = PACKAGE
        .replace(
            "</manifest>",
            "<item id=\"three\" href=\"text/three.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"four\" href=\"text/four.xhtml\" media-type=\"application/xhtml+xml\"/></manifest>",
        )
        .replace(
            "</spine>",
            "<itemref idref=\"three\"/><itemref idref=\"four\"/></spine>",
        );
    for (python, unknown) in [
        (r#"class="language-python""#, r#"data-language="unknown""#),
        (r#"data-lang="python""#, r#"class="language-unknown""#),
        (r#"data-language="python""#, r#"data-lang="unknown""#),
        (r#"class="lang-python""#, r#"class="lang-unknown""#),
    ] {
        let one = format!(
            "<html {python} onload=\"discard-html\"><head><title>Head stays excluded</title><pre>head code</pre></head><body hidden><pre><code>def html_hint(): return None</code></pre></body></html>"
        );
        let two = format!(
            "<html {python}><head><title>Another excluded head</title></head><body {unknown} style=\"discard-body\"><pre><code>#!/usr/bin/env python3\ndef body_unknown(): return None</code></pre></body></html>"
        );
        let three = format!(
            "<html {unknown}><body {python}><pre><code>def body_hint(): return None</code></pre></body></html>"
        );
        let four = "<html><body><pre><code>def unlabeled(): return None</code></pre></body></html>";
        let bytes = zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", package.as_bytes()),
            ("OEBPS/text/one.xhtml", one.as_bytes()),
            ("OEBPS/text/two.xhtml", two.as_bytes()),
            ("OEBPS/text/three.xhtml", three.as_bytes()),
            ("OEBPS/text/four.xhtml", four.as_bytes()),
        ]);
        let html = extract(&bytes, Format::Epub).expect("readable book");
        assert!(!html.contains("Head stays excluded"));
        assert!(!html.contains("Another excluded head"));
        assert!(!html.contains("head code"));
        assert!(!html.contains("discard-html"));
        assert!(!html.contains("discard-body"));
        assert!(!html.contains("hidden"));
        for width in [30, 60, 80] {
            let color = present(&html, Format::Epub, &Options::new(width, true));
            let plain = present(&html, Format::Epub, &Options::new(width, false));
            for (name, highlighted) in [
                ("html_hint", true),
                ("body_unknown", false),
                ("body_hint", true),
                ("unlabeled", false),
            ] {
                let line = color
                    .lines()
                    .find(|line| strip_sgr(line).contains(name))
                    .expect("chapter code remains visible");
                assert_eq!(
                    line.contains("\x1b[0;1;35mdef\x1b[0m"),
                    highlighted,
                    "{python}, width {width}, {name}: {line:?}"
                );
            }
            assert_eq!(strip_sgr(&color), plain);
            let positions = ["html_hint", "body_unknown", "body_hint", "unlabeled"]
                .map(|name| plain.find(name).expect("chapter code remains visible"));
            assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        }
        assert_invariants(&html, Format::Epub);
    }
}

#[test]
fn chapter_language_metadata_is_escaped_without_double_escaping_entities() {
    let chapter = r#"<html class='language-python extra" data-injected="<script>never</script>&quot;' data-lang="p&#121;thon" data-language="p&#121;thon">
<head><title>Excluded head</title></head><body><pre>def escaped_hint(): return None</pre></body></html>"#;
    let bytes = zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", PACKAGE.as_bytes()),
        ("OEBPS/text/one.xhtml", chapter.as_bytes()),
        ("OEBPS/text/two.xhtml", CHAPTER_TWO.as_bytes()),
    ]);
    let html = extract(&bytes, Format::Epub).expect("readable book");
    assert!(html.contains(
        r#"<div class="language-python extra&quot; data-injected=&quot;&lt;script&gt;never&lt;/script&gt;&quot;" data-lang="p&#121;thon" data-language="p&#121;thon">"#
    ));
    assert!(!html.contains(r#" data-injected=""#));
    assert!(!html.contains("<script>"));
    assert!(!html.contains("Excluded head"));
    let color = present(&html, Format::Epub, &Options::new(WIDTH, true));
    assert!(color.contains("\x1b[0;1;35mdef\x1b[0m"), "{color:?}");
    assert!(!color.contains("never"));
    assert_invariants(&html, Format::Epub);
}

#[test]
fn images_are_inlined_and_drawn() {
    let html = book_html();
    assert!(html.contains("<img src=\"data:image/svg+xml;base64,"));
    assert!(!html.contains("gone.png"));
    let options = Options {
        images: ImageOptions {
            mode: ImageMode::Auto,
            base: None,
            cell: CellSize::default(),
            protocol: Protocol::HalfBlock,
            max_rows: None,
        },
        ..Options::new(WIDTH, true)
    };
    let out = present(&html, Format::Epub, &options);
    assert!(out.contains(HALF_BLOCKS), "no picture drawn");
}

#[test]
fn book_output_keeps_the_invariants() {
    assert_invariants(&book_html(), Format::Epub);
}

#[test]
fn file_and_stdin_match() {
    let path = format!("{TMP_DIR}/sample.epub");
    std::fs::write(&path, book()).expect("write book");
    let from_file = run(&[&path], b"");
    let from_stdin = run(&[], &book());
    assert_eq!(from_file.0, 0, "{}", from_file.2);
    assert_eq!(from_file, from_stdin);
    let (_, toc, _) = run(&["--toc", &path], b"");
    assert!(toc.starts_with("The Sample Book\n"), "{toc}");
}

#[test]
fn unreadable_books_fail_with_a_reason() {
    let path = format!("{TMP_DIR}/broken.epub");
    std::fs::write(&path, b"PK\x03\x04 not a book").expect("write book");
    let (code, out, err) = run(&[&path], b"");
    assert_eq!(code, EXIT_FAILURE);
    assert!(out.is_empty());
    assert_eq!(
        err,
        format!("mu: {path}: not a readable EPUB: no zip directory\n")
    );
}
