use std::{fs, io, path::PathBuf};

use clap::ValueEnum;
use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ThemeName {
    #[value(alias = "dark", alias = "mocha")]
    CatppuccinMocha,
    CatppuccinMacchiato,
    #[value(alias = "light", alias = "latte")]
    CatppuccinLatte,
    TokyoNight,
    GruvboxDark,
    GruvboxLight,
    Nord,
    Dracula,
    RosePine,
    RosePineDawn,
    SolarizedDark,
    SolarizedLight,
    Everforest,
    /// Uses your terminal's own ANSI colours, so it follows your desktop theme
    Terminal,
}

pub const DEFAULT_THEME: ThemeName = ThemeName::CatppuccinMocha;

impl ThemeName {
    pub fn all() -> &'static [ThemeName] {
        Self::value_variants()
    }

    /// The name used on the command line and in the saved config.
    pub fn label(self) -> String {
        self.to_possible_value()
            .map(|v| v.get_name().to_string())
            .unwrap_or_default()
    }
}

/// Colours used by the renderer and UI. The page itself never paints a
/// background, so transparent / blurred terminals keep working.
#[derive(Clone, Debug)]
pub struct Theme {
    pub title: &'static str,
    pub dark: bool,
    pub fg: Color,
    pub dim: Color,
    pub accent: Color,
    pub headings: [Color; 6],
    pub link: Color,
    pub quote: Color,
    pub bullet: Color,
    pub rule: Color,
    pub code_bg: Color,
    pub inline_code_fg: Color,
    pub inline_code_bg: Color,
    pub search_fg: Color,
    pub search_bg: Color,
    pub search_cur_bg: Color,
    pub sel_fg: Color,
    pub sel_bg: Color,
    pub status_fg: Color,
    pub status_bg: Color,
    pub ok: Color,
    pub warn: Color,
    pub danger: Color,
    pub info: Color,
    pub syntect: &'static str,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Compact palette description; `Theme::build` derives the rest.
struct Pal {
    title: &'static str,
    dark: bool,
    fg: u32,
    dim: u32,
    quote: u32,
    rule: u32,
    /// Background of code blocks.
    code: u32,
    /// Background of inline code.
    inline: u32,
    /// Status bar background.
    bar: u32,
    /// Text drawn on top of accent-coloured backgrounds.
    on: u32,
    accent: u32,
    link: u32,
    bullet: u32,
    inline_fg: u32,
    /// h1..h6
    h: [u32; 6],
    ok: u32,
    warn: u32,
    danger: u32,
    info: u32,
    /// Background of the current search match.
    hit: u32,
    syntect: &'static str,
}

impl Theme {
    #[rustfmt::skip]
    pub fn from_name(name: ThemeName) -> Self {
        use ThemeName::*;
        match name {
            CatppuccinMocha => Self::build(Pal {
                title: "Catppuccin Mocha", dark: true,
                fg: 0xcdd6f4, dim: 0x7f849c, quote: 0xa6adc8, rule: 0x585b70,
                code: 0x1e1e2e, inline: 0x313244, bar: 0x313244, on: 0x11111b,
                accent: 0xcba6f7, link: 0x89b4fa, bullet: 0xfab387, inline_fg: 0xfab387,
                h: [0xf38ba8, 0xcba6f7, 0x89b4fa, 0x94e2d5, 0xa6e3a1, 0xf9e2af],
                ok: 0xa6e3a1, warn: 0xf9e2af, danger: 0xf38ba8, info: 0x89b4fa, hit: 0xfab387,
                syntect: "base16-ocean.dark",
            }),
            CatppuccinMacchiato => Self::build(Pal {
                title: "Catppuccin Macchiato", dark: true,
                fg: 0xcad3f5, dim: 0x8087a2, quote: 0xa5adcb, rule: 0x5b6078,
                code: 0x24273a, inline: 0x363a4f, bar: 0x363a4f, on: 0x181926,
                accent: 0xc6a0f6, link: 0x8aadf4, bullet: 0xf5a97f, inline_fg: 0xf5a97f,
                h: [0xed8796, 0xc6a0f6, 0x8aadf4, 0x8bd5ca, 0xa6da95, 0xeed49f],
                ok: 0xa6da95, warn: 0xeed49f, danger: 0xed8796, info: 0x8aadf4, hit: 0xf5a97f,
                syntect: "base16-ocean.dark",
            }),
            CatppuccinLatte => Self::build(Pal {
                title: "Catppuccin Latte", dark: false,
                fg: 0x4c4f69, dim: 0x8c8fa1, quote: 0x6c6f85, rule: 0xbcc0cc,
                code: 0xe6e9ef, inline: 0xdce0e8, bar: 0xccd0da, on: 0xeff1f5,
                accent: 0x8839ef, link: 0x1e66f5, bullet: 0xfe640b, inline_fg: 0xd20f39,
                h: [0xd20f39, 0x8839ef, 0x1e66f5, 0x179299, 0x40a02b, 0xdf8e1d],
                ok: 0x40a02b, warn: 0xdf8e1d, danger: 0xd20f39, info: 0x1e66f5, hit: 0xfe640b,
                syntect: "InspiredGitHub",
            }),
            TokyoNight => Self::build(Pal {
                title: "Tokyo Night", dark: true,
                fg: 0xc0caf5, dim: 0x565f89, quote: 0xa9b1d6, rule: 0x3b4261,
                code: 0x1a1b26, inline: 0x292e42, bar: 0x292e42, on: 0x15161e,
                accent: 0xbb9af7, link: 0x7aa2f7, bullet: 0xff9e64, inline_fg: 0xff9e64,
                h: [0xf7768e, 0xbb9af7, 0x7aa2f7, 0x7dcfff, 0x9ece6a, 0xe0af68],
                ok: 0x9ece6a, warn: 0xe0af68, danger: 0xf7768e, info: 0x7aa2f7, hit: 0xff9e64,
                syntect: "base16-ocean.dark",
            }),
            GruvboxDark => Self::build(Pal {
                title: "Gruvbox Dark", dark: true,
                fg: 0xebdbb2, dim: 0x928374, quote: 0xa89984, rule: 0x504945,
                code: 0x282828, inline: 0x3c3836, bar: 0x3c3836, on: 0x282828,
                accent: 0xd3869b, link: 0x83a598, bullet: 0xfe8019, inline_fg: 0xfe8019,
                h: [0xfb4934, 0xd3869b, 0x83a598, 0x8ec07c, 0xb8bb26, 0xfabd2f],
                ok: 0xb8bb26, warn: 0xfabd2f, danger: 0xfb4934, info: 0x83a598, hit: 0xfe8019,
                syntect: "base16-eighties.dark",
            }),
            GruvboxLight => Self::build(Pal {
                title: "Gruvbox Light", dark: false,
                fg: 0x3c3836, dim: 0x928374, quote: 0x665c54, rule: 0xbdae93,
                code: 0xebdbb2, inline: 0xd5c4a1, bar: 0xd5c4a1, on: 0xfbf1c7,
                accent: 0x8f3f71, link: 0x076678, bullet: 0xaf3a03, inline_fg: 0x9d0006,
                h: [0x9d0006, 0x8f3f71, 0x076678, 0x427b58, 0x79740e, 0xb57614],
                ok: 0x79740e, warn: 0xb57614, danger: 0x9d0006, info: 0x076678, hit: 0xaf3a03,
                syntect: "InspiredGitHub",
            }),
            Nord => Self::build(Pal {
                title: "Nord", dark: true,
                fg: 0xd8dee9, dim: 0x616e88, quote: 0xa7b1c7, rule: 0x4c566a,
                code: 0x2e3440, inline: 0x3b4252, bar: 0x3b4252, on: 0x2e3440,
                accent: 0xb48ead, link: 0x81a1c1, bullet: 0xd08770, inline_fg: 0xebcb8b,
                h: [0xbf616a, 0xb48ead, 0x81a1c1, 0x88c0d0, 0xa3be8c, 0xebcb8b],
                ok: 0xa3be8c, warn: 0xebcb8b, danger: 0xbf616a, info: 0x88c0d0, hit: 0xd08770,
                syntect: "base16-ocean.dark",
            }),
            Dracula => Self::build(Pal {
                title: "Dracula", dark: true,
                fg: 0xf8f8f2, dim: 0x6272a4, quote: 0xc0c4de, rule: 0x44475a,
                code: 0x282a36, inline: 0x44475a, bar: 0x343746, on: 0x282a36,
                accent: 0xbd93f9, link: 0x8be9fd, bullet: 0xffb86c, inline_fg: 0xff79c6,
                h: [0xff79c6, 0xbd93f9, 0x8be9fd, 0x50fa7b, 0xf1fa8c, 0xffb86c],
                ok: 0x50fa7b, warn: 0xf1fa8c, danger: 0xff5555, info: 0x8be9fd, hit: 0xffb86c,
                syntect: "base16-mocha.dark",
            }),
            RosePine => Self::build(Pal {
                title: "Rosé Pine", dark: true,
                fg: 0xe0def4, dim: 0x6e6a86, quote: 0x908caa, rule: 0x403d52,
                code: 0x191724, inline: 0x26233a, bar: 0x26233a, on: 0x191724,
                accent: 0xc4a7e7, link: 0x9ccfd8, bullet: 0xf6c177, inline_fg: 0xebbcba,
                h: [0xeb6f92, 0xc4a7e7, 0x9ccfd8, 0xebbcba, 0xf6c177, 0x908caa],
                ok: 0x9ccfd8, warn: 0xf6c177, danger: 0xeb6f92, info: 0x9ccfd8, hit: 0xebbcba,
                syntect: "base16-mocha.dark",
            }),
            RosePineDawn => Self::build(Pal {
                title: "Rosé Pine Dawn", dark: false,
                fg: 0x575279, dim: 0x9893a5, quote: 0x797593, rule: 0xdfdad9,
                code: 0xf2e9e1, inline: 0xe4dccf, bar: 0xf2e9e1, on: 0xfaf4ed,
                accent: 0x907aa9, link: 0x286983, bullet: 0xea9d34, inline_fg: 0xb4637a,
                h: [0xb4637a, 0x907aa9, 0x286983, 0x56949f, 0xd7827e, 0xea9d34],
                ok: 0x56949f, warn: 0xea9d34, danger: 0xb4637a, info: 0x286983, hit: 0xd7827e,
                syntect: "InspiredGitHub",
            }),
            SolarizedDark => Self::build(Pal {
                title: "Solarized Dark", dark: true,
                fg: 0x93a1a1, dim: 0x586e75, quote: 0x839496, rule: 0x2f5560,
                code: 0x002b36, inline: 0x073642, bar: 0x073642, on: 0x002b36,
                accent: 0x6c71c4, link: 0x268bd2, bullet: 0xcb4b16, inline_fg: 0xb58900,
                h: [0xdc322f, 0x6c71c4, 0x268bd2, 0x2aa198, 0x859900, 0xb58900],
                ok: 0x859900, warn: 0xb58900, danger: 0xdc322f, info: 0x268bd2, hit: 0xcb4b16,
                syntect: "Solarized (dark)",
            }),
            SolarizedLight => Self::build(Pal {
                title: "Solarized Light", dark: false,
                fg: 0x586e75, dim: 0x93a1a1, quote: 0x657b83, rule: 0xc9c2a8,
                code: 0xeee8d5, inline: 0xe3dcc4, bar: 0xeee8d5, on: 0xfdf6e3,
                accent: 0x6c71c4, link: 0x268bd2, bullet: 0xcb4b16, inline_fg: 0xdc322f,
                h: [0xdc322f, 0x6c71c4, 0x268bd2, 0x2aa198, 0x859900, 0xb58900],
                ok: 0x859900, warn: 0xb58900, danger: 0xdc322f, info: 0x268bd2, hit: 0xcb4b16,
                syntect: "Solarized (light)",
            }),
            Everforest => Self::build(Pal {
                title: "Everforest", dark: true,
                fg: 0xd3c6aa, dim: 0x859289, quote: 0x9da9a0, rule: 0x4f585e,
                code: 0x2d353b, inline: 0x3d484d, bar: 0x343f44, on: 0x2d353b,
                accent: 0xd699b6, link: 0x7fbbb3, bullet: 0xe69875, inline_fg: 0xdbbc7f,
                h: [0xe67e80, 0xd699b6, 0x7fbbb3, 0x83c092, 0xa7c080, 0xdbbc7f],
                ok: 0xa7c080, warn: 0xdbbc7f, danger: 0xe67e80, info: 0x7fbbb3, hit: 0xe69875,
                syntect: "base16-eighties.dark",
            }),
            Terminal => Self::terminal(),
        }
    }

