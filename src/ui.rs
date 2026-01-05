use crate::app::{App, AppMode, CleanupRow};
use crate::modal::Modal;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Paragraph},
    Frame,
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(f.area());

    // Title bar
    draw_title(f, app, chunks[0]);

    // Main content area
    match app.mode {
        AppMode::Cleanup => draw_cleanup(f, app, chunks[1]),
        _ => draw_browser(f, app, chunks[1]),
    }

    // Help/status bar
    draw_footer(f, chunks[2]);

    // Notification if present
    if let Some(notif) = &app.notification {
        draw_notification(f, notif);
    }

    // Modal overlay if present
    if let Some(modal) = &app.modal {
        draw_modal(f, modal);
    }

    // Progress bar if deleting
    if let Some(progress) = &app.delete_progress {
        draw_progress(f, progress);
    }

    // Cleanup delete progress
    if let Some(progress) = &app.cleanup_delete_progress {
        draw_cleanup_progress(f, progress);
    }

    // Loading overlay if scanning
    if app.is_scanning {
        draw_loading(f, app.scan_files_count, app.scanning_path.as_deref());
    }

    // Cleanup scanning overlay
    if app.cleanup_scanning {
        let count = app.cleanup_scan_progress.as_ref().map(|p| p.found_count as usize);
        draw_cleanup_loading(f, count);
    }

    // Help screen if shown
    if app.show_help {
        draw_help(f);
    }
}

fn draw_title(f: &mut Frame, app: &App, area: Rect) {
    let current_path = app.get_current_path();
    let title_text = format!(" 📊 mcdu v{} | {} ", env!("CARGO_PKG_VERSION"), current_path.display());

    let right_text = if app.is_scanning {
        format!("  ⟳ Scanning... {} files ", app.scan_files_count)
    } else {
        let mut info = format!("  {} items", app.entries_count());

        // Add disk space if available
        if let Some(ref disk) = app.disk_space {
            let avail = format_size(disk.available_bytes);
            let total = format_size(disk.total_bytes);
            let percent_used = (disk.used_bytes as f64 / disk.total_bytes as f64 * 100.0) as u8;
            info.push_str(&format!(" | 💾 {}/{} ({}%)", avail, total, percent_used));
        }

        format!("{} ", info)
    };

    // Layout for title bar
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(30),
            Constraint::Length(right_text.len() as u16 + 2),
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(title_text)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Left),
        chunks[0],
    );

    let right_style = if app.is_scanning {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    f.render_widget(
        Paragraph::new(right_text)
            .style(right_style)
            .alignment(Alignment::Right),
        chunks[1],
    );
}

