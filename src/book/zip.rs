// Author: machinageist
// Date: 2026-11-10
// Description: A small read-only zip reader: central directory, stored and deflated entries
// Notes: Enough for EPUB. Zip64, encryption and spanned archives are refused, and every
// size is capped before inflating so a hostile archive cannot exhaust memory

use std::cell::Cell;

// Record signatures
const END_SIGNATURE: u32 = 0x0605_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;

// Record sizes and field offsets
const END_SIZE: usize = 22;
const MAX_COMMENT: usize = 0xFFFF;
const CENTRAL_SIZE: usize = 46;
const LOCAL_SIZE: usize = 30;
const ZIP64_MARKER_16: u16 = 0xFFFF;
const ZIP64_MARKER_32: u32 = 0xFFFF_FFFF;
const FLAG_ENCRYPTED: u16 = 1;

// Compression methods
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATED: u16 = 8;

// Limits that keep a hostile archive bounded
pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_ENTRY_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;

// One file in the archive, as the central directory describes it
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    method: u16,
    encrypted: bool,
    compressed: usize,
    size: usize,
    offset: usize,
}

// An opened archive over borrowed bytes
#[derive(Debug)]
pub struct Archive<'a> {
    bytes: &'a [u8],
    pub entries: Vec<Entry>,
    // Bytes inflated so far, against the total limit
    spent: Cell<usize>,
}

// Read a little-endian u16
fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

// Read a little-endian u32
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

