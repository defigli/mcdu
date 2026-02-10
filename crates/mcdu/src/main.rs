use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use mcdu_tui::{app::App, app::AppMode, modal, ui};
use ratatui::prelude::*;
use ratatui::Terminal;
use std::error::Error;
use std::io;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// Optional path to start in the TUI
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Developer cleanup utilities
    Cleanup(CleanupCommand),
}

#[derive(Parser)]
pub struct CleanupCommand {
    /// Path to scan for cleanup candidates
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let (cleanup_mode, start_path) = match cli.command {
        Some(Commands::Cleanup(ref cmd)) => (true, validate_start_path(cmd.path.clone())?),
        _ => (false, validate_start_path(cli.path)?),
    };

    // Setup terminal
    enable_raw_mode()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    terminal.hide_cursor()?;

    // Run app
    let app = match (&start_path, cleanup_mode) {
        (_, true) => {
            let mut a = App::new();
            let _ = a.start_cleanup_scan_with_path(start_path);
            a
        }
        (Some(path), false) => App::new_with_root(path.clone()),
        (None, false) => App::new(),
    };

    let result = run_app(&mut terminal, app);

    // Cleanup terminal - always restore state even on error
    let _ = terminal.show_cursor();
    let _ = terminal.clear();
    disable_raw_mode()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

fn validate_start_path(path: Option<PathBuf>) -> Result<Option<PathBuf>, Box<dyn Error>> {
    if let Some(path) = path {
        if !path.exists() {
            return Err(format!("Path does not exist: {}", path.display()).into());
        }
        if !path.is_dir() {
            return Err(format!("Path is not a directory: {}", path.display()).into());
        }
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn validate_start_path_accepts_existing_dir() {
        let tmp = tempdir().unwrap();
        let result = validate_start_path(Some(tmp.path().to_path_buf()));
        assert!(result.is_ok());
    }

    #[test]
    fn validate_start_path_rejects_missing() {
        let missing = PathBuf::from("/path/does/not/exist");
        let result = validate_start_path(Some(missing));
        assert!(result.is_err());
    }
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<(), Box<dyn Error>> {
    loop {
        terminal.draw(|f| {
            ui::draw(f, &mut app);
        })?;

        if crossterm::event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if handle_input(&mut app, key)? {
                    break;
                }
            }
        }

        // Update scan progress if thread is running
        app.update_scan_progress();

        // Update delete progress if thread is running
        app.update_delete_progress();

        // Update cleanup scan/delete progress
        app.update_cleanup_scan();
        app.update_cleanup_delete();

        // Clear notification after 3 seconds
        if let Some(notif_time) = app.notification_time {
            if notif_time.elapsed().as_secs() > 3 {
                app.notification = None;
                app.notification_time = None;
            }
        }
    }

    Ok(())
}

fn handle_input(app: &mut App, key: KeyEvent) -> Result<bool, Box<dyn Error>> {
    // If help is shown, any key closes it
    if app.show_help {
        app.show_help = false;
        return Ok(false);
    }

    // If modal is open, handle modal input
    if app.modal.is_some() {
        return handle_modal_input(app, key);
    }

    if matches!(app.mode, AppMode::Cleanup) {
        return handle_cleanup_input(app, key);
    }

    // Normal file browser input
    match key.code {
        KeyCode::Char('q') => return Ok(true), // 'q' to quit
        KeyCode::Esc => {
            // Esc key closes modals if open, otherwise quits
            if app.modal.is_some() {
                app.modal = None;
            } else {
                return Ok(true);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.enter_directory(),
        KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => app.go_parent(),
        KeyCode::Char('d') => app.open_delete_modal(),
        KeyCode::Char('?') => app.toggle_help(),
        KeyCode::Char('r') => app.rescan_selected(), // Rescan selected directory
        KeyCode::Char('R') | KeyCode::Char('c') => app.refresh(), // Rescan full tree
        KeyCode::Char('C') => {
            let _ = app.start_cleanup_scan();
        }
        _ => {}
    }

    Ok(false)
}

fn handle_cleanup_input(app: &mut App, key: KeyEvent) -> Result<bool, Box<dyn Error>> {
    use mcdu_tui::cleanup_ui::CleanupTab;

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.mode = AppMode::Browsing;
        }
        KeyCode::Tab => {
            app.cleanup_active_tab = match app.cleanup_active_tab {
                CleanupTab::Overview => CleanupTab::Categories,
                CleanupTab::Categories => CleanupTab::Files,
                CleanupTab::Files => CleanupTab::Quarantine,
                CleanupTab::Quarantine => CleanupTab::Overview,
            };
        }
        KeyCode::BackTab => {
            app.cleanup_active_tab = match app.cleanup_active_tab {
                CleanupTab::Overview => CleanupTab::Quarantine,
                CleanupTab::Categories => CleanupTab::Overview,
                CleanupTab::Files => CleanupTab::Categories,
                CleanupTab::Quarantine => CleanupTab::Files,
            };
        }
        KeyCode::Char('1') => app.cleanup_active_tab = CleanupTab::Overview,
        KeyCode::Char('2') => app.cleanup_active_tab = CleanupTab::Categories,
        KeyCode::Char('3') => app.cleanup_active_tab = CleanupTab::Files,
        KeyCode::Char('4') => app.cleanup_active_tab = CleanupTab::Quarantine,
        KeyCode::Up | KeyCode::Char('k') => app.select_previous_cleanup(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next_cleanup(),
        KeyCode::Char(' ') => app.toggle_cleanup_selection(),
        KeyCode::Enter => app.toggle_cleanup_expand(),
        KeyCode::Char('a') => app.select_all_cleanup(),
        KeyCode::Char('n') => app.select_none_cleanup(),
        KeyCode::Char('d') => app.start_cleanup_delete(),
        KeyCode::Char('D') => app.start_cleanup_dry_run(),
        KeyCode::Char('C') => {
            let _ = app.start_cleanup_scan();
        }
        _ => {}
    }

    Ok(false)
}

fn handle_modal_input(app: &mut App, key: KeyEvent) -> Result<bool, Box<dyn Error>> {
    if let Some(modal_ref) = &mut app.modal {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                if modal_ref.selected_button > 0 {
                    modal_ref.selected_button -= 1;
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if modal_ref.selected_button < modal_ref.buttons.len() - 1 {
                    modal_ref.selected_button += 1;
                }
            }
            KeyCode::Tab => {
                modal_ref.selected_button =
                    (modal_ref.selected_button + 1) % modal_ref.buttons.len();
            }
            KeyCode::BackTab => {
                if modal_ref.selected_button > 0 {
                    modal_ref.selected_button -= 1;
                } else {
                    modal_ref.selected_button = modal_ref.buttons.len() - 1;
                }
            }
            KeyCode::Enter => {
                let action = modal_ref.buttons[modal_ref.selected_button].1.clone();
                return handle_modal_action(app, action);
            }
            KeyCode::Esc => {
                app.modal = None;
            }
            KeyCode::Char('y')
                if modal_ref.has_button("Yes") || modal_ref.has_button("YES, CLEANUP") =>
            {
                return handle_modal_action(app, modal::ModalAction::Confirm);
            }
            KeyCode::Char('n') if modal_ref.has_button("No") => {
                return handle_modal_action(app, modal::ModalAction::Cancel);
            }
            KeyCode::Char('d') if modal_ref.has_button("Dry-run") => {
                return handle_modal_action(app, modal::ModalAction::DryRun);
            }
            _ => {}
        }
    }

    Ok(false)
}

fn handle_modal_action(app: &mut App, action: modal::ModalAction) -> Result<bool, Box<dyn Error>> {
    match action {
        modal::ModalAction::Confirm => {
            if let Some(modal_instance) = app.modal.take() {
                match modal_instance.modal_type {
                    modal::ModalType::ConfirmDelete { path, size } => {
                        // Move to final confirmation
                        app.modal = Some(modal::Modal::final_confirm(&path, size));
                    }
                    modal::ModalType::FinalConfirm { path, size: _ } => {
                        // Start deletion
                        app.modal = None;
                        app.start_delete(&path)?;
                    }
                    modal::ModalType::CleanupConfirm { dry_run, .. } => {
                        app.handle_cleanup_modal_confirm(true);
                        if dry_run {
                            return Ok(false);
                        }
                    }
                    modal::ModalType::CleanupFinal { .. } => {
                        app.handle_cleanup_final_confirm(true);
                    }
                    #[allow(unreachable_patterns)]
                    _ => {}
                }
            }
        }
        modal::ModalAction::DryRun => {
            if let Some(modal_instance) = app.modal.take() {
                match modal_instance.modal_type {
                    modal::ModalType::ConfirmDelete { path, size: _ } => {
                        app.start_dry_run(&path)?;
                    }
                    modal::ModalType::CleanupConfirm { .. } => {
                        app.handle_cleanup_modal_confirm(true);
                    }
                    _ => {}
                }
            }
        }
        modal::ModalAction::Cancel => {
            app.modal = None;
            app.cleanup_pending = None;
        }
    }

    Ok(false)
}
