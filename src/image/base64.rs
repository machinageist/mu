// Author: machinageist
// Date: 2026-11-09
// Description: Standard base64 encode and decode for data: URIs and graphics protocols
// Notes: Hand-written to avoid a dependency; decode skips ASCII whitespace and accepts missing padding

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const PAD: u8 = b'=';
const INVALID: u8 = 0xFF;

// Encode bytes as padded standard base64
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let bits = u32::from(triple[0]) << 16 | u32::from(triple[1]) << 8 | u32::from(triple[2]);
        for index in 0..4 {
            if index <= chunk.len() {
                let sextet = (bits >> (18 - 6 * index)) & 0x3F;
                out.push(char::from(ALPHABET[sextet as usize]));
            } else {
                out.push(char::from(PAD));
            }
        }
    }
    out
}

// Decode standard base64, or None on any character outside the alphabet
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let mut table = [INVALID; 256];
    for (value, &symbol) in ALPHABET.iter().enumerate() {
        table[usize::from(symbol)] = value as u8;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut bits = 0u32;
    let mut count = 0;
    for byte in text.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == PAD {
            break;
        }
        let value = table[usize::from(byte)];
        if value == INVALID {
            return None;
        }
        bits = bits << 6 | u32::from(value);
        count += 1;
        if count == 4 {
            out.extend_from_slice(&[(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
            bits = 0;
            count = 0;
        }
    }
    match count {
        0 => {}
        2 => out.push((bits >> 4) as u8),
        3 => out.extend_from_slice(&[(bits >> 10) as u8, (bits >> 2) as u8]),
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_padding_length() {
        for input in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(decode(&encode(input)).as_deref(), Some(input));
        }
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode(b"fo"), "Zm8=");
    }

    #[test]
    fn decode_tolerates_whitespace_and_missing_padding() {
        assert_eq!(decode("Zm9v\nYmE").as_deref(), Some(&b"fooba"[..]));
        assert_eq!(decode("Zm*v"), None);
    }
}
