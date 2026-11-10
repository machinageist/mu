# Change: pager search and outline

Ships as **0.5.0**, the first release on `FEATURE/ROADMAP.md`.

## Outcome

Long documents become navigable without leaving the pager. You can search the rendered text, jump between matches, and jump to any heading. `--toc` prints a document's outline without paging it.

## Behaviour

### Search

- **Starting a search:** `/` searches forward and `?` searches backward. A prompt replaces the status bar: `/` or `?`, then what you type.
- **Prompt editing:**
  - Printable characters, including UTF-8, are typed in.
  - `Backspace` deletes one character.
  - `Ctrl-U` clears the prompt.
  - `Enter` runs the search.
  - `Esc` or `Ctrl-C` cancels, and so does `Backspace` on an empty prompt.
  - `Enter` on an empty prompt repeats the last search in the new direction.
- **Matching:**
  - Queries are literal substrings, not regular expressions.
  - Case is "smart": an all-lowercase query ignores case, and any uppercase letter makes it case-sensitive.
  - Text is matched as you see it on screen: SGR styling is removed first, and box-drawing, gutters and wrapped lines are matched as displayed.
  - A match never spans two lines.
- **Moving between matches:**
  - From the top visible line, mu moves to the first match in the search direction.
  - `n` repeats the search in the same direction and `N` in the opposite one. Both wrap around the document, and the status bar says `wrapped` when they do.
  - A match already on screen does not scroll the view. Otherwise its line becomes the top line, clamped at the end of the document.
- **Highlighting:**
  - Every visible match is shown in reverse video, and the current match is also bold.
  - Highlighting keeps the line's own colors outside the match, and each line still resets its style before the newline.
- **Status bar:**
  - During a search it shows the query, then `3/17` (current match out of the total), then any `wrapped` note.
  - When nothing matches it shows `not found: <query>`, the view stays where it was, and the next key clears the message.
- **Resizing:** when a resize re-lays out the document, the search runs again on the new text, and the current match becomes the first match at or after the new top line.
- **Clearing:** `Esc` with no prompt open clears the highlighting.

### Outline

- **Source:** the renderer records every heading it writes: its level, its text with styling removed, and the output line it starts on. This works for every document format, because they all emit the same heading events. Data formats have no headings.
- **Opening it:** `o` in the pager opens the outline over the document.
  - Headings are listed one per line, indented two spaces per level below the shallowest level present.
  - Long headings are cut with `…`.
  - The highlighted entry starts on the heading whose section contains the top visible line.
- **Keys in the outline:**
  - `j`/`k` and the arrow keys move one entry.
  - `space`/`b`, `d`/`u`, `g`/`G` and `Home`/`End` move as they do in the pager.
  - `Enter` jumps there, putting the heading's line at the top (clamped), and closes the outline.
  - `o`, `q`, `Esc` and `Ctrl-C` close it without moving.
  - The list scrolls when it is taller than the screen.
- **No headings:** for a document without headings, `o` shows `no headings` in the status bar until the next key.
- **Resizing:** heading line numbers come from the latest layout, so the outline stays correct after a re-layout.

### `--toc`

- **What it prints:** the outline and nothing else, without paging, then exit 0. Each heading goes on its own line, indented two spaces per level below the shallowest level present, and colored with that heading level's style when color is on. Long headings are cut to the width with `…`.
- **No headings:** a document without headings, including any data format, prints nothing and exits 0.
- **Inputs:** works with files, stdin and URLs. With `-I` or `-i` it lists only the body's headings.

### Status bar and help

- **Hint:** the hint becomes `/ search  o outline  q quit`, which `fit_left` trims as before when the window is narrow.
- **Help text:** the "Pager keys" block in `--help` and the README's pager table list `/ ? n N o Esc`.

## Library

- **New result type:** `present` keeps its signature. A new `present_outlined(input, format, options) -> Rendered` returns `Rendered { text: String, headings: Vec<Heading> }`, with `Heading { level: u8, text: String, line: usize }`. The library is pre-1.0 and documented as unstable from 0.10.0, so this is an addition, not a promise.
- **Line numbers:** `Heading.line` indexes lines of `text`, where a line is a `\n`-terminated row. Tests check that the line, with styling removed and trimmed, starts with the heading's text, or its first wrapped part, for every document fixture.
- **Re-layout:** the pager's `relayout` closure returns `Rendered`, so a re-layout updates the headings too.

## Constraints

- No new dependencies. Search is a plain substring scan over stripped lines, and smart case lowercases each line with `char::to_lowercase`.
- **Speed:** searching a 10 000-line document once, or redrawing with highlights, must not be visible to the reader. Cache each stripped line on first use and search incrementally from the current line.
- The `--color never` output of every existing fixture stays byte-identical, because headings are only recorded, never drawn differently.
- Pager keys that exist today keep their meaning. `?`, `/`, `n`, `N`, `o` and `Esc` are new and are not bound today.

## Acceptance criteria

- **Search:** unit tests cover smart case, wrap-around in both directions, `n`/`N` after `?`, repeating a search with an empty `Enter`, and the not-found state leaving the view unchanged.
- **Highlighting:** unit tests show that a highlighted line, with styling removed, equals the line without highlighting, and that it still resets before the newline. This includes lines with wide characters and styling changes inside a match.
- **Prompt editing:** unit tests cover UTF-8 input, Backspace, Ctrl-U, and Esc cancelling.
- **Outline:** unit tests cover the starting selection, scrolling, the jump target, and the no-headings message.
- **Heading line numbers:** a test sweeps every document fixture at widths 30–120 and checks every `Heading.line`.
- **`--toc`:** CLI tests cover a Markdown fixture with exact output, an HTML fixture, a JSON file giving no output, and a URL served by the local test server.
- **Manual check:** the pager is driven by hand in a pseudo-terminal: search, `n`/`N`, the outline, and resizing during a search. The result is noted in the PLAN.

## Out of scope

- Regular-expression search
- Searching image pixels, or text hidden inside a native-protocol image (the pager only uses half-blocks anyway)
- Remappable keys, which arrive with the config file in 0.9.0
- An outline for data formats, such as top-level keys
- Persistent search history
