// Author: machinageist
// Date: 2026-11-09
// Description: A small JSON value parser for formats built on JSON, such as Jupyter notebooks
// Notes: Strict JSON with a nesting limit; numbers stay as their source text, and any
// error returns None so callers fall back to showing the input as JSON

const MAX_DEPTH: usize = 256;
const SURROGATE_HIGH: std::ops::RangeInclusive<u32> = 0xD800..=0xDBFF;
const SURROGATE_LOW: std::ops::RangeInclusive<u32> = 0xDC00..=0xDFFF;
const SURROGATE_BASE: u32 = 0x10000;
const HEX_DIGITS: usize = 4;
const LITERALS: [(&str, Value); 3] = [
    ("null", Value::Null),
    ("true", Value::Bool(true)),
    ("false", Value::Bool(false)),
];

// One parsed JSON value
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    // Look up a key in an object
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    // The text of a string value
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    // The items of an array value, or none
    pub fn items(&self) -> &[Value] {
        match self {
            Value::Array(items) => items,
            _ => &[],
        }
    }

    // A string, or an array of strings joined, as notebooks store multi-line text
    pub fn text(&self) -> String {
        match self {
            Value::String(text) => text.clone(),
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => String::new(),
        }
    }
}

// Parse a whole JSON document, or None when it is not valid JSON
pub fn parse(input: &str) -> Option<Value> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        text: input,
        at: 0,
    };
    let value = parser.value(0)?;
    parser.blank();
    (parser.at == parser.bytes.len()).then_some(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    // Skip whitespace
    fn blank(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.at += 1;
        }
    }

    // Consume one expected byte
    fn expect(&mut self, byte: u8) -> Option<()> {
        self.blank();
        (self.bytes.get(self.at) == Some(&byte)).then(|| self.at += 1)
    }

    // Parse any value at a nesting depth
    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.blank();
        match self.bytes.get(self.at)? {
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'"' => self.string().map(Value::String),
            b'-' | b'0'..=b'9' => self.number(),
            _ => {
                let rest = &self.text[self.at..];
                let (word, value) = LITERALS.iter().find(|(word, _)| rest.starts_with(word))?;
                self.at += word.len();
                Some(value.clone())
            }
        }
    }

    // Parse an object
    fn object(&mut self, depth: usize) -> Option<Value> {
        self.at += 1;
        let mut entries = Vec::new();
        self.blank();
        if self.bytes.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Some(Value::Object(entries));
        }
        loop {
            self.blank();
            let key = self.string()?;
            self.expect(b':')?;
            entries.push((key, self.value(depth + 1)?));
            self.blank();
            match self.bytes.get(self.at)? {
                b',' => self.at += 1,
                b'}' => {
                    self.at += 1;
                    return Some(Value::Object(entries));
                }
                _ => return None,
            }
        }
    }

    // Parse an array
    fn array(&mut self, depth: usize) -> Option<Value> {
        self.at += 1;
        let mut items = Vec::new();
        self.blank();
        if self.bytes.get(self.at) == Some(&b']') {
            self.at += 1;
            return Some(Value::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.blank();
            match self.bytes.get(self.at)? {
                b',' => self.at += 1,
                b']' => {
                    self.at += 1;
                    return Some(Value::Array(items));
                }
                _ => return None,
            }
        }
    }

    // Parse a number, keeping its text
    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|byte| matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.at += 1;
        }
        let text = &self.text[start..self.at];
        text.parse::<f64>().ok()?;
        Some(Value::Number(text.to_owned()))
    }

    // Parse a string with its escapes
    fn string(&mut self) -> Option<String> {
        if self.bytes.get(self.at) != Some(&b'"') {
            return None;
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.at..];
            let stop = rest.find(['"', '\\'])?;
            let plain = &rest[..stop];
            if plain.chars().any(|ch| (ch as u32) < 0x20) {
                return None;
            }
            out.push_str(plain);
            self.at += stop;
            if self.bytes[self.at] == b'"' {
                self.at += 1;
                return Some(out);
            }
            self.at += 1;
            let escape = *self.bytes.get(self.at)?;
            self.at += 1;
            match escape {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{8}'),
                b'f' => out.push('\u{c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => out.push(self.unicode()?),
                _ => return None,
            }
        }
    }

    // Read four hex digits
    fn hex(&mut self) -> Option<u32> {
        let digits = self.text.get(self.at..self.at + HEX_DIGITS)?;
        self.at += HEX_DIGITS;
        u32::from_str_radix(digits, 16).ok()
    }

    // Decode `\uXXXX`, joining surrogate pairs; a lone surrogate becomes U+FFFD
    fn unicode(&mut self) -> Option<char> {
        let high = self.hex()?;
        if SURROGATE_HIGH.contains(&high) && self.text[self.at..].starts_with("\\u") {
            self.at += 2;
            let low = self.hex()?;
            if SURROGATE_LOW.contains(&low) {
                let code = SURROGATE_BASE
                    + ((high - SURROGATE_HIGH.start()) << 10)
                    + (low - SURROGATE_LOW.start());
                return char::from_u32(code);
            }
            return Some(char::REPLACEMENT_CHARACTER);
        }
        Some(char::from_u32(high).unwrap_or(char::REPLACEMENT_CHARACTER))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_parse() {
        let value =
            parse(r#" {"a": [1, -2.5e3, true, null], "b": "x\n\u00e9\ud83d\ude00"} "#).unwrap();
        assert_eq!(
            value.get("a").unwrap().items(),
            [
                Value::Number("1".into()),
                Value::Number("-2.5e3".into()),
                Value::Bool(true),
                Value::Null
            ]
        );
        assert_eq!(value.get("b").unwrap().as_str(), Some("x\né😀"));
    }

    #[test]
    fn multi_line_text_joins() {
        let value = parse(r#"["a\n", "b"]"#).unwrap();
        assert_eq!(value.text(), "a\nb");
    }

    #[test]
    fn bad_input_is_none() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "\"tab\there\"",
            "01x",
            "[1] 2",
            "nul",
            "\"\\q\"",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn depth_is_limited() {
        let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 2), "]".repeat(MAX_DEPTH + 2));
        assert_eq!(parse(&deep), None);
        let fine = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(&fine).is_some());
    }
}
