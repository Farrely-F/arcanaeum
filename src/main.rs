mod app;
mod render;
mod theme;
mod ui;

use std::{
    io::{self, IsTerminal, Read, stdout},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
};
use notify::{EventKind, RecursiveMode, Watcher};
use ratatui::DefaultTerminal;

use app::App;
use theme::ThemeName;

/// A fast, comfortable markdown reader for the terminal.
#[derive(Parser)]
#[command(name = "arca", version, about)]
struct Cli {
    /// Markdown file to read, a directory to browse, or `-` for stdin.
    /// Without an argument the current directory is browsed.
    file: Option<PathBuf>,

    /// Colour theme. Overrides the one saved from the in-app picker (press `T`)
    #[arg(short, long, value_enum, env = "ARCANAEUM_THEME")]
    theme: Option<ThemeName>,

    /// Maximum width of the text column
    #[arg(short, long, env = "ARCANAEUM_WIDTH", default_value_t = 100)]
    width: usize,

    /// Start with the table of contents open
    #[arg(long)]
    toc: bool,

    /// Let arcanaeum handle the mouse (wheel, click links/TOC) from the start.
    /// Off by default so you can select and copy text; toggle with `m`.
    #[arg(long)]
    mouse: bool,

    /// Don't reload automatically when the file changes
    #[arg(long)]
    no_watch: bool,
}

fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown" | "mdown" | "mkd"))
}

fn scan_markdown(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > 4 || out.len() >= 2000 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                if !name.starts_with('.') && !matches!(&*name, "node_modules" | "target" | "vendor" | "dist")
                {
                    walk(&path, depth + 1, out);
                }
            } else if is_markdown(&path) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, 0, &mut out);
    out.sort_by_key(|p| {
        let rel = p.strip_prefix(root).unwrap_or(p);
        (rel.components().count(), rel.to_path_buf())
    });
    out
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let theme = cli
        .theme
        .or_else(theme::load_saved)
        .unwrap_or(theme::DEFAULT_THEME);
    let mut app = App::new(theme, cli.width, cli.toc);
    app.mouse = cli.mouse;

    let piped = !io::stdin().is_terminal();
    match cli.file.as_deref() {
        Some(p) if p == Path::new("-") => load_stdin(&mut app)?,
        None if piped => load_stdin(&mut app)?,
        Some(p) if p.is_file() => {
            app.load_file(p)
                .with_context(|| format!("cannot read {}", p.display()))?;
            if let Some(dir) = p.parent() {
                let dir = if dir.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    dir
                };
                app.picker.root = dir.to_path_buf();
            }
        }
        other => {
            let root = other.map_or_else(|| PathBuf::from("."), Path::to_path_buf);
            if !root.is_dir() {
                bail!("{}: no such file or directory", root.display());
            }
            let files = scan_markdown(&root);
            if files.is_empty() {
                bail!("no markdown files found in {}", root.display());
            }
            app.open_picker(root, files);
        }
    }

    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let mut watcher = if cli.no_watch {
        None
    } else {
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res
                && matches!(ev.kind, EventKind::Create(_) | EventKind::Modify(_))
            {
                let _ = tx.send(ev.paths);
            }
        })
        .ok()
    };

    let mut terminal = ratatui::init();
    // Alternate-scroll: lets the terminal turn the wheel into arrow keys while
    // we don't own the mouse, so scrolling still works and selection stays native.
    print!("\x1b[?1007h");
    let result = run(&mut terminal, &mut app, &rx, &mut watcher);
    let _ = execute!(stdout(), DisableMouseCapture);
    print!("\x1b[?1007l");
    ratatui::restore();
    result
}

fn load_stdin(app: &mut App) -> Result<()> {
    let mut buf = Vec::new();
    io::stdin().read_to_end(&mut buf)?;
    app.set_source("stdin".into(), String::from_utf8_lossy(&buf).into_owned());
    Ok(())
}

fn watch_dir(path: &Path) -> PathBuf {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    abs.parent().map(Path::to_path_buf).unwrap_or(abs)
}

fn run(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &mpsc::Receiver<Vec<PathBuf>>,
    watcher: &mut Option<notify::RecommendedWatcher>,
) -> Result<()> {
    let mut watched: Option<PathBuf> = None;
    let mut dirty = true;
    let mut mouse_on = false;

    loop {
        if app.mouse != mouse_on {
            mouse_on = app.mouse;
            if mouse_on {
                execute!(stdout(), EnableMouseCapture)?;
            } else {
                execute!(stdout(), DisableMouseCapture)?;
            }
        }

        // Keep the watcher pointed at the directory of the file being read.
        let want = app.path.as_deref().map(watch_dir);
        if want != watched {
            if let Some(w) = watcher.as_mut() {
                if let Some(old) = &watched {
                    let _ = w.unwatch(old);
                }
                if let Some(new) = &want {
                    let _ = w.watch(new, RecursiveMode::NonRecursive);
                }
            }
            watched = want;
        }

        if dirty {
            terminal.draw(|f| ui::draw(f, app))?;
            dirty = false;
        }
        if app.quit {
            return Ok(());
        }

        if event::poll(Duration::from_millis(250))? {
            loop {
                match event::read()? {
                    Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                    Event::Mouse(m) => app.on_mouse(m),
                    _ => {}
                }
                dirty = true;
                if app.quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        if let Some(path) = app.edit_request.take() {
            edit_externally(terminal, &path, mouse_on)?;
            app.reload();
            dirty = true;
        }

        let mut changed = false;
        while let Ok(paths) = rx.try_recv() {
            if let Some(cur) = app.path.as_ref().and_then(|p| p.file_name()) {
                changed |= paths.iter().any(|p| p.file_name() == Some(cur));
            }
        }
        if changed {
            app.reload();
            dirty = true;
        }
        dirty |= app.tick();
    }
}

fn edit_externally(terminal: &mut DefaultTerminal, path: &Path, mouse_on: bool) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "nano".into());
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    let mut parts = editor.split_whitespace();
    if let Some(bin) = parts.next() {
        let _ = Command::new(bin).args(parts).arg(path).status();
    }
    *terminal = ratatui::init();
    if mouse_on {
        execute!(stdout(), EnableMouseCapture)?;
    }
    Ok(())
}
