// Author: machinageist
// Description: Fragment positions at physical wrap boundaries through public rendering

use mu::{Format, Options, present_outlined};
use unicode_width::UnicodeWidthStr;

mod common;

#[test]
fn an_inline_fragment_at_a_full_line_follows_its_next_visible_character() {
    let rendered = present_outlined(
        "<p><a href='#edge'>Go</a></p><p>abcdefghijklmnopqrst<span id='edge'>X</span></p>",
        Format::Epub,
        &Options::new(20, false),
    );
    let anchor = rendered
        .anchors
        .iter()
        .find(|anchor| anchor.id == "edge")
        .unwrap();
    assert_eq!(rendered.text.lines().nth(anchor.line), Some("X"));
}

#[test]
fn navigation_columns_include_gutters_alignment_and_wide_characters() {
    let documents = [
        "<p>先先 prefix <a href='#edge'>LINK label</a></p><p>abcdefghijklmnopqrst<span id='edge'>TARGET</span></p>",
        "<blockquote><ul><li>先先 <a href='#edge'>LINK label</a></li></ul></blockquote><blockquote><p>先先<span id='edge'>TARGET</span></p></blockquote>",
        "<table><tr><th>A</th><th align='right'>Wide header</th></tr><tr><td>padding padding padding</td><td>先先<a href='#edge'>LINK label</a></td></tr><tr><td>more</td><td>先先<span id='edge'>TARGET</span></td></tr></table>",
        "<p><a href='#edge'>LINK</a></p><pre class='language-rust'>先先\tpadding<span id='edge'>TARGET</span>\n</pre>",
    ];
    for source in documents {
        for width in [20, 30, 80] {
            for color in [false, true] {
                let rendered = present_outlined(source, Format::Epub, &Options::new(width, color));
                let link = rendered.links.first().expect("internal source link");
                let anchor = rendered
                    .anchors
                    .iter()
                    .find(|anchor| anchor.id == link.target)
                    .expect("internal target");
                for (line, column, marker) in [
                    (link.line, link.column, 'L'),
                    (anchor.line, anchor.column, 'T'),
                ] {
                    let visible = common::strip_sgr(rendered.text.lines().nth(line).unwrap());
                    let at = visible.find(marker).unwrap_or_else(|| {
                        panic!("missing {marker} at width {width}, color {color}: {visible:?}")
                    });
                    assert_eq!(
                        column,
                        visible[..at].width(),
                        "width {width}, color {color}: {visible:?}"
                    );
                }
            }
        }
    }
}
