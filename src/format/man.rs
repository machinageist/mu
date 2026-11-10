// Author: machinageist
// Date: 2026-11-09
// Description: Man pages in roff source (man and mdoc macros) to pulldown-cmark events
// Notes: A line-based reader, not a roff interpreter: requests map to blocks, escapes to text
// and fonts, and unknown requests are skipped while the text after them still shows

use pulldown_cmark::{CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd};

// Request lines start with a control character
const CONTROL_CHARS: [char; 2] = ['.', '\''];
const ESCAPE: char = '\\';
const QUOTE: char = '"';
const COMMENT_MARKS: [char; 2] = ['"', '#'];
// Ends a macro definition or ignored block
const BLOCK_END: &str = "..";
const MAILTO: &str = "mailto:";
// A `.so` page only points at another file, which mu does not open
const SOURCE_LEAD: &str = "See";

// Text shown for parts of a page
const SECTION_OPEN: &str = "(";
const SECTION_CLOSE: &str = ")";
const NAME_DASH: &str = "— ";
const DEFAULT_ARGUMENT: &str = "file ...";
const FLAG_DASH: &str = "-";
const COLUMN_GAP: &str = "  ";
const INCLUDE_OPEN: &str = "#include <";
const INCLUDE_CLOSE: &str = ">";
const FUNCTION_OPEN: &str = "(";
const FUNCTION_SEPARATOR: &str = ", ";
const FUNCTION_CLOSE: &str = ")";
const OPTION_OPEN: &str = "[";
const OPTION_CLOSE: &str = "]";
const QUOTED: (&str, &str) = ("‘", "’");
const EXIT_TEXT: (&str, &str) = (
    "The ",
    " utility exits 0 on success, and >0 if an error occurs.",
);
const RETURN_TEXT: (&str, &str) = (
    "The ",
    "() function returns the value 0 if successful; otherwise the value -1 is returned and \
     the global variable errno is set to indicate the error.",
);
const STANDARD_FLAG: &str = "-std";
// Names for `.St` standards; others show without their leading dash
const STANDARDS: [(&str, &str); 14] = [
    ("-p1003.1", "IEEE Std 1003.1 (“POSIX.1”)"),
    ("-p1003.1-96", "ISO/IEC 9945-1:1996 (“POSIX.1”)"),
    ("-p1003.1-2001", "IEEE Std 1003.1-2001 (“POSIX.1”)"),
    ("-p1003.1-2004", "IEEE Std 1003.1-2004 (“POSIX.1”)"),
    ("-p1003.1-2008", "IEEE Std 1003.1-2008 (“POSIX.1”)"),
    ("-p1003.2", "IEEE Std 1003.2 (“POSIX.2”)"),
    ("-p1003.2-92", "IEEE Std 1003.2-1992 (“POSIX.2”)"),
    ("-iso8601", "ISO 8601"),
    ("-ansiC", "ANSI X3.159-1989 (“ANSI C89”)"),
    ("-isoC", "ISO/IEC 9899:1990 (“ISO C90”)"),
    ("-isoC-99", "ISO/IEC 9899:1999 (“ISO C99”)"),
    (
        "-susv2",
        "Version 2 of the Single UNIX Specification (“SUSv2”)",
    ),
    (
        "-susv3",
        "Version 3 of the Single UNIX Specification (“SUSv3”)",
    ),
    ("-xpg4", "X/Open Portability Guide Issue 4 (“XPG4”)"),
];
const OPTION_DASH: char = '-';

// mdoc punctuation that attaches to the word before it, or to the word after it
const CLOSE_DELIMITERS: [&str; 8] = [".", ",", ":", ";", ")", "]", "?", "!"];
const OPEN_DELIMITERS: [&str; 2] = ["(", "["];

// IP tags drawn as list bullets rather than terms
const BULLET_TAGS: [&str; 7] = ["•", "·", "-", "*", "o", "–", "—"];
const NUMBER_TAG_ENDS: [char; 2] = ['.', ')'];

// mdoc list types
const LIST_BULLETS: [&str; 3] = ["-bullet", "-dash", "-hyphen"];
const LIST_NUMBERED: &str = "-enum";
const LIST_COLUMNS: &str = "-column";
const LIST_PLAIN: &str = "-item";
const DISPLAY_LITERAL: [&str; 2] = ["-literal", "-unfilled"];

// Sections whose `.Nm` lines each start a new line
const SYNOPSIS: &str = "SYNOPSIS";

// Sniffing: the first request must open a page
const SNIFF_OPENERS: [&str; 2] = [".TH", ".Dd"];
const SNIFF_DEFINE: [&str; 3] = [".de", ".ig", ".am"];
const SNIFF_SKIP_PREFIX: &str = "\\}";

// A text font, as set by `\f` escapes and font macros
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Font {
    Roman,
    Bold,
    Italic,
    BoldItalic,
    Mono,
}

// Font names after `\f` and `.ft`; `P` (previous) is handled apart
const FONTS: [(&str, Font); 16] = [
    ("R", Font::Roman),
    ("1", Font::Roman),
    ("", Font::Roman),
    ("B", Font::Bold),
    ("3", Font::Bold),
    ("I", Font::Italic),
    ("2", Font::Italic),
    ("BI", Font::BoldItalic),
    ("4", Font::BoldItalic),
    ("CW", Font::Mono),
    ("CR", Font::Mono),
    ("C", Font::Mono),
    ("CB", Font::Bold),
    ("CI", Font::Italic),
    ("TT", Font::Mono),
    ("SB", Font::Bold),
];
const PREVIOUS_FONT: &str = "P";

// What a request line does
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Title,
    Section,
    Subsection,
    Paragraph,
    Tagged,
    Indented,
    Indent,
    Dedent,
    NoFill,
    Fill,
    Example,
    ExampleEnd,
    LineBreak,
    Space,
    SetFont,
    Styled(Font),
    Alternate(Font, Font),
    UrlStart,
    UrlEnd,
    MailStart,
    MailEnd,
    Synopsis,
    SynopsisOption,
    SynopsisEnd,
    TableStart,
    TableEnd,
    Define,
    ManReference,
    Source,
    ListStart,
    ListItem,
    ListEnd,
    DisplayStart,
    DisplayEnd,
    DisplayLine,
    DisplayIndented,
    Skip,
}

