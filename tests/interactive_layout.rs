// Public interactive layout regressions through actual EPUB ZIP extraction.
use mu::{Format, Options, extract, present_interactive, present_outlined};

mod common;

// A complete stored ZIP: CRCs, local headers, central directory and end record.
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

fn book(chapters: &[(&str, &str)], assets: &[(&str, &[u8])]) -> String {
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
        (
            "META-INF/container.xml",
            b"<container><rootfiles><rootfile full-path='OEBPS/book.opf'/></rootfiles></container>"
                .as_slice(),
        ),
        ("OEBPS/book.opf", package.as_bytes()),
    ];
    files.extend(
        chapters
            .iter()
            .zip(&paths)
            .map(|((_, chapter), path)| (path.as_str(), chapter.as_bytes())),
    );
    files.extend_from_slice(assets);
    extract(&zip(&files), Format::Epub).expect("actual EPUB ZIP extracts")
}

fn spans(rendered: &mu::InteractiveRendered, link: usize) -> Vec<(usize, usize, usize)> {
    rendered
        .link_regions
        .iter()
        .find(|region| region.link == link)
        .unwrap()
        .spans
        .iter()
        .map(|span| (span.line, span.start, span.end))
        .collect()
}

#[test]
fn shared_units_keep_combining_cjk_flags_and_zwj_clusters_indivisible_across_styles() {
    assert_eq!(
        mu::display::units("e\u{301}中👩‍💻🇺🇸").collect::<Vec<_>>(),
        [("e\u{301}", 1), ("中", 2), ("👩‍💻", 2), ("🇺🇸", 2)]
    );
    assert_eq!(
        mu::display::units("\u{301}").collect::<Vec<_>>(),
        [("\u{301}", 0)]
    );
    assert_eq!(mu::display::width("e\u{301}中👩‍💻🇺🇸"), 7);
    let html = book(
        &[(
            "one.xhtml",
            "<blockquote><ul><li><a href='#t'>abcdefghijklmn<strong>👩</strong>‍<em>💻</em>中e<strong>́</strong>XYZ</a></li></ul></blockquote><p id='t'>TARGET</p>",
        )],
        &[],
    );
    for color in [false, true] {
        let rendered = present_interactive(&html, Format::Epub, &Options::new(20, color));
        let plain = common::strip_sgr(&rendered.document.text);
        let lines: Vec<_> = plain.lines().collect();
        assert_eq!(&lines[..2], ["│ • abcdefghijklmn👩‍💻", "│   中éXYZ"]);
        assert_eq!(spans(&rendered, 0), [(0, 4, 20), (1, 4, 10)]);
        assert_eq!(rendered.document.links[0].label, "abcdefghijklmn👩‍💻中éXYZ");
    }
}

#[test]
fn link_cells_are_exact_and_reindexed_after_unresolved_sources_are_removed() {
    let html = book(
        &[(
            "one.xhtml",
            "<p><a href='#missing'>NO</a> x <a href='#yes'>  A <strong>B</strong> C  </a> gap <a href='#yes'>D</a>.</p><p id='yes'>TARGET</p>",
        )],
        &[],
    );
    for color in [false, true] {
        let options = Options::new(80, color);
        let rendered = present_interactive(&html, Format::Epub, &options);
        assert_eq!(
            rendered.document,
            present_outlined(&html, Format::Epub, &options)
        );
        assert_eq!(
            common::strip_sgr(&rendered.document.text),
            "NO x A B C gap D.\n\nTARGET\n"
        );
        assert_eq!(
            rendered
                .document
                .links
                .iter()
                .map(|link| link.label.as_str())
                .collect::<Vec<_>>(),
            ["A B C", "D"]
        );
        assert_eq!(rendered.link_regions.len(), 2);
        assert_eq!(spans(&rendered, 0), [(0, 5, 10)]);
        assert_eq!(spans(&rendered, 1), [(0, 15, 16)]);
        assert!(rendered.images.is_empty());
    }
}

fn png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    writer.finish().unwrap();
    bytes
}

fn native_options(width: usize) -> Options {
    Options {
        images: mu::ImageOptions {
            mode: mu::ImageMode::Auto,
            protocol: mu::Protocol::Kitty,
            max_rows: Some(64),
            ..Default::default()
        },
        ..Options::new(width, true)
    }
}

