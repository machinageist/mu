// Author: machinageist
// Date: 2026-11-09
// Description: Raster codecs: PNG, JPEG, GIF and WebP decoding to RGBA, and PNG encoding
// Notes: Stub until implemented

use super::Rgba;

// Decode a raster image by its magic bytes, or None for SVG, unknown formats and limit breaches
pub(crate) fn decode(_bytes: &[u8]) -> Option<Rgba> {
    None
}

// Encode RGBA pixels as a PNG, for protocols that take an image file
pub(crate) fn encode_png(_image: &Rgba) -> Vec<u8> {
    Vec::new()
}