// Request and block macro names, man first, then mdoc
const REQUESTS: [(&str, Request); 64] = [
    ("TH", Request::Title),
    ("SH", Request::Section),
    ("SS", Request::Subsection),
    ("PP", Request::Paragraph),
    ("LP", Request::Paragraph),
    ("P", Request::Paragraph),
    ("HP", Request::Paragraph),
    ("TP", Request::Tagged),
    ("TQ", Request::Tagged),
    ("IP", Request::Indented),
    ("RS", Request::Indent),
    ("RE", Request::Dedent),
    ("nf", Request::NoFill),
    ("fi", Request::Fill),
    ("EX", Request::Example),
    ("EE", Request::ExampleEnd),
    ("br", Request::LineBreak),
    ("sp", Request::Space),
    ("ft", Request::SetFont),
    ("B", Request::Styled(Font::Bold)),
    ("I", Request::Styled(Font::Italic)),
    ("SB", Request::Styled(Font::Bold)),
    ("SM", Request::Styled(Font::Roman)),
    ("BR", Request::Alternate(Font::Bold, Font::Roman)),
    ("BI", Request::Alternate(Font::Bold, Font::Italic)),
    ("IB", Request::Alternate(Font::Italic, Font::Bold)),
    ("IR", Request::Alternate(Font::Italic, Font::Roman)),
    ("RB", Request::Alternate(Font::Roman, Font::Bold)),
    ("RI", Request::Alternate(Font::Roman, Font::Italic)),
    ("UR", Request::UrlStart),
    ("UE", Request::UrlEnd),
    ("MT", Request::MailStart),
    ("ME", Request::MailEnd),
    ("SY", Request::Synopsis),
    ("OP", Request::SynopsisOption),
    ("YS", Request::SynopsisEnd),
    ("TS", Request::TableStart),
    ("TE", Request::TableEnd),
    ("de", Request::Define),
    ("de1", Request::Define),
    ("am", Request::Define),
    ("ig", Request::Define),
    ("MR", Request::ManReference),
    ("so", Request::Source),
    ("Dt", Request::Title),
    ("Sh", Request::Section),
    ("Ss", Request::Subsection),
    ("Pp", Request::Paragraph),
    ("Lp", Request::Paragraph),
    ("Bl", Request::ListStart),
    ("It", Request::ListItem),
    ("El", Request::ListEnd),
    ("Bd", Request::DisplayStart),
    ("Ed", Request::DisplayEnd),
    ("Dl", Request::DisplayLine),
    ("D1", Request::DisplayIndented),
    ("Dd", Request::Skip),
    ("Os", Request::Skip),
    ("Rs", Request::Skip),
    ("Re", Request::Skip),
    ("Bf", Request::Skip),
    ("Ef", Request::Skip),
    ("Ek", Request::Skip),
    ("PD", Request::Skip),
];

// What an mdoc macro inside a line does
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Callable {
    Style(Font),
    Argument,
    Flag,
    Name,
    Description,
    CrossReference,
    Enclose(&'static str, &'static str),
    // `.Ql`: quoted literal text
    Quoted,
    Open(&'static str),
    Close(&'static str),
    NoSpace,
    Prefix,
    Function,
    Include,
    Link,
    Mail,
    Fixed(&'static str),
    Standard,
    Exit,
    Return,
    Tab,
    Nothing,
}

// mdoc macros that may appear anywhere in a macro line
const CALLABLE: [(&str, Callable); 64] = [
    ("Ad", Callable::Style(Font::Italic)),
    ("An", Callable::Style(Font::Roman)),
    ("Ar", Callable::Argument),
    ("Cd", Callable::Style(Font::Bold)),
    ("Cm", Callable::Style(Font::Bold)),
    ("Dv", Callable::Style(Font::Roman)),
    ("Em", Callable::Style(Font::Italic)),
    ("Er", Callable::Style(Font::Roman)),
    ("Ev", Callable::Style(Font::Roman)),
    ("Fa", Callable::Style(Font::Italic)),
    ("Fl", Callable::Flag),
    ("Fn", Callable::Function),
    ("Ft", Callable::Style(Font::Italic)),
    ("Ic", Callable::Style(Font::Bold)),
    ("In", Callable::Include),
    ("Li", Callable::Style(Font::Mono)),
    ("Lk", Callable::Link),
    ("Mt", Callable::Mail),
    ("Nm", Callable::Name),
    ("Nd", Callable::Description),
    ("No", Callable::Style(Font::Roman)),
    ("Ns", Callable::NoSpace),
    ("Pa", Callable::Style(Font::Italic)),
    ("Pf", Callable::Prefix),
    ("Sx", Callable::Style(Font::Italic)),
    ("Sy", Callable::Style(Font::Bold)),
    ("Tn", Callable::Style(Font::Roman)),
    ("Va", Callable::Style(Font::Italic)),
    ("Vt", Callable::Style(Font::Italic)),
    ("Ot", Callable::Style(Font::Italic)),
    ("Ms", Callable::Style(Font::Roman)),
    ("Lb", Callable::Style(Font::Roman)),
    ("%A", Callable::Style(Font::Roman)),
    ("%T", Callable::Style(Font::Italic)),
    ("%B", Callable::Style(Font::Italic)),
    ("%J", Callable::Style(Font::Italic)),
    ("%D", Callable::Style(Font::Roman)),
    ("Xr", Callable::CrossReference),
    ("Ql", Callable::Quoted),
    ("Dq", Callable::Enclose("“", "”")),
    ("Sq", Callable::Enclose("‘", "’")),
    ("Pq", Callable::Enclose("(", ")")),
    ("Bq", Callable::Enclose("[", "]")),
    ("Brq", Callable::Enclose("{", "}")),
    ("Aq", Callable::Enclose("⟨", "⟩")),
    ("Qq", Callable::Enclose("\"", "\"")),
    ("Op", Callable::Enclose(OPTION_OPEN, OPTION_CLOSE)),
    ("Oo", Callable::Open(OPTION_OPEN)),
    ("Oc", Callable::Close(OPTION_CLOSE)),
    ("Do", Callable::Open("“")),
    ("Dc", Callable::Close("”")),
    ("Po", Callable::Open("(")),
    ("Pc", Callable::Close(")")),
    ("Ta", Callable::Tab),
    ("Ux", Callable::Fixed("UNIX")),
    ("Bx", Callable::Fixed("BSD")),
    ("At", Callable::Fixed("AT&T UNIX")),
    ("Fx", Callable::Fixed("FreeBSD")),
    ("Nx", Callable::Fixed("NetBSD")),
    ("Ox", Callable::Fixed("OpenBSD")),
    ("St", Callable::Standard),
    ("Ex", Callable::Exit),
    ("Rv", Callable::Return),
    ("Xo", Callable::Nothing),
];
const IGNORED_CALLABLE: [&str; 3] = ["Xc", "Sm", "Bk"];

// Common `\(xx` and `\[name]` characters
const NAMED_CHARS: [(&str, &str); 48] = [
    ("em", "—"),
    ("en", "–"),
    ("hy", "-"),
    ("bu", "•"),
    ("aq", "'"),
    ("dq", "\""),
    ("lq", "“"),
    ("rq", "”"),
    ("oq", "‘"),
    ("cq", "’"),
    ("co", "©"),
    ("rg", "®"),
    ("tm", "™"),
    ("->", "→"),
    ("<-", "←"),
    ("<>", "↔"),
    ("ua", "↑"),
    ("da", "↓"),
    ("rA", "⇒"),
    ("lA", "⇐"),
    ("<=", "≤"),
    (">=", "≥"),
    ("!=", "≠"),
    ("==", "≡"),
    ("mu", "×"),
    ("di", "÷"),
    ("+-", "±"),
    ("de", "°"),
    ("sc", "§"),
    ("ps", "¶"),
    ("dg", "†"),
    ("ti", "~"),
    ("ha", "^"),
    ("rs", "\\"),
    ("sl", "/"),
    ("ba", "|"),
    ("or", "|"),
    ("lB", "["),
    ("rB", "]"),
    ("lC", "{"),
    ("rC", "}"),
    ("la", "⟨"),
    ("ra", "⟩"),
    ("ga", "`"),
    ("aa", "´"),
    ("ul", "_"),
    ("Fo", "«"),
    ("Fc", "»"),
];

// Common `\*x` strings
const STRINGS: [(&str, &str); 6] = [
    ("lq", "“"),
    ("rq", "”"),
    ("R", "®"),
    ("Tm", "™"),
    ("aq", "'"),
    ("q", "\""),
];

// Escapes that take a name (`\nx`, `\n(xx`, `\n[name]`) and print nothing
const NAMED_SILENT: &str = "nmMgkFVY$";
// Escapes that take a delimited argument (`\w'text'`) and print nothing
const DELIMITED_SILENT: &str = "hvwoXZblLxDRAHSB";
// Escapes that print nothing and take no argument
const SILENT: &str = "&)%:|^,/pdruaz{}";
// Escapes that print a space
const SPACES: &str = " ~0";
const UNICODE_PREFIX: char = 'u';
const NUMBERED_CHAR: char = 'N';

// One piece of a line after escapes
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(String),
    Font(Font),
    PreviousFont,
    // `\c`: the next line joins this one without a space
    Join,
}

// An open block on the stack
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    // `.RS`, `.D1` or an indented `.Bd`
    Indent,
    // Terms with definitions: `.TP`, `.IP tag`, `.Bl -tag`
    Terms,
    // Bullets or numbers: `.IP \(bu`, `.Bl -bullet`, `.Bl -enum`
    Items { numbered: bool },
    // `.Bl -column`: each item a line
    Columns,
}

#[derive(Clone, Copy, Debug)]
struct Block {
    kind: Kind,
    // An item or definition is open inside
    open: bool,
    // Opened by `.Bl` or `.Bd`, so only `.El` or `.Ed` closes it
    explicit: bool,
}

// What the next input line becomes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    None,
    Term,
    Heading(HeadingLevel),
    Styled(Font),
}

