// Author: machinageist
// Date: 2026-11-09
// Description: HTML documents to pulldown-cmark events
// Notes: Stub until implemented; shows the source verbatim in a code frame

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};

// Check whether the head of an input looks like HTML
pub fn sniff(_head: &str) -> bool {
    false
}

// Translate HTML into document events
pub fn events(input: &str) -> Vec<Event<'_>> {
    vec![
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(CowStr::Borrowed(
            "html",
        )))),
        Event::Text(CowStr::Borrowed(input)),
        Event::End(TagEnd::CodeBlock),
    ]
}
