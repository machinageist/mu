---
title: Pager
group: Guide
order: 6
summary: Keys, search, outline, sideways scrolling and the status bar
---

Built in, so `less` isn't needed. It takes over the screen on the alternate screen, so your scrollback is untouched on exit. It reads keys from `/dev/tty`, which means `curl … | mu` pages too.

## Keys {#keys}

| Keys | Action |
|---|---|
| `j` `e` `↓` / `k` `y` `↑` | Scroll one line down / up |
| `Enter` | Follow the selected internal EPUB link; otherwise scroll one line down |
| `space` `f` `PgDn` `Ctrl-F` / `b` `PgUp` `Ctrl-B` | Scroll one page down / up |
| `d` `Ctrl-D` / `u` `Ctrl-U` | Scroll half a page down / up |
| `g` `Home` `<` / `G` `End` `>` | Jump to top / bottom |
| Mouse wheel | Scroll; internal-link books use mouse reports, other documents use alternate-scroll arrow keys |
| Left-click | Follow an internal EPUB link under its painted label |
| `/` / `?` | Search forward / backward. See [Search](#search) |
| `n` / `N` | Next / previous match, wrapping round the ends |
| `Esc` | Clear the search highlighting |
| `o` | Open the [outline](#outline) of the document's headings |
| `Tab` / `Shift-Tab` | Select the next / previous internal [EPUB link](#chapter-links) |
| `Backspace` | Return after following an internal link; edits the query in the search prompt |
| `h` `←` / `l` `→` | Scroll sideways by half a window. See [Sideways](#sideways) |
| `q` `Ctrl-C` | Quit |

The pager opens when `--paging auto` finds the output taller than the screen minus one line, or always with `--paging always`. It never opens when stdout isn't a terminal. See [Paging](/wiki/usage#paging).

## Search {#search}

`/` searches forward and `?` searches backward. Added in 0.5.0.

- The query is typed into the status bar, shown as `/text█`. `Enter` runs it, `Backspace` deletes a character, `Ctrl-U` clears the line, and `Esc` or `Ctrl-C` cancels. `Backspace` on an empty prompt closes it. While the prompt is open every key types, so `q` doesn't quit.
- An empty query repeats the last one, in the direction of the key that opened the prompt.
- The match is a literal substring, not a regular expression. A query in lowercase ignores case; any capital letter makes it case-sensitive.
- mu matches the text as it appears on screen, with SGR styling and OSC 8 controls removed, so a styled word is found like any other. Hidden hyperlink destination bytes are never searched. A match never spans two lines.
- A forward search starts at the top line of the window, and a backward search above it. If there's no match that way, the search goes round the end.
- `n` moves on in the direction you searched and `N` goes the other way, so after `?` the `n` key moves up. Both wrap round the ends.
- A match that's already on screen doesn't scroll the view. One that's off screen sideways scrolls the view across to it.
- Every visible match is shown in reverse video, and the current one is also bold. `Esc` clears the highlighting; after that `n` does nothing until the next search.

The status bar shows the query, the current match and the count. `wrapped` appears after it when a jump went round the end:

```term
<rev> README.md     /install 3/7  41-80/230 34%  / search  o outline  q quit </rev>
```

When nothing matches, the view stays where it is and the title is replaced until the next key press:

```term
<rev> not found: zeta              1-40/230 17%  / search  o outline  q quit </rev>
```

## Outline {#outline}

`o` replaces the page with a list of the document's headings, indented two spaces per level. Added in 0.5.0.

- The list opens on the heading of the section you're reading: the last heading at or above the top line.
- `j` `k`, the arrow keys, the page and half-page keys, and `g` `G` move the selection.
- `Enter` jumps there, putting the heading's line at the top of the window. Near the end of the document the jump stops at the last full page. It also scrolls back to the left edge.
- `o`, `q`, `Esc` or `Ctrl-C` closes the list without moving.
- A document with no headings, such as JSON or a CSV table, shows `no headings` in the status bar instead.

```term
<rev> outline                                      4/14  Enter jump  o close </rev>
```

It works for every document format. `mu --toc FILE` prints the same list without paging. See [Outline](/wiki/usage#toc).

## EPUB chapter links {#chapter-links}

Internal EPUB links navigate inside the book with the keyboard or a plain left-click, independently of external terminal hyperlinks:

- `Tab` selects the next resolvable internal link and `Shift-Tab` the previous one, cycling in rendered reading order (line, then display column). The link is scrolled into view vertically and horizontally, and the status bar shows its label with `Enter follow`.
- Entering selection starts with the first visible source for `Tab`, or the last visible source for `Shift-Tab`, including the current horizontal window. If no source position is visible, selection continues in the requested reading direction and wraps around the book.
- The selected source's first rendered word is emphasized and the status bar includes `[position/total]`, so identical footnote labels can be distinguished. Invisible zero-width-only prefixes are skipped when choosing the focus word. Focus styling does not alter the document text and works with document color disabled; very narrow windows retain the label and ordinal before the key hint.
- `Enter` follows the selected link to its rendered destination line and display column, clamped at the last full page near the end. With no link selected, `Enter` still scrolls one line down.
- A plain left-button press follows any visible cell of the link's complete label, including later wrapped lines, inline styles and table cells. Gutters, borders, cell padding, the status bar and the blanked half of a clipped wide glyph are not link cells. Mouse releases and drag/motion reports do not follow links.
- `Backspace` returns to the view saved when following a link. In a search prompt it retains its query-editing behavior.
- Relative chapter paths resolve within readable chapters in the EPUB spine. A fragment-only link stays in its own chapter, and identical IDs in different chapters cannot collide. A path without a fragment targets the chapter's start.
- XHTML named/numeric references and equivalent Unicode/URI-encoded IDs resolve consistently without DTD fetching or recursive decoding. Empty page-break markers and table groups point to the next visible content in the chapter, never backward to preceding text or a border.
- Missing or empty chapters, missing or hidden fragments, trailing anchors without a visible destination and ambiguous targets stay readable but cannot be selected or followed. They do not jump to an unrelated chapter.
- Destinations use actual output lines and display columns, refreshed when the document reflows. Reflow clears the active selection; select again with `Tab` or `Shift-Tab`. Return positions are adjusted where the source link still matches, even when table wrapping changes the displayed order of links.
- This works with `--links never`, `--color never` and `NO_COLOR`. No OSC 8 support is needed. For a short book, use `--paging always` to open the pager.

These are in-pager jumps. mu does not invent file URIs for ZIP members, invoke an external opener or make network requests to follow them. External HTTP(S)/mailto links instead follow [the OSC 8 policy](/wiki/usage#links), with activation left to the emulator. Mouse press reporting is enabled only while a paged book has resolvable internal links; use the emulator's mouse-reporting bypass gesture (commonly Shift-drag) to select text. Ordinary documents retain terminal-managed selection.

## Sideways {#sideways}

Lines wider than the window, such as a wide [CSV table](/wiki/formats#tables), run past the right edge. `l` or `→` scrolls right and `h` or `←` scrolls left, by half the window width each time. Added in 0.6.0.

- Scrolling stops when the end of the widest line reaches the right edge.
- Lines are cut by display column, keeping their colors and OSC 8 destinations on the visible label. Hyperlink controls have zero width, and padding and borders are not linked. Clipping never splits a Unicode grapheme, including combining marks and joined emoji: a partially visible wide unit becomes spaces at either edge.
- While scrolled, the status bar shows the first visible column:

```term
<rev> data.csv             col 37  1-23/120 19%  / search  o outline  q quit </rev>
```

## Status bar {#status-bar}

The bottom row shows the file name (or the URL as typed) on the left. On the right are the search, if any, the column when scrolled sideways, the visible line range, how far through you are, and the key hints:

```term
<rev> README.md                    1-40/230 17%  / search  o outline  q quit </rev>
```

The bar is drawn in reverse video and fills exactly the window width. On a narrow window the key hints are dropped first. A long title is trimmed from the left, as in `…file.md`, so the file name stays visible.

Books with resolvable internal links also show `Tab links` when space permits. Selecting one replaces the title with its label and `Enter follow`; after a jump, `Backspace back` indicates a saved return position. The status bar is never part of an OSC 8 hyperlink.

## Resizing {#resize}

Resizing the window re-renders the document at the new width and keeps your place proportionally. With `-w`, text width stays fixed, but images still refit when height or cell-pixel geometry changes. A search runs again over the new layout, an open outline stays open with its selection in view, and EPUB navigation metadata is refreshed after reflow. Supported direct Kitty/Ghostty terminals keep high-resolution [images](/wiki/images#protocols) aligned while scrolling, clipping, reflowing and opening the outline; other environments retain half blocks.

## Terminal handling {#pager-internals}

- Raw mode via `termios`, restored on exit, on error, and from a panic hook.
- A self-pipe for `SIGWINCH`, so a resize can never be missed between a redraw and the next wait.
- The wait uses `select()`, not `poll()`, because macOS `poll()` rejects `/dev/tty`. Before 0.5.0 a resize on macOS only showed after the next key press.
- Synchronized-output frames to avoid flicker.
- Generated OSC 8 state is balanced across wrapping, highlighting and clipping, and closed before status bars and pager exit; `--links never` emits no OSC 8 cleanup sequence either.
- Key decoding covers letters, control keys, UTF-8, CSI, SGR/legacy mouse reports and Kitty graphics replies. Partial input has bounded buffering and a short deadline; malformed or oversized protocol packets cannot leak their payload into pager commands. Mouse reporting and owned graphics resources are cleaned up on exit, error and panic. Unit tests cover decoding and graphics lifecycle, and `tests/pager_pty.rs` drives the real pager in a pseudo-terminal.

The loop is drawn as [logic map 6](/wiki/logic-maps#map-6).
