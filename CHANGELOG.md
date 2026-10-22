# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-22

First release.

### Added
- Markdown rendering in the terminal: headings, nested and task lists, tables, blockquotes, GFM alerts,
  footnotes, front matter, inline code, links and images.
- Centred, word-wrapped reading column that re-flows on resize (`+` / `-` to change its width).
- Syntax-highlighted code blocks whose colours come from the active theme (pure Rust, no C dependencies).
- 14 built-in themes with a live-preview picker (`T`) that remembers your choice:
  Catppuccin (Mocha, Macchiato, Latte), Tokyo Night, Gruvbox (Dark, Light), Nord, Dracula,
  Rosé Pine (and Dawn), Solarized (Dark, Light), Everforest, and a `terminal` theme that uses your
  terminal's own ANSI palette.
- Table-of-contents sidebar, incremental smart-case search, link selection, and heading jumps.
- Relative `.md` links open in-app (with `#anchor` support) and `h` goes back.
- File picker when started without a file, and reading from stdin.
- Automatic reload when the file changes, and `e` to edit in `$EDITOR`.
- Optional mouse mode (`m` / `--mouse`); off by default so text can be selected and copied natively.
- `install.sh` for a per-user install with a desktop entry for common terminals.

[0.1.0]: https://github.com/Farrely-F/arcanaeum/releases/tag/v0.1.0
