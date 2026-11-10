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
| `j` `e` `↓` `Enter` / `k` `y` `↑` | Scroll one line down / up |
| `space` `f` `PgDn` `Ctrl-F` / `b` `PgUp` `Ctrl-B` | Scroll one page down / up |
| `d` `Ctrl-D` / `u` `Ctrl-U` | Scroll half a page down / up |
| `g` `Home` `<` / `G` `End` `>` | Jump to top / bottom |
| Mouse wheel | Scroll. It's sent as arrow keys, so text selection still works |
| `/` / `?` | Search forward / backward. See [Search](#search) |
| `n` / `N` | Next / previous match, wrapping round the ends |
| `Esc` | Clear the search highlighting |
| `o` | Open the [outline](#outline) of the document's headings |
| `h` `←` / `l` `→` | Scroll sideways by half a window. See [Sideways](#sideways) |
| `q` `Ctrl-C` | Quit |

The pager opens when `--paging auto` finds the output taller than the screen minus one line, or always with `--paging always`. It never opens when stdout isn't a terminal. See [Paging](/wiki/usage#paging).

## Search {#search}

`/` searches forward and `?` searches backward. Added in 0.5.0.

- The query is typed into the status bar, shown as `/text█`. `Enter` runs it, `Backspace` deletes a character, `Ctrl-U` clears the line, and `Esc` or `Ctrl-C` cancels. `Backspace` on an empty prompt closes it. While the prompt is open every key types, so `q` doesn't quit.
- An empty query repeats the last one, in the direction of the key that opened the prompt.
- The match is a literal substring, not a regular expression. A query in lowercase ignores case; any capital letter makes it case-sensitive.
- mu matches the text as it appears on screen, with styling removed, so a styled word is found like any other. A match never spans two lines.
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

## Sideways {#sideways}

Lines wider than the window, such as a wide [CSV table](/wiki/formats#tables), run past the right edge. `l` or `→` scrolls right and `h` or `←` scrolls left, by half the window width each time. Added in 0.6.0.

- Scrolling stops when the end of the widest line reaches the right edge.
- Lines are cut by display column, keeping their colors. A wide character such as a CJK glyph that's cut in half shows as spaces.
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

## Resizing {#resize}

Resizing the window re-renders the document at the new width and keeps your place proportionally. With `-w`, the layout stays fixed and only the window changes. A search runs again over the new layout, and an open outline stays open with its selection in view.

## Terminal handling {#pager-internals}

- Raw mode via `termios`, restored on exit, on error, and from a panic hook.
- A self-pipe for `SIGWINCH`, so a resize can never be missed between a redraw and the next wait.
- The wait uses `select()`, not `poll()`, because macOS `poll()` rejects `/dev/tty`. Before 0.5.0 a resize on macOS only showed after the next key press.
- Synchronized-output frames to avoid flicker.
- Key decoding covers letters, control keys, UTF-8 and CSI escape sequences. It's unit-tested along with scrolling, search, the outline, resize and the status bar, and `tests/pager_pty.rs` drives the real pager in a pseudo-terminal.

The loop is drawn as [logic map 6](/wiki/logic-maps#map-6).
