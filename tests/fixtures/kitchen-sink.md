---
title: Kitchen sink
date: 2026-11-09
---

# mdread kitchen sink

A paragraph with **bold**, *italic*, ***both***, ~~struck~~, `inline code`, and a
[link](https://example.com/some/long/path) plus an autolink <https://rust-lang.org>.
This sentence is long enough that it has to wrap across several lines when the width is narrow.

## Lists

- First item
- Second item with a much longer body that should wrap neatly under its own text, not under the bullet
  - Nested item
    - Deeper item
- [ ] Open task
- [x] Done task

1. One
2. Two
   1. Two-a
3. Three

Loose list:

- Para one

- Para two

### Quotes and alerts

> A plain quote that also needs to wrap because it rambles on for quite a while longer than the width.
>
> Second quote paragraph.
> > Nested quote

> [!WARNING]
> Mind the gap.

#### Code

```rust
fn main() {
	println!("tabs expand");
    let a_very_long_line = "this line is deliberately longer than the frame so that it wraps onto a continuation";
}
```

    indented code

##### Table

| Left | Center | Right |
|:-----|:------:|------:|
| a | **b** | 1 |
| longer cell text that wraps | c | 22 |

###### Misc

Text with a footnote[^1] and an image ![logo](logo.png) and ![](bare.png).
Line one<br>line two\
line three

<!-- hidden comment -->

<div align="center">raw html</div>

---

[^1]: The footnote body.
