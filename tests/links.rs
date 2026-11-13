// Author: machinageist
// Description: Public hyperlink rendering and terminal-state safety regressions

use mu::{Format, Options, present_outlined, render};

#[test]
fn enabled_links_emit_a_clickable_label_without_a_url_suffix() {
    let options = Options {
        links: true,
        ..Options::new(60, false)
    };
    assert_eq!(
        render("Read [the manual](https://example.org/manual).", &options),
        "Read \x1b]8;;https://example.org/manual\x1b\\the manual\x1b]8;;\x1b\\.\n"
    );
}

#[test]
fn epub_fragment_links_resolve_to_the_exact_visible_destination_line() {
    let rendered = present_outlined(
        "<p>Read <a href='#chapter-two'>the <em>next</em> chapter</a>.</p><h1 id='chapter-two'>Chapter Two</h1><p>Body</p>",
        Format::Epub,
        &Options::new(30, false),
    );
    assert_eq!(rendered.links.len(), 1);
    assert_eq!(rendered.links[0].label, "the next chapter");
    let anchor = rendered
        .anchors
        .iter()
        .find(|anchor| anchor.id == rendered.links[0].target)
        .unwrap();
    assert_eq!(rendered.text.lines().nth(anchor.line), Some("Chapter Two"));
    assert_eq!(
        rendered.text.lines().nth(rendered.links[0].line),
        Some("Read the next chapter.")
    );
    assert!(!rendered.text.contains("#chapter-two"));
    assert!(!rendered.text.contains('\x1b'));
}

#[test]
fn an_empty_clickable_label_still_exposes_its_destination() {
    let options = Options {
        links: true,
        ..Options::new(60, false)
    };
    assert_eq!(
        render("[](https://example.org/)", &options),
        "(https://example.org/)\n"
    );
}
