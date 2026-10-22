use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::{
    render::{self, Doc},
    theme::{self, Theme, ThemeName},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Help,
    Picker,
    Themes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Doc,
    Toc,
}

struct HistEntry {
    path: PathBuf,
    scroll: usize,
}

#[derive(Default)]
pub struct Picker {
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
    pub filter: String,
    pub sel: usize,
}

impl Picker {
    pub fn filtered(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        self.files
            .iter()
            .enumerate()
            .filter(|(_, p)| f.is_empty() || self.display(p).to_lowercase().contains(&f))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn display(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).unwrap_or(p).display().to_string()
    }
}

pub struct App {
    pub mode: Mode,
    pub focus: Focus,
    pub path: Option<PathBuf>,
    pub name: String,
    pub source: String,
    pub doc: Doc,
    doc_width: usize,
    pub scroll: usize,
    pub view_h: usize,
    pub text_rect: Rect,
    pub toc_rect: Rect,
    pub theme_name: ThemeName,
    pub theme: Theme,
    theme_origin: ThemeName,
    pub max_width: usize,
    pub show_toc: bool,
    pub toc_sel: usize,
    pub sel_link: Option<usize>,
    pub query: String,
    pub matches: Vec<(usize, usize)>,
    pub cur_match: Option<usize>,
    search_origin: usize,
    history: Vec<HistEntry>,
    pub picker: Picker,
    from_picker: bool,
    pub msg: Option<(String, Instant)>,
    pub quit: bool,
    /// Whether the app owns the mouse (wheel/click) instead of the terminal (text selection).
    pub mouse: bool,
    pub edit_request: Option<PathBuf>,
}

const MSG_TTL: Duration = Duration::from_secs(3);

impl App {
    pub fn new(theme_name: ThemeName, max_width: usize, show_toc: bool) -> Self {
        Self {
            mode: Mode::Normal,
            focus: Focus::Doc,
            path: None,
            name: String::new(),
            source: String::new(),
            doc: Doc::empty(),
            doc_width: 0,
            scroll: 0,
            view_h: 24,
            text_rect: Rect::default(),
            toc_rect: Rect::default(),
            theme_name,
            theme: Theme::from_name(theme_name),
            theme_origin: theme_name,
            max_width: max_width.clamp(40, 240),
            show_toc,
            toc_sel: 0,
            sel_link: None,
            query: String::new(),
            matches: Vec::new(),
            cur_match: None,
            search_origin: 0,
            history: Vec::new(),
            picker: Picker::default(),
            from_picker: false,
            msg: None,
            quit: false,
            mouse: false,
            edit_request: None,
        }
    }

    // ----- loading -------------------------------------------------------

    pub fn set_source(&mut self, name: String, source: String) {
        self.name = name;
        self.source = source;
        self.sel_link = None;
        self.matches.clear();
        self.cur_match = None;
        self.query.clear();
        // Use the last known content width so the next frame doesn't re-render.
        let w = match self.text_rect.width {
            0 => self.max_width,
            w => usize::from(w),
        };
        self.doc = render::render(&self.source, w, &self.theme);
        self.doc_width = w;
    }