fn assert_fallback(input: &str, format: Format, options: &Options) -> mu::InteractiveRendered {
    let rendered = present_interactive(input, format, options);
    let mut legacy = options.clone();
    legacy.images.protocol = mu::Protocol::HalfBlock;
    assert_eq!(rendered.document, present_outlined(input, format, &legacy));
    assert!(!rendered.document.text.contains("\x1b_G"));
    assert!(!rendered.document.text.contains("\x1b]1337;File="));
    rendered
}

#[test]
fn actual_zip_captures_full_rgba_pixels_and_body_box_not_gutters_or_caption() {
    let pixels: Vec<_> = (0..32u8)
        .flat_map(|y| (0..64u8).flat_map(move |x| [x, y, x ^ y, if x % 2 == 0 { 80 } else { 255 }]))
        .collect();
    let bytes = png(64, 32, &pixels);
    let html = book(
        &[(
            "one.xhtml",
            "<p>BEFORE</p><blockquote><ul><li><p><img src='cover.png' alt='DETAIL CAPTION'/></p></li></ul></blockquote><p>AFTER</p>",
        )],
        &[("OEBPS/cover.png", &bytes)],
    );
    let rendered = assert_fallback(&html, Format::Epub, &native_options(40));
    assert_eq!(rendered.images.len(), 1);
    let asset = &rendered.images[0];
    assert_eq!(
        (asset.line, asset.column, asset.columns, asset.rows),
        (2, 4, 8, 2)
    );
    assert_eq!((asset.raster.width, asset.raster.height), (64, 32));
    assert_eq!(asset.raster.pixels.as_ref(), pixels);
    let text = common::strip_sgr(&rendered.document.text);
    let lines: Vec<_> = text.lines().collect();
    assert!(lines[asset.line].starts_with("│ • "));
    assert_eq!(lines[asset.line + asset.rows], "│   DETAIL CAPTION");
    let cloned = rendered.clone();
    assert_eq!(cloned, rendered);
    assert!(std::sync::Arc::ptr_eq(
        &cloned.images[0].raster,
        &asset.raster
    ));
}

#[test]
fn svg_is_rasterized_at_cell_pixel_resolution_and_alpha_is_straight() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="80"><rect width="160" height="80" fill="#ff0000" fill-opacity="0.5"/></svg>"##;
    let html = book(
        &[("one.xhtml", "<p><img src='cover.svg' alt='CAPTION'/></p>")],
        &[("OEBPS/cover.svg", svg)],
    );
    let mut options = native_options(80);
    options.images.max_rows = Some(2);
    let rendered = assert_fallback(&html, Format::Epub, &options);
    let asset = &rendered.images[0];
    assert_eq!((asset.columns, asset.rows), (8, 2));
    assert_eq!((asset.raster.width, asset.raster.height), (64, 32));
    assert_eq!(asset.raster.pixels.len(), 64 * 32 * 4);
    assert!(
        asset
            .raster
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel == &[255, 0, 0, 128])
    );
}

#[test]
fn capture_is_bounded_per_layout_and_omitted_images_keep_exact_fallback() {
    let bytes = png(1024, 1024, &vec![200; 1024 * 1024 * 4]);
    let chapter = "<p><img src='large.png' alt='BUDGET'/></p>".repeat(9);
    let html = book(&[("one.xhtml", &chapter)], &[("OEBPS/large.png", &bytes)]);
    let rendered = assert_fallback(&html, Format::Epub, &native_options(128));
    assert_eq!(rendered.images.len(), 8);
    assert_eq!(
        rendered
            .images
            .iter()
            .map(|asset| asset.raster.pixels.len())
            .sum::<usize>(),
        32 * 1024 * 1024
    );
    assert_eq!(rendered.document.text.matches("BUDGET").count(), 9);
}

