// Author: machinageist
// Date: 2026-11-09
// Description: Raster codecs: PNG, JPEG, GIF and WebP decoding to RGBA, and PNG encoding
// Notes: Every decoder reads the header dimensions and checks the limits before it allocates
// pixels; corrupt or truncated input returns None. JPEG EXIF orientation is not applied

use std::io::Cursor;
use std::num::NonZeroU64;

use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use super::{MAX_PIXELS, MAX_SIDE, Rgba, within_limits};

// Magic bytes that identify each format
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF];
const GIF87_MAGIC: &[u8] = b"GIF87a";
const GIF89_MAGIC: &[u8] = b"GIF89a";
const RIFF_MAGIC: &[u8] = b"RIFF";
const WEBP_MAGIC: &[u8] = b"WEBP";
// WebP puts its form type after the RIFF tag and a 4-byte size
const WEBP_MAGIC_OFFSET: usize = 8;

// Bytes per pixel in and out, and the alpha of an opaque pixel
const RGB_CHANNELS: usize = 3;
const RGBA_CHANNELS: usize = 4;
const OPAQUE: u8 = 255;

// Decoder memory budget, one full-size RGBA image
const MAX_DECODE_BYTES: u64 = MAX_PIXELS * RGBA_CHANNELS as u64;

// Decode a raster image by its magic bytes, or None for SVG, unknown formats and limit breaches
pub(crate) fn decode(bytes: &[u8]) -> Option<Rgba> {
    let image = if bytes.starts_with(PNG_MAGIC) {
        decode_png(bytes)
    } else if bytes.starts_with(JPEG_MAGIC) {
        decode_jpeg(bytes)
    } else if bytes.starts_with(GIF87_MAGIC) || bytes.starts_with(GIF89_MAGIC) {
        decode_gif(bytes)
    } else if is_webp(bytes) {
        decode_webp(bytes)
    } else {
        None
    }?;
    image.is_valid().then_some(image)
}

// Encode RGBA pixels as a PNG, for protocols that take an image file
pub(crate) fn encode_png(image: &Rgba) -> Vec<u8> {
    let mut out = Vec::new();
    if !image.is_valid() || write_png(image, &mut out).is_err() {
        return Vec::new();
    }
    out
}

// Write an 8-bit RGBA PNG with fast compression into `out`
fn write_png(image: &Rgba, out: &mut Vec<u8>) -> Result<(), png::EncodingError> {
    let mut encoder = png::Encoder::new(out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image.pixels)?;
    writer.finish()
}

// Check for the RIFF container with a WEBP form type
fn is_webp(bytes: &[u8]) -> bool {
    bytes.starts_with(RIFF_MAGIC)
        && bytes.get(WEBP_MAGIC_OFFSET..WEBP_MAGIC_OFFSET + WEBP_MAGIC.len()) == Some(WEBP_MAGIC)
}

// Decode a PNG, expanding palette, gray and 16-bit samples to 8-bit channels
fn decode_png(bytes: &[u8]) -> Option<Rgba> {
    let limits = png::Limits {
        bytes: usize::try_from(MAX_DECODE_BYTES).ok()?,
    };
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(bytes), limits);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let header = decoder.read_header_info().ok()?;
    if !within_limits(header.width, header.height) {
        return None;
    }
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(frame.buffer_size());
    let channels = frame.color_type.samples();
    to_rgba(frame.width, frame.height, &buffer, channels)
}

// Decode a JPEG to RGB, letting the decoder convert gray, CMYK and YCCK
fn decode_jpeg(bytes: &[u8]) -> Option<Rgba> {
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(MAX_SIDE as usize)
        .set_max_height(MAX_SIDE as usize);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().ok()?;
    let info = decoder.info()?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if !within_limits(width, height) {
        return None;
    }
    let channels = decoder.output_colorspace()?.num_components();
    let mut buffer = vec![0; decoder.output_buffer_size()?];
    decoder.decode_into(&mut buffer).ok()?;
    to_rgba(width, height, &buffer, channels)
}

