// Author: machinageist
// Date: 2026-11-09
// Description: ANSI SGR styles and the color theme
// Notes: Only the 16 base colors are used so output follows the terminal's own palette

// Text attribute bits
pub const ATTR_BOLD: u8 = 1 << 0;
pub const ATTR_DIM: u8 = 1 << 1;
pub const ATTR_ITALIC: u8 = 1 << 2;
pub const ATTR_UNDERLINE: u8 = 1 << 3;
pub const ATTR_STRIKE: u8 = 1 << 4;

// Attribute bit to SGR parameter, in emission order
const ATTR_CODES: [(u8, &str); 5] = [
    (ATTR_BOLD, ";1"),
    (ATTR_DIM, ";2"),
    (ATTR_ITALIC, ";3"),
    (ATTR_UNDERLINE, ";4"),
    (ATTR_STRIKE, ";9"),
];

// Foreground color SGR parameters, zero keeps the terminal default
pub const FG_DEFAULT: u8 = 0;
pub const FG_RED: u8 = 31;
pub const FG_GREEN: u8 = 32;
pub const FG_YELLOW: u8 = 33;
pub const FG_BLUE: u8 = 34;
pub const FG_MAGENTA: u8 = 35;
pub const FG_CYAN: u8 = 36;
pub const FG_BRIGHT_GREEN: u8 = 92;
pub const FG_BRIGHT_BLUE: u8 = 94;
pub const FG_BRIGHT_MAGENTA: u8 = 95;
pub const FG_BRIGHT_CYAN: u8 = 96;

// Raw escape sequences
pub const SGR_RESET: &str = "\x1b[0m";
const SGR_OPEN: &str = "\x1b[0";
const SGR_CLOSE: char = 'm';

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub attrs: u8,
    pub fg: u8,
}

impl Style {
    pub const PLAIN: Style = Style::new(0, FG_DEFAULT);

    // Build a style from attribute bits and a foreground color
    pub const fn new(attrs: u8, fg: u8) -> Style {
        Style { attrs, fg }
    }

    // Layer extra attributes on top of this style
    pub const fn add(self, attrs: u8) -> Style {
        Style::new(self.attrs | attrs, self.fg)
    }

    // Swap the foreground color, keeping attributes
    pub const fn with_fg(self, fg: u8) -> Style {
        Style::new(self.attrs, fg)
    }

    // Write the escape sequence that puts the terminal into exactly this style
    pub fn write(self, out: &mut String) {
        out.push_str(SGR_OPEN);
        for (bit, code) in ATTR_CODES {
            if self.attrs & bit != 0 {
                out.push_str(code);
            }
        }
        if self.fg != FG_DEFAULT {
            out.push(';');
            out.push(char::from(b'0' + self.fg / 10));
            out.push(char::from(b'0' + self.fg % 10));
        }
        out.push(SGR_CLOSE);
    }
}

// Append text wrapped in a style, or bare when color is off
pub fn paint(out: &mut String, text: &str, style: Style, color: bool) {
    if !color || style == Style::PLAIN {
        out.push_str(text);
        return;
    }
    style.write(out);
    out.push_str(text);
    out.push_str(SGR_RESET);
}

// Append a glyph repeated `count` times inside one style
pub fn paint_repeat(out: &mut String, glyph: &str, count: usize, style: Style, color: bool) {
    let styled = color && style != Style::PLAIN;
    if styled {
        style.write(out);
    }
    for _ in 0..count {
        out.push_str(glyph);
    }
    if styled {
        out.push_str(SGR_RESET);
    }
}

// Theme: headings by level, H1 first
pub const STYLE_HEADINGS: [Style; 6] = [
    Style::new(ATTR_BOLD, FG_BRIGHT_MAGENTA),
    Style::new(ATTR_BOLD, FG_BRIGHT_BLUE),
    Style::new(ATTR_BOLD, FG_BRIGHT_CYAN),
    Style::new(ATTR_BOLD, FG_BRIGHT_GREEN),
    Style::new(ATTR_BOLD, FG_DEFAULT),
    Style::new(ATTR_BOLD | ATTR_DIM, FG_DEFAULT),
];

// Theme: inline spans
pub const FG_CODE: u8 = FG_YELLOW;
pub const FG_LINK: u8 = FG_BLUE;
pub const STYLE_URL: Style = Style::new(ATTR_DIM, FG_DEFAULT);
pub const STYLE_IMAGE: Style = Style::new(ATTR_ITALIC, FG_MAGENTA);
pub const STYLE_HTML: Style = Style::new(ATTR_DIM, FG_DEFAULT);
pub const STYLE_FOOTNOTE: Style = Style::new(0, FG_CYAN);

// Theme: block decoration
pub const STYLE_FRAME: Style = Style::new(ATTR_DIM, FG_DEFAULT);
pub const STYLE_CODE_LABEL: Style = Style::new(ATTR_ITALIC, FG_DEFAULT);
pub const STYLE_BULLET: Style = Style::new(ATTR_BOLD, FG_CYAN);
pub const STYLE_TASK_DONE: Style = Style::new(ATTR_BOLD, FG_GREEN);
pub const STYLE_TASK_TODO: Style = Style::new(ATTR_DIM, FG_DEFAULT);
pub const STYLE_TABLE_HEAD: Style = Style::new(ATTR_BOLD, FG_DEFAULT);
pub const STYLE_FRONT_MATTER: Style = Style::new(ATTR_DIM, FG_DEFAULT);

// Theme: GitHub alert callouts
pub const STYLE_ALERT_NOTE: Style = Style::new(ATTR_BOLD, FG_BLUE);
pub const STYLE_ALERT_TIP: Style = Style::new(ATTR_BOLD, FG_GREEN);
pub const STYLE_ALERT_IMPORTANT: Style = Style::new(ATTR_BOLD, FG_MAGENTA);
pub const STYLE_ALERT_WARNING: Style = Style::new(ATTR_BOLD, FG_YELLOW);
pub const STYLE_ALERT_CAUTION: Style = Style::new(ATTR_BOLD, FG_RED);

// Theme: data formats (JSON, YAML, TOML)
pub const STYLE_DATA_KEY: Style = Style::new(ATTR_BOLD, FG_BLUE);
pub const STYLE_DATA_STRING: Style = Style::new(0, FG_GREEN);
pub const STYLE_DATA_NUMBER: Style = Style::new(0, FG_MAGENTA);
pub const STYLE_DATA_LITERAL: Style = Style::new(0, FG_YELLOW);
pub const STYLE_DATA_COMMENT: Style = Style::new(ATTR_DIM | ATTR_ITALIC, FG_DEFAULT);
pub const STYLE_DATA_PUNCT: Style = Style::new(ATTR_DIM, FG_DEFAULT);
pub const STYLE_DATA_SECTION: Style = Style::new(ATTR_BOLD, FG_CYAN);
pub const STYLE_DATA_META: Style = Style::new(0, FG_CYAN);