// Verbatim lines being collected until an end request
struct Literal {
    text: String,
    ends: &'static [&'static str],
}
const NO_FILL_ENDS: &[&str] = &["fi"];
const EXAMPLE_ENDS: &[&str] = &["EE"];
const TABLE_ENDS: &[&str] = &["TE"];
const DISPLAY_ENDS: &[&str] = &["Ed"];

// Check whether the head of an input is roff source for a man page
pub fn sniff(head: &str) -> bool {
    let mut defining = false;
    for line in head.lines() {
        let line = line.trim_end();
        if defining {
            defining = line != BLOCK_END;
            continue;
        }
        if SNIFF_OPENERS
            .iter()
            .any(|opener| line == *opener || line.starts_with(&format!("{opener} ")))
        {
            return true;
        }
        if SNIFF_DEFINE.iter().any(|define| line.starts_with(define)) {
            defining = true;
            continue;
        }
        let skippable = line.is_empty()
            || line.starts_with(CONTROL_CHARS)
            || line.starts_with(SNIFF_SKIP_PREFIX);
        if !skippable {
            return false;
        }
    }
    false
}

// Convert a man page in roff source to document events
pub fn events(input: &str) -> Vec<Event<'static>> {
    let mut builder = Builder::new(is_mdoc(input));
    let mut skipping = false;
    for line in logical_lines(input) {
        if skipping {
            skipping = line.trim_end() != BLOCK_END;
            continue;
        }
        skipping = builder.line(&line);
    }
    builder.finish()
}

// True when the page uses mdoc macros, which starts with `.Dd`
fn is_mdoc(input: &str) -> bool {
    input
        .lines()
        .map(strip_comment)
        .filter_map(|line| request(&line))
        .map(|(name, _)| name)
        .find(|name| !name.is_empty())
        .is_some_and(|name| name == "Dd")
}

// Join escaped line ends and drop comments, keeping each logical line
fn logical_lines(input: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut joined = String::new();
    for raw in input.lines() {
        let comment_only = is_comment_line(raw);
        let line = strip_comment(raw);
        if comment_only || (line.trim().is_empty() && !raw.trim().is_empty()) {
            continue;
        }
        let trailing = line.len() - line.trim_end_matches(ESCAPE).len();
        if trailing % 2 == 1 {
            joined.push_str(&line[..line.len() - 1]);
            continue;
        }
        joined.push_str(&line);
        lines.push(std::mem::take(&mut joined));
    }
    if !joined.is_empty() {
        lines.push(joined);
    }
    lines
}

// A line that is only a comment: `.\"`, `'\"` or `.\#`
fn is_comment_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix(CONTROL_CHARS) else {
        return false;
    };
    let rest = rest.trim_start();
    rest.strip_prefix(ESCAPE)
        .is_some_and(|after| after.starts_with(COMMENT_MARKS))
}

// Cut a line at an unescaped `\"` or `\#`
fn strip_comment(line: &str) -> String {
    let mut chars = line.char_indices();
    while let Some((at, ch)) = chars.next() {
        if ch == ESCAPE {
            match chars.next() {
                Some((_, mark)) if COMMENT_MARKS.contains(&mark) => {
                    return line[..at].to_owned();
                }
                _ => {}
            }
        }
    }
    line.to_owned()
}

// Split a request line into its name and arguments
fn request(line: &str) -> Option<(String, Vec<String>)> {
    let rest = line.strip_prefix(CONTROL_CHARS)?.trim_start();
    let name_end = rest.find([' ', '\t']).unwrap_or(rest.len());
    Some((rest[..name_end].to_owned(), arguments(&rest[name_end..])))
}

// Split request arguments on blanks, honouring double quotes and escaped spaces
fn arguments(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|ch| *ch == ' ' || *ch == '\t').is_some() {}
        let Some(&first) = chars.peek() else {
            break;
        };
        let mut arg = String::new();
        if first == QUOTE {
            chars.next();
            while let Some(ch) = chars.next() {
                if ch == QUOTE {
                    if chars.next_if_eq(&QUOTE).is_some() {
                        arg.push(QUOTE);
                        continue;
                    }
                    break;
                }
                arg.push(ch);
            }
        } else {
            while let Some(ch) = chars.next_if(|ch| *ch != ' ' && *ch != '\t') {
                arg.push(ch);
                if ch == ESCAPE
                    && let Some(next) = chars.next()
                {
                    arg.push(next);
                }
            }
        }
        args.push(arg);
    }
    args
}

// Look up a request or block macro
fn request_kind(name: &str) -> Option<Request> {
    REQUESTS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, kind)| *kind)
}

// Look up an mdoc macro callable inside a line
fn callable(name: &str) -> Option<Callable> {
    if IGNORED_CALLABLE.contains(&name) {
        return Some(Callable::Nothing);
    }
    CALLABLE
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, kind)| *kind)
}

// Look up a font name
fn font(name: &str) -> Font {
    FONTS
        .iter()
        .find(|(known, _)| *known == name)
        .map_or(Font::Roman, |(_, font)| *font)
}

// Read an escape's name: one char, `(xx`, or `[name]`
fn escape_name(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    match chars.next() {
        Some('(') => chars.by_ref().take(2).collect(),
        Some('[') => chars.by_ref().take_while(|ch| *ch != ']').collect(),
        Some(ch) => ch.to_string(),
        None => String::new(),
    }
}

// Read a delimited argument such as `'text'`
fn delimited(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    match chars.next() {
        Some(delimiter) => chars.by_ref().take_while(|ch| *ch != delimiter).collect(),
        None => String::new(),
    }
}

