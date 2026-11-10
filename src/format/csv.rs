// Author: machinageist
// Date: 2026-11-09
// Description: CSV and TSV as box-drawn tables through the shared table engine
// Notes: Records become table events, so cells get the same scrubbing, wrapping and borders as
// Markdown tables; the first record is the header and numeric columns are right-aligned

use pulldown_cmark::{Alignment, Event, Tag, TagEnd};

use super::Format;

// Byte order mark some editors put at the start of a file
const BOM: char = '\u{feff}';
const QUOTE: char = '"';
const TAB: char = '\t';

// Delimiters a CSV may use, in the order they are tried
const CSV_DELIMITERS: [char; 4] = [',', ';', TAB, '|'];

// Records read to detect a CSV's delimiter, and at most this much text
const DETECT_RECORDS: usize = 20;
const DETECT_BYTES: usize = 64 * 1024;

// Narrowest a CSV column is squeezed to before the table overflows instead
pub(crate) const CSV_MIN_COLUMN: usize = 10;

// Sniffing needs this many records, and first lines starting with these are other formats
const SNIFF_MIN_RECORDS: usize = 3;
const SNIFF_MIN_COMMA_FIELDS: usize = 3;
const NOT_TABULAR_STARTS: [char; 9] = ['#', '-', '*', '>', '|', '<', '{', '[', '='];

// A number for alignment: sign, digits with fraction or exponent, optional percent
const PERCENT: char = '%';

// Read records from CSV (with RFC 4180 quoting) or TSV (without)
pub(crate) fn records(input: &str, delimiter: char, quoting: bool) -> Vec<Vec<String>> {
    let body = input.strip_prefix(BOM).unwrap_or(input);
    let mut reader = Reader::default();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if reader.in_quotes {
            if ch == QUOTE {
                if chars.peek() == Some(&QUOTE) {
                    chars.next();
                    reader.field.push(QUOTE);
                } else {
                    reader.in_quotes = false;
                }
            } else {
                reader.field.push(ch);
            }
            continue;
        }
        match ch {
            QUOTE if quoting && reader.at_field_start => {
                reader.in_quotes = true;
                reader.at_field_start = false;
                reader.has_content = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' | '\n' => reader.end_record(),
            _ if ch == delimiter => {
                reader.end_field();
                reader.has_content = true;
            }
            _ => {
                reader.field.push(ch);
                reader.at_field_start = false;
                reader.has_content = true;
            }
        }
    }
    reader.end_record();
    reader.records
}

#[derive(Default)]
struct Reader {
    records: Vec<Vec<String>>,
    record: Vec<String>,
    field: String,
    in_quotes: bool,
    at_field_start: bool,
    has_content: bool,
}

impl Reader {
    // Close the field being read
    fn end_field(&mut self) {
        self.record.push(std::mem::take(&mut self.field));
        self.at_field_start = true;
    }

    // Close the record being read, skipping blank lines
    fn end_record(&mut self) {
        let blank = !self.has_content
            || (self.record.is_empty() && self.field.trim().is_empty() && !self.in_quotes);
        if blank {
            self.record.clear();
            self.field.clear();
        } else {
            self.end_field();
            self.records.push(std::mem::take(&mut self.record));
        }
        self.at_field_start = true;
        self.has_content = false;
    }
}

// Pick a CSV's delimiter: the first that splits the opening records evenly into 2 or more fields
pub(crate) fn detect_delimiter(input: &str) -> char {
    let mut end = input.len().min(DETECT_BYTES);
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    let head = &input[..end];
    CSV_DELIMITERS
        .into_iter()
        .find(|delimiter| {
            let found = records(head, *delimiter, true);
            let opening = &found[..found.len().min(DETECT_RECORDS)];
            even_fields(opening).is_some()
        })
        .unwrap_or(CSV_DELIMITERS[0])
}

