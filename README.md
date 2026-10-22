# Arcanaeum

[![CI](https://github.com/Farrely-F/arcanaeum/actions/workflows/ci.yml/badge.svg)](https://github.com/Farrely-F/arcanaeum/actions/workflows/ci.yml)

A fast, comfortable markdown reader for the terminal. One native binary, no runtime.

Named after the library of the College of Winterhold, a fitting place to read.
The command is `arca`.

- Centred, word-wrapped reading column (default 100 cols), resizes live
- Headings, nested/task lists, tables, blockquotes, GFM alerts, footnotes, front matter
- Syntax-highlighted code blocks (pure Rust, no C dependencies)
- Table-of-contents sidebar, incremental search, link selection and navigation
- Relative `.md` links open in-app (with `#anchor` support); `h` goes back
- Reloads automatically when the file changes (great next to your editor)
- Never paints a page background, so transparent/blurred terminals (Hyprland) look right
- 14 themes with a live-preview picker (`T`); your choice is remembered (see [Themes](#themes))

## Install

```sh
./install.sh            # cargo install + desktop entry (kitty/ghostty/foot/... auto-detected)
./install.sh --default  # additionally make arcanaeum the default app for .md files
```

Without cloning:

```sh
cargo install --git https://github.com/Farrely-F/arcanaeum --locked
```

Prebuilt binaries (Linux and macOS, x86_64 and arm64) with SHA-256 checksums are attached to each
[GitHub release](https://github.com/Farrely-F/arcanaeum/releases). Unpack one and put `arca` on your `PATH`.

From a checkout, `cargo install --path . --locked` works too. Building needs Rust 1.88+ (edition 2024).

## Usage

```sh
arca README.md        # read a file
arca docs/            # pick a file from a directory
arca                  # pick a file from the current directory
cat notes.md | arca   # read stdin
arca -t nord -w 90    # theme / column width (env: ARCANAEUM_THEME, ARCANAEUM_WIDTH)
```

## Keys (press `?` in the app)

| Key | Action |
|---|---|
| `j` `k` / arrows | scroll line |
| `space` `b` / PgDn PgUp | scroll page |
| `d` `u` | half page |
| `g` `G` | top / bottom |
| `]` `[` | next / previous heading |
| `t` | table of contents (`j` `k`, `enter` to jump) |
| `/` then `n` `N` | search (smart-case) |
| `tab` `shift-tab` | select next / previous link |
| `enter` / `l` | open selected link |
| `h` / `backspace` | back |
| `o` | file picker |
| `+` `-` | wider / narrower column |
| `m` | toggle mouse mode (wheel + click links vs. native text selection) |
| `T` | theme picker (live preview, `enter` saves, `esc` cancels) |
| `r` / `e` | reload / edit in `$EDITOR` |
| `q` | quit |

Mouse: off by default so you can select and copy text with the terminal. Press `m` (or start
with `--mouse`) to let arcanaeum handle the wheel and clicks on links / TOC entries; hold Shift to
select text while it's on. With the mouse off, the wheel still scrolls in most terminals.

## Themes

Press `T` to open the picker: moving the selection previews the theme on your document,
`enter` saves it, `esc` restores the previous one. The choice is stored in
`~/.config/arcanaeum/theme` (or `$XDG_CONFIG_HOME/arcanaeum/theme`). `-t/--theme` or `ARCANAEUM_THEME`
override it for a single run.

| Dark | Light |
|---|---|
| `catppuccin-mocha` (default), `catppuccin-macchiato`, `tokyo-night`, `gruvbox-dark`, `nord`, `dracula`, `rose-pine`, `solarized-dark`, `everforest` | `catppuccin-latte`, `gruvbox-light`, `rose-pine-dawn`, `solarized-light` |

`terminal` uses your terminal's own 16 ANSI colours, so the reader follows whatever palette
your terminal (and desktop theme) already uses. The old names `dark` and `light` still work.

## Hyprland

The desktop entry launches arcanaeum in its own terminal window with class `arcanaeum`, so you can
style it:

```
windowrulev2 = float, class:^(arcanaeum)$
windowrulev2 = size 1000 800, class:^(arcanaeum)$
windowrulev2 = center, class:^(arcanaeum)$
```

Open files from a keybind, e.g. `bind = SUPER, M, exec, kitty --class arcanaeum -e arca`.

## License

MIT, see [LICENSE](LICENSE).
