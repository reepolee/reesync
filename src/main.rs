mod diff;
mod sync;
mod tree;

use std::io;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use tree::{DisplayItem, TreeNode};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

fn executable_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("Failed to determine executable path")?;
    exe.parent()
        .map(PathBuf::from)
        .context("Executable path has no parent directory")
}

fn main() -> Result<()> {
    // ── Parse CLI args ──────────────────────────────────
    if std::env::args().any(|a| a == "--where") {
        println!("{}", executable_dir()?.display());
        return Ok(());
    }
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let template_dir = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("Usage: reesync <TEMPLATE_DIR>"))?;

    let template_path = std::path::Path::new(&template_dir);
    if !template_path.is_dir() {
        return Err(anyhow!("Template directory not found: {}", template_dir));
    }

    let project_path = std::env::current_dir().context("Failed to get current directory")?;

    // ── Diff the directories ────────────────────────────
    println!("→ Walking directories and comparing files...");
    let mut entries = diff::diff_directories(&project_path, template_path)?;

    if entries.is_empty() {
        println!("  No differences found. Template is in sync with project.");
        return Ok(());
    }

    println!("  Found {} differing files", entries.len());

    // ── Enrich with git commit info ────────────────────
    println!("→ Fetching commit info from template...");
    diff::enrich_commit_info(template_path, &mut entries);

    // ── Build the tree ─────────────────────────────────
    let root = tree::build_tree(&entries);
    let total_files = root.file_count();

    // ── Start the TUI ──────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_tui(&mut terminal, root, total_files, &template_dir);

    // ── Restore terminal ────────────────────────────────
    disable_raw_mode()?;
    crossterm::execute!(io::stdout(), LeaveAlternateScreen)?;

    let checked_paths = match result {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Error: {}", e);
            return Ok(());
        }
    };

    // ── Sync ────────────────────────────────────────────
    if checked_paths.is_empty() {
        println!("  No files selected. Nothing to sync.");
        return Ok(());
    }

    println!("\n→ Syncing {} file(s) from template...", checked_paths.len());
    let synced = sync::sync_files(&project_path, template_path, &checked_paths)
        .context("Failed to sync files")?;

    println!("\n✅ Done! Synced {} file(s) from template.", synced);

    Ok(())
}

/// Run the TUI loop, returning the list of checked file paths.
fn run_tui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut root: TreeNode,
    total_files: usize,
    template_dir: &str,
) -> Result<Vec<PathBuf>> {
    let mut list_state = ListState::default();
    let mut items: Vec<DisplayItem> = Vec::new();
    root.flatten(0, &mut items);
    if !items.is_empty() {
        list_state.select(Some(0));
    }

    loop {
        // Render
        terminal.draw(|frame| {
            let area = frame.area();
            render_ui(frame, area, &items, &mut list_state, total_files, template_dir);
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return Ok(Vec::new());
                    }
                    KeyCode::Enter => {
                        let mut paths = Vec::new();
                        root.collect_checked_paths(&mut paths);
                        return Ok(paths);
                    }
                    KeyCode::Up => {
                        let selected = list_state.selected().unwrap_or(0);
                        if selected > 0 {
                            list_state.select(Some(selected - 1));
                        }
                    }
                    KeyCode::Down => {
                        let selected = list_state.selected().unwrap_or(0);
                        if selected + 1 < items.len() {
                            list_state.select(Some(selected + 1));
                        }
                    }
                    KeyCode::Right => {
                        if let Some(selected) = list_state.selected() {
                            if selected < items.len() {
                                let path = items[selected].path.clone();
                                if path != PathBuf::from("/") {
                                    root.toggle_expand_at(&path);
                                    items.clear();
                                    root.flatten(0, &mut items);
                                }
                            }
                        }
                    }
                    KeyCode::Left => {
                        if let Some(selected) = list_state.selected() {
                            if selected < items.len() {
                                let path = items[selected].path.clone();
                                if path != PathBuf::from("/") {
                                    root.toggle_expand_at(&path);
                                    items.clear();
                                    root.flatten(0, &mut items);
                                }
                            }
                        }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(selected) = list_state.selected() {
                            if selected < items.len() {
                                let path = items[selected].path.clone();
                                if path != PathBuf::from("/") {
                                    root.toggle_at(&path);
                                    items.clear();
                                    root.flatten(0, &mut items);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Count checked files across all display items.
fn count_checked(items: &[DisplayItem]) -> usize {
    items.iter().filter(|i| !i.is_folder && i.checked).count()
}

/// Render the TUI.
fn render_ui(
    frame: &mut Frame,
    area: Rect,
    items: &[DisplayItem],
    list_state: &mut ListState,
    total_files: usize,
    template_dir: &str,
) {
    // ── Layout ──────────────────────────────────────────
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);

    // ── Header ──────────────────────────────────────────
    let checked_count = count_checked(items);
    let header_text = format!(
        " reesync — Template: {}  |  {}/{} files selected",
        template_dir, checked_count, total_files
    );
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Cyan).bold())
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, chunks[0]);

    // ── Tree list ───────────────────────────────────────
    let list_items: Vec<ListItem> = items
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.depth);
            let icon = if item.is_folder {
                if item.is_expanded { "📂" } else { "📁" }
            } else {
                "📄"
            };
            let expand = if item.is_folder {
                if item.is_expanded { "[-] " } else { "[+] " }
            } else {
                ""
            };

            let checkbox = if item.is_folder {
                if item.checked_count == 0 {
                    "[ ]".to_string()
                } else if item.checked_count < item.total_count {
                    "[~]".to_string()
                } else {
                    "[x]".to_string()
                }
            } else {
                match item.state {
                    diff::FileState::Deleted => "[─]".to_string(),
                    _ if item.checked => "[x]".to_string(),
                    _ => "[ ]".to_string(),
                }
            };

            let state_suffix = match item.state {
                diff::FileState::Deleted => "  (deleted)".to_string(),
                _ => String::new(),
            };

            let commit_suffix = match &item.commit_info {
                Some(info) if !item.is_folder => format!("  {}", info),
                _ => String::new(),
            };

            let style = match item.state {
                diff::FileState::Deleted => Style::default().fg(Color::DarkGray),
                _ => Style::default(),
            };

            let text = format!(
                "{} {} {} {}{}{}{}",
                indent, checkbox, icon, expand, item.label, commit_suffix, state_suffix
            );

            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title(" Files "))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, chunks[1], list_state);

    // ── Status bar ──────────────────────────────────────
    let checked_count = count_checked(items);
    let status_text = format!(
        " {}  │  ↑↓ nav  space toggle  → expand  ← collapse  enter sync  q quit",
        if checked_count == 0 {
            "No files selected".to_string()
        } else {
            format!("{} files selected", checked_count)
        }
    );
    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Yellow).bg(Color::Black));
    frame.render_widget(status_bar, chunks[2]);
}