// The shared field count when every record has the same number, at least 2
fn even_fields(records: &[Vec<String>]) -> Option<usize> {
    let count = records.first()?.len();
    (count >= 2 && records.iter().all(|record| record.len() == count)).then_some(count)
}

// Guess CSV or TSV from the head of an input, conservatively
pub(crate) fn sniff(head: &str) -> Option<Format> {
    let body = head.strip_prefix(BOM).unwrap_or(head).trim_start();
    if body.starts_with(NOT_TABULAR_STARTS) {
        return None;
    }
    let lines: Vec<&str> = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    // The last line of a cut-off head may be partial
    let kept = if lines.len() > SNIFF_MIN_RECORDS {
        &lines[..lines.len() - 1]
    } else {
        &lines[..]
    };
    if kept.len() < SNIFF_MIN_RECORDS {
        return None;
    }
    let text = kept.join("\n");
    let tabs = records(&text, TAB, false);
    if tabs.len() >= SNIFF_MIN_RECORDS && even_fields(&tabs).is_some() {
        return Some(Format::Tsv);
    }
    [',', ';'].into_iter().find_map(|delimiter| {
        let found = records(&text, delimiter, true);
        let fields = even_fields(&found).filter(|_| found.len() >= SNIFF_MIN_RECORDS)?;
        let spaced = text.contains(&format!("{delimiter} "));
        (fields >= SNIFF_MIN_COMMA_FIELDS || !spaced).then_some(Format::Csv)
    })
}

// Check whether a cell holds a number, for right alignment
fn is_number(cell: &str) -> bool {
    let text = cell.trim();
    let text = text.strip_suffix(PERCENT).unwrap_or(text);
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (text, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.chars().all(|ch| ch.is_ascii_digit());
    let mantissa_ok = (!whole.is_empty() || !fraction.is_empty())
        && digits(whole)
        && digits(fraction)
        && (mantissa.contains('.') || !whole.is_empty());
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        !exponent.is_empty() && digits(exponent)
    });
    mantissa_ok && exponent_ok
}

// Right-align columns whose non-empty body cells are all numbers
fn alignments(rows: &[Vec<String>], columns: usize) -> Vec<Alignment> {
    (0..columns)
        .map(|column| {
            let mut cells = rows
                .iter()
                .skip(1)
                .filter_map(|row| row.get(column))
                .filter(|cell| !cell.trim().is_empty())
                .peekable();
            if cells.peek().is_some() && cells.all(|cell| is_number(cell)) {
                Alignment::Right
            } else {
                Alignment::None
            }
        })
        .collect()
}