    fn build(p: Pal) -> Self {
        Self {
            title: p.title,
            dark: p.dark,
            fg: rgb(p.fg),
            dim: rgb(p.dim),
            accent: rgb(p.accent),
            headings: p.h.map(rgb),
            link: rgb(p.link),
            quote: rgb(p.quote),
            bullet: rgb(p.bullet),
            rule: rgb(p.rule),
            code_bg: rgb(p.code),
            inline_code_fg: rgb(p.inline_fg),
            inline_code_bg: rgb(p.inline),
            search_fg: rgb(if p.dark { p.on } else { p.fg }),
            search_bg: rgb(p.warn),
            search_cur_bg: rgb(p.hit),
            sel_fg: rgb(p.on),
            sel_bg: rgb(p.link),
            status_fg: rgb(p.fg),
            status_bg: rgb(p.bar),
            ok: rgb(p.ok),
            warn: rgb(p.warn),
            danger: rgb(p.danger),
            info: rgb(p.info),
            syntect: p.syntect,
        }
    }

    fn terminal() -> Self {
        Self {
            title: "Terminal (ANSI)",
            dark: true,
            fg: Color::Reset,
            dim: Color::DarkGray,
            accent: Color::Magenta,
            headings: [
                Color::Red,
                Color::Magenta,
                Color::Blue,
                Color::Cyan,
                Color::Green,
                Color::Yellow,
            ],
            link: Color::Blue,
            quote: Color::Gray,
            bullet: Color::Yellow,
            rule: Color::DarkGray,
            code_bg: Color::Indexed(236),
            inline_code_fg: Color::Yellow,
            inline_code_bg: Color::Indexed(237),
            search_fg: Color::Black,
            search_bg: Color::Yellow,
            search_cur_bg: Color::LightRed,
            sel_fg: Color::Black,
            sel_bg: Color::Blue,
            status_fg: Color::White,
            status_bg: Color::Indexed(237),
            ok: Color::Green,
            warn: Color::Yellow,
            danger: Color::Red,
            info: Color::Blue,
            syntect: "base16-ocean.dark",
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence: the last theme picked in the app is remembered.
// ---------------------------------------------------------------------------

fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base)
}

fn config_file() -> Option<PathBuf> {
    Some(config_dir()?.join("arcanaeum").join("theme"))
}

pub fn load_saved() -> Option<ThemeName> {
    let base = config_dir()?;
    // `mdr` was this project's name before it was renamed; honour its saved theme.
    let text = fs::read_to_string(base.join("arcanaeum").join("theme"))
        .or_else(|_| fs::read_to_string(base.join("mdr").join("theme")))
        .ok()?;
    ThemeName::from_str(text.trim(), true).ok()
}

pub fn save(name: ThemeName) -> io::Result<()> {
    let path = config_file().ok_or_else(|| io::Error::other("no config directory"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, name.label())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_builds_and_has_a_known_syntax_theme() {
        let known = [
            "base16-ocean.dark",
            "base16-eighties.dark",
            "base16-mocha.dark",
            "InspiredGitHub",
            "Solarized (dark)",
            "Solarized (light)",
        ];
        for &n in ThemeName::all() {
            let t = Theme::from_name(n);
            assert!(known.contains(&t.syntect), "{n:?}");
            assert_eq!(ThemeName::from_str(&n.label(), true).unwrap(), n);
        }
    }

    #[test]
    fn legacy_names_still_work() {
        assert_eq!(
            ThemeName::from_str("dark", true).unwrap(),
            ThemeName::CatppuccinMocha
        );
        assert_eq!(
            ThemeName::from_str("light", true).unwrap(),
            ThemeName::CatppuccinLatte
        );
    }
}