fn draw_browser(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();

    // Path display
    let current_path = app.get_current_path();
    lines.push(Line::from(format!("Path: {}", current_path.display())));
    lines.push(Line::from("".to_string()));

    let entries = app.get_display_entries();

    // Calculate viewport bounds
    let viewport_height = area.height.saturating_sub(4) as usize;
    let start_idx = app.scroll_offset;
    let end_idx = (start_idx + viewport_height).min(entries.len());

    let total_size: u64 = entries
        .iter()
        .filter(|entry| entry.name != "..")
        .map(|entry| entry.size)
        .sum();

    // Directory entries - only render visible items
    for (idx, entry) in entries
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(end_idx - start_idx)
    {
        let is_selected = idx == app.selected_index;
        let size_str = format_size(entry.size);
        let percent_bar = if entry.size > 0 {
            create_bar(entry.size, 100_000_000_000) // 100GB as max
        } else {
            String::new()
        };
        let percent_of_total = if total_size > 0 && entry.name != ".." {
            (entry.size as f64 / total_size as f64) * 100.0
        } else {
            0.0
        };
        let percent_str = format!("{:>4.0}%", percent_of_total.round());

        let size_color = get_color_by_size(entry.size);
        let name_prefix = if entry.is_dir { "📁 " } else { "📄 " };

        let name_style = if is_selected {
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let size_style = Style::default().fg(size_color).add_modifier(Modifier::BOLD);

        let line_spans = vec![
            Span::styled(
                format!(
                    "{}{:<25}",
                    name_prefix,
                    &entry.name[..entry.name.len().min(25)]
                ),
                name_style,
            ),
            Span::styled(format!("{:>10}", size_str), size_style),
            Span::styled(
                format!("{:>6}", percent_str),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(format!("  {} ", percent_bar)),
        ];

        lines.push(Line::from(line_spans));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Directory Contents");

    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(area);

    // Left: navigation hints
    let nav_text = "[↑↓jk] Navigate  [Enter] Open  [h] Parent";
    f.render_widget(
        Paragraph::new(nav_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Left),
        chunks[0],
    );

    // Center: main actions
    let main_text = "[d] Delete  [r] Rescan  [R] Rescan all  [?] Help  [C] Cleanup";
    f.render_widget(
        Paragraph::new(main_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center),
        chunks[1],
    );

    // Right: quit
    let quit_text = "[q/Esc] Quit";
    f.render_widget(
        Paragraph::new(quit_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Right),
        chunks[2],
    );
}

fn draw_modal(f: &mut Frame, modal: &Modal) {
    let centered = centered_rect(60, 30, f.area());

    f.render_widget(Clear, centered);

    let title = modal.get_title();
    let message = modal.get_message();

    let mut button_spans = Vec::new();
    for (idx, (label, _)) in modal.buttons.iter().enumerate() {
        let button_style = if idx == modal.selected_button {
            Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        };

        button_spans.push(Span::styled(format!(" {} ", label), button_style));

        if idx < modal.buttons.len() - 1 {
            button_spans.push(Span::raw("  "));
        }
    }

    let content = vec![
        Line::from(message),
        Line::from(""),
        Line::from(button_spans),
    ];

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    f.render_widget(
        Paragraph::new(content)
            .block(block)
            .style(Style::default().bg(Color::Black))
            .alignment(Alignment::Center),
        centered,
    );
}

fn draw_cleanup_loading(f: &mut Frame, count: Option<usize>) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);
    let text = match count {
        Some(c) => format!("Scanning cleanup candidates... found {}", c),
        None => "Scanning cleanup candidates...".to_string(),
    };
    f.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow)),
        area,
    );
}

