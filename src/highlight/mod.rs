// Author: machinageist
// Date: 2026-11-10
// Description: Syntax highlighting for source files and fenced code blocks, from built-in lexers
// Notes: One table-driven lexer covers most languages: each is a row of comment markers,
// quotes, keywords and a few flags. Data formats reuse their own highlighters, and diffs
// are colored by line. Highlighting only changes styles, never the text

mod languages;

use crate::data::{self, Span, StyledLine};
use crate::style::{
    STYLE_CODE_COMMENT, STYLE_CODE_CONSTANT, STYLE_CODE_DELETED, STYLE_CODE_FUNCTION,
    STYLE_CODE_HEADER, STYLE_CODE_INSERTED, STYLE_CODE_KEYWORD, STYLE_CODE_META, STYLE_CODE_NUMBER,
    STYLE_CODE_STRING, STYLE_CODE_TYPE, STYLE_CODE_VARIABLE, STYLE_DATA_KEY, Style,
};
use languages::LANGUAGES;

// How a language is highlighted
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Generic,
    Diff,
    Ini,
    Json,
    Yaml,
    Toml,
    Xml,
}

// A string delimiter pair
#[derive(Clone, Copy, Debug)]
pub(crate) struct Quote {
    pub open: &'static str,
    pub close: &'static str,
    // Backslash escapes the next character inside
    pub escapes: bool,
    // May run across lines
    pub multiline: bool,
}

// Everything the lexer needs to know about one language
#[derive(Clone, Copy, Debug)]
pub(crate) struct Language {
    // Info-string names and file extensions, lowercase
    pub names: &'static [&'static str],
    pub kind: Kind,
    pub line_comments: &'static [&'static str],
    pub block_comments: &'static [(&'static str, &'static str)],
    pub quotes: &'static [Quote],
    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub constants: &'static [&'static str],
    // Keywords match in any case, as in SQL
    pub ignore_case: bool,
    // `#` at the start of a line begins a preprocessor line, as in C
    pub preprocessor: bool,
    // `@name` is a decorator or annotation
    pub decorators: bool,
    // `#[...]` is an attribute, as in Rust
    pub attributes: bool,
    // `'a` can be a lifetime rather than a character
    pub lifetimes: bool,
    // `$name` is a variable, as in shells, PHP and Perl
    pub variables: bool,
    // Capitalised identifiers are types
    pub capital_types: bool,
    // Characters allowed inside identifiers besides letters, digits and `_`
    pub ident_extra: &'static str,
}

// The row every language starts from
pub(crate) const BASE: Language = Language {
    names: &[],
    kind: Kind::Generic,
    line_comments: &[],
    block_comments: &[],
    quotes: &[],
    keywords: &[],
    types: &[],
    constants: &[],
    ignore_case: false,
    preprocessor: false,
    decorators: false,
    attributes: false,
    lifetimes: false,
    variables: false,
    capital_types: false,
    ident_extra: "",
};

// Shebang interpreters and the language they mean
const SHEBANG: &str = "#!";
const ENV_COMMAND: &str = "env";
const ENV_SPLIT_FLAG: &str = "-S";
const INTERPRETERS: [(&str, &str); 12] = [
    ("sh", "sh"),
    ("bash", "sh"),
    ("zsh", "sh"),
    ("ksh", "sh"),
    ("dash", "sh"),
    ("fish", "fish"),
    ("python", "python"),
    ("ruby", "ruby"),
    ("perl", "perl"),
    ("node", "javascript"),
    ("php", "php"),
    ("lua", "lua"),
];

// Syntax characters the lexer looks for
const ESCAPE: char = '\\';
const DECORATOR: char = '@';
const PREPROCESSOR: char = '#';
const ATTRIBUTE_OPEN: &str = "#[";
const INNER_ATTRIBUTE_OPEN: &str = "#![";
const ATTRIBUTE_CLOSE: char = ']';
const VARIABLE: char = '$';
const BRACE_OPEN: char = '{';
const BRACE_CLOSE: char = '}';
const CALL_OPEN: char = '(';
const LIFETIME: char = '\'';
const SHELL_COMMENT: &str = "#";
const NUMBER_EXPONENTS: [char; 2] = ['e', 'E'];
const NUMBER_SIGNS: [char; 2] = ['+', '-'];
const HEX_PREFIXES: [&str; 2] = ["0x", "0X"];
const DECIMAL_POINT: char = '.';
const SPECIAL_VARIABLES: &str = "@*#?$!-0123456789";
const MACRO_CALLS: [&str; 3] = ["!(", "![", "!{"];

