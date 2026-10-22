use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{App, Focus, Mode, toc_offset},
    render::RLine,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.layout(area);

    if app.mode == Mode::Picker {
        draw_picker(f, app, area);
    } else {
        draw_reader(f, app, area);
    }
    draw_status(f, app, area);
    if app.mode == Mode::Help {
        draw_help(f, app, area);
    }
    if app.mode == Mode::Themes {
        draw_themes(f, app, area);
    }
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

fn draw_reader(f: &mut Frame, app: &App, area: Rect) {
    if app.toc_rect.width > 0 {
        draw_toc(f, app, app.toc_rect);
    }

    let total = app.doc.lines.len();
    let start = app.scroll.min(total);
    let end = (start + app.view_h).min(total);
    let lines: Vec<Line> = (start..end).map(|i| to_line(app, i)).collect();
    f.render_widget(Paragraph::new(lines), app.text_rect);

    if total > app.view_h {
        let bar_area = Rect::new(area.right().saturating_sub(1), area.y, 1, app.text_rect.height);
        let mut state = ScrollbarState::new(total.saturating_sub(app.view_h))
            .position(app.scroll)
            .viewport_content_length(app.view_h);
        let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
            .thumb_symbol("▐")
            .thumb_style(Style::default().fg(app.theme.rule));
        f.render_stateful_widget(bar, bar_area, &mut state);
    }
}

/// Convert one pre-rendered line to ratatui spans, overlaying the selected
/// link and search matches.
fn to_line(app: &App, idx: usize) -> Line<'static> {
    let rl: &RLine = &app.doc.lines[idx];
    let th = &app.theme;
    let link_style = Style::default().fg(th.sel_fg).bg(th.sel_bg);

    // Matches on this line as (start, end, is_current), in char offsets.
    let mut ranges: Vec<(usize, usize, bool)> = Vec::new();
    if !app.matches.is_empty() {
        let qlen = app.query_len();
        let first = app.matches.partition_point(|m| m.0 < idx);
        for (k, m) in app.matches.iter().enumerate().skip(first) {
            if m.0 != idx {
                break;
            }
            ranges.push((m.1, m.1 + qlen, app.cur_match == Some(k)));
        }
    }

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(rl.spans.len());
    let mut off = 0usize;
    for s in &rl.spans {
        let base = if s.link.is_some() && s.link == app.sel_link {
            s.style.patch(link_style)
        } else {
            s.style
        };
        if ranges.is_empty() {
            spans.push(Span::styled(s.text.clone(), base));
            continue;
        }
        let chars: Vec<char> = s.text.chars().collect();
        let n = chars.len();
        let mut pos = 0;
        while pos < n {
            let g = off + pos;
            match ranges.iter().find(|r| r.1 > g) {
                Some(&(rs, re, cur)) if rs <= g => {
                    let end = (re - off).min(n);
                    let bg = if cur { th.search_cur_bg } else { th.search_bg };
                    let st = base.fg(th.search_fg).bg(bg);
                    spans.push(Span::styled(chars[pos..end].iter().collect::<String>(), st));
                    pos = end;
                }
                Some(&(rs, _, _)) => {
                    let end = (rs - off).min(n);
                    spans.push(Span::styled(chars[pos..end].iter().collect::<String>(), base));
                    pos = end;
                }
                None => {
                    spans.push(Span::styled(chars[pos..].iter().collect::<String>(), base));
                    pos = n;
                }
            }
        }
        off += n;
    }
    Line::from(spans)
}