#[test]
fn hidden_invalid_linked_and_inline_sources_do_not_gain_native_assets() {
    let bytes = png(8, 16, &[255; 8 * 16 * 4]);
    let html = book(
        &[(
            "one.xhtml",
            "<head><img src='good.png'/></head><script><img src='good.png'/></script><template><img src='good.png'/></template><p>inline <img src='good.png' alt='INLINE'/></p><p><a href='#t'><img src='good.png' alt='LINKED'/></a></p><p><img src='bad.png' alt='INVALID'/></p><p><img src='missing.png' alt='MISSING'/></p><p><img src='https://example.invalid/remote.png' alt='REMOTE'/></p><p id='t'>TARGET</p>",
        )],
        &[("OEBPS/good.png", &bytes), ("OEBPS/bad.png", b"not a PNG")],
    );
    let options = native_options(80);
    let rendered = assert_fallback(&html, Format::Epub, &options);
    assert!(rendered.images.is_empty());
    assert_eq!(rendered.document.links.len(), 1);
    assert_eq!(rendered.document.links[0].label, "[image: LINKED]");
    for marker in ["INLINE", "LINKED", "INVALID", "MISSING", "REMOTE"] {
        assert!(rendered.document.text.contains(marker));
    }
}

#[test]
fn unsupported_modes_and_invalid_geometry_never_capture_or_inject_native_payload() {
    let bytes = png(8, 16, &[255; 8 * 16 * 4]);
    let html = book(
        &[("one.xhtml", "<p><img src='one.png' alt='SAFE'/></p>")],
        &[("OEBPS/one.png", &bytes)],
    );
    for protocol in [mu::Protocol::HalfBlock, mu::Protocol::Iterm] {
        let mut options = native_options(80);
        options.images.protocol = protocol;
        assert!(
            assert_fallback(&html, Format::Epub, &options)
                .images
                .is_empty()
        );
    }
    for cell in [
        mu::CellSize {
            width: 0,
            height: 0,
        },
        mu::CellSize {
            width: u32::MAX,
            height: u32::MAX,
        },
    ] {
        let mut options = native_options(80);
        options.images.cell = cell;
        assert!(
            assert_fallback(&html, Format::Epub, &options)
                .images
                .is_empty()
        );
    }
    for (color, mode) in [(false, mu::ImageMode::Auto), (true, mu::ImageMode::Never)] {
        let mut options = native_options(80);
        options.color = color;
        options.images.mode = mode;
        assert!(
            assert_fallback(&html, Format::Epub, &options)
                .images
                .is_empty()
        );
    }
}

#[test]
fn source_link_order_and_exact_wrapped_spans_survive_tables_and_gutters() {
    let html = book(
        &[(
            "one.xhtml",
            "<blockquote><table><tr><th align='center'>CENTER</th><th align='right'>RIGHTSIDE</th></tr><tr><td><a href='#t'>one two three four</a></td><td><a href='#missing'>DROP</a> <a href='#t'>R</a></td></tr><tr><td><a href='#t'>Z</a></td><td>end</td></tr></table></blockquote><p id='t'>TARGET</p>",
        )],
        &[],
    );
    for color in [false, true] {
        let rendered = assert_fallback(&html, Format::Epub, &Options::new(26, color));
        assert_eq!(
            rendered
                .document
                .links
                .iter()
                .map(|link| link.label.as_str())
                .collect::<Vec<_>>(),
            ["one two three four", "R", "Z"]
        );
        let plain = common::strip_sgr(&rendered.document.text);
        assert_eq!(plain.lines().nth(4), Some("│ │ one two   │ DROP R   │"));
        assert_eq!(spans(&rendered, 0), [(4, 4, 11), (5, 4, 9), (6, 4, 8)]);
        assert_eq!(spans(&rendered, 1), [(4, 21, 22)]);
        assert_eq!(spans(&rendered, 2), [(7, 4, 5)]);
        assert_eq!(rendered.link_regions.len(), 3);
        for region in &rendered.link_regions {
            let first = &region.spans[0];
            let link = &rendered.document.links[region.link];
            assert_eq!((link.line, link.column), (first.line, first.start));
        }
    }
}

#[test]
fn previous_public_struct_literals_remain_source_compatible() {
    let _ = mu::Rendered {
        text: String::new(),
        headings: vec![],
        anchors: vec![],
        links: vec![],
    };
    let _ = mu::NavigationLink {
        target: String::new(),
        label: String::new(),
        line: 0,
        column: 0,
    };
    let _ = mu::Options {
        width: 80,
        color: true,
        links: true,
        images: mu::ImageOptions {
            mode: mu::ImageMode::Never,
            base: None,
            cell: mu::CellSize {
                width: 8,
                height: 16,
            },
            protocol: mu::Protocol::HalfBlock,
            max_rows: None,
        },
        language: None,
    };
    let _: mu::InteractiveRendered = Default::default();
}