// Diff line prefixes
const DIFF_HEADERS: [&str; 5] = ["+++", "---", "diff ", "index ", "Only in "];
const DIFF_HUNK: &str = "@@";
const DIFF_INSERTED: char = '+';
const DIFF_DELETED: char = '-';

// INI syntax
const INI_SECTION: char = '[';
const INI_COMMENTS: [char; 2] = [';', '#'];
const INI_SEPARATORS: [char; 2] = ['=', ':'];

// Find a language by an info string or file extension, ignoring case
pub fn language(name: &str) -> Option<&'static Language> {
    let name = name.trim().to_ascii_lowercase();
    let name = name.trim_start_matches('.');
    LANGUAGES
        .iter()
        .find(|language| language.names.contains(&name))
}

// Find a language from a script's `#!` line
pub fn language_from_shebang(input: &str) -> Option<&'static Language> {
    let first = input.lines().next()?.strip_prefix(SHEBANG)?;
    let mut words = first.split_whitespace();
    let mut program = words.next()?.rsplit('/').next()?;
    while program == ENV_COMMAND || program == ENV_SPLIT_FLAG {
        program = words.next()?;
    }
    let program = program.trim_end_matches(|ch: char| ch.is_ascii_digit() || ch == '.');
    INTERPRETERS
        .iter()
        .find(|(name, _)| *name == program)
        .and_then(|(_, language_name)| language(language_name))
}

// Highlight source text into styled lines, one per input line
pub fn highlight(input: &str, language: &Language) -> Vec<StyledLine> {
    match language.kind {
        Kind::Generic => Lexer::new(language).lines(input),
        Kind::Diff => input.lines().map(diff_line).collect(),
        Kind::Ini => input.lines().map(ini_line).collect(),
        Kind::Json => data::json::highlight_verbatim(input),
        Kind::Yaml => data::yaml::highlight(input),
        Kind::Toml => data::toml::highlight(input),
        Kind::Xml => data::xml::highlight_verbatim(input),
    }
}

// Color one line of a unified diff
fn diff_line(line: &str) -> StyledLine {
    let style = if DIFF_HEADERS.iter().any(|header| line.starts_with(header)) {
        STYLE_CODE_HEADER
    } else if line.starts_with(DIFF_HUNK) {
        STYLE_CODE_META
    } else if line.starts_with(DIFF_INSERTED) {
        STYLE_CODE_INSERTED
    } else if line.starts_with(DIFF_DELETED) {
        STYLE_CODE_DELETED
    } else {
        Style::PLAIN
    };
    vec![(style, line.to_owned())]
}

// Color one line of an INI-style config file
fn ini_line(line: &str) -> StyledLine {
    let trimmed = line.trim_start();
    if trimmed.starts_with(INI_COMMENTS) {
        return vec![(STYLE_CODE_COMMENT, line.to_owned())];
    }
    if trimmed.starts_with(INI_SECTION) {
        return vec![(STYLE_CODE_HEADER, line.to_owned())];
    }
    match line.find(INI_SEPARATORS) {
        Some(at) => {
            let mut spans = Vec::new();
            push_span(&mut spans, STYLE_DATA_KEY, &line[..at]);
            push_span(&mut spans, Style::PLAIN, &line[at..at + 1]);
            push_span(&mut spans, STYLE_CODE_STRING, &line[at + 1..]);
            spans
        }
        None => vec![(Style::PLAIN, line.to_owned())],
    }
}

// Append text in a style, merging with the last span when the style matches
fn push_span(spans: &mut Vec<Span>, style: Style, text: &str) {
    if text.is_empty() {
        return;
    }
    match spans.last_mut() {
        Some((last, existing)) if *last == style => existing.push_str(text),
        _ => spans.push((style, text.to_owned())),
    }
}

// What is still open at the end of a line
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Nothing,
    // A block comment, by index into the language's pairs, and its nesting depth
    Comment(usize, usize),
    // A multi-line string, by index into the language's quotes
    String(usize),
}

