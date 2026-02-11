//! mcdu-tui - Terminal UI components for mcdu
//!
//! This crate provides:
//! - Application state management
//! - TUI rendering with ratatui
//! - Modal dialogs
//! - File tree navigation
//! - Cleanup view state

pub mod app;
pub mod cache;
pub mod changes;
pub mod cleanup_ui;
pub mod delete;
pub mod logger;
pub mod modal;
pub mod scan;
pub mod tree;
pub mod ui;

// Re-exports
pub use app::{App, AppMode, DeleteProgress};
pub use cleanup_ui::CleanupViewState;