#[test]
fn every_public_format_preserves_legacy_document_without_images() {
    let cases = [
        (Format::Markdown, include_str!("fixtures/kitchen-sink.md")),
        (Format::Html, include_str!("fixtures/sample.html")),
        (Format::Rst, include_str!("fixtures/sample.rst")),
        (Format::Org, include_str!("fixtures/sample.org")),
        (Format::AsciiDoc, include_str!("fixtures/sample.adoc")),
        (Format::Json, include_str!("fixtures/sample.json")),
        (Format::Yaml, include_str!("fixtures/sample.yaml")),
        (Format::Toml, include_str!("fixtures/sample.toml")),
        (Format::Csv, include_str!("fixtures/sample.csv")),
        (Format::Tsv, include_str!("fixtures/sample.tsv")),
        (Format::Xml, include_str!("fixtures/sample.rss")),
        (Format::Code, include_str!("fixtures/sample.c")),
        (Format::Man, include_str!("fixtures/sample.1")),
        (Format::Man, include_str!("fixtures/sample-man-output.txt")),
        (Format::Jupyter, include_str!("fixtures/sample.ipynb")),
        (Format::Jupyter, "{\"invalid\":true}"),
        (Format::Epub, "<p>EPUB é👩‍💻</p>"),
        (Format::Pdf, "Extracted PDF é👩‍💻\n"),
    ];
    for (format, input) in cases {
        for width in [20, 80] {
            for color in [false, true] {
                let options = Options {
                    links: true,
                    ..Options::new(width, color)
                };
                let rendered = present_interactive(input, format, &options);
                assert_eq!(
                    rendered.document,
                    present_outlined(input, format, &options),
                    "{format:?} at {width}, color {color}"
                );
                assert!(rendered.images.is_empty());
            }
        }
    }
}

#[test]
fn link_regions_exclude_prose_gaps_and_mixed_owner_graphemes() {
    let html = book(
        &[(
            "one.xhtml",
            "<p>abcdefghijklmnop <a href='#t'>αβγδεζηθ</a> tail</p><p><a href='#t'>A</a>́Z <a href='#t'>B</a></p><p id='t'>TARGET</p>",
        )],
        &[],
    );
    let rendered = assert_fallback(&html, Format::Epub, &Options::new(20, true));
    assert_eq!(
        rendered
            .document
            .links
            .iter()
            .map(|link| link.label.as_str())
            .collect::<Vec<_>>(),
        ["αβγδεζηθ", "A", "B"]
    );
    assert_eq!(spans(&rendered, 0), [(1, 0, 8)]);
    assert!(spans(&rendered, 1).is_empty());
    assert_eq!(spans(&rendered, 2), [(3, 3, 4)]);
    for link in &rendered.document.links {
        assert!(!rendered.document.text.contains(&link.target));
    }
}

#[test]
fn cluster_interior_and_zero_width_ids_bind_forward_without_crossing_chapters() {
    let html = book(
        &[
            (
                "one.xhtml",
                "<p><a href='#mid'>MID</a> <a href='#tail'>TAIL</a></p><p>abcdefghijklmnopqr👩<span id='mid'/>‍💻X<span id='tail'/>́</p>",
            ),
            ("two.xhtml", "<p>UNRELATED</p>"),
        ],
        &[],
    );
    let rendered = assert_fallback(&html, Format::Epub, &Options::new(20, true));
    assert_eq!(
        rendered
            .document
            .links
            .iter()
            .map(|link| link.label.as_str())
            .collect::<Vec<_>>(),
        ["MID"]
    );
    let target = &rendered.document.links[0].target;
    let anchor = rendered
        .document
        .anchors
        .iter()
        .find(|anchor| &anchor.id == target)
        .unwrap();
    assert_eq!((anchor.line, anchor.column), (3, 0));
    assert_eq!(
        common::strip_sgr(rendered.document.text.lines().nth(3).unwrap()),
        "X́"
    );
}