// Decode the first GIF frame onto a transparent canvas the size of the logical screen
fn decode_gif(bytes: &[u8]) -> Option<Rgba> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.set_memory_limit(gif::MemoryLimit::Bytes(NonZeroU64::new(MAX_DECODE_BYTES)?));
    let mut decoder = options.read_info(bytes).ok()?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    if !within_limits(width, height) {
        return None;
    }
    let frame = decoder.next_frame_info().ok()??;
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    let (frame_width, frame_height) = (usize::from(frame.width), usize::from(frame.height));
    if !within_limits(u32::from(frame.width), u32::from(frame.height)) {
        return None;
    }
    let mut frame_pixels = vec![0; decoder.buffer_size()];
    decoder.read_into_buffer(&mut frame_pixels).ok()?;

    // Copy each frame row into the canvas, clipped to the screen
    let (canvas_width, canvas_height) = (width as usize, height as usize);
    let mut pixels = vec![0; canvas_width * canvas_height * RGBA_CHANNELS];
    let visible_width = frame_width.min(canvas_width.saturating_sub(left));
    let visible_height = frame_height.min(canvas_height.saturating_sub(top));
    for row in 0..visible_height {
        let source = row * frame_width * RGBA_CHANNELS;
        let target = ((top + row) * canvas_width + left) * RGBA_CHANNELS;
        let length = visible_width * RGBA_CHANNELS;
        pixels[target..target + length].copy_from_slice(&frame_pixels[source..source + length]);
    }
    Some(Rgba {
        width,
        height,
        pixels,
    })
}

// Decode a lossy or lossless WebP, or the first frame of an animated one
fn decode_webp(bytes: &[u8]) -> Option<Rgba> {
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    if !within_limits(width, height) {
        return None;
    }
    decoder.set_memory_limit(usize::try_from(MAX_DECODE_BYTES).ok()?);
    let mut buffer = vec![0; decoder.output_buffer_size()?];
    decoder.read_image(&mut buffer).ok()?;
    let channels = if decoder.has_alpha() {
        RGBA_CHANNELS
    } else {
        RGB_CHANNELS
    };
    to_rgba(width, height, &buffer, channels)
}