// Turn delimited text into table events: the first record as the header, ragged rows padded
pub(crate) fn events(input: &str, format: Format) -> Vec<Event<'static>> {
    let rows = match format {
        Format::Tsv => records(input, TAB, false),
        _ => records(input, detect_delimiter(input), true),
    };
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }
    let mut events = vec![Event::Start(Tag::Table(alignments(&rows, columns)))];
    for (index, row) in rows.iter().enumerate() {
        let (open, close) = if index == 0 {
            (Tag::TableHead, TagEnd::TableHead)
        } else {
            (Tag::TableRow, TagEnd::TableRow)
        };
        events.push(Event::Start(open));
        for column in 0..columns {
            events.push(Event::Start(Tag::TableCell));
            let cell = row.get(column).map_or("", String::as_str);
            // Line breaks inside a quoted field read as spaces in a cell
            let text: String = cell
                .chars()
                .map(|ch| if matches!(ch, '\r' | '\n') { ' ' } else { ch })
                .collect();
            if !text.is_empty() {
                events.push(Event::Text(text.into()));
            }
            events.push(Event::End(TagEnd::TableCell));
        }
        events.push(Event::End(close));
    }
    events.push(Event::End(TagEnd::Table));
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    // Records as string slices, for compact assertions
    fn parse(input: &str, delimiter: char, quoting: bool) -> Vec<Vec<String>> {
        records(input, delimiter, quoting)
    }

    #[test]
    fn quoting_follows_rfc_4180() {
        let input = "\u{feff}a,\"b,c\",\"say \"\"hi\"\"\"\r\n1,\"two\nlines\",3\r\n";
        assert_eq!(
            parse(input, ',', true),
            [vec!["a", "b,c", "say \"hi\""], vec!["1", "two\nlines", "3"]]
        );
    }

    #[test]
    fn malformed_quotes_and_blank_lines_still_read() {
        assert_eq!(
            parse("a,\"b\"x,c\n\n   \nd,\"\"\n", ',', true),
            [vec!["a", "bx", "c"], vec!["d", ""]]
        );
        assert_eq!(
            parse("say \"hi\",b\n", ',', true),
            [vec!["say \"hi\"", "b"]]
        );
    }

    #[test]
    fn tsv_splits_on_tabs_without_quoting() {
        assert_eq!(
            parse("a\t\"b\t c\"\n1\t2\t3\n", TAB, false),
            [vec!["a", "\"b", " c\""], vec!["1", "2", "3"]]
        );
    }

    #[test]
    fn delimiters_are_detected() {
        assert_eq!(detect_delimiter("a,b\n1,2\n"), ',');
        assert_eq!(detect_delimiter("a;b;c\n1,5;2;3\n"), ';');
        assert_eq!(detect_delimiter("a\tb\n1\t2\n"), TAB);
        assert_eq!(detect_delimiter("a|b\n1|2\n"), '|');
        assert_eq!(detect_delimiter("just one column\nhere\n"), ',');
    }

    #[test]
    fn numbers_are_recognised() {
        for number in ["1", "-2.5", "+.5", "3.", "1e9", "2.5E-3", "40%", " 7 "] {
            assert!(is_number(number), "{number}");
        }
        for text in ["", "-", ".", "1.2.3", "1e", "e5", "12a", "1,000", "N/A"] {
            assert!(!is_number(text), "{text}");
        }
    }

    #[test]
    fn numeric_columns_align_right() {
        let rows = parse("name,qty,note\nx,1,\ny,,2\nz,3.5,n\n", ',', true);
        assert_eq!(
            alignments(&rows, 3),
            [Alignment::None, Alignment::Right, Alignment::None]
        );
    }

    #[test]
    fn sniffing_is_conservative() {
        assert_eq!(
            sniff("id,name,score\n1,a,2\n2,b,3\n3,c,4\n"),
            Some(Format::Csv)
        );
        assert_eq!(sniff("a\tb\n1\t2\n3\t4\n"), Some(Format::Tsv));
        assert_eq!(sniff("x;y\n1;2\n3;4\n"), Some(Format::Csv));
        // Prose with commas
        assert_eq!(sniff("Hello, world.\nThis, that.\nOne, two.\n"), None);
        // Too few records, or uneven ones
        assert_eq!(sniff("a,b\n1,2\n"), None);
        assert_eq!(sniff("a,b\n1,2,3\n4,5\n"), None);
        // Other formats' openings
        assert_eq!(sniff("# Title, with comma\na,b\nc,d\n"), None);
        assert_eq!(sniff("| a | b |\n|---|---|\n| 1 | 2 |\n"), None);
    }

    #[test]
    fn events_pad_ragged_rows_and_flatten_line_breaks() {
        let events = events("a,b,c\n1\n\"x\ny\",2\n", Format::Csv);
        let cells: Vec<String> = events
            .iter()
            .filter_map(|event| match event {
                Event::Text(text) => Some(text.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(cells, ["a", "b", "c", "1", "x y", "2"]);
        let cell_count = events
            .iter()
            .filter(|event| matches!(event, Event::Start(Tag::TableCell)))
            .count();
        assert_eq!(cell_count, 9);
        assert!(super::events("", Format::Csv).is_empty());
    }
}
