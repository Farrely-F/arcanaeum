---
title: Demo
---

# arcanaeum demo

A comfortable **terminal** markdown reader with *italics*, ~~strikethrough~~, `inline code`
and [a link](https://example.com) plus a [section link](#tables). Long paragraphs wrap at word
boundaries so the text stays pleasant to read even in a narrow window, and the column is centred.

## Lists

- First item
- Second item with a much longer line that has to wrap around to the next line of the terminal
  - Nested item
    - Deeper
- [x] done task
- [ ] open task

1. One
2. Two
10. Ten

> A blockquote that carries some wisdom
> across multiple lines.

> [!WARNING]
> Alerts are supported too.

### Code

```rust
fn main() {
    let greeting = "hello";
    println!("{greeting}, world");
}
```

```python
def f(x):
    return x * 2
```

## Tables

| Name   | Role        | Score |
|:-------|:-----------:|------:|
| Ada    | Engineer    |    42 |
| Grace  | Admiral and compiler pioneer with a long description | 7 |

---

#### Deep heading

See [the readme](../README.md) or an image ![logo](logo.png).

Footnote reference[^1].

[^1]: The footnote text.