// Expand gray, gray-alpha, RGB or RGBA samples to RGBA
fn to_rgba(width: u32, height: u32, samples: &[u8], channels: usize) -> Option<Rgba> {
    let count = width as usize * height as usize;
    if channels == 0 || samples.len() < count * channels {
        return None;
    }
    let mut pixels = Vec::with_capacity(count * RGBA_CHANNELS);
    for sample in samples.chunks_exact(channels).take(count) {
        let pixel = match *sample {
            [gray] => [gray, gray, gray, OPAQUE],
            [gray, alpha] => [gray, gray, gray, alpha],
            [red, green, blue] => [red, green, blue, OPAQUE],
            [red, green, blue, alpha] => [red, green, blue, alpha],
            _ => return None,
        };
        pixels.extend_from_slice(&pixel);
    }
    Some(Rgba {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated with cjpeg from an 8x8 image, red top half and blue bottom half
    const JPEG_RGB: &[u8] = &[
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x03, 0x02, 0x02, 0x03, 0x02,
        0x02, 0x03, 0x03, 0x03, 0x03, 0x04, 0x03, 0x03, 0x04, 0x05, 0x08, 0x05, 0x05, 0x04, 0x04,
        0x05, 0x0a, 0x07, 0x07, 0x06, 0x08, 0x0c, 0x0a, 0x0c, 0x0c, 0x0b, 0x0a, 0x0b, 0x0b, 0x0d,
        0x0e, 0x12, 0x10, 0x0d, 0x0e, 0x11, 0x0e, 0x0b, 0x0b, 0x10, 0x16, 0x10, 0x11, 0x13, 0x14,
        0x15, 0x15, 0x15, 0x0c, 0x0f, 0x17, 0x18, 0x16, 0x14, 0x18, 0x12, 0x14, 0x15, 0x14, 0xff,
        0xdb, 0x00, 0x43, 0x01, 0x03, 0x04, 0x04, 0x05, 0x04, 0x05, 0x09, 0x05, 0x05, 0x09, 0x14,
        0x0d, 0x0b, 0x0d, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x08,
        0x00, 0x08, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xff, 0xc4, 0x00,
        0x15, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x08, 0xff, 0xc4, 0x00, 0x18, 0x10, 0x01, 0x00, 0x03, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16, 0x64, 0xa3,
        0xe2, 0xff, 0xc4, 0x00, 0x15, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x08, 0xff, 0xc4, 0x00, 0x1b, 0x11, 0x00,
        0x00, 0x07, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x03, 0x06, 0x16, 0x54, 0xa2, 0xd2, 0x17, 0xff, 0xda, 0x00, 0x0c, 0x03, 0x01, 0x00,
        0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0x9b, 0x26, 0xd4, 0xb5, 0xe4, 0x05, 0x1d, 0xce,
        0x92, 0xf1, 0x2e, 0x66, 0xc3, 0x6f, 0xc5, 0x14, 0x9a, 0x17, 0x91, 0xff, 0xd9,
    ];
    // The same image encoded as grayscale
    const JPEG_GRAY: &[u8] = &[
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x03, 0x02, 0x02, 0x03, 0x02,
        0x02, 0x03, 0x03, 0x03, 0x03, 0x04, 0x03, 0x03, 0x04, 0x05, 0x08, 0x05, 0x05, 0x04, 0x04,
        0x05, 0x0a, 0x07, 0x07, 0x06, 0x08, 0x0c, 0x0a, 0x0c, 0x0c, 0x0b, 0x0a, 0x0b, 0x0b, 0x0d,
        0x0e, 0x12, 0x10, 0x0d, 0x0e, 0x11, 0x0e, 0x0b, 0x0b, 0x10, 0x16, 0x10, 0x11, 0x13, 0x14,
        0x15, 0x15, 0x15, 0x0c, 0x0f, 0x17, 0x18, 0x16, 0x14, 0x18, 0x12, 0x14, 0x15, 0x14, 0xff,
        0xc0, 0x00, 0x0b, 0x08, 0x00, 0x08, 0x00, 0x08, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x08, 0xff, 0xc4, 0x00, 0x18, 0x10, 0x00, 0x02, 0x03, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16, 0x64, 0xa3, 0xe2,
        0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00, 0x36, 0x3b, 0x42, 0xb7, 0x27,
        0xff, 0xd9,
    ];
    // Generated with cwebp, lossy, red at half alpha on top and opaque blue below
    const WEBP_LOSSY_ALPHA: &[u8] = &[
        0x52, 0x49, 0x46, 0x46, 0x80, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38,
        0x58, 0x0a, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x07, 0x00, 0x00,
        0x41, 0x4c, 0x50, 0x48, 0x0d, 0x00, 0x00, 0x00, 0x01, 0x0f, 0xf0, 0xc0, 0xff, 0x88, 0x88,
        0x30, 0xff, 0x11, 0xfd, 0x8f, 0x07, 0x00, 0x56, 0x50, 0x38, 0x20, 0x4c, 0x00, 0x00, 0x00,
        0x70, 0x03, 0x00, 0x9d, 0x01, 0x2a, 0x08, 0x00, 0x08, 0x00, 0x00, 0xc0, 0x12, 0x25, 0xa8,
        0x02, 0x74, 0xba, 0x01, 0x26, 0x03, 0xf8, 0x07, 0xe0, 0x05, 0xdf, 0xfd, 0xd8, 0x07, 0xf0,
        0x00, 0x60, 0x6b, 0x37, 0x0a, 0x80, 0x00, 0xfe, 0xfb, 0x67, 0xe7, 0xa6, 0xa7, 0x9f, 0xfe,
        0x7a, 0x93, 0xd9, 0xf5, 0x7f, 0xa1, 0x96, 0xa9, 0x93, 0xff, 0xec, 0xf2, 0xff, 0xec, 0xf2,
        0xff, 0xfa, 0xe1, 0x01, 0xf0, 0x85, 0xff, 0xe7, 0x08, 0x0f, 0x84, 0x2f, 0x88, 0xa3, 0x00,
        0x00,
    ];
    // The same image, lossy with the alpha dropped
    const WEBP_LOSSY: &[u8] = &[
        0x52, 0x49, 0x46, 0x46, 0x58, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38,
        0x20, 0x4c, 0x00, 0x00, 0x00, 0x70, 0x03, 0x00, 0x9d, 0x01, 0x2a, 0x08, 0x00, 0x08, 0x00,
        0x00, 0xc0, 0x12, 0x25, 0xa8, 0x02, 0x74, 0xba, 0x01, 0x26, 0x03, 0xf8, 0x07, 0xe0, 0x05,
        0xdf, 0xfd, 0xd8, 0x07, 0xf0, 0x00, 0x60, 0x6b, 0x37, 0x0a, 0x80, 0x00, 0xfe, 0xfb, 0x67,
        0xe7, 0xa6, 0xa7, 0x9f, 0xfe, 0x7a, 0x93, 0xd9, 0xf5, 0x7f, 0xa1, 0x96, 0xa9, 0x93, 0xff,
        0xec, 0xf2, 0xff, 0xec, 0xf2, 0xff, 0xfa, 0xe1, 0x01, 0xf0, 0x85, 0xff, 0xe7, 0x08, 0x0f,
        0x84, 0x2f, 0x88, 0xa3, 0x00, 0x00,
    ];

    // Fixture colors and how far lossy decoding may drift from them
    const RED: [u8; 3] = [200, 40, 40];
    const BLUE: [u8; 3] = [40, 40, 200];
    const HALF_ALPHA: u8 = 128;
    const FIXTURE_SIDE: u32 = 8;
    const LOSSY_TOLERANCE: u8 = 24;

    // A PNG side length over the limits, and the CRC-32 polynomial PNG chunks use
    const HUGE_SIDE: u32 = 20_000;
    const CRC_POLYNOMIAL: u32 = 0xEDB8_8320;

    // Encode a PNG with the png crate in any color type and depth
    fn png_bytes(
        size: (u32, u32),
        color: png::ColorType,
        depth: png::BitDepth,
        data: &[u8],
        palette: Option<(&[u8], &[u8])>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, size.0, size.1);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if let Some((colors, alphas)) = palette {
            encoder.set_palette(colors.to_vec());
            encoder.set_trns(alphas.to_vec());
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    // Compute the CRC-32 that PNG chunks carry
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ CRC_POLYNOMIAL
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    // Read one pixel of a decoded image
    fn pixel(image: &Rgba, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * image.width + x) as usize) * RGBA_CHANNELS;
        image.pixels[start..start + RGBA_CHANNELS]
            .try_into()
            .unwrap()
    }

    // Assert a lossy color is close to the expected one
    fn assert_close(actual: [u8; 4], expected: [u8; 3]) {
        for (got, want) in actual.iter().zip(expected) {
            assert!(
                got.abs_diff(want) <= LOSSY_TOLERANCE,
                "{actual:?} vs {expected:?}"
            );
        }
    }

    // Check the halves of the red-over-blue fixture
    fn assert_fixture(image: &Rgba) {
        assert_eq!((image.width, image.height), (FIXTURE_SIDE, FIXTURE_SIDE));
        assert_close(pixel(image, 0, 0), RED);
        assert_close(pixel(image, 7, 7), BLUE);
    }

    // Build a 4x3 two-frame GIF whose first frame is 2x2 at offset (1, 1)
    fn gif_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        let palette = [255, 0, 0, 0, 255, 0, 0, 0, 255];
        let mut encoder = gif::Encoder::new(&mut out, 4, 3, &palette).unwrap();
        let mut first = gif::Frame::from_indexed_pixels(2, 2, vec![0, 1, 2, 2], Some(2));
        first.left = 1;
        first.top = 1;
        encoder.write_frame(&first).unwrap();
        let second = gif::Frame::from_indexed_pixels(4, 3, vec![1; 12], None);
        encoder.write_frame(&second).unwrap();
        drop(encoder);
        out
    }

    // Encode a lossless WebP with the image-webp encoder
    fn webp_bytes(data: &[u8], size: (u32, u32), color: image_webp::ColorType) -> Vec<u8> {
        let mut out = Vec::new();
        image_webp::WebPEncoder::new(&mut out)
            .encode(data, size.0, size.1, color)
            .unwrap();
        out
    }

    // Build the four-pixel image used by the exact round trips
    fn sample_rgba() -> Rgba {
        Rgba {
            width: 2,
            height: 2,
            pixels: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 10, 20, 30, 40],
        }
    }

    #[test]
    fn png_round_trips_exactly() {
        let image = sample_rgba();
        let bytes = encode_png(&image);
        assert!(bytes.starts_with(PNG_MAGIC));
        assert_eq!(decode(&bytes), Some(image));
    }

    #[test]
    fn invalid_image_encodes_to_nothing() {
        let image = Rgba {
            width: 2,
            height: 2,
            pixels: vec![0; 3],
        };
        assert!(encode_png(&image).is_empty());
    }

    #[test]
    fn png_rgb_gets_opaque_alpha() {
        let bytes = png_bytes(
            (2, 1),
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[1, 2, 3, 4, 5, 6],
            None,
        );
        let image = decode(&bytes).unwrap();
        assert_eq!(image.pixels, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn png_gray_expands_to_rgb() {
        let bytes = png_bytes(
            (2, 1),
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &[7, 9],
            None,
        );
        let image = decode(&bytes).unwrap();
        assert_eq!(image.pixels, vec![7, 7, 7, 255, 9, 9, 9, 255]);
    }

    #[test]
    fn png_palette_expands_with_transparency() {
        let colors = [10, 20, 30, 40, 50, 60];
        let alphas = [0, 255];
        let bytes = png_bytes(
            (2, 1),
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            &[1, 0],
            Some((&colors, &alphas)),
        );
        let image = decode(&bytes).unwrap();
        assert_eq!(image.pixels, vec![40, 50, 60, 255, 10, 20, 30, 0]);
    }

    #[test]
    fn png_16_bit_strips_to_8_bit() {
        let bytes = png_bytes(
            (1, 1),
            png::ColorType::Rgba,
            png::BitDepth::Sixteen,
            &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xFF, 0xFF],
            None,
        );
        let image = decode(&bytes).unwrap();
        assert_eq!(image.pixels, vec![0x12, 0x56, 0x9A, 0xFF]);
    }

    #[test]
    fn png_over_the_limits_is_rejected() {
        // Signature, then an IHDR claiming 20000x20000 RGBA with a valid CRC
        let mut chunk = b"IHDR".to_vec();
        chunk.extend_from_slice(&HUGE_SIDE.to_be_bytes());
        chunk.extend_from_slice(&HUGE_SIDE.to_be_bytes());
        chunk.extend_from_slice(&[8, 6, 0, 0, 0]);
        let mut bytes = PNG_MAGIC.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(&chunk);
        bytes.extend_from_slice(&crc32(&chunk).to_be_bytes());

        // The header parses cleanly, so the limit check is what rejects it
        let mut decoder = png::Decoder::new(Cursor::new(bytes.as_slice()));
        let info = decoder.read_header_info().unwrap();
        assert_eq!((info.width, info.height), (HUGE_SIDE, HUGE_SIDE));
        assert_eq!(decode(&bytes), None);
    }

    #[test]
    fn jpeg_decodes_opaque_rgb() {
        let image = decode(JPEG_RGB).unwrap();
        assert_fixture(&image);
        assert!(
            image
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[3] == OPAQUE)
        );
    }

    #[test]
    fn jpeg_gray_expands_to_rgb() {
        let image = decode(JPEG_GRAY).unwrap();
        assert_eq!((image.width, image.height), (FIXTURE_SIDE, FIXTURE_SIDE));
        let gray = |p: &[u8; 4]| p[0] == p[1] && p[1] == p[2] && p[3] == OPAQUE;
        assert!(image.pixels.as_chunks::<4>().0.iter().all(gray));
    }

    #[test]
    fn gif_first_frame_lands_at_its_offset() {
        let image = decode(&gif_bytes()).unwrap();
        assert_eq!((image.width, image.height), (4, 3));
        assert_eq!(pixel(&image, 0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&image, 1, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(&image, 2, 1), [0, 255, 0, 255]);
        assert_eq!(pixel(&image, 1, 2)[3], 0);
        assert_eq!(pixel(&image, 3, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn webp_lossless_keeps_exact_pixels() {
        let image = sample_rgba();
        let bytes = webp_bytes(&image.pixels, (2, 2), image_webp::ColorType::Rgba8);
        assert_eq!(decode(&bytes), Some(image));
    }

    #[test]
    fn webp_lossless_rgb_gets_opaque_alpha() {
        let bytes = webp_bytes(&[1, 2, 3, 4, 5, 6], (2, 1), image_webp::ColorType::Rgb8);
        let image = decode(&bytes).unwrap();
        assert_eq!(image.pixels, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn webp_lossy_decodes() {
        let image = decode(WEBP_LOSSY).unwrap();
        assert_fixture(&image);
        assert_eq!(pixel(&image, 0, 0)[3], OPAQUE);
    }

    #[test]
    fn webp_lossy_keeps_alpha() {
        let image = decode(WEBP_LOSSY_ALPHA).unwrap();
        assert_fixture(&image);
        assert_eq!(pixel(&image, 0, 0)[3], HALF_ALPHA);
        assert_eq!(pixel(&image, 7, 7)[3], OPAQUE);
    }

    #[test]
    fn truncated_input_is_rejected() {
        let sources = [
            encode_png(&sample_rgba()),
            JPEG_RGB.to_vec(),
            gif_bytes(),
            WEBP_LOSSY.to_vec(),
            webp_bytes(&sample_rgba().pixels, (2, 2), image_webp::ColorType::Rgba8),
        ];
        for bytes in sources {
            assert!(decode(&bytes).is_some());
            for length in [4, bytes.len() / 2] {
                assert_eq!(decode(&bytes[..length]), None, "length {length}");
            }
        }
    }

    #[test]
    fn unknown_and_svg_are_rejected() {
        assert_eq!(decode(b""), None);
        assert_eq!(decode(b"hello world"), None);
        assert_eq!(decode(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), None);
        assert_eq!(decode(b"RIFF\0\0\0\0WAVE"), None);
    }
}
