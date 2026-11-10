---
title: Pager
group: Guide
order: 6
summary: Keys, status bar and resizing
---

Built in, so `less` isn't needed. It takes over the screen on the alternate screen, so your scrollback is untouched on exit. It reads keys from `/dev/tty`, which means `curl … | mu` pages too.

## Keys {#keys}

| Keys | Action |
|---|---|
| `j` `↓` `Enter` / `k` `↑` | Scroll one line down / up |
| `space` `f` `PgDn` `Ctrl-F` / `b` `PgUp` `Ctrl-B` | Scroll one page down / up |
| `d` `Ctrl-D` / `u` `Ctrl-U` | Scroll half a page down / up |
| `g` `Home` `<` / `G` `End` `>` | Jump to top / bottom |
| Mouse wheel | Scroll. It's sent as arrow keys, so text selection still works |
| `q` `Ctrl-C` | Quit |

The pager opens when `--paging auto` finds the output taller than the screen minus one line, or always with `--paging always`. It never opens when stdout isn't a terminal. See [Paging](/wiki/usage#paging).

## Status bar {#status-bar}

The bottom row shows the file name (or the URL as typed), the visible line range, how far through you are, and a quit hint.

```term
<rev> docs/README.md                        1-40/300 13%  q quit </rev>
```

The bar is drawn in reverse video and fills exactly the window width. A long title is trimmed from the left, as in `…file.md`, so the file name stays visible.

## Resizing {#resize}

Resizing the window re-renders the document at the new width and keeps your place proportionally. With `-w`, the layout stays fixed and only the window changes. Lines wider than the window are clipped rather than wrapped.

## Terminal handling {#pager-internals}

- Raw mode via `termios`, restored on exit, on error, and from a panic hook.
- A self-pipe for `SIGWINCH`, so a resize can never be missed between a redraw and the next `poll()`.
- Synchronized-output frames to avoid flicker.
- Key decoding covers letters, control keys and CSI escape sequences, and is unit-tested along with scroll clamping, resize and the status bar.

The loop is drawn as [logic map 6](/wiki/logic-maps#map-6). There's no search (`/`) yet.
