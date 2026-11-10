// Author: machinageist
// Date: 2026-11-09
// Description: Images drawn in documents: standalone images become pictures, everything else stays alt text
// Notes: Uses a local SVG fixture so no binary files or network are needed

mod common;

use common::strip_sgr;
use mu::{CellSize, Format, ImageMode, ImageOptions, Options, Protocol, present};
use unicode_width::UnicodeWidthStr;

const PAGE: &str = include_str!("fixtures/images/page.html");
const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/images");
const HALF_BLOCKS: [char; 2] = ['▀', '▄'];
const WIDTH: usize = 60;

// Options with images on, half-block output, and a chosen cell geometry
fn with_images(width: usize, cell: CellSize, max_rows: Option<usize>) -> Options {
    Options {
        images: ImageOptions {
            mode: ImageMode::Auto,
            base: Some(FIXTURE_DIR.to_owned()),
            cell,
            protocol: Protocol::HalfBlock,
            max_rows,
        },
        ..Options::new(width, true)
    }
}

// Lines that hold picture cells
fn picture_lines(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|line| line.contains(HALF_BLOCKS))
        .collect()
}

#[test]
fn standalone_image_is_drawn_with_its_caption() {
    let out = present(
        PAGE,
        Format::Html,
        &with_images(WIDTH, CellSize::default(), None),
    );
    let plain = strip_sgr(&out);
    assert!(!picture_lines(&out).is_empty(), "{plain}");
    assert!(plain.contains("A yellow sun on blue"), "{plain}");
    assert!(!plain.contains("[image: A yellow sun on blue]"), "{plain}");
}

#[test]
fn inline_and_missing_images_stay_alt_text() {
    let plain = strip_sgr(&present(
        PAGE,
        Format::Html,
        &with_images(WIDTH, CellSize::default(), None),
    ));
    assert!(plain.contains("[image: icon]"), "{plain}");
    assert!(plain.contains("[image: Missing picture]"), "{plain}");
}

#[test]
fn aspect_ratio_holds_across_cell_geometries() {
    // 160x80 natural size, 2:1
    for (cell, width) in [((8, 16), 60), ((10, 20), 12), ((9, 21), 40), ((7, 7), 20)] {
        let cell = CellSize {
            width: cell.0,
            height: cell.1,
        };
        let out = present(PAGE, Format::Html, &with_images(width, cell, None));
        let lines = picture_lines(&out);
        let rows = lines.len() as f64;
        let cols = strip_sgr(lines[0]).width() as f64;
        let drawn = (cols * f64::from(cell.width)) / (rows * f64::from(cell.height));
        // Rounding to whole cells allows half a cell of error on each side
        let slack =
            0.5 * f64::from(cell.width) / (rows * f64::from(cell.height)) + 0.5 * drawn / rows;
        assert!(
            (drawn - 2.0).abs() <= slack + 1e-9,
            "cell {cell:?}: {cols}x{rows} draws {drawn}"
        );
    }
}

#[test]
fn image_never_exceeds_width_or_max_rows() {
    for width in [20, 33, 60, 100] {
        let out = present(
            PAGE,
            Format::Html,
            &with_images(width, CellSize::default(), Some(3)),
        );
        for line in out.lines() {
            assert!(strip_sgr(line).width() <= width, "width {width}: {line:?}");
        }
        assert!(picture_lines(&out).len() <= 3);
    }
}

#[test]
fn small_images_are_not_enlarged() {
    // 160 px wide at 8 px cells is 20 columns, even with 100 available
    let out = present(
        PAGE,
        Format::Html,
        &with_images(100, CellSize::default(), None),
    );
    let cols = strip_sgr(picture_lines(&out)[0]).width();
    assert_eq!(cols, 20);
}

#[test]
fn images_off_or_color_off_match_text_output() {
    let text = present(PAGE, Format::Html, &Options::new(WIDTH, true));
    let mut never = with_images(WIDTH, CellSize::default(), None);
    never.images.mode = ImageMode::Never;
    assert_eq!(present(PAGE, Format::Html, &never), text);

    let plain = present(PAGE, Format::Html, &Options::new(WIDTH, false));
    let mut colorless = with_images(WIDTH, CellSize::default(), None);
    colorless.color = false;
    assert_eq!(present(PAGE, Format::Html, &colorless), plain);
}

#[test]
fn markdown_image_paragraph_is_drawn() {
    let markdown = "Before\n\n![sun](wide.svg)\n\nAfter\n";
    let out = present(
        markdown,
        Format::Markdown,
        &with_images(WIDTH, CellSize::default(), None),
    );
    assert!(!picture_lines(&out).is_empty());
    assert!(strip_sgr(&out).starts_with("Before\n\n"));
}