// Characters for a `\(xx` or `\[name]` escape; unknown names print nothing
fn named_char(name: &str) -> String {
    if let Some(hex) = name.strip_prefix(UNICODE_PREFIX)
        && let Some(ch) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
    {
        return ch.to_string();
    }
    NAMED_CHARS
        .iter()
        .find(|(known, _)| *known == name)
        .map_or(String::new(), |(_, text)| (*text).to_owned())
}

// Split text into runs and font changes, resolving escapes
fn pieces(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut run = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != ESCAPE {
            run.push(ch);
            continue;
        }
        let Some(code) = chars.next() else {
            break;
        };
        match code {
            'f' => {
                let name = escape_name(&mut chars);
                out.push(Piece::Text(std::mem::take(&mut run)));
                out.push(if name == PREVIOUS_FONT {
                    Piece::PreviousFont
                } else {
                    Piece::Font(font(&name))
                });
            }
            '(' => {
                let name: String = chars.by_ref().take(2).collect();
                run.push_str(&named_char(&name));
            }
            '[' => {
                let name: String = chars.by_ref().take_while(|ch| *ch != ']').collect();
                run.push_str(&named_char(&name));
            }
            '*' => {
                let name = escape_name(&mut chars);
                if let Some((_, value)) = STRINGS.iter().find(|(known, _)| *known == name) {
                    run.push_str(value);
                }
            }
            's' => {
                chars.next_if(|ch| *ch == '+' || *ch == '-');
                match chars.peek() {
                    Some('(' | '[') => {
                        escape_name(&mut chars);
                    }
                    Some('\'') => {
                        delimited(&mut chars);
                    }
                    _ => {
                        chars.next_if(char::is_ascii_digit);
                    }
                }
            }
            NUMBERED_CHAR => {
                let number = delimited(&mut chars);
                if let Some(ch) = number.parse().ok().and_then(char::from_u32) {
                    run.push(ch);
                }
            }
            'C' => run.push_str(&named_char(&delimited(&mut chars))),
            'c' => {
                out.push(Piece::Text(std::mem::take(&mut run)));
                out.push(Piece::Join);
            }
            'e' | ESCAPE => run.push(ESCAPE),
            '-' => run.push('-'),
            't' => run.push('\t'),
            _ if SPACES.contains(code) => run.push(' '),
            _ if SILENT.contains(code) => {}
            _ if NAMED_SILENT.contains(code) => {
                escape_name(&mut chars);
            }
            _ if DELIMITED_SILENT.contains(code) => {
                delimited(&mut chars);
            }
            // Anything else prints the character itself, as roff does
            _ => run.push(code),
        }
    }
    out.push(Piece::Text(run));
    out.retain(|piece| !matches!(piece, Piece::Text(text) if text.is_empty()));
    out
}

// Text with escapes resolved and font changes dropped
fn plain(text: &str) -> String {
    pieces(text)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

// Wrap text in events for its font
fn styled(text: String, font: Font) -> Vec<Event<'static>> {
    let body = Event::Text(CowStr::from(text.clone()));
    match font {
        Font::Roman => vec![body],
        Font::Bold => vec![Event::Start(Tag::Strong), body, Event::End(TagEnd::Strong)],
        Font::Italic => vec![
            Event::Start(Tag::Emphasis),
            body,
            Event::End(TagEnd::Emphasis),
        ],
        Font::BoldItalic => vec![
            Event::Start(Tag::Strong),
            Event::Start(Tag::Emphasis),
            body,
            Event::End(TagEnd::Emphasis),
            Event::End(TagEnd::Strong),
        ],
        Font::Mono => vec![Event::Code(CowStr::from(text))],
    }
}

// A heading tag at a level
fn heading(level: HeadingLevel) -> Tag<'static> {
    Tag::Heading {
        level,
        id: None,
        classes: vec![],
        attrs: vec![],
    }
}

// A link tag to a destination
fn link(url: String) -> Tag<'static> {
    Tag::Link {
        link_type: LinkType::Inline,
        dest_url: CowStr::from(url),
        title: CowStr::from(""),
        id: CowStr::from(""),
    }
}

struct Builder {
    out: Vec<Event<'static>>,
    inline: Vec<Event<'static>>,
    // A space is due before the next word
    space: bool,
    font: Font,
    previous_font: Font,
    blocks: Vec<Block>,
    literal: Option<Literal>,
    pending: Pending,
    // Inside `.UR`/`.MT`: the destination and where the link text started
    link: Option<(String, usize)>,
    mdoc: bool,
    name: String,
    section: String,
}

impl Builder {
    // Start an empty page
    fn new(mdoc: bool) -> Builder {
        Builder {
            out: Vec::new(),
            inline: Vec::new(),
            space: false,
            font: Font::Roman,
            previous_font: Font::Roman,
            blocks: Vec::new(),
            literal: None,
            pending: Pending::None,
            link: None,
            mdoc,
            name: String::new(),
            section: String::new(),
        }
    }

