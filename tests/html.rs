// Author: machinageist
// Date: 2026-11-09
// Description: HTML presenter tests with exact expected output plus layout invariants
// Notes: Exact cases run with color off; the fixture is a realistic page with every mapped construct

mod common;

use common::{assert_invariants, colored, plain, strip_sgr};
use mu::{Format, Options, present_outlined};

const FIXTURE: &str = include_str!("fixtures/sample.html");
const KITCHEN_SINK: &str = include_str!("fixtures/kitchen-sink.md");
const README: &str = include_str!("../README.md");
const WIDTH: usize = 40;

// Render an HTML snippet with color off at the test width
fn html(input: &str) -> String {
    plain(input, Format::Html, WIDTH)
}

#[test]
fn fixture_passes_invariants() {
    assert_invariants(FIXTURE, Format::Html);
}

#[test]
fn fixture_hides_scripts_styles_comments_and_controls() {
    let out = html(FIXTURE);
    assert!(!out.contains("SHOULD NOT SHOW"), "{out}");
    assert!(out.contains("Rendering HTML in the Terminal"));
    assert!(out.contains("© 2026 Field Notes."));
}

#[test]
fn sniff_recognises_html_documents() {
    assert_eq!(mu::sniff(FIXTURE), Format::Html);
    assert_eq!(mu::sniff("\u{feff}  <HTML><body>x"), Format::Html);
    assert_eq!(mu::sniff("<body>x</body>"), Format::Html);
    assert_eq!(mu::sniff("<!-- c -->\n<!doctype HTML>"), Format::Html);
    assert_eq!(
        mu::sniff("<?xml version=\"1.0\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\">"),
        Format::Html
    );
}

#[test]
fn sniff_leaves_markdown_alone() {
    assert_ne!(mu::sniff(KITCHEN_SINK), Format::Html);
    assert_ne!(mu::sniff(README), Format::Html);
    assert_ne!(
        mu::sniff("Some <b>bold</b> text\n\n<div>x</div>"),
        Format::Html
    );
    assert_ne!(mu::sniff("<header>x</header>"), Format::Html);
    assert_ne!(
        mu::sniff("<?xml version=\"1.0\"?><rss></rss>"),
        Format::Html
    );
}

#[test]
fn headings_map_to_heading_styles() {
    assert_eq!(
        html("<h1>Top</h1><h2>Sub</h2><h3>Low</h3>"),
        "Top\n═══\n\nSub\n───\n\nLow\n"
    );
}

#[test]
fn title_becomes_heading_only_without_h1() {
    let doc = "<html><head><title>Page &amp; Title</title></head><body><p>Body</p></body></html>";
    assert_eq!(html(doc), "Page & Title\n════════════\n\nBody\n");
    let with_h1 = "<head><title>Ignored</title></head><body><h1>Real</h1></body>";
    assert_eq!(html(with_h1), "Real\n════\n");
}

#[test]
fn paragraphs_collapse_whitespace() {
    let out = html("<p>  one\n   two\tthree  </p>\n\n<div>four</div><section>five</section>");
    assert_eq!(out, "one two three\n\nfour\n\nfive\n");
}

#[test]
fn br_breaks_lines_and_hr_draws_a_rule() {
    let out = plain("<p>a<br>b<br/></p><hr><p>c</p>", Format::Html, 20);
    assert_eq!(out, "a\nb\n\n────────────────────\n\nc\n");
}

#[test]
fn inline_styles_and_code() {
    let out = html(
        "<p><strong>s</strong> <b>b</b> <em>e</em> <i>i</i> <del>d</del> <s>x</s> <code>c</code> <kbd>k</kbd></p>",
    );
    assert_eq!(out, "s b e i d x c k\n");
    let color = colored("<p><b>bold</b></p>", Format::Html, WIDTH);
    assert!(color.contains("\x1b["), "{color:?}");
}

#[test]
fn links_show_their_url_and_images_their_alt() {
    let out = html(
        "<p>See <a href=\"https://x.io\">docs</a>. <img src=\"a.png\" alt=\"Logo\"> <img src=\"b.png\"></p>",
    );
    assert_eq!(out, "See docs (https://x.io). [image: Logo]\n[image]\n");
    assert_eq!(html("<p><a name=\"top\">anchor</a></p>"), "anchor\n");
}

#[test]
fn tight_and_nested_lists() {
    let out = html("<ul><li>one</li><li>two<ul><li>inner</li></ul></li></ul>");
    assert_eq!(out, "• one\n• two\n  ◦ inner\n");
}

#[test]
fn ordered_list_honours_start() {
    assert_eq!(
        html("<ol start=\"7\"><li>a</li><li>b</li></ol>"),
        "7. a\n8. b\n"
    );
}

