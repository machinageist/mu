// Author: machinageist
// Date: 2026-11-09
// Description: Org-mode documents to pulldown-cmark events
// Notes: Stub until implemented; shows the source verbatim in a code frame

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};

// Check whether the head of an input looks like Org-mode
pub fn sniff(_head: &str) -> bool {
    false
}

// Translate Org-mode into document events
pub fn events(input: &str) -> Vec<Event<'_>> {
    vec![
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(CowStr::Borrowed(
            "org",
        )))),
        Event::Text(CowStr::Borrowed(input)),
        Event::End(TagEnd::CodeBlock),
    ]
}
