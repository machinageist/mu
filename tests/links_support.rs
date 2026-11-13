// Author: machinageist
// Description: Exercise semantic links across adapters, layout and untrusted input

use mu::{Format, ImageMode, Options, present, present_outlined};

// Independently inspect the terminal protocol, collecting visible and linked characters
fn audit(output: &str) -> (String, Vec<(String, char)>) {
    let mut visible = String::new();
    let mut linked = Vec::new();
    let mut active: Option<&str> = None;
    let mut rest = output;
    while !rest.is_empty() {
        if let Some(payload) = rest.strip_prefix("\x1b]8;;") {
            let (uri, tail) = payload.split_once("\x1b\\").expect("terminated OSC 8");
            if uri.is_empty() {
                assert!(active.take().is_some(), "unbalanced close");
            } else {
                assert!(active.is_none(), "nested hyperlink");
                assert!(uri.is_ascii() && uri.len() <= 8192);
                assert!(!uri.chars().any(char::is_control));
                active = Some(uri);
            }
            rest = tail;
            continue;
        }
        if let Some(payload) = rest.strip_prefix("\x1b[") {
            let (params, tail) = payload.split_once('m').expect("only SGR is allowed here");
            assert!(
                params
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b';')
            );
            rest = tail;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        assert!(
            !ch.is_control() || ch == '\n',
            "unexpected source control {ch:?}"
        );
        if ch == '\n' {
            assert!(active.is_none(), "link leaked across a physical line");
        }
        if let Some(uri) = active {
            linked.push((uri.to_owned(), ch));
        }
        visible.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    assert!(active.is_none(), "link leaked at end of output");
    (visible, linked)
}

#[test]
fn every_semantic_link_adapter_uses_the_same_safe_protocol() {
    let documents = [
        (Format::Markdown, "Read [manual](https://example.org/)."),
        (
            Format::Html,
            "<p>Read <a href='https://example.org/'>manual</a>.</p>",
        ),
        (
            Format::Epub,
            "<p>Read <a href='https://example.org/'>manual</a>.</p>",
        ),
        (Format::Rst, "Read `manual <https://example.org/>`_."),
        (Format::Org, "Read [[https://example.org/][manual]]."),
        (Format::AsciiDoc, "Read https://example.org/[manual]."),
        (
            Format::Man,
            ".TH LINK 1\n.SH NAME\n.UR https://example.org/\nmanual\n.UE\n",
        ),
        (
            Format::Jupyter,
            r#"{"cells":[{"cell_type":"markdown","metadata":{},"source":["Read [manual](https://example.org/)."]}],"metadata":{},"nbformat":4,"nbformat_minor":5}"#,
        ),
    ];
    for (format, source) in documents {
        for width in [16, 30, 80] {
            let options = Options {
                links: true,
                ..Options::new(width, false)
            };
            let plain = present(source, format, &options);
            let (text, linked) = audit(&plain);
            assert!(text.contains("manual"), "{format:?}: {text:?}");
            assert!(!text.contains("https://"), "{format:?}: {text:?}");
            assert_eq!(
                linked.iter().map(|(_, ch)| ch).collect::<String>(),
                "manual",
                "{format:?}"
            );
            assert!(linked.iter().all(|(uri, _)| uri == "https://example.org/"));
            let colored = present(
                source,
                format,
                &Options {
                    color: true,
                    ..options
                },
            );
            assert_eq!(audit(&colored).0, text, "{format:?}, width {width}");
        }
    }
}

#[test]
fn wrapping_and_container_chrome_never_expand_the_clickable_region() {
    let source = "# [A long linked heading with 先先 and more text](https://example.org/heading)\n\n> - Before [a very long manual label with 先先 and more text](https://example.org/manual) after.\n\n| [manual label](https://example.org/table) | Other |\n| :---: | --- |\n| [ABCDEFGHIJKLMNOPQRSTUVWXYZ](https://example.org/word) | tail |\n\nPlain ending.\n";
    for width in [16, 30, 80] {
        let options = Options {
            links: true,
            ..Options::new(width, false)
        };
        let plain = present_outlined(source, Format::Markdown, &options);
        let (visible, linked) = audit(&plain.text);
        assert!(visible.ends_with("Plain ending.\n"));
        assert!(!visible.contains("https://"));
        assert!(!linked.iter().any(|(_, ch)| "│•─═┌┬┐├┼┤└┴┘".contains(*ch)));
        let colored = present_outlined(
            source,
            Format::Markdown,
            &Options {
                color: true,
                ..options
            },
        );
        assert_eq!(audit(&colored.text).0, visible);
        assert_eq!(plain.headings, colored.headings);
        assert_eq!(
            plain.headings[0].text,
            "A long linked heading with 先先 and more text"
        );
    }
}

#[test]
fn uri_encoding_preserves_existing_escapes_and_blocks_actionable_control_targets() {
    let options = Options {
        links: true,
        ..Options::new(200, false)
    };
    let source = "<p><a href='HTTPS://example.org/café?q=a b&amp;x=%2F'>web</a> <a href='MAILTO:a@example.org?subject=Hi there'>mail</a></p>";
    let (visible, linked) = audit(&present(source, Format::Html, &options));
    assert_eq!(visible, "web mail\n");
    assert!(
        linked
            .iter()
            .any(|(uri, _)| uri == "HTTPS://example.org/caf%C3%A9?q=a%20b&x=%2F")
    );
    assert!(
        linked
            .iter()
            .any(|(uri, _)| uri == "MAILTO:a@example.org?subject=Hi%20there")
    );
    for target in [
        "javascript:alert(1)",
        "data:text/html,no",
        "file:///etc/passwd",
        "chapter.html#end",
        "#end",
        "//example.org/",
        "https://",
        "https://:80/",
        "https://example.org/%GG",
        "https://example.org/&#27;]52;c;payload&#7;",
        "https://example.org/&#157;payload",
        "https://example.org/&#8238;hidden",
        "https://example.org\\@evil.org/",
    ] {
        let source = format!("<p><a href='{target}'>label</a></p>");
        let (visible, linked) = audit(&present(&source, Format::Html, &options));
        assert!(linked.is_empty(), "{target:?}");
        assert!(visible.contains("label"));
    }
    let oversized = format!("https://example.org/{}", "a".repeat(8192));
    assert!(
        audit(&present(
            &format!("<a href='{oversized}'>large</a>"),
            Format::Html,
            &options
        ))
        .1
        .is_empty()
    );
}

#[test]
fn disabling_links_preserves_fallback_and_does_not_linkify_code_or_data() {
    let source = "Read [manual](https://example.org/).";
    assert_eq!(
        present(source, Format::Markdown, &Options::new(80, false)),
        "Read manual (https://example.org/).\n"
    );
    for (format, source) in [
        (Format::Code, "https://example.org/"),
        (Format::Json, r#"{"url":"https://example.org/"}"#),
        (
            Format::Markdown,
            "`https://example.org/`\n\n```text\nhttps://example.org/\n```\n",
        ),
    ] {
        let output = present(
            source,
            format,
            &Options {
                links: true,
                ..Options::new(80, true)
            },
        );
        let (visible, linked) = audit(&output);
        assert!(visible.contains("https://example.org/"));
        assert!(linked.is_empty());
    }
    let (visible, linked) = audit(&present(
        "<https://example.org/> <a@example.org>",
        Format::Markdown,
        &Options {
            links: true,
            ..Options::new(80, false)
        },
    ));
    assert_eq!(visible, "https://example.org/ a@example.org\n");
    assert!(linked.iter().any(|(uri, _)| uri == "mailto:a@example.org"));
}

#[test]
fn raw_fragment_positions_follow_tabs_wide_characters_wrapping_and_highlighting() {
    for width in [20, 30, 60] {
        for color in [false, true] {
            let source = "<p><a href='#raw'>code</a></p><pre class='language-rust'>aaaaaaaaaaaaaaaaaaaa\t先先<span id='raw'>X</span> remainder\n</pre>";
            let rendered = present_outlined(source, Format::Epub, &Options::new(width, color));
            assert_eq!(rendered.links.len(), 1);
            let anchor = rendered
                .anchors
                .iter()
                .find(|anchor| anchor.id == "raw")
                .unwrap();
            assert!(
                audit(rendered.text.lines().nth(anchor.line).unwrap())
                    .0
                    .contains('X'),
                "width {width}, color {color}: {:?}",
                rendered.text
            );
        }
    }
}

#[test]
fn linked_images_keep_actionable_alt_text_when_graphics_are_enabled() {
    let source = r#"<p><a href="https://example.org/diagram"><img alt="Diagram" src="data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='4'%20height='4'%3E%3Crect%20width='4'%20height='4'/%3E%3C/svg%3E"/></a></p>"#;
    let mut options = Options {
        links: true,
        ..Options::new(60, true)
    };
    options.images.mode = ImageMode::Auto;
    let (visible, linked) = audit(&present(source, Format::Html, &options));
    assert_eq!(visible, "[image: Diagram]\n");
    assert_eq!(
        linked.iter().map(|(_, ch)| ch).collect::<String>(),
        "[image: Diagram]"
    );
}