#[test]
fn list_items_with_paragraphs_are_loose() {
    let out = html("<ol><li><p>a</p><p>b</p></li><li><p>c</p></li></ol>");
    assert_eq!(out, "1. a\n\n   b\n\n2. c\n");
}

#[test]
fn blockquote_draws_a_bar() {
    assert_eq!(
        html("<blockquote><p>q1</p><p>q2</p></blockquote>"),
        "│ q1\n│\n│ q2\n"
    );
}

#[test]
fn pre_keeps_whitespace_and_language() {
    let out = plain(
        "<pre><code class=\"language-sh\">a  b\n    c &amp;&amp; d\n</code></pre>",
        Format::Html,
        20,
    );
    assert_eq!(
        out,
        "╭─ sh ──────────────\n│ a  b\n│     c && d\n╰───────────────────\n"
    );
}

#[test]
fn table_maps_head_and_body_rows() {
    let out = html(
        "<table><thead><tr><th>A</th><th>B</th></tr></thead>\
         <tbody><tr><td>1</td><td>2</td></tr><tr><td>3</td></tr></tbody></table>",
    );
    assert_eq!(
        out,
        "┌───┬───┐\n│ A │ B │\n├───┼───┤\n│ 1 │ 2 │\n│ 3 │   │\n└───┴───┘\n"
    );
}

#[test]
fn definition_lists_show_bold_terms_and_indented_definitions() {
    let out = html("<dl><dt>Term</dt><dd>Meaning</dd><dd>Other</dd></dl>");
    assert_eq!(out, "Term\n\n• Meaning\n• Other\n");
}

#[test]
fn scripts_styles_and_comments_are_hidden() {
    let out = html(
        "<style>p{color:red}</style><script>var a = '<p>x</p>';</script>\
         <!-- note --><p>shown</p><noscript>no</noscript><template>t</template>",
    );
    assert_eq!(out, "shown\n");
}

#[test]
fn entities_decode_and_unknown_ones_pass_through() {
    let out = html(
        "<p>&lt;a&gt; &amp; &quot;q&quot; &apos;s&apos; &mdash; &hellip; &#65;&#x263A; &nope; AT&T</p>",
    );
    assert_eq!(out, "<a> & \"q\" 's' — … A☺ &nope; AT&T\n");
}

#[test]
fn unclosed_and_misnested_tags_keep_their_text() {
    let out = html("<p>one<p>two <b>bold <i>both</b> tail</i><ul><li>a<li>b</ul><p>end <em>open");
    assert_eq!(out, "one\n\ntwo bold both tail\n\n• a\n• b\n\nend open\n");
    assert_eq!(html("text < 3 and <unterminated"), "text < 3 and\n");
    assert_eq!(
        html("<table><tr><td>a<td>b<tr><td>c</table>"),
        "┌───┬───┐\n│ a │ b │\n├───┼───┤\n│ c │   │\n└───┴───┘\n"
    );
}

#[test]
fn missing_head_end_tag_still_shows_body() {
    assert_eq!(
        html("<html><head><title>T</title><h1>H</h1><p>p</p>"),
        "H\n═\n\np\n"
    );
}

#[test]
fn heading_permalinks_do_not_interrupt_the_title_or_outline() {
    let input = "<h2 id=\"setup\"><a href=\"#setup\" class=\"headerlink\">¶</a>Using <code>mu</code> <a href=\"#setup\">#</a></h2><p>After</p>";
    for width in [30, 40, 80] {
        let rendered = present_outlined(input, Format::Html, &Options::new(width, true));
        assert_eq!(strip_sgr(&rendered.text), "Using mu\n────────\n\nAfter\n");
        assert_eq!(rendered.headings.len(), 1);
        assert_eq!(rendered.headings[0].text, "Using mu");
        assert_eq!(rendered.headings[0].line, 0);
    }
    assert_invariants(input, Format::Html);
}

#[test]
fn heading_fragment_links_keep_meaningful_text_and_inline_formatting() {
    let input = "<h2><a href=\"#topic\">The <em>topic</em></a> and <strong>more</strong></h2>";
    assert_eq!(html(input), "The topic and more\n──────────────────\n");
    let out = colored(input, Format::Html, WIDTH);
    assert!(out.contains("\x1b[0;1;3;94mtopic"), "{out:?}");
    assert_invariants(input, Format::Html);
    // Ordinary document links must still expose their destination.
    assert_eq!(
        html("<p><a href=\"#topic\">topic</a></p>"),
        "topic (#topic)\n"
    );
}

#[test]
fn heading_breaks_separate_words_and_empty_anchors_preserve_spacing() {
    let input = "<h2>One<br><em>two</em> <a href=\"#two\"></a>three</h2>";
    assert_eq!(html(input), "One two three\n─────────────\n");
    assert_invariants(input, Format::Html);
}