fn truncate(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw + 1 > max {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

fn draw_toc(f: &mut Frame, app: &App, rect: Rect) {
    let th = &app.theme;
    let focused = app.focus == Focus::Toc;
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(if focused { th.accent } else { th.rule }));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let h = usize::from(inner.height);
    let cur = app.current_heading();
    let sel = if focused { app.toc_sel } else { cur.unwrap_or(0) };
    let offset = toc_offset(sel, h, app.doc.headings.len());

    let min_level = app.doc.headings.iter().map(|h| h.level).min().unwrap_or(1);
    let lines: Vec<Line> = app
        .doc
        .headings
        .iter()
        .enumerate()
        .skip(offset)
        .take(h)
        .map(|(i, hd)| {
            let indent = usize::from(hd.level - min_level).min(4) * 2;
            let text = truncate(
                &format!(" {}{}", " ".repeat(indent), hd.title),
                usize::from(inner.width).saturating_sub(1),
            );
            let color = th.headings[usize::from(hd.level - 1).min(5)];
            let style = if focused && i == app.toc_sel {
                Style::default().fg(th.sel_fg).bg(th.sel_bg)
            } else if Some(i) == cur {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(th.dim)
            };
            Line::styled(text, style)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let bar = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
    let base = Style::default().fg(th.status_fg).bg(th.status_bg);
    f.render_widget(Block::default().style(base), bar);

    if app.mode == Mode::Search {
        let line = Line::from(vec![
            Span::styled(" / ", base.fg(th.accent).add_modifier(Modifier::BOLD)),
            Span::styled(app.query.clone(), base),
            Span::styled("█", base.fg(th.accent)),
            Span::styled(
                if app.query.is_empty() {
                    String::new()
                } else if app.matches.is_empty() {
                    "   no matches".to_string()
                } else {
                    format!("   {}/{}", app.cur_match.map_or(0, |c| c + 1), app.matches.len())
                },
                base.fg(th.dim),
            ),
        ]);
        f.render_widget(Paragraph::new(line), bar);
        return;
    }

    let mut right = String::new();
    if app.mode == Mode::Picker {
        right.push_str(" ↑↓ select · enter open · esc clear/quit ");
    } else if app.mode == Mode::Themes {
        right.push_str(" j/k preview · enter save · esc cancel ");
    } else {
        if !app.matches.is_empty() {
            right.push_str(&format!(
                " \"{}\" {}/{} ·",
                app.query,
                app.cur_match.map_or(0, |c| c + 1),
                app.matches.len()
            ));
        }
        right.push_str(&format!(
            " {}% · {}/{} · ? help ",
            app.progress(),
            (app.scroll + 1).min(app.doc.lines.len()),
            app.doc.lines.len()
        ));
    }

    let left_name = if app.mode == Mode::Picker {
        "arcanaeum".to_string()
    } else {
        app.name.clone()
    };
    let mut left = vec![
        Span::styled(
            " arcanaeum ",
            Style::default()
                .fg(th.sel_fg)
                .bg(th.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {left_name} "), base.add_modifier(Modifier::BOLD)),
    ];
    if let Some((m, _)) = &app.msg {
        left.push(Span::styled(format!(" {m}"), base.fg(th.warn)));
    }
    let used: usize = left.iter().map(|s| s.content.width()).sum();
    let rw = right.width();
    let pad = usize::from(area.width).saturating_sub(used + rw);
    left.push(Span::styled(" ".repeat(pad), base));
    left.push(Span::styled(right, base.fg(th.dim)));
    f.render_widget(Paragraph::new(Line::from(left)), bar);
}

// ---------------------------------------------------------------------------
// Picker + help
// ---------------------------------------------------------------------------

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn draw_picker(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    let rect = centered(body, 90, body.height.saturating_sub(2).max(5));
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.accent))
        .title(Span::styled(
            " Open markdown file ",
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let filtered = app.picker.filtered();
    let mut lines = vec![
        Line::from(vec![
            Span::styled("› ", Style::default().fg(th.accent)),
            Span::raw(app.picker.filter.clone()),
            Span::styled("█", Style::default().fg(th.accent)),
        ]),
        Line::styled("─".repeat(usize::from(inner.width)), Style::default().fg(th.rule)),
    ];
    let h = usize::from(inner.height).saturating_sub(2);
    let sel = app.picker.sel.min(filtered.len().saturating_sub(1));
    let offset = toc_offset(sel, h, filtered.len());
    if filtered.is_empty() {
        lines.push(Line::styled("  no matching files", Style::default().fg(th.dim)));
    }
    for (row, &fi) in filtered.iter().enumerate().skip(offset).take(h) {
        let text = truncate(
            &app.picker.display(&app.picker.files[fi]),
            usize::from(inner.width).saturating_sub(3),
        );
        let style = if row == sel {
            Style::default().fg(th.sel_fg).bg(th.sel_bg)
        } else {
            Style::default().fg(th.fg)
        };
        lines.push(Line::styled(format!(" {text}"), style));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    const KEYS: &[(&str, &str)] = &[
        ("j k ↓ ↑", "scroll line"),
        ("space b  PgDn PgUp", "scroll page"),
        ("d u", "half page down / up"),
        ("g G", "top / bottom"),
        ("] [", "next / previous heading"),
        ("t", "table of contents (j k, enter to jump)"),
        ("/  n N", "search, next / previous match"),
        ("tab  shift-tab", "select next / previous link"),
        ("enter l", "open selected link (.md opens in-app)"),
        ("h backspace", "go back"),
        ("o", "file picker"),
        ("+ -", "wider / narrower text column"),
        ("T", "theme picker (live preview, remembered)"),
        ("r", "reload file"),
        ("e", "edit in $EDITOR"),
        ("m", "mouse mode: wheel + click links (off = select text)"),
        ("q  ctrl-c", "quit"),
    ];
    let key_w = KEYS.iter().map(|k| k.0.width()).max().unwrap_or(0);
    let mut lines = vec![Line::raw("")];
    for (k, d) in KEYS {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {k:<key_w$}  "),
                Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{d}  "), Style::default().fg(th.fg)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(
        Line::styled("press any key to close", Style::default().fg(th.dim)).alignment(Alignment::Center),
    );

    let w = (key_w + 50) as u16;
    let rect = centered(area, w, lines.len() as u16 + 2);
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.accent))
        .title(Span::styled(
            " Keys ",
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_themes(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let all = crate::theme::ThemeName::all();
    let cur = all.iter().position(|&n| n == app.theme_name).unwrap_or(0);

    let body_h = area.height.saturating_sub(1);
    let h = (all.len() as u16 + 2).min(body_h);
    let w = 38u16.min(area.width);
    let rect = Rect::new(
        area.right().saturating_sub(w + 1),
        area.y + 1.min(body_h.saturating_sub(h)),
        w,
        h,
    );
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.accent))
        .title(Span::styled(
            " Theme ",
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " enter save · esc cancel ",
            Style::default().fg(th.dim),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let rows = usize::from(inner.height);
    let offset = toc_offset(cur, rows, all.len());
    let lines: Vec<Line> = all
        .iter()
        .enumerate()
        .skip(offset)
        .take(rows)
        .map(|(i, &n)| {
            let t = crate::theme::Theme::from_name(n);
            let selected = i == cur;
            let text_style = if selected {
                Style::default()
                    .fg(th.sel_fg)
                    .bg(th.sel_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(th.fg)
            };
            let sw = |c| Span::styled("█", Style::default().fg(c));
            let name_w = usize::from(inner.width).saturating_sub(8); // marker + 4 swatches + icon
            let mut name = truncate(t.title, name_w);
            name.push_str(&" ".repeat(name_w.saturating_sub(name.width())));
            Line::from(vec![
                Span::styled(if selected { " ▶ " } else { "   " }, text_style),
                Span::styled(name, text_style),
                sw(t.headings[0]),
                sw(t.accent),
                sw(t.link),
                sw(t.ok),
                Span::styled(if t.dark { "☾" } else { "☀" }, text_style),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}
