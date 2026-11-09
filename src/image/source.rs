// Author: machinageist
// Date: 2026-11-09
// Description: Resolve an image source to bytes: local path, data: URI, or http(s) in fetch mode
// Notes: Stub until implemented

use super::ImageOptions;

// Read the bytes an image source points at, honouring the image mode and size cap
pub(crate) fn fetch(_src: &str, _options: &ImageOptions) -> Option<Vec<u8>> {
    None
}