#[test]
fn common_documentation_permalink_icons_do_not_become_heading_text() {
    for anchor in [
        "<a class=\"doc-anchor\" href=\"#title\">§</a>",
        "<a class=\"headerlink\" href=\"#title\">\u{f0c1}</a>",
        "<a href=\"#title\"><svg><path d=\"x\"/></svg></a>",
    ] {
        let input = format!("<h2>{anchor}Title</h2>");
        assert_eq!(html(&input), "Title\n─────\n");
        assert_invariants(&input, Format::Html);
    }
    assert_eq!(
        html("<h2><a href=\"https://x.io\">Title</a></h2>"),
        "Title (https://x.io)\n────────────────────\n"
    );
}

#[test]
fn successive_and_mismatched_headings_recover_without_swallowing_sections() {
    for input in [
        "<h1>Top<h2>Sub</h2><p>Body</p>",
        "<h1>Top</h2><h2>Sub</h2><p>Body</p>",
    ] {
        assert_eq!(html(input), "Top\n═══\n\nSub\n───\n\nBody\n");
        assert_invariants(input, Format::Html);
    }
}

#[test]
fn hidden_headings_do_not_suppress_the_document_title() {
    let input = "<head><title>Visible</title></head><template><h1>Hidden</h1></template><noscript><h1>Also hidden</h1></noscript><p>Body</p>";
    assert_eq!(html(input), "Visible\n═══════\n\nBody\n");
    assert_invariants(input, Format::Html);
}

#[test]
fn hidden_headings_inside_a_visible_heading_stay_hidden() {
    let input = "<h1>Visible<template><h2>Hidden</h2></template> title</h1>";
    assert_eq!(html(input), "Visible title\n═════════════\n");
    assert_invariants(input, Format::Html);
}

#[test]
fn meaningful_symbol_links_in_headings_are_not_decorative_permalinks() {
    for input in [
        "<h2 id=\"hash-comment\">Shell <a href=\"#hash-comment\"><code>#</code></a> comments</h2>",
        "<h2>Shell <a href=\"#hash-comment\">#</a> comments</h2>",
    ] {
        assert_eq!(html(input), "Shell # comments\n────────────────\n");
        assert_invariants(input, Format::Html);
    }
    for input in [
        "<h2><a href=\"#hash-comment\">#</a></h2>",
        "<h2 id=\"hash-comment\"><a href=\"#hash-comment\">#</a></h2>",
        "<h2 id=\"hash-comment\"><a class=\"headerlink\" href=\"#hash-comment\">#</a></h2>",
    ] {
        let out = present_outlined(input, Format::Html, &Options::new(40, false));
        assert_eq!(out.text, "#\n─\n");
        assert_eq!(out.headings[0].text, "#");
    }
}

#[test]
fn hidden_descendants_do_not_change_symbol_heading_classification() {
    for hidden in [
        "<template>Hidden</template>",
        "<noscript>Hidden</noscript>",
        "<svg><title>Hidden</title></svg>",
        "<script>Hidden</script>",
    ] {
        let input = format!("<h2 id=x>{hidden}<a href=#x>#</a> <a href=#x>¶</a></h2>");
        for width in [30, 60, 80] {
            let out = present_outlined(&input, Format::Html, &Options::new(width, true));
            assert_eq!(strip_sgr(&out.text), "# ¶\n───\n");
            assert_eq!(out.headings.len(), 1);
            assert_eq!(out.headings[0].text, "# ¶");
        }
        assert_invariants(&input, Format::Html);
    }
}

#[test]
fn multi_symbol_titles_keep_their_text_and_outline() {
    for input in [
        "<h2 id=x><a href=#x>#</a> <a href=#x>¶</a></h2>",
        "<h2 id=x><a class=headerlink href=#x>#</a><a class=headerlink href=#x>§</a></h2>",
    ] {
        for width in [30, 60, 80] {
            let out = present_outlined(input, Format::Html, &Options::new(width, true));
            assert_eq!(out.headings.len(), 1, "{input}: {:?}", out.text);
            assert!(out.headings[0].text.contains('#'));
            assert!(strip_sgr(&out.text).contains('#'));
        }
        assert_invariants(input, Format::Html);
    }
}

#[test]
fn outer_dropped_containers_close_over_unclosed_dropped_children() {
    for input in [
        "<template><select></template><p>Body</p>",
        "<template><button><select></template><p>Body</p>",
        "<noscript><select></noscript><p>Body</p>",
    ] {
        assert_eq!(html(input), "Body\n");
        assert_invariants(input, Format::Html);
    }
}

#[test]
fn templates_inside_head_cannot_expose_hidden_headings() {
    for input in [
        "<head><title>Visible</title><template><h1>Hidden</h1></template></head><p>Body</p>",
        "<head><title>Visible</title><template></head><h1>Hidden</h1></template></head><p>Body</p>",
    ] {
        assert_eq!(html(input), "Visible\n═══════\n\nBody\n");
        assert_invariants(input, Format::Html);
    }
}