    // Close everything still open and return the events
    fn finish(mut self) -> Vec<Event<'static>> {
        if let Some(literal) = self.literal.take() {
            self.code_block(literal.text);
        }
        if self.link.is_some() {
            self.end_link(&[]);
        }
        self.end_line();
        self.close_all();
        self.out
    }

    // Handle one logical line; true when a definition block starts that must be skipped
    fn line(&mut self, line: &str) -> bool {
        if self.literal.is_some() {
            self.literal_line(line);
            return false;
        }
        match request(line) {
            Some((name, args)) => return self.request(&name, &args),
            // A blank line breaks the paragraph but keeps the indent, like `.sp`
            None if line.trim().is_empty() => self.flush(),
            None => {
                self.text_line(line);
                self.end_line();
            }
        }
        false
    }

    // Collect one line of verbatim text, or finish the block at its end request
    fn literal_line(&mut self, line: &str) {
        let Some(literal) = self.literal.as_mut() else {
            return;
        };
        match request(line) {
            Some((name, args)) => {
                if literal.ends.contains(&name.as_str()) {
                    let text = std::mem::take(&mut literal.text);
                    self.literal = None;
                    self.code_block(text);
                } else if name == "sp" {
                    literal.text.push('\n');
                } else if matches!(
                    request_kind(&name),
                    Some(Request::Styled(_) | Request::Alternate(..))
                ) || callable(&name).is_some()
                {
                    let words: Vec<String> = args.iter().map(|arg| plain(arg)).collect();
                    literal.text.push_str(&words.join(" "));
                    literal.text.push('\n');
                }
            }
            None => {
                literal.text.push_str(&plain(line));
                literal.text.push('\n');
            }
        }
    }

    // Handle a request line; true when a definition block starts
    fn request(&mut self, name: &str, args: &[String]) -> bool {
        if let Some(kind) = request_kind(name) {
            self.block_request(kind, args);
            return kind == Request::Define;
        }
        if self.mdoc && callable(name).is_some() {
            if name == "Nm" && self.section == SYNOPSIS && !self.inline.is_empty() {
                self.flush();
            }
            let mut tokens = vec![name.to_owned()];
            tokens.extend_from_slice(args);
            self.macro_words(&tokens);
            self.end_line();
        }
        false
    }

    // Act on a known request
    fn block_request(&mut self, kind: Request, args: &[String]) {
        match kind {
            Request::Title => {
                self.close_all();
                let mut title = args.first().map(|name| plain(name)).unwrap_or_default();
                if let Some(section) = args.get(1) {
                    title.push_str(SECTION_OPEN);
                    title.push_str(&plain(section));
                    title.push_str(SECTION_CLOSE);
                }
                if !self.mdoc && self.name.is_empty() {
                    self.name = args.first().map(|name| plain(name)).unwrap_or_default();
                }
                self.heading_text(HeadingLevel::H1, title);
            }
            Request::Section | Request::Subsection => {
                self.close_all();
                let level = if kind == Request::Section {
                    HeadingLevel::H2
                } else {
                    HeadingLevel::H3
                };
                let words: Vec<String> = args.iter().map(|arg| plain(arg)).collect();
                if words.is_empty() {
                    self.pending = Pending::Heading(level);
                } else {
                    self.section = words.join(" ");
                    self.heading_text(level, words.join(" "));
                }
            }
            Request::Paragraph => self.paragraph(),
            Request::Tagged => {
                self.flush();
                self.open_terms();
                self.out.push(Event::Start(Tag::DefinitionListTitle));
                self.pending = Pending::Term;
            }
            Request::Indented => self.indented(args.first()),
            Request::Indent => {
                // Text right before an indent is its label, kept tight against it
                if !self.inline.is_empty() && self.pending == Pending::None {
                    self.out.push(Event::Start(Tag::DefinitionListTitle));
                    self.out.append(&mut self.inline);
                    self.out.push(Event::End(TagEnd::DefinitionListTitle));
                    self.space = false;
                }
                self.flush();
                self.push_block(Kind::Indent, false);
            }
            Request::Dedent => {
                self.flush();
                while let Some(block) = self.blocks.pop() {
                    self.close_block(block);
                    if block.kind == Kind::Indent && !block.explicit {
                        break;
                    }
                }
            }
            Request::NoFill => self.start_literal(NO_FILL_ENDS),
            Request::Example => self.start_literal(EXAMPLE_ENDS),
            Request::TableStart => self.start_literal(TABLE_ENDS),
            Request::Fill | Request::ExampleEnd | Request::TableEnd | Request::Skip => {}
            Request::LineBreak => {
                if !self.inline.is_empty() {
                    self.inline.push(Event::HardBreak);
                    self.space = false;
                }
            }
            Request::Space => self.flush(),
            Request::SetFont => {
                let name = args.first().map_or("", String::as_str);
                self.set_font(if name == PREVIOUS_FONT {
                    self.previous_font
                } else {
                    font(name)
                });
            }
            Request::Styled(style) => {
                if args.is_empty() {
                    self.pending = Pending::Styled(style);
                } else {
                    self.gap();
                    let words: Vec<String> = args.iter().map(|arg| plain(arg)).collect();
                    self.run(words.join(" "), style);
                    self.space = true;
                    self.end_line();
                }
            }
            Request::Alternate(first, second) => {
                self.gap();
                for (at, arg) in args.iter().enumerate() {
                    let style = if at % 2 == 0 { first } else { second };
                    self.run(plain(arg), style);
                }
                self.space = true;
                self.end_line();
            }
            Request::UrlStart => self.start_link(args.first().map(|url| plain(url))),
            Request::MailStart => self.start_link(
                args.first()
                    .map(|address| format!("{MAILTO}{}", plain(address))),
            ),
            Request::UrlEnd | Request::MailEnd => self.end_link(args),
            Request::Synopsis => {
                self.paragraph();
                self.gap();
                let words: Vec<String> = args.iter().map(|arg| plain(arg)).collect();
                self.run(words.join(" "), Font::Bold);
                self.space = true;
            }
            Request::SynopsisOption => {
                self.gap();
                self.run(OPTION_OPEN.to_owned(), Font::Roman);
                if let Some(option) = args.first() {
                    self.run(plain(option), Font::Bold);
                }
                for arg in args.iter().skip(1) {
                    self.run(" ".to_owned(), Font::Roman);
                    self.run(plain(arg), Font::Italic);
                }
                self.run(OPTION_CLOSE.to_owned(), Font::Roman);
                self.space = true;
            }
            Request::SynopsisEnd => self.flush(),
            Request::Define => {}
            Request::Source => {
                if let Some(file) = args.first() {
                    self.flush();
                    self.word(SOURCE_LEAD, Font::Roman);
                    self.word(file, Font::Bold);
                    self.flush();
                }
            }
            Request::ManReference => {
                self.gap();
                if let Some(page) = args.first() {
                    self.run(plain(page), Font::Bold);
                }
                if let Some(section) = args.get(1) {
                    self.run(
                        format!("{SECTION_OPEN}{}{SECTION_CLOSE}", plain(section)),
                        Font::Roman,
                    );
                }
                if let Some(trailer) = args.get(2) {
                    self.run(plain(trailer), Font::Roman);
                }
                self.space = true;
                self.end_line();
            }
            Request::ListStart => self.start_list(args),
            Request::ListItem => self.list_item(args),
            Request::ListEnd | Request::DisplayEnd => {
                self.flush();
                while let Some(block) = self.blocks.pop() {
                    self.close_block(block);
                    if block.explicit {
                        break;
                    }
                }
            }
            Request::DisplayStart => {
                if args
                    .iter()
                    .any(|arg| DISPLAY_LITERAL.contains(&arg.as_str()))
                {
                    self.start_literal(DISPLAY_ENDS);
                } else {
                    self.flush();
                    self.push_block(Kind::Indent, true);
                }
            }
            Request::DisplayLine => {
                self.flush();
                let words: Vec<String> = args.iter().map(|arg| plain(arg)).collect();
                self.code_block(format!("{}\n", words.join(" ")));
            }
            Request::DisplayIndented => {
                self.flush();
                self.push_block(Kind::Indent, true);
                self.macro_words(args);
                self.flush();
                if let Some(block) = self.blocks.pop() {
                    self.close_block(block);
                }
            }
        }
    }

    // Add one line of running text, with its escapes and fonts
    fn text_line(&mut self, line: &str) {
        // A line starting with a blank starts a new output line
        if line.starts_with([' ', '\t']) && !self.inline.is_empty() {
            self.inline.push(Event::HardBreak);
            self.space = false;
        }
        let forced = match self.pending {
            Pending::Styled(style) => {
                self.pending = Pending::None;
                Some(style)
            }
            _ => None,
        };
        self.gap();
        let mut join = false;
        for piece in pieces(line.trim_start()) {
            match piece {
                Piece::Text(text) => self.run(text, forced.unwrap_or(self.font)),
                Piece::Font(next) => self.set_font(next),
                Piece::PreviousFont => self.set_font(self.previous_font),
                Piece::Join => join = true,
            }
        }
        self.space = !join;
    }

    // Lay out an mdoc macro line: macros style the words after them
    fn macro_words(&mut self, tokens: &[String]) {
        let mut at = 0;
        while at < tokens.len() {
            at = match callable(&tokens[at]) {
                Some(kind) => self.callable(kind, tokens, at + 1),
                None => {
                    self.word(&tokens[at], Font::Roman);
                    at + 1
                }
            };
        }
    }

    // Apply one callable macro to the tokens after it, returning where the next one starts
    fn callable(&mut self, kind: Callable, tokens: &[String], start: usize) -> usize {
        let end = tokens[start..]
            .iter()
            .position(|token| callable(token).is_some())
            .map_or(tokens.len(), |offset| start + offset);
        let args = &tokens[start..end];
        match kind {
            Callable::Style(style) => self.words(args, style),
            Callable::Argument => {
                if args.iter().all(|arg| is_delimiter(arg)) {
                    self.word(DEFAULT_ARGUMENT, Font::Italic);
                }
                self.words(args, Font::Italic);
            }
            Callable::Flag => {
                if args.iter().all(|arg| is_delimiter(arg)) {
                    self.word(FLAG_DASH, Font::Bold);
                }
                for arg in args {
                    if is_delimiter(arg) {
                        self.word(arg, Font::Roman);
                    } else {
                        self.word(&format!("{FLAG_DASH}{arg}"), Font::Bold);
                    }
                }
            }
            Callable::Name => {
                if let Some(first) = args.iter().find(|arg| !is_delimiter(arg))
                    && self.name.is_empty()
                {
                    self.name = plain(first);
                }
                if args.iter().all(|arg| is_delimiter(arg)) {
                    let name = self.name.clone();
                    self.word(&name, Font::Bold);
                }
                self.words(args, Font::Bold);
            }
            Callable::Description => {
                self.word(NAME_DASH.trim_end(), Font::Roman);
                self.macro_words(&tokens[start..]);
                return tokens.len();
            }
            Callable::CrossReference => {
                let mut rest = args;
                if let Some(page) = rest.first() {
                    let mut text = plain(page);
                    if let Some(section) = rest.get(1).filter(|arg| !is_delimiter(arg)) {
                        text.push_str(&format!("{SECTION_OPEN}{}{SECTION_CLOSE}", plain(section)));
                        rest = &rest[2..];
                    } else {
                        rest = &rest[1..];
                    }
                    self.word(&text, Font::Bold);
                }
                self.words(rest, Font::Roman);
            }
            Callable::Enclose(..) | Callable::Quoted => {
                let (open, close) = match kind {
                    Callable::Enclose(open, close) => (open, close),
                    _ => QUOTED,
                };
                let mut inner_end = tokens.len();
                while inner_end > start
                    && CLOSE_DELIMITERS.contains(&tokens[inner_end - 1].as_str())
                {
                    inner_end -= 1;
                }
                self.gap();
                self.run(open.to_owned(), Font::Roman);
                self.space = false;
                let inner = &tokens[start..inner_end];
                if kind == Callable::Quoted {
                    self.words(inner, Font::Mono);
                } else {
                    self.macro_words(inner);
                }
                self.space = false;
                self.run(close.to_owned(), Font::Roman);
                self.space = true;
                return inner_end;
            }
            Callable::Open(open) => {
                self.gap();
                self.run(open.to_owned(), Font::Roman);
                self.space = false;
                self.words(args, Font::Roman);
            }
            Callable::Close(close) => {
                self.space = false;
                self.run(close.to_owned(), Font::Roman);
                self.space = true;
                self.words(args, Font::Roman);
            }
            Callable::NoSpace => {
                self.space = false;
                self.words(args, Font::Roman);
                if args.is_empty() {
                    self.space = false;
                }
            }
            Callable::Prefix => {
                if let Some((prefix, rest)) = args.split_first() {
                    self.word(prefix, Font::Roman);
                    self.space = false;
                    self.words(rest, Font::Roman);
                }
                if args.len() <= 1 {
                    self.space = false;
                }
            }
            Callable::Function => {
                if let Some((name, params)) = args.split_first() {
                    self.word(name, Font::Bold);
                    self.space = false;
                    self.run(FUNCTION_OPEN.to_owned(), Font::Roman);
                    for (at, param) in params.iter().enumerate() {
                        if at > 0 {
                            self.run(FUNCTION_SEPARATOR.to_owned(), Font::Roman);
                        }
                        self.run(plain(param), Font::Italic);
                    }
                    self.run(FUNCTION_CLOSE.to_owned(), Font::Roman);
                }
            }
            Callable::Include => {
                if let Some((file, rest)) = args.split_first() {
                    self.word(&format!("{INCLUDE_OPEN}{file}{INCLUDE_CLOSE}"), Font::Bold);
                    self.words(rest, Font::Roman);
                }
            }
            Callable::Link | Callable::Mail => {
                if let Some((target, rest)) = args.split_first() {
                    let target = plain(target);
                    let url = if kind == Callable::Mail {
                        format!("{MAILTO}{target}")
                    } else {
                        target.clone()
                    };
                    let (label, trailer): (Vec<&String>, Vec<&String>) =
                        rest.iter().partition(|arg| !is_delimiter(arg));
                    self.gap();
                    self.inline.push(Event::Start(link(url)));
                    if label.is_empty() {
                        self.inline.push(Event::Text(CowStr::from(target)));
                    } else {
                        let words: Vec<String> = label.iter().map(|arg| plain(arg)).collect();
                        self.inline.push(Event::Text(CowStr::from(words.join(" "))));
                    }
                    self.inline.push(Event::End(TagEnd::Link));
                    self.space = true;
                    for arg in trailer {
                        self.word(arg, Font::Roman);
                    }
                }
            }
            Callable::Fixed(text) => {
                self.word(text, Font::Roman);
                self.words(args, Font::Roman);
            }
            Callable::Standard => {
                for arg in args {
                    let named = STANDARDS.iter().find(|(flag, _)| flag == arg);
                    match named {
                        Some((_, name)) => self.word(name, Font::Roman),
                        None => self.word(arg.trim_start_matches(OPTION_DASH), Font::Roman),
                    }
                }
            }
            Callable::Exit | Callable::Return => {
                let names: Vec<String> = args
                    .iter()
                    .filter(|arg| arg.as_str() != STANDARD_FLAG && !is_delimiter(arg))
                    .map(|arg| plain(arg))
                    .collect();
                let name = names.first().cloned().unwrap_or_else(|| self.name.clone());
                let (before, after) = if kind == Callable::Exit {
                    EXIT_TEXT
                } else {
                    RETURN_TEXT
                };
                self.word(before.trim_end(), Font::Roman);
                self.word(&name, Font::Bold);
                self.space = false;
                self.word(after, Font::Roman);
            }
            Callable::Tab => {
                self.space = false;
                self.run(COLUMN_GAP.to_owned(), Font::Roman);
                self.words(args, Font::Roman);
            }
            Callable::Nothing => self.words(args, Font::Roman),
        }
        end
    }

    // Add words in one style, attaching punctuation the mdoc way
    fn words(&mut self, args: &[String], style: Font) {
        for arg in args {
            let style = if is_delimiter(arg) {
                Font::Roman
            } else {
                style
            };
            self.word(arg, style);
        }
    }

    // Add one word, spaced from the last unless punctuation says otherwise
    fn word(&mut self, raw: &str, style: Font) {
        if CLOSE_DELIMITERS.contains(&raw) {
            self.space = false;
        }
        self.gap();
        self.run(plain(raw), style);
        self.space = !OPEN_DELIMITERS.contains(&raw);
    }

    // Write the space owed before the next word
    fn gap(&mut self) {
        if self.space && !self.inline.is_empty() {
            self.inline.push(Event::Text(CowStr::from(" ")));
        }
        self.space = false;
    }

    // Append a run of text in a style
    fn run(&mut self, text: String, style: Font) {
        if !text.is_empty() {
            self.inline.extend(styled(text, style));
        }
    }

    // Change the current font, remembering the last one for `\fP`
    fn set_font(&mut self, next: Font) {
        self.previous_font = self.font;
        self.font = next;
    }

    // Finish an input line: a pending term or heading takes what the line added
    fn end_line(&mut self) {
        match self.pending {
            Pending::Term => {
                self.pending = Pending::None;
                self.out.append(&mut self.inline);
                self.out.push(Event::End(TagEnd::DefinitionListTitle));
                self.out.push(Event::Start(Tag::DefinitionListDefinition));
                if let Some(block) = self.blocks.last_mut() {
                    block.open = true;
                }
                self.space = false;
            }
            Pending::Heading(level) => {
                self.pending = Pending::None;
                let text = std::mem::take(&mut self.inline);
                self.out.push(Event::Start(heading(level)));
                self.out.extend(text);
                self.out.push(Event::End(TagEnd::Heading(level)));
                self.space = false;
            }
            Pending::None | Pending::Styled(_) => {}
        }
    }

    // Emit a heading with plain text
    fn heading_text(&mut self, level: HeadingLevel, text: String) {
        self.out.push(Event::Start(heading(level)));
        self.out.push(Event::Text(CowStr::from(text)));
        self.out.push(Event::End(TagEnd::Heading(level)));
    }

    // Write the paragraph collected so far
    fn flush(&mut self) {
        if self.pending == Pending::Term || matches!(self.pending, Pending::Heading(_)) {
            self.end_line();
        }
        if !self.inline.is_empty() {
            self.out.push(Event::Start(Tag::Paragraph));
            self.out.append(&mut self.inline);
            self.out.push(Event::End(TagEnd::Paragraph));
        }
        self.space = false;
    }

    // End a paragraph, closing `.TP`/`.IP` lists back to the enclosing indent
    fn paragraph(&mut self) {
        self.flush();
        while let Some(block) = self.blocks.last().copied() {
            if block.kind == Kind::Indent || block.explicit {
                break;
            }
            self.blocks.pop();
            self.close_block(block);
        }
    }

    // Open a block and its events
    fn push_block(&mut self, kind: Kind, explicit: bool) {
        match kind {
            Kind::Indent => self.out.push(Event::Start(Tag::DefinitionListDefinition)),
            Kind::Terms => self.out.push(Event::Start(Tag::DefinitionList)),
            Kind::Items { numbered } => {
                self.out
                    .push(Event::Start(Tag::List(numbered.then_some(1))));
            }
            Kind::Columns => {}
        }
        self.blocks.push(Block {
            kind,
            open: false,
            explicit,
        });
    }

    // Emit the events that close a block
    fn close_block(&mut self, block: Block) {
        match block.kind {
            Kind::Indent => self.out.push(Event::End(TagEnd::DefinitionListDefinition)),
            Kind::Terms => {
                if block.open {
                    self.out.push(Event::End(TagEnd::DefinitionListDefinition));
                }
                self.out.push(Event::End(TagEnd::DefinitionList));
            }
            Kind::Items { numbered } => {
                if block.open {
                    self.out.push(Event::End(TagEnd::Item));
                }
                self.out.push(Event::End(TagEnd::List(numbered)));
            }
            Kind::Columns => {}
        }
    }

    // Close every open block, as a section heading does
    fn close_all(&mut self) {
        self.flush();
        while let Some(block) = self.blocks.pop() {
            self.close_block(block);
        }
    }

    // Make sure a list of terms is open, closing the previous definition
    fn open_terms(&mut self) {
        match self.blocks.last().copied() {
            Some(Block {
                kind: Kind::Terms,
                open,
                ..
            }) => {
                if open {
                    self.out.push(Event::End(TagEnd::DefinitionListDefinition));
                }
                if let Some(block) = self.blocks.last_mut() {
                    block.open = false;
                }
            }
            Some(Block {
                kind: Kind::Items { .. },
                explicit: false,
                ..
            }) => {
                if let Some(block) = self.blocks.pop() {
                    self.close_block(block);
                }
                self.push_block(Kind::Terms, false);
            }
            _ => self.push_block(Kind::Terms, false),
        }
    }

    // Handle `.IP`: a bullet, a number, a term, or a plain indented paragraph
    fn indented(&mut self, tag: Option<&String>) {
        self.flush();
        let tag = tag.map(|tag| plain(tag)).unwrap_or_default();
        let tag = tag.trim();
        if tag.is_empty() {
            if self.blocks.last().is_some_and(|block| block.open) {
                return;
            }
            self.open_terms();
            self.out.push(Event::Start(Tag::DefinitionListDefinition));
            if let Some(block) = self.blocks.last_mut() {
                block.open = true;
            }
            return;
        }
        let numbered = tag.strip_suffix(NUMBER_TAG_ENDS).is_some_and(|digits| {
            !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
        });
        if BULLET_TAGS.contains(&tag) || numbered {
            self.open_item(numbered);
            return;
        }
        self.open_terms();
        self.out.push(Event::Start(Tag::DefinitionListTitle));
        self.out.push(Event::Text(CowStr::from(tag.to_owned())));
        self.out.push(Event::End(TagEnd::DefinitionListTitle));
        self.out.push(Event::Start(Tag::DefinitionListDefinition));
        if let Some(block) = self.blocks.last_mut() {
            block.open = true;
        }
    }

    // Open the next list item, starting the list when needed
    fn open_item(&mut self, numbered: bool) {
        let same = self
            .blocks
            .last()
            .is_some_and(|block| block.kind == Kind::Items { numbered });
        if same {
            if let Some(block) = self.blocks.last_mut()
                && block.open
            {
                block.open = false;
                self.out.push(Event::End(TagEnd::Item));
            }
        } else {
            if self
                .blocks
                .last()
                .is_some_and(|block| block.kind == Kind::Terms && !block.explicit)
                && let Some(block) = self.blocks.pop()
            {
                self.close_block(block);
            }
            self.push_block(Kind::Items { numbered }, false);
        }
        self.out.push(Event::Start(Tag::Item));
        if let Some(block) = self.blocks.last_mut() {
            block.open = true;
        }
    }

    // Handle `.Bl`, opening the kind of list its type asks for
    fn start_list(&mut self, args: &[String]) {
        self.flush();
        let has = |name: &str| args.iter().any(|arg| arg == name);
        let kind = if LIST_BULLETS.iter().any(|name| has(name)) {
            Kind::Items { numbered: false }
        } else if has(LIST_NUMBERED) {
            Kind::Items { numbered: true }
        } else if has(LIST_COLUMNS) {
            Kind::Columns
        } else {
            Kind::Terms
        };
        self.push_block(kind, true);
        if has(LIST_PLAIN)
            && let Some(block) = self.blocks.last_mut()
        {
            block.kind = Kind::Terms;
        }
    }

    // Handle `.It`, starting the next entry of the innermost `.Bl`
    fn list_item(&mut self, args: &[String]) {
        self.flush();
        let Some(block) = self.blocks.last().copied() else {
            self.macro_words(args);
            return;
        };
        match block.kind {
            Kind::Terms => {
                if block.open {
                    self.out.push(Event::End(TagEnd::DefinitionListDefinition));
                }
                if !args.is_empty() {
                    self.out.push(Event::Start(Tag::DefinitionListTitle));
                    self.macro_words(args);
                    self.out.append(&mut self.inline);
                    self.space = false;
                    self.out.push(Event::End(TagEnd::DefinitionListTitle));
                }
                self.out.push(Event::Start(Tag::DefinitionListDefinition));
            }
            Kind::Items { .. } => {
                if block.open {
                    self.out.push(Event::End(TagEnd::Item));
                }
                self.out.push(Event::Start(Tag::Item));
            }
            Kind::Columns => {
                self.macro_words(args);
                self.end_line();
            }
            Kind::Indent => self.macro_words(args),
        }
        if let Some(last) = self.blocks.last_mut() {
            last.open = !matches!(last.kind, Kind::Columns | Kind::Indent);
        }
    }

    // Start collecting verbatim lines until one of the end requests
    fn start_literal(&mut self, ends: &'static [&'static str]) {
        self.flush();
        self.literal = Some(Literal {
            text: String::new(),
            ends,
        });
    }

    // Emit a code block with no language, without blank lines at its ends
    fn code_block(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        let start = text
            .lines()
            .take_while(|line| line.trim().is_empty())
            .map(|line| line.len() + 1)
            .sum::<usize>();
        let text = format!("{}\n", text[start..].trim_end());
        self.out
            .push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(
                CowStr::from(""),
            ))));
        self.out.push(Event::Text(CowStr::from(text)));
        self.out.push(Event::End(TagEnd::CodeBlock));
    }

    // Open a link whose text is the lines up to `.UE` or `.ME`
    fn start_link(&mut self, url: Option<String>) {
        let Some(url) = url else {
            return;
        };
        self.gap();
        self.inline.push(Event::Start(link(url.clone())));
        self.link = Some((url, self.inline.len()));
    }

    // Close the open link, showing its address when it had no text
    fn end_link(&mut self, args: &[String]) {
        let Some((url, start)) = self.link.take() else {
            return;
        };
        if self.inline.len() == start {
            let shown = url.strip_prefix(MAILTO).unwrap_or(&url).to_owned();
            self.inline.push(Event::Text(CowStr::from(shown)));
        }
        self.inline.push(Event::End(TagEnd::Link));
        self.space = true;
        if let Some(trailer) = args.first() {
            self.space = false;
            self.run(plain(trailer), Font::Roman);
            self.space = true;
        }
    }
}

