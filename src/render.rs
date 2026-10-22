//! Markdown -> pre-wrapped, styled lines.
//!
//! The document is laid out once for a given content width and then simply
//! sliced by the UI, which keeps scrolling O(viewport).

use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use std::str::FromStr;
use std::sync::OnceLock;
use syntect::{
    easy::HighlightLines,
    highlighting::{
        Color as SynColor, FontStyle, ScopeSelectors, StyleModifier, Theme as SynTheme, ThemeItem, ThemeSet,
        ThemeSettings,
    },
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;

#[derive(Clone, Debug, PartialEq)]
pub struct RSpan {
    pub text: String,
    pub style: Style,
    pub link: Option<usize>,
}

impl RSpan {
    fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            link: None,
        }
    }

    fn width(&self) -> usize {
        self.text.width()
    }
}

#[derive(Clone, Debug, Default)]
pub struct RLine {
    pub spans: Vec<RSpan>,
}

pub struct Heading {
    pub level: u8,
    pub title: String,
    pub slug: String,
    pub line: usize,
}

pub struct LinkInfo {
    pub url: String,
    /// First rendered line containing this link (`usize::MAX` if none).
    pub line: usize,
}

pub struct Doc {
    pub lines: Vec<RLine>,
    pub plain: Vec<String>,
    pub headings: Vec<Heading>,
    pub links: Vec<LinkInfo>,
}

impl Doc {
    pub fn empty() -> Self {
        Self {
            lines: Vec::new(),
            plain: Vec::new(),
            headings: Vec::new(),
            links: Vec::new(),
        }
    }
}

pub fn slugify(s: &str) -> String {
    s.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                Some(c.to_lowercase().collect::<String>())
            } else if c.is_whitespace() {
                Some("-".to_string())
            } else {
                None
            }
        })
        .collect()
}

pub fn render(src: &str, width: usize, theme: &Theme) -> Doc {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_GFM
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut b = Builder::new(width.max(20), theme);
    for ev in Parser::new_ext(src, opts) {
        b.event(ev);
    }
    b.finish()
}

// ---------------------------------------------------------------------------
// Inline tokenising + word wrapping
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Piece {
    /// A run of non-space characters; may span several styles (`**a**,`).
    Word(Vec<RSpan>),
    Space(RSpan),
    Break,
}

fn tokenize(segs: &[RSpan]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut word: Vec<RSpan> = Vec::new();

    fn flush(word: &mut Vec<RSpan>, out: &mut Vec<Piece>) {
        if !word.is_empty() {
            out.push(Piece::Word(std::mem::take(word)));
        }
    }

    for seg in segs {
        let mut buf = String::new();
        for c in seg.text.chars() {
            if c == '\n' {
                if !buf.is_empty() {
                    word.push(RSpan {
                        text: std::mem::take(&mut buf),
                        ..seg.clone()
                    });
                }
                flush(&mut word, &mut out);
                out.push(Piece::Break);
            } else if c.is_whitespace() {
                if !buf.is_empty() {
                    word.push(RSpan {
                        text: std::mem::take(&mut buf),
                        ..seg.clone()
                    });
                }
                flush(&mut word, &mut out);
                if !matches!(out.last(), Some(Piece::Space(_) | Piece::Break) | None) {
                    out.push(Piece::Space(RSpan {
                        text: " ".into(),
                        ..seg.clone()
                    }));
                }
            } else {
                buf.push(c);
            }
        }
        if !buf.is_empty() {
            word.push(RSpan {
                text: buf,
                ..seg.clone()
            });
        }
    }
    flush(&mut word, &mut out);
    out
}

fn pieces_width(p: &[Piece]) -> usize {
    p.iter()
        .map(|p| match p {
            Piece::Word(w) => w.iter().map(RSpan::width).sum(),
            Piece::Space(_) => 1,
            Piece::Break => 0,
        })
        .sum()
}

