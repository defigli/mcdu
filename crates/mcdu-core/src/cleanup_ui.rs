//! Cleanup UI state management
//!
//! Handles the state for the cleanup view including:
//! - Candidate selection
//! - Category grouping and expansion
//! - Scan/delete progress
//! - Quarantine integration

use crate::rules::Candidate;
use crate::scanner::CategoryGroup;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// State for the cleanup view
#[derive(Debug, Default, Clone)]
pub struct CleanupViewState {
    /// All candidates grouped by category
    pub categories: Vec<CategoryGroup>,
    /// Flat list of all candidates (for compatibility)
    pub candidates: Vec<Candidate>,
    /// Selected paths
    pub selected: HashSet<PathBuf>,
    /// Expanded categories
    pub expanded: HashSet<String>,
    /// Currently focused category index
    pub focused_category: usize,
    /// Currently focused item within category (-1 = category header)
    pub focused_item: i32,
    /// Whether scanning is in progress
    pub scanning: bool,
    /// Whether deletion is in progress
    pub deleting: bool,
    /// Scan progress info
    pub scan_progress: Option<ScanProgressInfo>,
    /// Delete progress info
    pub delete_progress: Option<DeleteProgressInfo>,
}

/// Scan progress information
#[derive(Debug, Clone)]
pub struct ScanProgressInfo {
    pub current_category: String,
    pub found_count: u64,
    pub total_size_bytes: u64,
    pub current_path: Option<PathBuf>,
}

/// Delete progress information
#[derive(Debug, Clone)]
pub struct DeleteProgressInfo {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub current_file: String,
}

impl CleanupViewState {
    /// Create new state from candidates
    pub fn new(candidates: Vec<Candidate>) -> Self {
        let categories = crate::scanner::group_by_category(candidates.clone());
        
        // Expand all categories by default
        let expanded: HashSet<String> = categories.iter().map(|c| c.name.clone()).collect();
        
        CleanupViewState {
            categories,
            candidates,
            selected: HashSet::new(),
            expanded,
            focused_category: 0,
            focused_item: -1,
            scanning: false,
            deleting: false,
            scan_progress: None,
            delete_progress: None,
        }
    }
    
    /// Create empty state (for initial/scanning state)
    pub fn empty() -> Self {
        Self::default()
    }
    
    /// Update with new candidates (after scan)
    pub fn update_candidates(&mut self, candidates: Vec<Candidate>) {
        self.categories = crate::scanner::group_by_category(candidates.clone());
        self.candidates = candidates;
        self.expanded = self.categories.iter().map(|c| c.name.clone()).collect();
        self.focused_category = 0;
        self.focused_item = -1;
    }
    
    /// Toggle selection of a path
    pub fn toggle_selection(&mut self, path: &PathBuf) {
        if self.selected.contains(path) {
            self.selected.remove(path);
        } else {
            self.selected.insert(path.clone());
        }
    }
    
    /// Check if a path is selected
    pub fn is_selected(&self, path: &PathBuf) -> bool {
        self.selected.contains(path)
    }
    
    /// Select all items in a category
    pub fn select_category(&mut self, category: &str) {
        if let Some(cat) = self.categories.iter().find(|c| c.name == category) {
            for candidate in &cat.candidates {
                self.selected.insert(candidate.path.clone());
            }
        }
    }
    
    /// Deselect all items in a category
    pub fn deselect_category(&mut self, category: &str) {
        if let Some(cat) = self.categories.iter().find(|c| c.name == category) {
            for candidate in &cat.candidates {
                self.selected.remove(&candidate.path);
            }
        }
    }
    
    /// Toggle all items in a category
    pub fn toggle_category(&mut self, category: &str) {
        if let Some(cat) = self.categories.iter().find(|c| c.name == category) {
            let all_selected = cat.candidates.iter().all(|c| self.selected.contains(&c.path));
            if all_selected {
                self.deselect_category(category);
            } else {
                self.select_category(category);
            }
        }
    }
    
    /// Toggle category expansion
    pub fn toggle_expand(&mut self, category: &str) {
        if self.expanded.contains(category) {
            self.expanded.remove(category);
        } else {
            self.expanded.insert(category.to_string());
        }
    }
    
    /// Check if category is expanded
    pub fn is_expanded(&self, category: &str) -> bool {
        self.expanded.contains(category)
    }
    
    /// Get total selected size
    pub fn selected_size(&self) -> u64 {
        self.candidates
            .iter()
            .filter(|c| self.selected.contains(&c.path))
            .map(|c| c.size_bytes)
            .sum()
    }
    