// Widen a u32 field to usize
fn size(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

impl<'a> Archive<'a> {
    // Open an archive by reading its central directory
    pub fn open(bytes: &'a [u8]) -> Result<Archive<'a>, String> {
        let end = find_end(bytes).ok_or("no zip directory")?;
        let total = u16_at(bytes, end + 10).ok_or("truncated zip")?;
        let directory_size = u32_at(bytes, end + 12).ok_or("truncated zip")?;
        let directory_offset = u32_at(bytes, end + 16).ok_or("truncated zip")?;
        if total == ZIP64_MARKER_16
            || directory_size == ZIP64_MARKER_32
            || directory_offset == ZIP64_MARKER_32
        {
            return Err("Zip64 archives are not supported".to_owned());
        }
        if u16_at(bytes, end + 4) != Some(0) || u16_at(bytes, end + 6) != Some(0) {
            return Err("multi-disk archives are not supported".to_owned());
        }
        let total = usize::from(total);
        if total > MAX_ENTRIES {
            return Err(format!("more than {MAX_ENTRIES} entries"));
        }
        let mut entries = Vec::with_capacity(total);
        let mut at = size(directory_offset);
        for _ in 0..total {
            if u32_at(bytes, at) != Some(CENTRAL_SIGNATURE) {
                return Err("corrupt zip directory".to_owned());
            }
            let field = |offset: usize| u32_at(bytes, at + offset).ok_or("truncated zip");
            let short = |offset: usize| u16_at(bytes, at + offset).ok_or("truncated zip");
            let name_len = usize::from(short(28)?);
            let extra_len = usize::from(short(30)?);
            let comment_len = usize::from(short(32)?);
            let name_bytes = bytes
                .get(at + CENTRAL_SIZE..at + CENTRAL_SIZE + name_len)
                .ok_or("truncated zip")?;
            let (compressed, uncompressed, offset) = (field(20)?, field(24)?, field(42)?);
            if [compressed, uncompressed, offset].contains(&ZIP64_MARKER_32) {
                return Err("Zip64 entries are not supported".to_owned());
            }
            entries.push(Entry {
                name: String::from_utf8_lossy(name_bytes).into_owned(),
                method: short(10)?,
                encrypted: short(8)? & FLAG_ENCRYPTED != 0,
                compressed: size(compressed),
                size: size(uncompressed),
                offset: size(offset),
            });
            at += CENTRAL_SIZE + name_len + extra_len + comment_len;
        }
        Ok(Archive {
            bytes,
            entries,
            spent: Cell::new(0),
        })
    }

    // Look up an entry by its exact name
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    // Read an entry by name, inflating it if needed
    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let entry = self
            .find(name)
            .ok_or_else(|| format!("{name} is missing"))?;
        self.read_entry(entry)
    }

    // Read one entry's bytes, checking every limit first
    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>, String> {
        if entry.encrypted {
            return Err(format!("{} is encrypted", entry.name));
        }
        if entry.size > MAX_ENTRY_BYTES {
            return Err(format!("{} is too large", entry.name));
        }
        let spent = self.spent.get() + entry.size;
        if spent > MAX_TOTAL_BYTES {
            return Err("the archive expands to too much data".to_owned());
        }
        self.spent.set(spent);
        let at = entry.offset;
        if u32_at(self.bytes, at) != Some(LOCAL_SIGNATURE) {
            return Err(format!("{} has a corrupt header", entry.name));
        }
        let name_len = usize::from(u16_at(self.bytes, at + 26).ok_or("truncated zip")?);
        let extra_len = usize::from(u16_at(self.bytes, at + 28).ok_or("truncated zip")?);
        let start = at + LOCAL_SIZE + name_len + extra_len;
        let data = self
            .bytes
            .get(start..start.saturating_add(entry.compressed))
            .ok_or_else(|| format!("{} is truncated", entry.name))?;
        let out = match entry.method {
            METHOD_STORED => data.to_vec(),
            METHOD_DEFLATED => miniz_oxide::inflate::decompress_to_vec_with_limit(data, entry.size)
                .map_err(|_| format!("{} does not inflate", entry.name))?,
            other => {
                return Err(format!(
                    "{} uses unsupported compression {other}",
                    entry.name
                ));
            }
        };
        if out.len() != entry.size {
            return Err(format!("{} has the wrong size", entry.name));
        }
        Ok(out)
    }
}

// Find the end-of-directory record, scanning back over any comment
fn find_end(bytes: &[u8]) -> Option<usize> {
    let last = bytes.len().checked_sub(END_SIZE)?;
    let first = last.saturating_sub(MAX_COMMENT);
    (first..=last)
        .rev()
        .find(|at| u32_at(bytes, *at) == Some(END_SIGNATURE))
}

// Build archives in tests and fixtures
#[cfg(test)]
pub mod build {
    // Write a zip holding the given files; deflated when `deflate` is set
    pub fn zip(files: &[(&str, &[u8])], deflate: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut directory = Vec::new();
        for (name, data) in files {
            let body = if deflate {
                miniz_oxide::deflate::compress_to_vec(data, 6)
            } else {
                data.to_vec()
            };
            let method: u16 = if deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            let header = |signature: u32, central: bool| {
                let mut record = Vec::new();
                record.extend_from_slice(&signature.to_le_bytes());
                if central {
                    record.extend_from_slice(&20u16.to_le_bytes());
                }
                record.extend_from_slice(&20u16.to_le_bytes());
                record.extend_from_slice(&0u16.to_le_bytes());
                record.extend_from_slice(&method.to_le_bytes());
                record.extend_from_slice(&[0; 8]);
                record.extend_from_slice(&(body.len() as u32).to_le_bytes());
                record.extend_from_slice(&(data.len() as u32).to_le_bytes());
                record.extend_from_slice(&(name.len() as u16).to_le_bytes());
                record.extend_from_slice(&0u16.to_le_bytes());
                if central {
                    record.extend_from_slice(&[0; 10]);
                    record.extend_from_slice(&offset.to_le_bytes());
                }
                record.extend_from_slice(name.as_bytes());
                record
            };
            out.extend(header(super::LOCAL_SIGNATURE, false));
            out.extend(&body);
            directory.extend(header(super::CENTRAL_SIGNATURE, true));
        }
        let directory_offset = out.len() as u32;
        out.extend(&directory);
        out.extend_from_slice(&super::END_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
        out.extend_from_slice(&directory_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_and_deflated_entries_read_back() {
        let text = b"hello hello hello hello".repeat(20);
        for deflate in [false, true] {
            let bytes = build::zip(&[("a.txt", b"one"), ("dir/b.txt", &text)], deflate);
            let archive = Archive::open(&bytes).unwrap();
            assert_eq!(archive.entries.len(), 2);
            assert_eq!(archive.read("a.txt").unwrap(), b"one");
            assert_eq!(archive.read("dir/b.txt").unwrap(), text);
            assert!(archive.read("nope").is_err());
        }
    }

    #[test]
    fn corrupt_input_is_an_error() {
        assert!(Archive::open(b"").is_err());
        assert!(Archive::open(b"PK\x03\x04 not really").is_err());
        let bytes = build::zip(&[("a.txt", b"one")], true);
        assert!(Archive::open(&bytes[..bytes.len() - 5]).is_err());
        let mut bad_size = bytes.clone();
        // The central directory's uncompressed size, for the only entry
        let central = bytes.len() - END_SIZE - (CENTRAL_SIZE + 5);
        bad_size[central + 24] = 9;
        let archive = Archive::open(&bad_size).unwrap();
        assert!(archive.read("a.txt").is_err());
    }

    #[test]
    fn zip64_markers_are_refused() {
        let mut bytes = build::zip(&[("a.txt", b"one")], false);
        let end = bytes.len() - END_SIZE;
        bytes[end + 16..end + 20].copy_from_slice(&ZIP64_MARKER_32.to_le_bytes());
        assert!(Archive::open(&bytes).unwrap_err().contains("Zip64"));
    }

    #[test]
    fn oversized_entries_are_refused_before_inflating() {
        let mut bytes = build::zip(&[("a.txt", b"one")], false);
        let central = bytes.len() - END_SIZE - (CENTRAL_SIZE + 5);
        let huge = (MAX_ENTRY_BYTES as u32 + 1).to_le_bytes();
        bytes[central + 24..central + 28].copy_from_slice(&huge);
        let archive = Archive::open(&bytes).unwrap();
        assert!(archive.read("a.txt").unwrap_err().contains("too large"));
    }
}