struct Lexer<'l> {
    language: &'l Language,
    open: Open,
}

impl<'l> Lexer<'l> {
    // Start a lexer for a language
    fn new(language: &'l Language) -> Lexer<'l> {
        Lexer {
            language,
            open: Open::Nothing,
        }
    }

    // Highlight every line, carrying open comments and strings across lines
    fn lines(&mut self, input: &str) -> Vec<StyledLine> {
        input.lines().map(|line| self.line(line)).collect()
    }

    // Highlight one line
    fn line(&mut self, line: &str) -> StyledLine {
        let mut spans = Vec::new();
        let mut at = 0;
        if self.language.preprocessor
            && self.open == Open::Nothing
            && line.trim_start().starts_with(PREPROCESSOR)
        {
            let end = self.preprocessor_end(line);
            push_span(&mut spans, STYLE_CODE_META, &line[..end]);
            at = end;
        }
        while at < line.len() {
            at = match self.open {
                Open::Comment(index, depth) => {
                    self.continue_comment(line, at, index, depth, &mut spans)
                }
                Open::String(index) => self.continue_string(line, at, index, &mut spans),
                Open::Nothing => self.token(line, at, &mut spans),
            };
        }
        // A single-line string never stays open past its line
        if let Open::String(index) = self.open
            && !self.language.quotes[index].multiline
        {
            self.open = Open::Nothing;
        }
        spans
    }

    // Where a preprocessor line's directive ends: before a trailing comment
    fn preprocessor_end(&self, line: &str) -> usize {
        let comment_starts = self
            .language
            .line_comments
            .iter()
            .chain(self.language.block_comments.iter().map(|(open, _)| open))
            .filter_map(|marker| line.find(marker));
        comment_starts.min().unwrap_or(line.len())
    }

    // Lex one token starting at `at`, returning where the next one starts
    fn token(&mut self, line: &str, at: usize, spans: &mut Vec<Span>) -> usize {
        let rest = &line[at..];
        let language = self.language;
        let previous = line[..at].chars().next_back();

        // Line comments run to the end of the line
        // A shell `#` needs a blank before it, and a word marker such as `rem` a word break
        let comment_allowed = |marker: &str| {
            let shell_hash = language.variables && marker == SHELL_COMMENT;
            let wordy = marker.starts_with(char::is_alphabetic);
            (!shell_hash || previous.is_none_or(char::is_whitespace))
                && (!wordy || previous.is_none_or(|ch| !is_ident_char(ch, "")))
        };
        if language
            .line_comments
            .iter()
            .any(|marker| comment_allowed(marker) && rest.starts_with(marker))
        {
            push_span(spans, STYLE_CODE_COMMENT, rest);
            return line.len();
        }
        if let Some(index) = language
            .block_comments
            .iter()
            .position(|(open, _)| rest.starts_with(open))
        {
            let open_len = language.block_comments[index].0.len();
            push_span(spans, STYLE_CODE_COMMENT, &rest[..open_len]);
            return self.continue_comment(line, at + open_len, index, 1, spans);
        }
        if language.attributes
            && (rest.starts_with(ATTRIBUTE_OPEN) || rest.starts_with(INNER_ATTRIBUTE_OPEN))
        {
            let end = rest
                .find(ATTRIBUTE_CLOSE)
                .map_or(rest.len(), |close| close + 1);
            push_span(spans, STYLE_CODE_META, &rest[..end]);
            return at + end;
        }
        let first = rest.chars().next().unwrap_or_default();
        if language.lifetimes && first == LIFETIME && is_lifetime(rest) {
            let end = 1 + ident_len(&rest[1..], language.ident_extra);
            push_span(spans, STYLE_CODE_TYPE, &rest[..end]);
            return at + end;
        }
        if let Some(index) = language
            .quotes
            .iter()
            .position(|quote| rest.starts_with(quote.open))
        {
            let open_len = language.quotes[index].open.len();
            push_span(spans, STYLE_CODE_STRING, &rest[..open_len]);
            return self.continue_string(line, at + open_len, index, spans);
        }
        if language.decorators && first == DECORATOR {
            let end = 1 + ident_len(&rest[1..], ".");
            if end > 1 {
                push_span(spans, STYLE_CODE_META, &rest[..end]);
                return at + end;
            }
        }
        if language.variables && first == VARIABLE {
            let end = variable_len(rest);
            push_span(spans, STYLE_CODE_VARIABLE, &rest[..end]);
            return at + end;
        }
        let after_word = previous.is_some_and(|ch| is_ident_char(ch, language.ident_extra));
        if !after_word && starts_number(rest) {
            let end = number_len(rest);
            push_span(spans, STYLE_CODE_NUMBER, &rest[..end]);
            return at + end;
        }
        if !after_word && is_ident_start(first) {
            let end = ident_len(rest, language.ident_extra);
            let word = &rest[..end];
            let style = self.word_style(word, &rest[end..]);
            push_span(spans, style, word);
            return at + end;
        }
        let end = first.len_utf8().max(1);
        push_span(spans, Style::PLAIN, &rest[..end]);
        at + end
    }

    // Style an identifier from the language's word lists and its shape
    fn word_style(&self, word: &str, after: &str) -> Style {
        let language = self.language;
        let listed = |words: &[&str]| {
            if language.ignore_case {
                words.iter().any(|known| known.eq_ignore_ascii_case(word))
            } else {
                words.contains(&word)
            }
        };
        if listed(language.keywords) {
            STYLE_CODE_KEYWORD
        } else if listed(language.constants) {
            STYLE_CODE_CONSTANT
        } else if listed(language.types) {
            STYLE_CODE_TYPE
        } else if after.trim_start().starts_with(CALL_OPEN) || self.is_macro_call(after) {
            STYLE_CODE_FUNCTION
        } else if language.capital_types && word.starts_with(|ch: char| ch.is_ascii_uppercase()) {
            if word.len() > 1 && word.chars().all(|ch| !ch.is_ascii_lowercase()) {
                STYLE_CODE_CONSTANT
            } else {
                STYLE_CODE_TYPE
            }
        } else {
            Style::PLAIN
        }
    }

    // A Rust-style macro call such as `println!(` or `vec![`
    fn is_macro_call(&self, after: &str) -> bool {
        self.language.attributes && MACRO_CALLS.iter().any(|opener| after.starts_with(opener))
    }

    // Continue a block comment, honouring nesting where the language allows it
    fn continue_comment(
        &mut self,
        line: &str,
        mut at: usize,
        index: usize,
        mut depth: usize,
        spans: &mut Vec<Span>,
    ) -> usize {
        let (open, close) = self.language.block_comments[index];
        let nests = open != close && NESTING_COMMENTS.contains(&open);
        let start = at;
        while at < line.len() {
            let rest = &line[at..];
            if rest.starts_with(close) {
                at += close.len();
                depth -= 1;
                if depth == 0 {
                    push_span(spans, STYLE_CODE_COMMENT, &line[start..at]);
                    self.open = Open::Nothing;
                    return at;
                }
            } else if nests && rest.starts_with(open) {
                at += open.len();
                depth += 1;
            } else {
                at += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
        push_span(spans, STYLE_CODE_COMMENT, &line[start..]);
        self.open = Open::Comment(index, depth);
        line.len()
    }

    // Continue a string to its closing quote, or to the end of the line
    fn continue_string(
        &mut self,
        line: &str,
        mut at: usize,
        index: usize,
        spans: &mut Vec<Span>,
    ) -> usize {
        let quote = self.language.quotes[index];
        let start = at;
        while at < line.len() {
            let rest = &line[at..];
            if quote.escapes && rest.starts_with(ESCAPE) {
                at += 1;
                at += line[at..].chars().next().map_or(0, char::len_utf8);
            } else if rest.starts_with(quote.close) {
                at += quote.close.len();
                push_span(spans, STYLE_CODE_STRING, &line[start..at]);
                self.open = Open::Nothing;
                return at;
            } else {
                at += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
        push_span(spans, STYLE_CODE_STRING, &line[start..]);
        self.open = Open::String(index);
        line.len()
    }
}

// Block comment openers that nest, as in Rust, Swift, Haskell and OCaml
const NESTING_COMMENTS: [&str; 2] = ["/*", "{-"];

// True when `'` starts a lifetime or label, not a character literal
fn is_lifetime(rest: &str) -> bool {
    let mut chars = rest.chars().skip(1);
    match (chars.next(), chars.next()) {
        (Some(first), second) => is_ident_start(first) && second != Some(LIFETIME),
        _ => false,
    }
}

// True for a character that can start an identifier
fn is_ident_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_'
}

// True for a character that can continue an identifier
fn is_ident_char(ch: char, extra: &str) -> bool {
    ch.is_alphanumeric() || ch == '_' || extra.contains(ch)
}

// Byte length of the identifier at the start of `text`
fn ident_len(text: &str, extra: &str) -> usize {
    text.char_indices()
        .find(|(_, ch)| !is_ident_char(*ch, extra))
        .map_or(text.len(), |(at, _)| at)
}

// True when a number starts here: a digit, or a point followed by one
fn starts_number(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_digit() => true,
        Some(DECIMAL_POINT) => chars.next().is_some_and(|ch| ch.is_ascii_digit()),
        _ => false,
    }
}

// Byte length of a number literal, with any suffix such as `u32` or `px`
fn number_len(rest: &str) -> usize {
    let hex = HEX_PREFIXES.iter().any(|prefix| rest.starts_with(prefix));
    let mut end = 0;
    let mut previous = '\0';
    for (at, ch) in rest.char_indices() {
        let exponent_sign =
            !hex && NUMBER_SIGNS.contains(&ch) && NUMBER_EXPONENTS.contains(&previous);
        let point =
            ch == DECIMAL_POINT && rest[at + 1..].starts_with(|next: char| next.is_ascii_digit());
        if !(ch.is_ascii_alphanumeric() || ch == '_' || exponent_sign || point) {
            break;
        }
        end = at + ch.len_utf8();
        previous = ch;
    }
    end
}

// Byte length of a `$name`, `${...}` or `$?` variable
fn variable_len(rest: &str) -> usize {
    let after = &rest[1..];
    if after.starts_with(BRACE_OPEN) {
        return after
            .find(BRACE_CLOSE)
            .map_or(rest.len(), |close| close + 2);
    }
    let name = ident_len(after, "");
    if name > 0 {
        return 1 + name;
    }
    match after.chars().next() {
        Some(ch) if SPECIAL_VARIABLES.contains(ch) => 1 + ch.len_utf8(),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pair each styled span's text with a short name for its style; plain text is left out
    fn tokens(code: &str, name: &str) -> Vec<(String, &'static str)> {
        let language = language(name).expect("known language");
        highlight(code, language)
            .into_iter()
            .flatten()
            .filter(|(style, text)| *style != Style::PLAIN && !text.trim().is_empty())
            .map(|(style, text)| (text.trim().to_owned(), style_name(style)))
            .collect()
    }

    // A readable name for a style, for assertions
    fn style_name(style: Style) -> &'static str {
        [
            (STYLE_CODE_KEYWORD, "keyword"),
            (STYLE_CODE_TYPE, "type"),
            (STYLE_CODE_FUNCTION, "function"),
            (STYLE_CODE_STRING, "string"),
            (STYLE_CODE_NUMBER, "number"),
            (STYLE_CODE_CONSTANT, "constant"),
            (STYLE_CODE_COMMENT, "comment"),
            (STYLE_CODE_META, "meta"),
            (STYLE_CODE_VARIABLE, "variable"),
            (STYLE_CODE_INSERTED, "inserted"),
            (STYLE_CODE_DELETED, "deleted"),
            (STYLE_CODE_HEADER, "header"),
            (STYLE_DATA_KEY, "key"),
        ]
        .iter()
        .find(|(known, _)| *known == style)
        .map_or("plain", |(_, name)| name)
    }

    // Shorthand for expected tokens
    fn expect(pairs: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
        pairs
            .iter()
            .map(|(text, name)| ((*text).to_owned(), *name))
            .collect()
    }

    #[test]
    fn every_language_has_unique_names() {
        let mut seen = std::collections::HashSet::new();
        for language in LANGUAGES {
            assert!(!language.names.is_empty());
            for name in language.names {
                assert!(seen.insert(*name), "{name} listed twice");
                assert_eq!(*name, name.to_ascii_lowercase());
            }
        }
    }

    #[test]
    fn every_source_extension_has_a_language() {
        for extension in crate::format::code_extensions() {
            assert!(language(extension).is_some(), "no lexer for .{extension}");
        }
    }

    #[test]
    fn c_preprocessor_strings_and_comments() {
        assert_eq!(
            tokens(
                "#include <stdio.h> // io\nint main(void) { return 0x1F; } /* a\nb */",
                "c"
            ),
            expect(&[
                ("#include <stdio.h>", "meta"),
                ("// io", "comment"),
                ("int", "type"),
                ("main", "function"),
                ("void", "type"),
                ("return", "keyword"),
                ("0x1F", "number"),
                ("/* a", "comment"),
                ("b */", "comment"),
            ])
        );
    }

    #[test]
    fn rust_lifetimes_attributes_and_types() {
        assert_eq!(
            tokens(
                "#[derive(Debug)]\nfn f<'a>(x: &'a str) -> Option<char> { Some('x') }",
                "rust"
            ),
            expect(&[
                ("#[derive(Debug)]", "meta"),
                ("fn", "keyword"),
                ("'a", "type"),
                ("'a", "type"),
                ("str", "type"),
                ("Option", "type"),
                ("char", "type"),
                ("Some", "constant"),
                ("'x'", "string"),
            ])
        );
    }

    #[test]
    fn python_triple_quotes_span_lines() {
        assert_eq!(
            tokens(
                "@cache\ndef f():\n    \"\"\"doc\n    more\"\"\"\n    return None",
                "py"
            ),
            expect(&[
                ("@cache", "meta"),
                ("def", "keyword"),
                ("f", "function"),
                ("\"\"\"doc", "string"),
                ("more\"\"\"", "string"),
                ("return", "keyword"),
                ("None", "constant"),
            ])
        );
    }

    #[test]
    fn shell_variables_and_comments() {
        assert_eq!(
            tokens("if [ \"$1\" = x#y ]; then echo ${HOME} # done\nfi", "bash"),
            expect(&[
                ("if", "keyword"),
                ("\"$1\"", "string"),
                ("then", "keyword"),
                ("${HOME}", "variable"),
                ("# done", "comment"),
                ("fi", "keyword"),
            ])
        );
    }

    #[test]
    fn sql_ignores_case() {
        assert_eq!(
            tokens("select 1 FROM t -- c", "sql"),
            expect(&[
                ("select", "keyword"),
                ("1", "number"),
                ("FROM", "keyword"),
                ("-- c", "comment"),
            ])
        );
    }

    #[test]
    fn diff_lines_are_colored() {
        assert_eq!(
            tokens("--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n same", "diff"),
            expect(&[
                ("--- a", "header"),
                ("+++ b", "header"),
                ("@@ -1 +1 @@", "meta"),
                ("-old", "deleted"),
                ("+new", "inserted"),
            ])
        );
    }

    #[test]
    fn numbers_need_a_word_boundary() {
        assert_eq!(
            tokens("x1 = 1.5e-3 + .5", "js"),
            expect(&[("1.5e-3", "number"), (".5", "number"),])
        );
    }

    #[test]
    fn nested_comments_close_at_the_right_depth() {
        assert_eq!(
            tokens("/* a /* b */ c */ fn", "rust"),
            expect(&[("/* a /* b */ c */", "comment"), ("fn", "keyword")])
        );
    }

    #[test]
    fn shebangs_name_languages() {
        let name = |input: &str| language_from_shebang(input).map(|language| language.names[0]);
        assert_eq!(name("#!/bin/bash\n"), Some("sh"));
        assert_eq!(name("#!/usr/bin/env python3\n"), Some("python"));
        assert_eq!(name("#!/usr/bin/env -S node --flag\n"), Some("javascript"));
        assert_eq!(name("#!/opt/thing\n"), None);
        assert_eq!(name("no shebang"), None);
    }

    #[test]
    fn highlighting_keeps_every_character() {
        let code = "fn main() { let s = \"a\\\"b\"; /* x */ }\n\tint y = 'c'; // é\n";
        for language in LANGUAGES {
            let text: Vec<String> = highlight(code, language)
                .into_iter()
                .map(|line| line.into_iter().map(|(_, text)| text).collect())
                .collect();
            let expected: Vec<&str> = code.lines().collect();
            assert_eq!(text, expected, "{:?}", language.names);
        }
    }
}