fn draw_cleanup(f: &mut Frame, app: &App, area: Rect) {
    let rows = app.cleanup_rows();
    let mut lines = Vec::new();
    lines.push(Line::from("Cleanup"));
    lines.push(Line::from(""));

    let viewport_height = area.height.saturating_sub(4) as usize;
    let start_idx = app.cleanup_selected_index.saturating_sub(viewport_height / 2);
    let end_idx = (start_idx + viewport_height).min(rows.len());

    for (idx, row) in rows.iter().enumerate().skip(start_idx).take(end_idx - start_idx) {
        let cursor = idx == app.cleanup_selected_index;
        match row {
            CleanupRow::Category { name } => {
                let cat = app
                    .cleanup_categories
                    .iter()
                    .find(|c| &c.name == name);
                let (selected_count, total_count, total_size) = match cat {
                    Some(c) => (
                        c.candidates
                            .iter()
                            .filter(|cand| app.cleanup_selected.contains(&cand.path))
                            .count(),
                        c.candidates.len(),
                        c.candidates.iter().map(|c| c.size_bytes).sum::<u64>(),
                    ),
                    None => (0, 0, 0),
                };
                let checkbox = if total_count == 0 {
                    "[ ]"
                } else if selected_count == total_count {
                    "[x]"
                } else if selected_count > 0 {
                    "[-]"
                } else {
                    "[ ]"
                };
                let expanded = app.cleanup_expanded.contains(name);
                let arrow = if expanded { "▾" } else { "▸" };
                let size_str = format_size(total_size);
                let mut spans = vec![Span::raw(format!("{} {} {}", arrow, checkbox, name))];
                spans.push(Span::styled(
                    format!(" {:>8}", size_str),
                    Style::default().fg(Color::Green),
                ));
                let line = Line::from(spans).style(if cursor {
                    Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                });
                lines.push(line);
            }
            CleanupRow::Candidate {
                path,
                rule,
                pattern,
                size,
            } => {
                let selected = app.cleanup_selected.contains(path);
                let mark = if selected { "[x]" } else { "[ ]" };
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("(unknown)");
                let size_str = format_size(*size);
                let mut spans = vec![Span::raw(format!("  {} {}", mark, name))];
                spans.push(Span::styled(
                    format!(" {:>8}", size_str),
                    Style::default().fg(Color::Green),
                ));
                spans.push(Span::styled(
                    format!("  ({})", rule),
                    Style::default().fg(Color::Gray),
                ));
                let line = Line::from(spans).style(if cursor {
                    Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                });
                lines.push(line);
                lines.push(Line::from(Span::styled(
                    format!("      matched pattern {}", pattern),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Cleanup (Space toggle, Enter expand, a all, n none, d delete, D dry-run, q back)");

    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_progress(f: &mut Frame, progress: &crate::app::DeleteProgress) {
    let centered = centered_rect(70, 40, f.area());

    f.render_widget(Clear, centered);

    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(centered);

    f.render_widget(
        Paragraph::new(format!("🗑️  {}", progress.status)).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
                .bg(Color::Black),
        ),
        inner_layout[0],
    );

    let ratio = if progress.total_bytes > 0 {
        progress.deleted_bytes as f64 / progress.total_bytes as f64
    } else {
        0.0
    };

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(ratio)
        .label(format!("{:.1}%", ratio * 100.0));

    f.render_widget(gauge, inner_layout[1]);

    let stats = format!(
        "Deleted: {} / {} ({} files)",
        format_size(progress.deleted_bytes),
        format_size(progress.total_bytes),
        progress.deleted_files
    );
    f.render_widget(
        Paragraph::new(stats).style(Style::default().bg(Color::Black)),
        inner_layout[2],
    );
}

fn draw_cleanup_progress(f: &mut Frame, progress: &crate::cleanup::executor::CleanupProgress) {
    let centered = centered_rect(70, 40, f.area());

    f.render_widget(Clear, centered);

    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(centered);

    let stage_label = match progress.stage {
        crate::cleanup::executor::CleanupStage::Files => "Files",
        crate::cleanup::executor::CleanupStage::Git => "Git",
    };
    f.render_widget(
        Paragraph::new(format!("Cleanup [{}]: {}", stage_label, progress.path.display())).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
                .bg(Color::Black),
        ),
        inner_layout[0],
    );

    let ratio = if progress.total > 0 {
        progress.current as f64 / progress.total as f64
    } else {
        0.0
    };
    f.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Cleanup Progress"))
            .gauge_style(Style::default().fg(Color::Green))
            .ratio(ratio),
        inner_layout[1],
    );

    f.render_widget(
        Paragraph::new(format!("Freed {} bytes", progress.freed_bytes))
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center),
        inner_layout[2],
    );
}

fn draw_loading(f: &mut Frame, files_scanned: usize, scanning_path: Option<&str>) {
    let centered = centered_rect(70, 25, f.area());

    f.render_widget(Clear, centered);

    let mut loading_text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("⟳ ", Style::default().fg(Color::Yellow)),
            Span::styled(
                "Scanning directory tree...",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    // Show file count
    loading_text.push(Line::from(vec![Span::styled(
        format!("{} files scanned", files_scanned),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));

    loading_text.push(Line::from(""));

    // Show current path being scanned
    if let Some(path) = scanning_path {
        let max_width = (f.area().width as usize).saturating_sub(10);
        let truncated = if path.len() > max_width {
            format!("...{}", &path[path.len().saturating_sub(max_width - 3)..])
        } else {
            path.to_string()
        };

        loading_text.push(Line::from(vec![Span::styled(
            truncated,
            Style::default().fg(Color::Cyan),
        )]));
    } else {
        loading_text.push(Line::from(vec![Span::styled(
            "Initializing...",
            Style::default().fg(Color::Gray),
        )]));
    }

    loading_text.push(Line::from(""));
    loading_text.push(Line::from(vec![Span::styled(
        "Please wait",
        Style::default().fg(Color::Gray),
    )]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Black));

    f.render_widget(
        Paragraph::new(loading_text)
            .block(block)
            .style(Style::default().bg(Color::Black))
            .alignment(Alignment::Center),
        centered,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.1} {}", size, UNITS[unit_idx])
}

fn get_color_by_size(size: u64) -> Color {
    match size {
        s if s > 100_000_000_000 => Color::Red,   // >100GB
        s if s > 10_000_000_000 => Color::Yellow, // >10GB
        s if s > 1_000_000_000 => Color::Cyan,    // >1GB
        _ => Color::Green,                        // <1GB
    }
}

fn create_bar(current: u64, max: u64) -> String {
    let ratio = (current as f64 / max as f64).clamp(0.0, 1.0);
    let filled = (ratio * 10.0) as usize;
    let empty = 10 - filled;
    format!("▓{}{}", "▓".repeat(filled), "░".repeat(empty))
}

fn draw_notification(f: &mut Frame, notif: &str) {
    let centered = centered_rect(60, 10, f.area());

    f.render_widget(Clear, centered);

    let notification_widget = Paragraph::new(notif)
        .block(Block::default().borders(Borders::ALL).title("Notification"))
        .alignment(Alignment::Center)
        .style(if notif.contains('✓') {
            Style::default().fg(Color::Green).bg(Color::Black)
        } else if notif.contains('✗') {
            Style::default().fg(Color::Red).bg(Color::Black)
        } else {
            Style::default().fg(Color::Cyan).bg(Color::Black)
        });

    f.render_widget(notification_widget, centered);
}

pub fn draw_help(f: &mut Frame) {
    let centered = centered_rect(80, 90, f.area());

    f.render_widget(Clear, centered);

    let help_text = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "NAVIGATION",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  ↑ / k               Move cursor up"),
        Line::from("  ↓ / j               Move cursor down"),
        Line::from("  Enter / → / l       Enter directory"),
        Line::from("  Backspace / ← / h   Go to parent directory"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "DELETION",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]),
        Line::from("  d                   Delete selected file/directory"),
        Line::from("  y / n / d           Quick confirm in modals (yes/no/dry-run)"),
        Line::from("  ← / →               Navigate modal buttons (arrow keys)"),
        Line::from("  Enter               Confirm selected button"),
        Line::from("  Esc                 Close modal or quit"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "GENERAL",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  r                   Rescan selected directory"),
        Line::from("  R / c               Rescan entire tree"),
        Line::from("  ?                   Show this help screen"),
        Line::from("  q / Esc             Quit application"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "COLOR LEGEND",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled("  ", Style::default().bg(Color::Red)),
            Span::raw("  Red: >100 GB"),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default().bg(Color::Yellow)),
            Span::raw("  Yellow: 10-100 GB"),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default().bg(Color::Cyan)),
            Span::raw("  Cyan: 1-10 GB"),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default().bg(Color::Green)),
            Span::raw("  Green: <1 GB"),
        ]),
        Line::from(""),
        Line::from("Logs are saved to: ~/.mcdu/logs/"),
        Line::from(""),
        Line::from("Press any key to close this help screen..."),
    ];

    let help_widget = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(format!(" 🎯 HELP - mcdu v{} ", env!("CARGO_PKG_VERSION")))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().bg(Color::Black))
        .alignment(Alignment::Left);

    f.render_widget(help_widget, centered);
}
