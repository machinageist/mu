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
