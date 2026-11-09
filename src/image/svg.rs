// Author: machinageist
// Date: 2026-11-09
// Description: SVG parsing and rasterising with resvg, external resources disabled
// Notes: Built without resvg's svgz, text and raster-images features, so gzip input is
// rejected, text is skipped and <image> elements never load anything

use resvg::tiny_skia;
use resvg::usvg;

use super::{Rgba, within_limits};

// Byte order mark some editors put before the XML
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

// Markup that may come before the root element
const PROCESSING_START: &[u8] = b"<?";
const PROCESSING_END: &[u8] = b"?>";
const COMMENT_START: &[u8] = b"<!--";
const COMMENT_END: &[u8] = b"-->";
const DECLARATION_START: &[u8] = b"<!";
const SVG_ROOT: &[u8] = b"<svg";

// A parsed SVG document, boxed so Picture stays small
pub(crate) struct Svg {
    tree: Box<usvg::Tree>,
}

impl Svg {
    // Parse SVG bytes, or None when they are not SVG, are malformed or are too large
    pub(crate) fn parse(bytes: &[u8]) -> Option<Svg> {
        if !looks_like_svg(bytes) {
            return None;
        }
        let options = usvg::Options {
            resources_dir: None,
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: Box::new(|_, _, _| None),
                resolve_string: Box::new(|_, _| None),
            },
            ..usvg::Options::default()
        };
        let tree = usvg::Tree::from_data(bytes, &options).ok()?;
        let size = tree.size();
        let (width, height) = (size.width(), size.height());
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return None;
        }
        if !within_limits(width.ceil() as u32, height.ceil() as u32) {
            return None;
        }
        Some(Svg {
            tree: Box::new(tree),
        })
    }

    // Natural size in pixels, rounded up
    pub(crate) fn size(&self) -> (u32, u32) {
        let size = self.tree.size();
        (
            (size.width().ceil() as u32).max(1),
            (size.height().ceil() as u32).max(1),
        )
    }

    // Render at exactly width x height pixels
    pub(crate) fn rasterize(&self, width: u32, height: u32) -> Option<Rgba> {
        if !within_limits(width, height) {
            return None;
        }
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        // One scale for both axes, since callers keep the aspect ratio
        let size = self.tree.size();
        let scale = (width as f32 / size.width()).min(height as f32 / size.height());
        let transform = tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());
        Some(Rgba {
            width,
            height,
            pixels: pixmap.take_demultiplied(),
        })
    }
}

// Check that the root element is <svg>, after any BOM, prolog, comments and doctype
fn looks_like_svg(bytes: &[u8]) -> bool {
    let mut rest = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    loop {
        rest = rest.trim_ascii_start();
        let skipped = if rest.starts_with(PROCESSING_START) {
            skip_past(rest, PROCESSING_END)
        } else if rest.starts_with(COMMENT_START) {
            skip_past(&rest[COMMENT_START.len()..], COMMENT_END)
        } else if rest.starts_with(DECLARATION_START) {
            skip_doctype(rest)
        } else {
            return is_svg_tag(rest);
        };
        match skipped {
            Some(after) => rest = after,
            None => return false,
        }
    }
}

// Return the bytes after the first `end`, or None when it never appears
fn skip_past<'a>(bytes: &'a [u8], end: &[u8]) -> Option<&'a [u8]> {
    let index = bytes.windows(end.len()).position(|window| window == end)?;
    Some(&bytes[index + end.len()..])
}

// Skip a doctype, including an internal subset in square brackets
fn skip_doctype(bytes: &[u8]) -> Option<&[u8]> {
    let close = bytes.iter().position(|&byte| byte == b'>')?;
    match bytes[..close].iter().position(|&byte| byte == b'[') {
        Some(open) => skip_past(skip_past(&bytes[open..], b"]")?, b">"),
        None => Some(&bytes[close + 1..]),
    }
}

