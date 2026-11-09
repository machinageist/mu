// Author: machinageist
// Date: 2026-11-09
// Description: SVG parsing and rasterising with resvg, external resources disabled
// Notes: Stub until implemented

use super::Rgba;

pub(crate) struct Svg;

impl Svg {
    // Parse SVG bytes, or None when they are not SVG
    pub(crate) fn parse(_bytes: &[u8]) -> Option<Svg> {
        None
    }

    // Natural size in pixels
    pub(crate) fn size(&self) -> (u32, u32) {
        (1, 1)
    }

    // Render at exactly width x height pixels
    pub(crate) fn rasterize(&self, _width: u32, _height: u32) -> Option<Rgba> {
        None
    }
}