    pub fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let bytes = fs::read(path)?;
        let src = String::from_utf8_lossy(&bytes).into_owned();
        let name = path
            .file_name()
            .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into());
        self.set_source(name, src);
        self.path = Some(path.to_path_buf());
        self.scroll = 0;
        self.mode = Mode::Normal;
        self.focus = Focus::Doc;
        Ok(())
    }

    pub fn reload(&mut self) {
        let Some(path) = self.path.clone() else { return };
        match fs::read(&path) {
            Ok(bytes) => {
                let keep = self.scroll;
                let src = String::from_utf8_lossy(&bytes).into_owned();
                if src == self.source {
                    return;
                }
                let query = std::mem::take(&mut self.query);
                let name = self.name.clone();
                self.set_source(name, src);
                self.query = query;
                self.recompute_matches();
                self.scroll = keep;
                self.clamp_scroll();
                self.flash("reloaded");
            }
            Err(e) => self.flash(format!("reload failed: {e}")),
        }
    }

    pub fn open_picker(&mut self, root: PathBuf, files: Vec<PathBuf>) {
        self.picker = Picker {
            root,
            files,
            filter: String::new(),
            sel: 0,
        };
        self.mode = Mode::Picker;
        self.from_picker = true;
    }

    fn navigate(&mut self, path: &Path, anchor: Option<&str>) {
        let prev = self.path.clone().map(|p| HistEntry {
            path: p,
            scroll: self.scroll,
        });
        match self.load_file(path) {
            Ok(()) => {
                if let Some(p) = prev {
                    self.history.push(p);
                }
                if let Some(a) = anchor {
                    self.jump_anchor(a);
                }
            }
            Err(e) => self.flash(format!("cannot open {}: {e}", path.display())),
        }
    }

    fn go_back(&mut self) {
        if let Some(h) = self.history.pop() {
            if self.load_file(&h.path).is_ok() {
                self.scroll = h.scroll;
                self.clamp_scroll();
            }
        } else if self.from_picker && !self.picker.files.is_empty() {
            self.mode = Mode::Picker;
        }
    }

    // ----- layout --------------------------------------------------------

    pub fn layout(&mut self, area: Rect) {
        let body_h = area.height.saturating_sub(1);
        self.view_h = usize::from(body_h);

        let toc_w = if self.show_toc && !self.doc.headings.is_empty() && area.width >= 60 {
            (area.width / 4).clamp(22, 38)
        } else {
            0
        };
        let reading_w = area.width - toc_w;
        let margin = if reading_w >= 80 { 3 } else { 1 };
        let cw = usize::from(reading_w.saturating_sub(2 * margin + 1))
            .min(self.max_width)
            .max(20)
            .min(usize::from(reading_w));
        let left = toc_w + (reading_w - cw as u16) / 2;

        self.toc_rect = Rect::new(area.x, area.y, toc_w, body_h);
        self.text_rect = Rect::new(area.x + left, area.y, cw as u16, body_h);

        if cw != self.doc_width {
            let ratio = self.scroll as f64 / self.doc.lines.len().max(1) as f64;
            self.doc = render::render(&self.source, cw, &self.theme);
            self.doc_width = cw;
            self.scroll = (ratio * self.doc.lines.len() as f64).round() as usize;
            self.recompute_matches();
            if self.sel_link.is_some_and(|l| l >= self.doc.links.len()) {
                self.sel_link = None;
            }
        }
        self.clamp_scroll();
    }

    fn relayout_now(&mut self) {
        self.doc_width = 0;
    }

    pub fn max_scroll(&self) -> usize {
        self.doc.lines.len().saturating_sub(self.view_h)
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }

    pub fn scroll_by(&mut self, d: isize) {
        let s = self.scroll as isize + d;
        self.scroll = s.clamp(0, self.max_scroll() as isize) as usize;
    }

    fn scroll_to(&mut self, line: usize) {
        self.scroll = line.min(self.max_scroll());
    }

    fn ensure_visible(&mut self, line: usize) {
        if line < self.scroll || line >= self.scroll + self.view_h {
            self.scroll_to(line.saturating_sub(self.view_h / 3));
        }
    }

    pub fn progress(&self) -> u16 {
        let m = self.max_scroll();
        (self.scroll * 100).checked_div(m).map_or(100, |p| p as u16)
    }

    pub fn current_heading(&self) -> Option<usize> {
        let n = self.doc.headings.partition_point(|h| h.line <= self.scroll + 1);
        n.checked_sub(1)
    }

    pub fn flash(&mut self, m: impl Into<String>) {
        self.msg = Some((m.into(), Instant::now()));
    }

    /// Returns true if something visible changed (message expired).
    pub fn tick(&mut self) -> bool {
        if self.msg.as_ref().is_some_and(|(_, t)| t.elapsed() > MSG_TTL) {
            self.msg = None;
            return true;
        }
        false
    }

    // ----- headings / links ---------------------------------------------

    fn next_heading(&mut self) {
        if let Some(h) = self.doc.headings.iter().find(|h| h.line > self.scroll) {
            let l = h.line;
            self.scroll_to(l);
        } else {
            self.scroll_to(usize::MAX);
        }
    }

    fn prev_heading(&mut self) {
        if let Some(h) = self.doc.headings.iter().rev().find(|h| h.line < self.scroll) {
            let l = h.line;
            self.scroll_to(l);
        } else {
            self.scroll_to(0);
        }
    }

    fn jump_anchor(&mut self, anchor: &str) {
        let slug = render::slugify(&anchor.replace("%20", " "));
        match self.doc.headings.iter().find(|h| h.slug == slug) {
            Some(h) => {
                let l = h.line;
                self.scroll_to(l);
            }
            None => self.flash(format!("no heading #{anchor}")),
        }
    }

    fn step_link(&mut self, forward: bool) {
        let n = self.doc.links.len();
        if n == 0 {
            self.flash("no links in this document");
            return;
        }
        let visible = |a: &App, i: usize| {
            let l = a.doc.links[i].line;
            l != usize::MAX && l >= a.scroll && l < a.scroll + a.view_h
        };
        let next = match self.sel_link {
            Some(i) if visible(self, i) => {
                let mut j = i;
                loop {
                    j = if forward { (j + 1) % n } else { (j + n - 1) % n };
                    if self.doc.links[j].line != usize::MAX || j == i {
                        break j;
                    }
                }
            }
            _ => {
                let top = self.scroll;
                let found = if forward {
                    (0..n).find(|&i| self.doc.links[i].line != usize::MAX && self.doc.links[i].line >= top)
                } else {
                    (0..n).rev().find(|&i| {
                        self.doc.links[i].line != usize::MAX && self.doc.links[i].line < top + self.view_h
                    })
                };
                found.unwrap_or_else(|| if forward { 0 } else { n - 1 })
            }
        };
        self.sel_link = Some(next);
        let line = self.doc.links[next].line;
        if line != usize::MAX {
            self.ensure_visible(line);
        }
        self.flash(self.doc.links[next].url.clone());
    }

    fn open_link(&mut self, id: usize) {
        let Some(info) = self.doc.links.get(id) else {
            return;
        };
        let url = info.url.clone();
        if let Some(anchor) = url.strip_prefix('#') {
            self.jump_anchor(anchor);
            return;
        }
        if url.contains("://") || url.starts_with("mailto:") {
            self.xdg_open(&url);
            return;
        }
        let (file, anchor) = match url.split_once('#') {
            Some((f, a)) => (f, Some(a.to_string())),
            None => (url.as_str(), None),
        };
        let base = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let target = base.join(file.replace("%20", " "));
        let is_md = target
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown" | "mdown" | "mkd"));
        if is_md && target.is_file() {
            self.navigate(&target, anchor.as_deref());
        } else if target.exists() {
            self.xdg_open(&target.to_string_lossy());
        } else {
            self.flash(format!("not found: {file}"));
        }
    }

    fn xdg_open(&mut self, target: &str) {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        match Command::new(opener)
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_) => self.flash(format!("opening {target}")),
            Err(e) => self.flash(format!("{opener} failed: {e}")),
        }
    }

    // ----- search --------------------------------------------------------

    pub fn query_len(&self) -> usize {
        self.query.chars().count()
    }

    pub fn recompute_matches(&mut self) {
        self.matches.clear();
        self.cur_match = None;
        if self.query.is_empty() {
            return;
        }
        let case_sensitive = self.query.chars().any(char::is_uppercase);
        let fold = |c: char| {
            if case_sensitive {
                c
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        };
        let q: Vec<char> = self.query.chars().map(fold).collect();
        for (i, line) in self.doc.plain.iter().enumerate() {
            let chars: Vec<char> = line.chars().map(fold).collect();
            let mut j = 0;
            while j + q.len() <= chars.len() {
                if chars[j..j + q.len()] == q[..] {
                    self.matches.push((i, j));
                    j += q.len();
                } else {
                    j += 1;
                }
            }
        }
    }

    fn goto_first_match_from(&mut self, from_line: usize) {
        if self.matches.is_empty() {
            self.cur_match = None;
            return;
        }
        let idx = self.matches.partition_point(|m| m.0 < from_line);
        let idx = if idx >= self.matches.len() { 0 } else { idx };
        self.goto_match(idx);
    }

    fn goto_match(&mut self, idx: usize) {
        self.cur_match = Some(idx);
        let line = self.matches[idx].0;
        self.ensure_visible(line);
    }

    fn step_match(&mut self, forward: bool) {
        if self.matches.is_empty() {
            if !self.query.is_empty() {
                self.flash(format!("no match for \"{}\"", self.query));
            }
            return;
        }
        let n = self.matches.len();
        let cur = self.cur_match.unwrap_or(if forward { n - 1 } else { 0 });
        let next = if forward { (cur + 1) % n } else { (cur + n - 1) % n };
        self.goto_match(next);
    }

    // ----- input ---------------------------------------------------------

    pub fn on_key(&mut self, mut key: KeyEvent) {
        // Terminals using the kitty keyboard protocol (Ghostty, kitty, ...) can
        // report Shift+t as `t` + SHIFT instead of `T`. Fold it to the latter.
        if let KeyCode::Char(c) = key.code
            && key.modifiers.contains(KeyModifiers::SHIFT)
            && c.is_ascii_lowercase()
        {
            key.code = KeyCode::Char(c.to_ascii_uppercase());
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.mode {
            Mode::Help => self.mode = Mode::Normal,
            Mode::Search => self.on_key_search(key),
            Mode::Picker => self.on_key_picker(key),
            Mode::Themes => self.on_key_themes(key),
            Mode::Normal if self.focus == Focus::Toc => self.on_key_toc(key),
            Mode::Normal => self.on_key_normal(key),
        }
    }

    fn on_key_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.query.clear();
                self.recompute_matches();
                self.scroll = self.search_origin;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                if self.query.pop().is_none() {
                    self.mode = Mode::Normal;
                } else {
                    self.search_changed();
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.clear();
                self.search_changed();
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.search_changed();
            }
            _ => {}
        }
    }

    fn search_changed(&mut self) {
        self.recompute_matches();
        self.goto_first_match_from(self.search_origin);
    }

    fn on_key_picker(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let count = self.picker.filtered().len();
        match key.code {
            KeyCode::Esc => {
                if self.picker.filter.is_empty() {
                    if self.path.is_some() {
                        self.mode = Mode::Normal;
                    } else {
                        self.quit = true;
                    }
                } else {
                    self.picker.filter.clear();
                    self.picker.sel = 0;
                }
            }
            KeyCode::Down => self.picker.sel = (self.picker.sel + 1).min(count.saturating_sub(1)),
            KeyCode::Up => self.picker.sel = self.picker.sel.saturating_sub(1),
            KeyCode::Char('n') if ctrl => {
                self.picker.sel = (self.picker.sel + 1).min(count.saturating_sub(1))
            }
            KeyCode::Char('p') if ctrl => self.picker.sel = self.picker.sel.saturating_sub(1),
            KeyCode::PageDown => self.picker.sel = (self.picker.sel + 10).min(count.saturating_sub(1)),
            KeyCode::PageUp => self.picker.sel = self.picker.sel.saturating_sub(10),
            KeyCode::Backspace => {
                self.picker.filter.pop();
                self.picker.sel = 0;
            }
            KeyCode::Enter => {
                let f = self.picker.filtered();
                if let Some(&i) = f.get(self.picker.sel) {
                    let path = self.picker.files[i].clone();
                    self.history.clear();
                    if let Err(e) = self.load_file(&path) {
                        self.flash(format!("cannot open: {e}"));
                    }
                }
            }
            KeyCode::Char(c) if !ctrl => {
                self.picker.filter.push(c);
                self.picker.sel = 0;
            }
            _ => {}
        }
    }

    pub fn set_theme(&mut self, name: ThemeName) {
        self.theme_name = name;
        self.theme = Theme::from_name(name);
        self.relayout_now();
    }

    fn on_key_themes(&mut self, key: KeyEvent) {
        let all = ThemeName::all();
        let cur = all.iter().position(|&n| n == self.theme_name).unwrap_or(0);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let next = match key.code {
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => Some((cur + 1) % all.len()),
            KeyCode::Char('n') if ctrl => Some((cur + 1) % all.len()),
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => Some((cur + all.len() - 1) % all.len()),
            KeyCode::Char('p') if ctrl => Some((cur + all.len() - 1) % all.len()),
            KeyCode::Char('g') | KeyCode::Home => Some(0),
            KeyCode::Char('G') | KeyCode::End => Some(all.len() - 1),
            KeyCode::PageDown => Some((cur + 5).min(all.len() - 1)),
            KeyCode::PageUp => Some(cur.saturating_sub(5)),
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                let title = self.theme.title;
                match theme::save(self.theme_name) {
                    Ok(()) => self.flash(format!("theme: {title} (saved)")),
                    Err(e) => self.flash(format!("theme: {title} (not saved: {e})")),
                }
                None
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('T') => {
                self.set_theme(self.theme_origin);
                self.mode = Mode::Normal;
                None
            }
            _ => None,
        };
        if let Some(i) = next {
            self.set_theme(all[i]);
        }
    }

    fn on_key_toc(&mut self, key: KeyEvent) {
        let n = self.doc.headings.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.toc_sel = (self.toc_sel + 1).min(n.saturating_sub(1)),
            KeyCode::Char('k') | KeyCode::Up => self.toc_sel = self.toc_sel.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.toc_sel = 0,
            KeyCode::Char('G') | KeyCode::End => self.toc_sel = n.saturating_sub(1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                if let Some(h) = self.doc.headings.get(self.toc_sel) {
                    let l = h.line;
                    self.scroll_to(l);
                }
                self.focus = Focus::Doc;
            }
            KeyCode::Esc | KeyCode::Tab | KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Doc,
            KeyCode::Char('t') => {
                self.show_toc = false;
                self.focus = Focus::Doc;
            }
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
    }

    fn on_key_normal(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let page = self.view_h.saturating_sub(2).max(1) as isize;
        let half = (self.view_h / 2).max(1) as isize;
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                self.query.clear();
                self.recompute_matches();
                self.sel_link = None;
            }
            KeyCode::Char('j') | KeyCode::Down => self.scroll_by(1),
            KeyCode::Char('n') if ctrl => self.scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_by(-1),
            KeyCode::Char('p') if ctrl => self.scroll_by(-1),
            KeyCode::Char('d') => self.scroll_by(half),
            KeyCode::Char('u') => self.scroll_by(-half),
            KeyCode::Char(' ') if shift => self.scroll_by(-page),
            KeyCode::Char(' ') | KeyCode::PageDown => self.scroll_by(page),
            KeyCode::Char('f') if ctrl => self.scroll_by(page),
            KeyCode::Char('b') | KeyCode::PageUp => self.scroll_by(-page),
            KeyCode::Char('g') | KeyCode::Home => self.scroll_to(0),
            KeyCode::Char('G') | KeyCode::End => self.scroll_to(usize::MAX),
            KeyCode::Char(']') | KeyCode::Char('}') => self.next_heading(),
            KeyCode::Char('[') | KeyCode::Char('{') => self.prev_heading(),
            KeyCode::Char('/') => {
                self.search_origin = self.scroll;
                self.query.clear();
                self.recompute_matches();
                self.mode = Mode::Search;
            }
            KeyCode::Char('n') => self.step_match(true),
            KeyCode::Char('N') => self.step_match(false),
            KeyCode::Tab => self.step_link(true),
            KeyCode::BackTab => self.step_link(false),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => match self.sel_link {
                Some(id) => self.open_link(id),
                None if key.code == KeyCode::Enter => self.scroll_by(1),
                None => {}
            },
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => self.go_back(),
            KeyCode::Char('t') => {
                if self.doc.headings.is_empty() {
                    self.flash("no headings in this document");
                } else if !self.show_toc {
                    self.show_toc = true;
                    self.focus = Focus::Toc;
                    self.toc_sel = self.current_heading().unwrap_or(0);
                } else {
                    self.focus = Focus::Toc;
                    self.toc_sel = self.current_heading().unwrap_or(0);
                }
            }
            KeyCode::Char('T') => {
                self.theme_origin = self.theme_name;
                self.mode = Mode::Themes;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.max_width = (self.max_width + 10).min(240);
                self.flash(format!("width {}", self.max_width));
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                self.max_width = self.max_width.saturating_sub(10).max(40);
                self.flash(format!("width {}", self.max_width));
            }
            KeyCode::Char('r') => {
                self.relayout_now();
                self.reload();
            }
            KeyCode::Char('e') => match self.path.clone() {
                Some(p) => self.edit_request = Some(p),
                None => self.flash("nothing to edit (reading from stdin)"),
            },
            KeyCode::Char('o') if !self.picker.files.is_empty() => self.mode = Mode::Picker,
            KeyCode::Char('m') => {
                self.mouse = !self.mouse;
                self.flash(if self.mouse {
                    "mouse on: wheel + click links (hold Shift to select text)"
                } else {
                    "mouse off: select and copy text with the terminal"
                });
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        if self.mode != Mode::Normal {
            return;
        }
        match m.kind {
            MouseEventKind::ScrollDown => self.scroll_by(3),
            MouseEventKind::ScrollUp => self.scroll_by(-3),
            MouseEventKind::Down(MouseButton::Left) => self.click(m.column, m.row),
            _ => {}
        }
    }

    fn click(&mut self, col: u16, row: u16) {
        let tr = self.toc_rect;
        if tr.width > 0 && col < tr.right() && row < tr.bottom() {
            let h = usize::from(tr.height);
            let sel = match self.focus {
                Focus::Toc => self.toc_sel,
                Focus::Doc => self.current_heading().unwrap_or(0),
            };
            let offset = toc_offset(sel, h, self.doc.headings.len());
            let idx = offset + usize::from(row - tr.y);
            if let Some(hd) = self.doc.headings.get(idx) {
                let l = hd.line;
                self.scroll_to(l);
                self.toc_sel = idx;
            }
            return;
        }
        let r = self.text_rect;
        if col < r.x || col >= r.right() || row >= r.bottom() {
            return;
        }
        let line = self.scroll + usize::from(row - r.y);
        let Some(rl) = self.doc.lines.get(line) else {
            return;
        };
        let mut x = usize::from(r.x);
        for s in &rl.spans {
            let w = s.text.width();
            if usize::from(col) < x + w {
                if let Some(id) = s.link {
                    self.sel_link = Some(id);
                    self.open_link(id);
                }
                return;
            }
            x += w;
        }
    }
}

/// First visible TOC row so that `sel` stays roughly centred.
pub fn toc_offset(sel: usize, height: usize, total: usize) -> usize {
    if total <= height {
        return 0;
    }
    sel.saturating_sub(height / 2).min(total - height)
}
