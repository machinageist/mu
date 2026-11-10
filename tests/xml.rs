// Author: machinageist
// Date: 2026-11-09
// Description: XML is highlighted, and re-indented when its structure allows
// Notes: Exact output for a plist and an RSS feed, plus the shared output invariants

mod common;

use common::{assert_invariants, plain};
use mu::{Format, detect};

const PLIST: &str = include_str!("fixtures/sample.plist");
const RSS: &str = include_str!("fixtures/sample.rss");

#[test]
fn plist_is_re_indented() {
    assert_eq!(
        plain(PLIST, Format::Xml, 120),
        "\
<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
  <dict>
    <key>CFBundleName</key>
    <string>mu</string>
    <key>CFBundleVersion</key>
    <string>0.6.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>Formats</key>
    <array>
      <string>csv</string>
      <string>xml</string>
    </array>
  </dict>
</plist>
"
    );
}

#[test]
fn rss_keeps_comments_cdata_and_entities() {
    assert_eq!(
        plain(RSS, Format::Xml, 100),
        "\
<?xml version=\"1.0\"?>
<rss version=\"2.0\">
  <channel>
    <title>mu releases</title>
    <link>https://github.com/machinageist/mu</link>
    <!-- newest first -->
    <item>
      <title>mu 0.6.0</title>
      <description><![CDATA[CSV & XML <b>support</b>]]></description>
      <pubDate>Mon, 09 Nov 2026 00:00:00 GMT</pubDate>
    </item>
    <item>
      <title>Tables &amp; trees</title>
    </item>
  </channel>
</rss>
"
    );
}

#[test]
fn mixed_content_is_shown_as_written() {
    let input = "<doc>\n  <p>Some <em>mixed</em> text</p>\n</doc>\n";
    assert_eq!(plain(input, Format::Xml, 80), input);
}

#[test]
fn xml_keeps_the_output_invariants() {
    assert_invariants(PLIST, Format::Xml);
    assert_invariants(RSS, Format::Xml);
}

#[test]
fn names_extensions_and_media_types_resolve() {
    assert_eq!(Format::from_name("plist"), Some(Format::Xml));
    assert_eq!(Format::from_path("Info.plist"), Some(Format::Xml));
    assert_eq!(Format::from_path("logo.svg"), Some(Format::Xml));
    assert_eq!(
        Format::from_media_type("text/xml; charset=utf-8"),
        Some(Format::Xml)
    );
    assert_eq!(
        Format::from_media_type("application/rss+xml"),
        Some(Format::Xml)
    );
    assert_eq!(Format::from_media_type("image/svg+xml"), Some(Format::Xml));
    assert_eq!(
        Format::from_media_type("application/xhtml+xml"),
        Some(Format::Html)
    );
}

#[test]
fn readmes_opening_with_html_stay_markdown() {
    let readme =
        "<div align=\"center\">\n  <img src=\"logo.png\">\n</div>\n\n# Project\n\nWords.\n";
    assert_eq!(detect(None, readme), Format::Markdown);
}