// Check for `<svg` followed by whitespace, `>` or `/`
fn is_svg_tag(bytes: &[u8]) -> bool {
    match bytes.strip_prefix(SVG_ROOT) {
        Some(after) => matches!(
            after.first(),
            Some(b'>' | b'/') | Some(b' ' | b'\t' | b'\r' | b'\n')
        ),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED_RECT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20" viewBox="0 0 10 20"><rect width="10" height="20" fill="#ff0000"/></svg>"##;

    // Read one RGBA pixel
    fn pixel(image: &Rgba, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * image.width + x) * 4) as usize;
        image.pixels[index..index + 4].try_into().unwrap()
    }

    // Check whether the tree holds any image node
    fn has_image(group: &usvg::Group) -> bool {
        group.children().iter().any(|node| match node {
            usvg::Node::Image(_) => true,
            usvg::Node::Group(inner) => has_image(inner),
            _ => false,
        })
    }

    // Detection

    #[test]
    fn accepts_plain_svg() {
        assert!(Svg::parse(RED_RECT.as_bytes()).is_some());
    }

    #[test]
    fn accepts_prolog_comment_and_doctype() {
        let text = format!(
            "\u{FEFF}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- made by hand -->\n\
             <!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \
             \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n{RED_RECT}"
        );
        assert!(Svg::parse(text.as_bytes()).is_some());
    }

    #[test]
    fn accepts_doctype_with_internal_subset() {
        let text = format!("<!DOCTYPE svg [ <!ENTITY a \"b\"> ]>{RED_RECT}");
        assert!(Svg::parse(text.as_bytes()).is_some());
    }

    #[test]
    fn rejects_non_svg() {
        assert!(Svg::parse(b"<!DOCTYPE html><html><body><svg></svg></body></html>").is_none());
        assert!(Svg::parse(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").is_none());
        assert!(Svg::parse(b"").is_none());
        assert!(Svg::parse(b"<svgfoo/>").is_none());
        assert!(Svg::parse(b"\x1f\x8b\x08\0\0\0\0\0").is_none());
    }

    // Rendering

    #[test]
    fn rasterizes_solid_red_at_requested_size() {
        let svg = Svg::parse(RED_RECT.as_bytes()).unwrap();
        let image = svg.rasterize(20, 40).unwrap();
        assert!(image.is_valid());
        assert_eq!((image.width, image.height), (20, 40));
        assert_eq!(pixel(&image, 10, 20), [255, 0, 0, 255]);
    }

    #[test]
    fn unpremultiplies_half_transparent_fill() {
        let text = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="#ff8000" fill-opacity="0.5"/></svg>"##;
        let image = Svg::parse(text.as_bytes())
            .unwrap()
            .rasterize(4, 4)
            .unwrap();
        let [red, green, blue, alpha] = pixel(&image, 2, 2);
        assert!(red >= 254, "red {red}");
        assert!(green.abs_diff(128) <= 1, "green {green}");
        assert_eq!(blue, 0);
        assert!(alpha.abs_diff(128) <= 1, "alpha {alpha}");
    }

    #[test]
    fn rasterize_rejects_zero_and_huge_targets() {
        let svg = Svg::parse(RED_RECT.as_bytes()).unwrap();
        assert!(svg.rasterize(0, 10).is_none());
        assert!(svg.rasterize(100_000, 10).is_none());
    }

    // Size

    #[test]
    fn size_rounds_up() {
        let text = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10.2" height="5.5"/>"#;
        assert_eq!(Svg::parse(text.as_bytes()).unwrap().size(), (11, 6));
    }

    #[test]
    fn huge_size_is_rejected() {
        let text = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="10"/>"#;
        assert!(Svg::parse(text.as_bytes()).is_none());
    }

    // External resources

    #[test]
    fn external_images_load_nothing() {
        for href in [
            "file:///etc/passwd",
            "/etc/passwd",
            "https://example.com/a.png",
            "data:image/png;base64,iVBORw0KGgo=",
        ] {
            let text = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="4" height="4"><image href="{href}" width="4" height="4"/><image xlink:href="{href}" width="4" height="4"/></svg>"#
            );
            let svg = Svg::parse(text.as_bytes()).unwrap();
            assert!(!has_image(svg.tree.root()), "{href}");
            assert!(svg.rasterize(4, 4).is_some());
        }
    }

    // Text

    #[test]
    fn text_is_skipped_and_shapes_still_render() {
        let text = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/><text x="1" y="8">hi</text></svg>"#;
        let image = Svg::parse(text.as_bytes())
            .unwrap()
            .rasterize(10, 10)
            .unwrap();
        assert_eq!(pixel(&image, 5, 5), [255, 0, 0, 255]);
    }

    // Robustness

    #[test]
    fn malformed_svg_is_rejected() {
        assert!(Svg::parse(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect").is_none());
        assert!(Svg::parse(b"<svg <<>>").is_none());
        assert!(Svg::parse(b"<!-- never closed <svg/>").is_none());
    }
}