    /// Get total selected count
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }
    
    /// Get total reclaimable size (all candidates)
    pub fn total_size(&self) -> u64 {
        self.candidates.iter().map(|c| c.size_bytes).sum()
    }
    
    /// Get selected candidates
    pub fn selected_candidates(&self) -> Vec<&Candidate> {
        self.candidates
            .iter()
            .filter(|c| self.selected.contains(&c.path))
            .collect()
    }
    
    /// Get category statistics
    pub fn category_stats(&self) -> Vec<CategoryStats> {
        self.categories
            .iter()
            .map(|cat| {
                let total_size: u64 = cat.candidates.iter().map(|c| c.size_bytes).sum();
                let selected_count = cat.candidates.iter()
                    .filter(|c| self.selected.contains(&c.path))
                    .count();
                let has_warnings = cat.candidates.iter().any(|c| c.warning.is_some());
                let has_active = cat.candidates.iter().any(|c| c.is_active);
                
                CategoryStats {
                    name: cat.name.clone(),
                    item_count: cat.candidates.len(),
                    selected_count,
                    total_size_bytes: total_size,
                    has_warnings,
                    has_active,
                }
            })
            .collect()
    }
    
    /// Move focus up
    pub fn focus_up(&mut self) {
        if self.categories.is_empty() {
            return;
        }
        
        if self.focused_item > -1 {
            self.focused_item -= 1;
        } else if self.focused_category > 0 {
            self.focused_category -= 1;
            let cat = &self.categories[self.focused_category];
            if self.is_expanded(&cat.name) && !cat.candidates.is_empty() {
                self.focused_item = (cat.candidates.len() - 1) as i32;
            } else {
                self.focused_item = -1;
            }
        }
    }
    
    /// Move focus down
    pub fn focus_down(&mut self) {
        if self.categories.is_empty() {
            return;
        }
        
        let cat = &self.categories[self.focused_category];
        let max_item = if self.is_expanded(&cat.name) {
            cat.candidates.len() as i32 - 1
        } else {
            -1
        };
        
        if self.focused_item < max_item {
            self.focused_item += 1;
        } else if self.focused_category < self.categories.len() - 1 {
            self.focused_category += 1;
            self.focused_item = -1;
        }
    }
    
    /// Toggle selection of focused item
    pub fn toggle_focused(&mut self) {
        if self.categories.is_empty() {
            return;
        }
        
        let cat_name = self.categories[self.focused_category].name.clone();
        
        if self.focused_item == -1 {
            // Toggle entire category
            self.toggle_category(&cat_name);
        } else {
            // Toggle single item
            let path = self.categories[self.focused_category]
                .candidates[self.focused_item as usize]
                .path
                .clone();
            self.toggle_selection(&path);
        }
    }
    
    /// Toggle expansion of focused category
    pub fn toggle_focused_expand(&mut self) {
        if self.categories.is_empty() {
            return;
        }
        
        let cat_name = self.categories[self.focused_category].name.clone();
        self.toggle_expand(&cat_name);
        
        // Reset item focus when collapsing
        if !self.is_expanded(&cat_name) {
            self.focused_item = -1;
        }
    }
}

/// Statistics for a single category
#[derive(Debug, Clone)]
pub struct CategoryStats {
    pub name: String,
    pub item_count: usize,
    pub selected_count: usize,
    pub total_size_bytes: u64,
    pub has_warnings: bool,
    pub has_active: bool,
}

/// Format bytes as human-readable string
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format age as human-readable string
pub fn format_age(age_secs: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = MINUTE * 60;
    const DAY: u64 = HOUR * 24;
    const WEEK: u64 = DAY * 7;
    const MONTH: u64 = DAY * 30;
    
    if age_secs >= MONTH {
        let months = age_secs / MONTH;
        if months == 1 {
            "1 month old".to_string()
        } else {
            format!("{} months old", months)
        }
    } else if age_secs >= WEEK {
        let weeks = age_secs / WEEK;
        if weeks == 1 {
            "1 week old".to_string()
        } else {
            format!("{} weeks old", weeks)
        }
    } else if age_secs >= DAY {
        let days = age_secs / DAY;
        if days == 1 {
            "1 day old".to_string()
        } else {
            format!("{} days old", days)
        }
    } else if age_secs >= HOUR {
        let hours = age_secs / HOUR;
        if hours == 1 {
            "1 hour old".to_string()
        } else {
            format!("{} hours old", hours)
        }
    } else {
        "recent".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(path: &str, category: &str, size: u64) -> Candidate {
        Candidate::new(
            PathBuf::from(path),
            "rule".into(),
            category.into(),
            "**/*".into(),
            size,
            None,
            false,
        )
    }

    #[test]
    fn toggles_selection_state() {
        let mut state = CleanupViewState::new(vec![candidate("/tmp/a", "Test", 100)]);
        let path = PathBuf::from("/tmp/a");
        assert!(!state.is_selected(&path));
        state.toggle_selection(&path);
        assert!(state.is_selected(&path));
        state.toggle_selection(&path);
        assert!(!state.is_selected(&path));
    }
    
    #[test]
    fn groups_by_category() {
        let state = CleanupViewState::new(vec![
            candidate("/a", "Cat1", 100),
            candidate("/b", "Cat1", 200),
            candidate("/c", "Cat2", 300),
        ]);
        
        assert_eq!(state.categories.len(), 2);
        assert_eq!(state.total_size(), 600);
    }
    
    #[test]
    fn selects_category() {
        let mut state = CleanupViewState::new(vec![
            candidate("/a", "Cat1", 100),
            candidate("/b", "Cat1", 200),
            candidate("/c", "Cat2", 300),
        ]);
        
        state.select_category("Cat1");
        assert_eq!(state.selected_count(), 2);
        assert_eq!(state.selected_size(), 300);
    }
    
    #[test]
    fn formats_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1_572_864), "1.5 MB");
        assert_eq!(format_size(1_610_612_736), "1.5 GB");
    }
    
    #[test]
    fn formats_age() {
        assert_eq!(format_age(30), "recent");
        assert_eq!(format_age(3600), "1 hour old");
        assert_eq!(format_age(86400), "1 day old");
        assert_eq!(format_age(604800), "1 week old");
        assert_eq!(format_age(2592000), "1 month old");
    }
}
