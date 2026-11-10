// Author: machinageist
// Date: 2026-11-10
// Description: Binary document formats, converted to text once at load time
// Notes: EPUB becomes HTML through a built-in zip reader; PDF becomes page text through pdftotext

pub mod epub;
pub mod pdf;
pub mod zip;

use crate::format::Format;

// PDF magic, which may follow some junk at the start of the file
const PDF_MAGIC: &[u8] = b"%PDF-";
const PDF_MAGIC_WINDOW: usize = 1024;
// An EPUB is a zip whose first entry is an uncompressed `mimetype` file naming the type
const ZIP_LOCAL_MAGIC: &[u8] = b"PK\x03\x04";
const EPUB_FIRST_ENTRY: &[u8] = b"mimetype";
const EPUB_MEDIA_TYPE: &[u8] = b"application/epub+zip";
const ZIP_NAME_AT: usize = 30;
const EPUB_MAGIC_WINDOW: usize = 128;

// Recognise a binary format from its first bytes
pub fn sniff_bytes(bytes: &[u8]) -> Option<Format> {
    let head = |len: usize| &bytes[..bytes.len().min(len)];
    if contains(head(PDF_MAGIC_WINDOW), PDF_MAGIC) {
        Some(Format::Pdf)
    } else if bytes.starts_with(ZIP_LOCAL_MAGIC)
        && bytes
            .get(ZIP_NAME_AT..)
            .is_some_and(|rest| rest.starts_with(EPUB_FIRST_ENTRY))
        && contains(head(EPUB_MAGIC_WINDOW), EPUB_MEDIA_TYPE)
    {
        Some(Format::Epub)
    } else {
        None
    }
}

// Turn input bytes into the text a format renders: EPUB into HTML, PDF into page text,
// anything else decoded as UTF-8 with invalid bytes replaced
pub fn extract(bytes: &[u8], format: Format) -> Result<String, String> {
    match format {
        Format::Epub => epub::html(bytes),
        Format::Pdf => pdf::text(bytes),
        _ => Ok(String::from_utf8_lossy(bytes).into_owned()),
    }
}

// Check whether a byte string occurs anywhere in another
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_bytes_name_binary_formats() {
        assert_eq!(sniff_bytes(b"%PDF-1.7\n"), Some(Format::Pdf));
        assert_eq!(sniff_bytes(b"\xef\xbb\xbfjunk%PDF-1.4"), Some(Format::Pdf));
        let epub = zip::build::zip(&[("mimetype", b"application/epub+zip")], false);
        assert_eq!(sniff_bytes(&epub), Some(Format::Epub));
        let other = zip::build::zip(&[("mimetype", b"application/zip")], false);
        assert_eq!(sniff_bytes(&other), None);
        assert_eq!(sniff_bytes(b"# Title\n"), None);
        assert_eq!(sniff_bytes(b""), None);
    }

    #[test]
    fn text_formats_decode_lossily() {
        assert_eq!(extract(b"a\xffb", Format::Markdown).unwrap(), "a\u{fffd}b");
    }
}