// True for mdoc punctuation tokens
fn is_delimiter(token: &str) -> bool {
    CLOSE_DELIMITERS.contains(&token) || OPEN_DELIMITERS.contains(&token)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Render a page to plain text for checks
    fn show(input: &str) -> String {
        crate::render::render_events(events(input), &crate::render::Options::new(60, false), 0).text
    }

    #[test]
    fn sniff_needs_a_page_opener_first() {
        assert!(sniff(".TH LS 1\n.SH NAME\n"));
        assert!(sniff(".\\\" comment\n.\\\"\n.TH LS 1\n"));
        assert!(sniff(".de Sp\n.if n .sp\nText\n..\n.TH X 1\n"));
        assert!(sniff(".Dd May 1, 2020\n.Dt LS 1\n"));
        assert!(!sniff("Hello\n.TH LS 1\n"));
        assert!(!sniff("# Title\n"));
        assert!(!sniff(".. note::\n"));
    }

    #[test]
    fn escapes_resolve() {
        assert_eq!(plain("a\\-b \\(em c\\(aqd\\e\\&"), "a-b — c'd\\");
        assert_eq!(plain("\\[u00E9]t\\[rq]\\*(lq"), "ét”“");
        assert_eq!(plain("\\s-1small\\s0 \\w'xx'w"), "small w");
        assert_eq!(plain("unknown \\@"), "unknown @");
    }

    #[test]
    fn fonts_split_into_pieces() {
        assert_eq!(
            pieces("a \\fBb\\fP c\\f(CWd"),
            vec![
                Piece::Text("a ".into()),
                Piece::Font(Font::Bold),
                Piece::Text("b".into()),
                Piece::PreviousFont,
                Piece::Text(" c".into()),
                Piece::Font(Font::Mono),
                Piece::Text("d".into()),
            ]
        );
    }

    #[test]
    fn arguments_honour_quotes() {
        assert_eq!(
            arguments(r#" a "b c" "d ""e""" f\ g"#),
            vec!["a", "b c", "d \"e\"", "f\\ g"]
        );
    }

    #[test]
    fn comments_and_continuations_are_handled() {
        let lines = logical_lines(".\\\" gone\nkeep \\\" tail\njoin\\\nme\n\\\" only\n");
        assert_eq!(lines, vec!["keep ", "joinme"]);
    }

    #[test]
    fn man_page_structure() {
        let page = ".TH LS 1\n.SH NAME\nls \\- list\n.SH OPTIONS\n.TP\n.B \\-a\nall \\fIentries\\fR\n.TP\n\\fB\\-l\\fR\nlong\n.PP\nAfter.\n";
        assert_eq!(
            show(page),
            "LS(1)\n═════\n\nNAME\n────\n\nls - list\n\nOPTIONS\n───────\n\n-a\n    all entries\n\n-l\n    long\n\nAfter.\n"
        );
    }

    #[test]
    fn alternating_fonts_join_without_spaces() {
        let events = events(".BR ls (1),\n");
        let text: String = events
            .iter()
            .filter_map(|event| match event {
                Event::Text(text) => Some(text.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "ls(1),");
        assert!(events.contains(&Event::Start(Tag::Strong)));
    }

    #[test]
    fn indent_bullets_and_no_fill() {
        let page = ".TH X 1\n.SH A\n.IP \\(bu 2\none\n.IP \\(bu\ntwo\n.PP\n.RS\nin\n.RE\n.nf\n  a  *b*\n.fi\nend\n.unknown x\nstill\n";
        assert_eq!(
            show(page),
            format!(
                "X(1)\n════\n\nA\n─\n\n• one\n\n• two\n\n    in\n\n╭{rule}\n│   a  *b*\n╰{rule}\n\nend still\n",
                rule = "─".repeat(59)
            )
        );
    }

    #[test]
    fn mdoc_page_structure() {
        let page = ".Dd May 1, 2020\n.Dt LS 1\n.Os\n.Sh NAME\n.Nm ls\n.Nd list directory contents\n.Sh SYNOPSIS\n.Nm\n.Op Fl ABC\n.Op Ar file ...\n.Sh DESCRIPTION\n.Bl -tag -width indent\n.It Fl a\nInclude dot files.\n.It Fl d Ar dir\nDirectories.\n.El\nSee\n.Xr stat 2 .\n.Ex -std\n";
        assert_eq!(
            show(page),
            "LS(1)\n═════\n\nNAME\n────\n\nls — list directory contents\n\nSYNOPSIS\n────────\n\nls [-ABC] [file ...]\n\nDESCRIPTION\n───────────\n\n-a\n    Include dot files.\n\n-d dir\n    Directories.\n\nSee stat(2). The ls utility exits 0 on success, and >0 if an\nerror occurs.\n"
        );
    }

    #[test]
    fn links_show_their_text_or_address() {
        let out = show(".TH X 1\n.SH A\n.UR https://e.org\nsite\n.UE .\n.MT a@b.c\n.ME\n");
        assert!(out.contains("site"), "{out}");
        assert!(out.contains("a@b.c"), "{out}");
    }
}