#[test]
fn markdown_html_and_notebooks_capture_native_assets_with_unchanged_fallback() {
    let svg = "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='64'%20height='32'%3E%3Crect%20width='64'%20height='32'%20fill='red'/%3E%3C/svg%3E";
    let markdown = format!("Before\n\n![CAPTION]({svg})\n\nAfter");
    let html = format!("<p>Before</p><p><img src=\"{svg}\" alt='CAPTION'/></p><p>After</p>");
    let notebook = format!(
        "{{\"cells\":[{{\"cell_type\":\"markdown\",\"source\":[\"![CAPTION]({svg})\"]}}],\"metadata\":{{}},\"nbformat\":4,\"nbformat_minor\":5}}"
    );
    for (format, input) in [
        (Format::Markdown, markdown),
        (Format::Html, html),
        (Format::Jupyter, notebook),
    ] {
        let rendered = assert_fallback(&input, format, &native_options(80));
        assert_eq!(rendered.images.len(), 1, "{format:?}");
        assert_eq!(
            (
                rendered.images[0].raster.width,
                rendered.images[0].raster.height
            ),
            (64, 32)
        );
    }
}

#[test]
fn code_wrapping_and_code_anchor_columns_use_the_same_graphemes() {
    let html = book(
        &[(
            "one.xhtml",
            "<p><a href='#t'>CODE</a></p><pre class='language-rust'>abcdefghijklmnopqr👩‍💻<span id='t'/>X\né中🇺🇸</pre>",
        )],
        &[],
    );
    for color in [false, true] {
        let rendered = assert_fallback(&html, Format::Epub, &Options::new(22, color));
        let plain = common::strip_sgr(&rendered.document.text);
        assert!(
            plain.contains("│ abcdefghijklmnopqr👩‍💻\n┆ X\n│ é中🇺🇸\n"),
            "{plain:?}"
        );
        let target = &rendered.document.links[0].target;
        let anchor = rendered
            .document
            .anchors
            .iter()
            .find(|anchor| &anchor.id == target)
            .unwrap();
        assert_eq!((anchor.line, anchor.column), (4, 2));
    }
    let source = "abcdefghijklmnopqr👩‍💻X\né中🇺🇸";
    for color in [false, true] {
        let options = Options {
            language: Some("rust".into()),
            ..Options::new(20, color)
        };
        let rendered = assert_fallback(source, Format::Code, &options);
        assert_eq!(
            common::strip_sgr(&rendered.document.text),
            "abcdefghijklmnopqr👩‍💻\n┆ X\né中🇺🇸\n"
        );
    }
}

#[test]
fn high_detail_portrait_keeps_every_source_pixel_not_upscaled_half_block_samples() {
    let pixels: Vec<_> = (0..384u32)
        .flat_map(|y| {
            (0..256u32).flat_map(move |x| {
                [
                    x as u8,
                    y as u8,
                    (x ^ y) as u8,
                    if (x + y) % 3 == 0 { 100 } else { 255 },
                ]
            })
        })
        .collect();
    let bytes = png(256, 384, &pixels);
    let html = book(
        &[(
            "one.xhtml",
            "<p><img src='portrait.png' alt='HIGH DETAIL PORTRAIT'/></p>",
        )],
        &[("OEBPS/portrait.png", &bytes)],
    );
    let rendered = assert_fallback(&html, Format::Epub, &native_options(80));
    assert_eq!(rendered.images.len(), 1);
    let asset = &rendered.images[0];
    assert_eq!((asset.columns, asset.rows), (32, 24));
    assert_eq!((asset.raster.width, asset.raster.height), (256, 384));
    assert_eq!(asset.raster.pixels.as_ref(), pixels);
}

#[test]
fn trailing_cluster_interior_id_waits_for_the_next_visible_block_or_expires() {
    for following in ["<p>NEXT_TARGET</p>", ""] {
        let chapter =
            format!("<p><a href='#tail'>FOLLOW</a></p><p>👩<span id='tail'/>‍💻</p>{following}");
        let html = book(
            &[("one.xhtml", &chapter), ("two.xhtml", "<p>UNRELATED</p>")],
            &[],
        );
        for color in [false, true] {
            let rendered = assert_fallback(&html, Format::Epub, &Options::new(20, color));
            if following.is_empty() {
                assert!(rendered.document.links.is_empty());
            } else {
                assert_eq!(rendered.document.links.len(), 1);
                let target = &rendered.document.links[0].target;
                let anchor = rendered
                    .document
                    .anchors
                    .iter()
                    .find(|anchor| &anchor.id == target)
                    .unwrap();
                assert_eq!(
                    rendered.document.text.lines().nth(anchor.line),
                    Some("NEXT_TARGET")
                );
                assert_eq!(anchor.column, 0);
            }
        }
    }
}
