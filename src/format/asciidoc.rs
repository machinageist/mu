// Author: machinageist
// Date: 2026-11-09
// Description: AsciiDoc documents to pulldown-cmark events
// Notes: Stub until implemented; shows the source verbatim in a code frame

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};

// Check whether the head of an input looks like AsciiDoc
pub fn sniff(_head: &str) -> bool {
    false
}

// Translate AsciiDoc into document events
pub fn events(input: &str) -> Vec<Event<'_>> {
    vec![
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(CowStr::Borrowed(
            "adoc",
        )))),
        Event::Text(CowStr::Borrowed(input)),
        Event::End(TagEnd::CodeBlock),
    ]
}
