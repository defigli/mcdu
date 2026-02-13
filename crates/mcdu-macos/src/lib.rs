//! macOS-specific features for mcdu
//!
//! This crate provides orphaned application data detection for macOS.
//! When apps are uninstalled (dragged to Trash), their data persists
//! across ~/Library subdirectories. This module finds that orphaned data.

#[cfg(target_os = "macos")]
pub mod bundle;
#[cfg(target_os = "macos")]
pub mod installed;
#[cfg(target_os = "macos")]
pub mod orphans;

#[cfg(target_os = "macos")]
pub use orphans::scan_orphans;
