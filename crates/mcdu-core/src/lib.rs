//! mcdu-core - Core library for developer cleanup
//!
//! This crate provides:
//! - Rule-based cleanup scanning
//! - Project root detection
//! - Quarantine system with undo
//! - Parallel scanning support
//! - Platform-specific path resolution

pub mod config;
pub mod platform;
pub mod rules;
pub mod scanner;
pub mod quarantine;
pub mod parallel;
pub mod executor;
pub mod git;

// Re-exports for convenience
pub use config::{CleanupConfig, CleanupState};
pub use platform::PlatformPaths;
pub use rules::{Rule, Candidate, MatchType};
pub use scanner::{scan, group_by_category, CategoryGroup, ScanProgress};
pub use quarantine::{Quarantine, QuarantineSettings, QuarantineManifest, QuarantineError};
pub use parallel::{ParallelScanner, ParallelScanConfig, parallel_scan};
pub use executor::{CleanupResult, CleanupProgress};