fn longest_word(p: &[Piece]) -> usize {
    p.iter()
        .map(|p| match p {
            Piece::Word(w) => w.iter().map(RSpan::width).sum(),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

/// Hard-split spans into chunks no wider than `width` display columns.
fn split_spans(spans: Vec<RSpan>, width: usize) -> Vec<Vec<RSpan>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<RSpan>> = vec![Vec::new()];
    let mut cur_w = 0;
    for sp in spans {
        let mut buf = String::new();
        for c in sp.text.chars() {
            let cw = c.width().unwrap_or(0);
            if cur_w + cw > width && cur_w > 0 {
                if !buf.is_empty() {
                    lines.last_mut().unwrap().push(RSpan {
                        text: std::mem::take(&mut buf),
                        ..sp.clone()
                    });
                }
                lines.push(Vec::new());
                cur_w = 0;
            }
            buf.push(c);
            cur_w += cw;
        }
        if !buf.is_empty() {
            lines.last_mut().unwrap().push(RSpan {
                text: buf,
                ..sp.clone()
            });
        }
    }
    lines
}

fn wrap(pieces: &[Piece], width: usize) -> Vec<Vec<RSpan>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<RSpan>> = Vec::new();
    let mut cur: Vec<RSpan> = Vec::new();
    let mut cur_w = 0usize;
    let mut pending: Option<RSpan> = None;

    for p in pieces {
        match p {
            Piece::Break => {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
                pending = None;
            }
            Piece::Space(sp) => {
                if cur_w > 0 {
                    pending = Some(sp.clone());
                }
            }
            Piece::Word(parts) => {
                let ww: usize = parts.iter().map(RSpan::width).sum();
                let sw = usize::from(pending.is_some());
                if cur_w + sw + ww <= width {
                    if let Some(sp) = pending.take() {
                        cur.push(sp);
                        cur_w += 1;
                    }
                    cur.extend(parts.iter().cloned());
                    cur_w += ww;
                } else {
                    if cur_w > 0 {
                        lines.push(std::mem::take(&mut cur));
                    }
                    pending = None;
                    if ww <= width {
                        cur = parts.clone();
                        cur_w = ww;
                    } else {
                        let mut chunks = split_spans(parts.clone(), width);
                        let last = chunks.pop().unwrap_or_default();
                        lines.extend(chunks);
                        cur_w = last.iter().map(RSpan::width).sum();
                        cur = last;
                    }
                }
            }
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn merge(spans: Vec<RSpan>) -> Vec<RSpan> {
    let mut out: Vec<RSpan> = Vec::with_capacity(spans.len());
    for s in spans {
        if s.text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(l) if l.style == s.style && l.link == s.link => l.text.push_str(&s.text),
            _ => out.push(s),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Syntax highlighting assets (loaded lazily, only if a code block exists)
// ---------------------------------------------------------------------------

fn syntax_assets() -> &'static (SyntaxSet, ThemeSet) {
    static A: OnceLock<(SyntaxSet, ThemeSet)> = OnceLock::new();
    A.get_or_init(|| (SyntaxSet::load_defaults_newlines(), ThemeSet::load_defaults()))
}

/// Syntax-highlighting theme built from the reader theme's own palette, so
/// code always matches the rest of the page. `None` for the ANSI theme, whose
/// colours can't be expressed as RGB.
fn palette_syntax_theme(t: &Theme) -> Option<SynTheme> {
    use ratatui::style::Color;
    let rgb = |c: Color| match c {
        Color::Rgb(r, g, b) => Some(SynColor { r, g, b, a: 255 }),
        _ => None,
    };
    let item = |scopes: &str, c: Color, fs: Option<FontStyle>| -> Option<ThemeItem> {
        Some(ThemeItem {
            scope: ScopeSelectors::from_str(scopes).ok()?,
            style: StyleModifier {
                foreground: Some(rgb(c)?),
                background: None,
                font_style: fs,
            },
        })
    };
    let italic = Some(FontStyle::ITALIC);
    let scopes = vec![
        item("comment, punctuation.definition.comment", t.dim, italic)?,
        item("punctuation", t.quote, None)?,
        item("keyword.operator", t.headings[3], None)?,
        item("keyword, storage, keyword.control", t.accent, None)?,
        item("string, string.quoted, punctuation.definition.string", t.ok, None)?,
        item(
            "constant.numeric, constant.language, constant.character, constant.other",
            t.bullet,
            None,
        )?,
        item(
            "variable.parameter.option, variable.parameter, entity.other.attribute-name",
            t.bullet,
            None,
        )?,
        item(
            "variable.other, variable.language, variable.function",
            t.headings[0],
            None,
        )?,
        item(
            "entity.name.function, support.function, variable.function",
            t.link,
            None,
        )?,
        item(
            "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, support.type, support.class, storage.type",
            t.warn,
            None,
        )?,
        item(
            "entity.name.tag, markup.heading, entity.name.section",
            t.headings[0],
            None,
        )?,
        item(
            "support.constant, meta.preprocessor, entity.name.namespace, entity.name.macro",
            t.headings[3],
            None,
        )?,
        item("invalid", t.danger, None)?,
    ];
    Some(SynTheme {
        name: None,
        author: None,
        settings: ThemeSettings {
            foreground: Some(rgb(t.fg)?),
            ..ThemeSettings::default()
        },
        scopes,
    })
}

fn canonical_lang(info: &str) -> String {
    let token = info
        .split(|c: char| c.is_whitespace() || c == ',' || c == '{')
        .next()
        .unwrap_or("")
        .to_lowercase();
    match token.as_str() {
        "ts" | "tsx" | "typescript" | "jsx" | "mjs" | "cjs" => "js".into(),
        "jsonc" | "json5" => "json".into(),
        "yml" => "yaml".into(),
        "shell" | "zsh" | "fish" | "console" => "sh".into(),
        "rs" => "rust".into(),
        "py" => "python".into(),
        "kt" => "java".into(),
        _ => token,
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

struct Ctx {
    first: String,
    first_style: Style,
    rest: String,
    rest_style: Style,
    used: bool,
}

struct TableState {
    aligns: Vec<Alignment>,
    rows: Vec<Vec<Vec<RSpan>>>,
    cur_row: Vec<Vec<RSpan>>,
    header_rows: usize,
}

struct Builder<'a> {
    theme: &'a Theme,
    width: usize,
    lines: Vec<RLine>,
    headings: Vec<Heading>,
    links: Vec<LinkInfo>,
    inline: Vec<RSpan>,
    styles: Vec<Style>,
    link_stack: Vec<usize>,
    ctx: Vec<Ctx>,
    lists: Vec<Option<u64>>,
    code: Option<(String, String)>,
    table: Option<TableState>,
    heading: Option<u8>,
    last_blank: bool,
}

impl<'a> Builder<'a> {
    fn new(width: usize, theme: &'a Theme) -> Self {
        Self {
            theme,
            width,
            lines: vec![RLine::default()],
            headings: Vec::new(),
            links: Vec::new(),
            inline: Vec::new(),
            styles: vec![Style::default().fg(theme.fg)],
            link_stack: Vec::new(),
            ctx: Vec::new(),
            lists: Vec::new(),
            code: None,
            table: None,
            heading: None,
            last_blank: true,
        }
    }

    fn cur(&self) -> Style {
        *self.styles.last().unwrap()
    }

    fn push_style(&mut self, s: Style) {
        let merged = self.cur().patch(s);
        self.styles.push(merged);
    }

    fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    fn push_seg(&mut self, text: impl Into<String>, style: Style) {
        self.inline.push(RSpan {
            text: text.into(),
            style,
            link: self.link_stack.last().copied(),
        });
    }

    fn prefix_width(&self) -> usize {
        self.ctx.iter().map(|c| c.rest.width()).sum()
    }

    fn avail(&self) -> usize {
        self.width.saturating_sub(self.prefix_width()).max(10)
    }

    fn prefix(&mut self) -> Vec<RSpan> {
        self.ctx
            .iter_mut()
            .map(|c| {
                if c.used {
                    RSpan::new(c.rest.clone(), c.rest_style)
                } else {
                    c.used = true;
                    RSpan::new(c.first.clone(), c.first_style)
                }
            })
            .collect()
    }

    fn emit(&mut self, spans: Vec<RSpan>) {
        let mut all = self.prefix();
        all.extend(spans);
        self.lines.push(RLine { spans: merge(all) });
        self.last_blank = false;
    }

    fn blank(&mut self) {
        let spans: Vec<RSpan> = self
            .ctx
            .iter()
            .map(|c| RSpan::new(c.rest.clone(), c.rest_style))
            .collect();
        self.lines.push(RLine { spans: merge(spans) });
        self.last_blank = true;
    }

    fn gap(&mut self) {
        if !self.last_blank {
            self.blank();
        }
    }

    fn flush_inline(&mut self) {
        if self.inline.is_empty() {
            return;
        }
        let segs = std::mem::take(&mut self.inline);
        let pieces = tokenize(&segs);
        let w = self.avail();
        for line in wrap(&pieces, w) {
            self.emit(line);
        }
    }

    fn rule_line(&mut self, ch: char, style: Style) {
        let w = self.avail();
        self.emit(vec![RSpan::new(ch.to_string().repeat(w), style)]);
    }

    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if let Some((_, buf)) = &mut self.code {
                    buf.push_str(&t);
                } else {
                    let st = self.cur();
                    self.push_seg(t.to_string(), st);
                }
            }
            Event::Code(c) | Event::InlineMath(c) | Event::DisplayMath(c) => {
                let st = self.cur().patch(
                    Style::default()
                        .fg(self.theme.inline_code_fg)
                        .bg(self.theme.inline_code_bg),
                );
                self.push_seg(c.to_string(), st);
            }
            Event::SoftBreak => {
                let st = self.cur();
                self.push_seg(" ", st);
            }
            Event::HardBreak => {
                let st = self.cur();
                self.push_seg("\n", st);
            }
            Event::Rule => {
                self.flush_inline();
                self.gap();
                self.rule_line('─', Style::default().fg(self.theme.rule));
            }
            Event::TaskListMarker(done) => {
                let theme = self.theme;
                if let Some(c) = self.ctx.last_mut() {
                    c.first = if done { "☑ " } else { "☐ " }.to_string();
                    c.first_style = Style::default().fg(if done { theme.ok } else { theme.dim });
                    c.rest = " ".repeat(c.first.width());
                }
            }
            Event::FootnoteReference(l) => {
                let st = Style::default().fg(self.theme.dim);
                self.push_seg(format!("[{l}]"), st);
            }
            Event::Html(h) | Event::InlineHtml(h) => {
                if h.trim_start().to_lowercase().starts_with("<br") {
                    let st = self.cur();
                    self.push_seg("\n", st);
                }
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        let th = self.theme;
        match tag {
            Tag::Paragraph => {
                self.flush_inline();
                self.gap();
            }
            Tag::Heading { level, .. } => {
                self.flush_inline();
                self.gap();
                let n = level as usize;
                self.heading = Some(n as u8);
                let color = th.headings[(n - 1).min(5)];
                let mut st = Style::default().fg(color).add_modifier(Modifier::BOLD);
                if n == 6 {
                    st = st.add_modifier(Modifier::ITALIC);
                }
                self.push_style(st);
            }
            Tag::BlockQuote(kind) => {
                self.flush_inline();
                self.gap();
                let (label, color) = match kind {
                    Some(BlockQuoteKind::Note) => (Some("Note"), th.info),
                    Some(BlockQuoteKind::Tip) => (Some("Tip"), th.ok),
                    Some(BlockQuoteKind::Important) => (Some("Important"), th.accent),
                    Some(BlockQuoteKind::Warning) => (Some("Warning"), th.warn),
                    Some(BlockQuoteKind::Caution) => (Some("Caution"), th.danger),
                    None => (None, th.rule),
                };
                let bar = Style::default().fg(color);
                self.ctx.push(Ctx {
                    first: "▎ ".into(),
                    first_style: bar,
                    rest: "▎ ".into(),
                    rest_style: bar,
                    used: false,
                });
                self.push_style(Style::default().fg(th.quote));
                if let Some(l) = label {
                    self.push_seg(l, Style::default().fg(color).add_modifier(Modifier::BOLD));
                    self.flush_inline();
                }
            }
            Tag::CodeBlock(kind) => {
                self.flush_inline();
                self.gap();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::MetadataBlock(_) => {
                self.flush_inline();
                self.gap();
                self.code = Some(("frontmatter".into(), String::new()));
            }
            Tag::List(start) => {
                self.flush_inline();
                if self.lists.is_empty() {
                    self.gap();
                }
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush_inline();
                let depth = self.lists.len().saturating_sub(1);
                let bullet = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let s = format!("{n}. ");
                        *n += 1;
                        s
                    }
                    _ => ["• ", "◦ ", "▪ "][depth % 3].to_string(),
                };
                let rest = " ".repeat(bullet.width());
                self.ctx.push(Ctx {
                    first: bullet,
                    first_style: Style::default().fg(th.bullet),
                    rest,
                    rest_style: Style::default(),
                    used: false,
                });
            }
            Tag::FootnoteDefinition(label) => {
                self.flush_inline();
                self.gap();
                let first = format!("[{label}] ");
                let rest = " ".repeat(first.width());
                self.ctx.push(Ctx {
                    first,
                    first_style: Style::default().fg(th.dim),
                    rest,
                    rest_style: Style::default(),
                    used: false,
                });
            }
            Tag::Table(aligns) => {
                self.flush_inline();
                self.gap();
                self.table = Some(TableState {
                    aligns,
                    rows: Vec::new(),
                    cur_row: Vec::new(),
                    header_rows: 0,
                });
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(t) = &mut self.table {
                    t.cur_row.clear();
                }
            }
            Tag::TableCell => self.inline.clear(),
            Tag::Emphasis => self.push_style(Style::default().add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(Style::default().add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => self.push_style(Style::default().add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { dest_url, .. } => {
                let id = self.links.len();
                self.links.push(LinkInfo {
                    url: dest_url.to_string(),
                    line: usize::MAX,
                });
                self.link_stack.push(id);
                self.push_style(Style::default().fg(th.link).add_modifier(Modifier::UNDERLINED));
            }
            Tag::Image { dest_url, .. } => {
                let id = self.links.len();
                self.links.push(LinkInfo {
                    url: dest_url.to_string(),
                    line: usize::MAX,
                });
                self.link_stack.push(id);
                self.push_style(Style::default().fg(th.link).add_modifier(Modifier::ITALIC));
                let st = self.cur();
                self.push_seg("▣ ", st);
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush_inline(),
            TagEnd::Heading(level) => {
                let title: String = self.inline.iter().map(|s| s.text.as_str()).collect();
                let title = title.trim().to_string();
                let line = self.lines.len();
                self.flush_inline();
                let n = level as usize;
                if n <= 2 && self.ctx.is_empty() {
                    let ch = if n == 1 { '━' } else { '─' };
                    let color = self.theme.headings[n - 1];
                    self.rule_line(ch, Style::default().fg(color).add_modifier(Modifier::DIM));
                }
                self.headings.push(Heading {
                    level: n as u8,
                    slug: slugify(&title),
                    title,
                    line,
                });
                self.pop_style();
                self.heading = None;
            }
            TagEnd::BlockQuote(_) => {
                self.flush_inline();
                self.ctx.pop();
                self.pop_style();
            }
            TagEnd::CodeBlock | TagEnd::MetadataBlock(_) => self.render_code(),
            TagEnd::List(_) => {
                self.flush_inline();
                self.lists.pop();
            }
            TagEnd::Item | TagEnd::FootnoteDefinition => {
                self.flush_inline();
                self.ctx.pop();
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.inline);
                if let Some(t) = &mut self.table {
                    t.cur_row.push(cell);
                }
            }
            TagEnd::TableHead => {
                if let Some(t) = &mut self.table {
                    let row = std::mem::take(&mut t.cur_row);
                    t.rows.push(row);
                    t.header_rows = 1;
                }
            }
            TagEnd::TableRow => {
                if let Some(t) = &mut self.table {
                    let row = std::mem::take(&mut t.cur_row);
                    t.rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    self.render_table(t);
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_style(),
            TagEnd::Link | TagEnd::Image => {
                self.link_stack.pop();
                self.pop_style();
            }
            _ => {}
        }
    }

    fn render_code(&mut self) {
        let Some((info, text)) = self.code.take() else {
            return;
        };
        let theme = self.theme;
        let avail = self.avail();
        // bar + space on the left, one column of padding on the right
        let inner = avail.saturating_sub(3).max(8);
        let bg = Style::default().bg(theme.code_bg);
        let bar = Style::default().fg(theme.accent).bg(theme.code_bg);
        let lang = canonical_lang(&info);

        let (ss, ts) = syntax_assets();
        let syntax = if lang.is_empty() || lang == "frontmatter" {
            None
        } else {
            ss.find_syntax_by_token(&lang)
        };
        let syntax = syntax.unwrap_or_else(|| ss.find_syntax_plain_text());
        let palette_theme = palette_syntax_theme(theme);
        let hl_theme = palette_theme
            .as_ref()
            .or_else(|| ts.themes.get(theme.syntect))
            .or_else(|| ts.themes.values().next())
            .expect("syntect ships default themes");
        let mut h = HighlightLines::new(syntax, hl_theme);

        let frame_line = |this: &mut Self, spans: Vec<RSpan>, right: Option<RSpan>| {
            let used: usize = spans.iter().map(RSpan::width).sum();
            let right_w = right.as_ref().map_or(0, |r| r.width() + 1);
            let mut line = vec![RSpan::new("▎", bar), RSpan::new(" ", bg)];
            line.extend(spans);
            line.push(RSpan::new(
                " ".repeat(avail.saturating_sub(used + 2 + right_w).max(1)),
                bg,
            ));
            if let Some(r) = right {
                line.push(r);
                line.push(RSpan::new(" ", bg));
            }
            this.emit(line);
        };

        let label = info.trim().to_string();
        let label_style = bg.fg(theme.dim).add_modifier(Modifier::ITALIC);
        let label = (!label.is_empty()).then(|| RSpan::new(label, label_style));
        frame_line(self, vec![], label);

        for raw in LinesWithEndings::from(&text) {
            let ranges = h.highlight_line(raw, ss).unwrap_or_default();
            let mut spans: Vec<RSpan> = Vec::new();
            for (st, t) in ranges {
                let t = t.trim_end_matches(['\n', '\r']).replace('\t', "    ");
                if t.is_empty() {
                    continue;
                }
                let fg = st.foreground;
                let mut style = bg.fg(ratatui::style::Color::Rgb(fg.r, fg.g, fg.b));
                if st.font_style.contains(FontStyle::BOLD) {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if st.font_style.contains(FontStyle::ITALIC) {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                spans.push(RSpan::new(t, style));
            }
            for chunk in split_spans(merge(spans), inner) {
                frame_line(self, chunk, None);
            }
        }
        frame_line(self, vec![], None);
    }

    fn render_table(&mut self, t: TableState) {
        let th = self.theme;
        let ncols = t.rows.iter().map(Vec::len).max().unwrap_or(0).max(t.aligns.len());
        if ncols == 0 {
            return;
        }
        let head_style = Style::default().fg(th.accent).add_modifier(Modifier::BOLD);
        let border = Style::default().fg(th.rule);

        let cells: Vec<Vec<Vec<Piece>>> = t
            .rows
            .iter()
            .enumerate()
            .map(|(ri, row)| {
                (0..ncols)
                    .map(|c| {
                        let mut segs = row.get(c).cloned().unwrap_or_default();
                        if ri < t.header_rows {
                            for s in &mut segs {
                                s.style = s.style.patch(head_style);
                            }
                        }
                        tokenize(&segs)
                    })
                    .collect()
            })
            .collect();

        let mut widths: Vec<usize> = (0..ncols)
            .map(|c| {
                cells
                    .iter()
                    .map(|r| pieces_width(&r[c]))
                    .max()
                    .unwrap_or(0)
                    .max(1)
            })
            .collect();
        let mut floors: Vec<usize> = (0..ncols)
            .map(|c| {
                cells
                    .iter()
                    .map(|r| longest_word(&r[c]))
                    .max()
                    .unwrap_or(1)
                    .clamp(3, 12)
            })
            .collect();
        let budget = self.avail().saturating_sub(3 * ncols + 1).max(ncols);
        while widths.iter().sum::<usize>() > budget {
            let pick = (0..ncols)
                .filter(|&c| widths[c] > floors[c])
                .max_by_key(|&c| widths[c]);
            match pick {
                Some(c) => widths[c] -= 1,
                None if floors.iter().any(|&f| f > 1) => floors.iter_mut().for_each(|f| *f = 1),
                None => break,
            }
        }

        let hline = |l: &str, m: &str, r: &str| -> Vec<RSpan> {
            let mut s = String::from(l);
            for (i, w) in widths.iter().enumerate() {
                s.push_str(&"─".repeat(w + 2));
                s.push_str(if i + 1 == ncols { r } else { m });
            }
            vec![RSpan::new(s, border)]
        };

        self.emit(hline("┌", "┬", "┐"));
        for (ri, row) in cells.iter().enumerate() {
            let wrapped: Vec<Vec<Vec<RSpan>>> = (0..ncols).map(|c| wrap(&row[c], widths[c])).collect();
            let height = wrapped.iter().map(Vec::len).max().unwrap_or(1).max(1);
            for li in 0..height {
                let mut spans = vec![RSpan::new("│", border)];
                for c in 0..ncols {
                    let content = wrapped[c].get(li).cloned().unwrap_or_default();
                    let cw: usize = content.iter().map(RSpan::width).sum();
                    let pad = widths[c].saturating_sub(cw);
                    let (l, r) = match t.aligns.get(c) {
                        Some(Alignment::Right) => (pad, 0),
                        Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                        _ => (0, pad),
                    };
                    spans.push(RSpan::new(" ".repeat(l + 1), Style::default()));
                    spans.extend(content);
                    spans.push(RSpan::new(" ".repeat(r + 1), Style::default()));
                    spans.push(RSpan::new("│", border));
                }
                self.emit(spans);
            }
            if ri + 1 == t.header_rows && ri + 1 < cells.len() {
                self.emit(hline("├", "┼", "┤"));
            }
        }
        self.emit(hline("└", "┴", "┘"));
    }

    fn finish(mut self) -> Doc {
        self.flush_inline();
        while self.lines.len() > 1 && self.lines.last().is_some_and(|l| l.spans.is_empty()) {
            self.lines.pop();
        }
        self.lines.push(RLine::default());

        for (i, line) in self.lines.iter().enumerate() {
            for s in &line.spans {
                if let Some(id) = s.link
                    && self.links[id].line == usize::MAX
                {
                    self.links[id].line = i;
                }
            }
        }
        let plain = self
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect();
        Doc {
            lines: self.lines,
            plain,
            headings: self.headings,
            links: self.links,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, ThemeName};

    fn doc(src: &str, w: usize) -> Doc {
        render(src, w, &Theme::from_name(ThemeName::Terminal))
    }

    #[test]
    fn wraps_to_width() {
        let d = doc("one two three four five six seven eight nine ten", 20);
        assert!(d.plain.iter().all(|l| l.width() <= 20), "{:?}", d.plain);
        assert!(d.plain.iter().filter(|l| !l.is_empty()).count() >= 3);
    }

    #[test]
    fn collects_headings_and_links() {
        let d = doc("# Hello World\n\ntext [site](https://x.dev) more\n\n## Sub\n", 60);
        assert_eq!(d.headings.len(), 2);
        assert_eq!(d.headings[0].slug, "hello-world");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].url, "https://x.dev");
        assert!(d.links[0].line < d.lines.len());
    }

    #[test]
    fn headings_have_no_literal_hashes() {
        let d = doc("### Third level\n\n#### Fourth\n", 40);
        assert!(d.plain.iter().all(|l| !l.contains('#')), "{:?}", d.plain);
        assert_eq!(d.headings[0].title, "Third level");
    }

    #[test]
    fn palette_syntax_theme_for_rgb_themes_only() {
        use crate::theme::ThemeName;
        assert!(palette_syntax_theme(&Theme::from_name(ThemeName::Nord)).is_some());
        assert!(palette_syntax_theme(&Theme::from_name(ThemeName::Terminal)).is_none());
    }

    #[test]
    fn lists_are_tight_and_nested() {
        let d = doc("- a\n  - b\n- c\n", 40);
        let text: Vec<&str> = d
            .plain
            .iter()
            .map(String::as_str)
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(text, vec!["• a", "  ◦ b", "• c"]);
    }

    #[test]
    fn table_fits_and_has_borders() {
        let src = "| a | b |\n|---|---|\n| one two three four five | x |\n";
        let d = doc(src, 24);
        assert!(d.plain.iter().all(|l| l.width() <= 24), "{:?}", d.plain);
        assert!(d.plain.iter().any(|l| l.starts_with('┌')));
        assert!(d.plain.iter().any(|l| l.starts_with('└')));
    }

    #[test]
    fn code_block_is_padded_to_width() {
        let d = doc("```rust\nfn main() {}\n```\n", 30);
        let code: Vec<_> = d.plain.iter().filter(|l| l.contains("fn main")).collect();
        assert_eq!(code.len(), 1);
        assert_eq!(code[0].width(), 30);
    }
}
