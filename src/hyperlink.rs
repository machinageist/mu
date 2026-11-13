// Author: machinageist
// Description: Validate and encode external OSC 8 destinations without replaying source controls

use std::fmt::Write;
use std::net::Ipv6Addr;

pub(crate) const OPEN: &str = "\x1b]8;;";
pub(crate) const CLOSE: &str = "\x1b]8;;\x1b\\";
pub(crate) const TERMINATOR: &str = "\x1b\\";

// Keep a generated control string bounded even for a very large source URL
const MAX_URI_BYTES: usize = 8192;

// Accept only passive web/mail destinations and return their portable ASCII URI spelling
pub(crate) fn destination(url: &str) -> Option<String> {
    // Parsers can replace source NUL/invalid entities with U+FFFD before this boundary;
    // reject that ambiguity instead of turning a repaired target into an actionable URI
    if url.len() > MAX_URI_BYTES
        || url.chars().any(|ch| {
            ch.is_control()
                || matches!(ch, '\\' | '\u{fffd}' | '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
    {
        return None;
    }
    let (scheme, rest) = url.split_once(':')?;
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        let authority = rest.strip_prefix("//")?.split(['/', '?', '#']).next()?;
        if !valid_authority(authority) {
            return None;
        }
    } else if !scheme.eq_ignore_ascii_case("mailto") || rest.is_empty() {
        return None;
    }

    let mut encoded = String::with_capacity(url.len());
    let bytes = url.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'%'
            && !bytes
                .get(index + 1..index + 3)
                .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit))
        {
            return None;
        }
        if byte.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&'()*+,;=%".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            // Writing into a String is infallible
            let _ = write!(encoded, "%{byte:02X}");
        }
        if encoded.len() > MAX_URI_BYTES {
            return None;
        }
    }
    Some(encoded)
}

// Check components without rewriting the authority into a different destination
fn valid_authority(authority: &str) -> bool {
    let host_port = match authority.split_once('@') {
        Some((userinfo, host_port)) => {
            if !userinfo.chars().all(|ch| reg_name_char(ch) || ch == ':') {
                return false;
            }
            host_port
        }
        None => authority,
    };
    let port = if let Some(literal) = host_port.strip_prefix('[') {
        let Some((address, suffix)) = literal.split_once(']') else {
            return false;
        };
        if address.parse::<Ipv6Addr>().is_err() {
            return false;
        }
        if suffix.is_empty() {
            None
        } else if let Some(port) = suffix.strip_prefix(':') {
            Some(port)
        } else {
            return false;
        }
    } else {
        let (host, port) = host_port
            .split_once(':')
            .map_or((host_port, None), |(host, port)| (host, Some(port)));
        if host.is_empty() || !host.chars().all(reg_name_char) {
            return false;
        }
        port
    };
    // An explicit port must contain decimal digits in the 16-bit range, not an empty repair
    port.is_none_or(|port| {
        !port.is_empty()
            && port.bytes().all(|byte| byte.is_ascii_digit())
            && port.parse::<u16>().is_ok()
    })
}

// URI reg-name/userinfo characters, allowing Unicode for the ASCII-encoding pass
// Percent escape syntax is checked by destination before any payload is emitted
fn reg_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || "-._~!$&'()*+,;=%".contains(ch)
        || (!ch.is_ascii() && !ch.is_whitespace())
}
