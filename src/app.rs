use crate::delete;
use crate::logger;
use crate::modal::Modal;
use crate::platform::{self, DiskSpace};
use crate::tree::{scan_tree, FileNode, ScanProgress};
use chrono::Local;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Browsing,
    Deleting,
    DryRun,
}

pub struct DeleteProgress {
    pub deleted_bytes: u64,
    pub total_bytes: u64,
    pub deleted_files: u64,
    pub total_files: u64,
    pub current_file: String,
    pub status: String,
}

pub struct App {
    pub root_path: PathBuf,
    pub tree: Option<FileNode>,
    pub nav_stack: Vec<usize>,       // Indices into children at each level
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub mode: AppMode,
    pub modal: Option<Modal>,
    pub delete_progress: Option<DeleteProgress>,
    pub delete_thread: Option<JoinHandle<Result<(), String>>>,
    pub delete_rx: Option<mpsc::Receiver<DeleteProgressUpdate>>,
    pub deleting_path: Option<PathBuf>,  // Path being deleted (for tree update)
    pub notification: Option<String>,
    pub notification_time: Option<Instant>,
    pub show_help: bool,
    // Async scanning
    pub scan_thread: Option<JoinHandle<()>>,
    pub scan_rx: Option<mpsc::Receiver<ScanProgress>>,
    pub is_scanning: bool,
    pub scan_files_count: usize,
    pub scanning_path: Option<String>,
    // Disk space info
    pub disk_space: Option<DiskSpace>,
}

pub enum DeleteProgressUpdate {
    #[allow(dead_code)]
    Progress {
        bytes_done: u64,
        bytes_total: u64,
        files_done: u64,
        files_total: u64,
        current_file: String,
    },
    Complete {
        total_bytes: u64,
        total_files: u64,
    },
    Error(String),
}

/// Entry for display in the UI (derived from tree)
pub struct DisplayEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
}

impl App {
    pub fn new() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        Self::new_with_root(root)
    }

    pub fn new_with_root(root: PathBuf) -> Self {
        let disk_space = platform::get_disk_space(&root);

        let mut app = App {
            root_path: root.clone(),
            tree: None,
            nav_stack: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            mode: AppMode::Browsing,
            modal: None,
            delete_progress: None,
            delete_thread: None,
            delete_rx: None,
            deleting_path: None,
            notification: None,
            notification_time: None,
            show_help: false,
            scan_thread: None,
            scan_rx: None,
            is_scanning: false,
            scan_files_count: 0,
            scanning_path: None,
            disk_space,
        };
        app.start_scan();
        app
    }

    /// Get the current directory node we're viewing
    pub fn get_current_node(&self) -> Option<&FileNode> {
        let tree = self.tree.as_ref()?;
        let mut node = tree;

        for &idx in &self.nav_stack {
            node = node.children.get(idx)?;
        }

        Some(node)
    }

    /// Get entries for display (current directory's children)
    pub fn get_display_entries(&self) -> Vec<DisplayEntry> {
        let mut entries = Vec::new();

        // Add parent entry if not at root
        if !self.nav_stack.is_empty() {
            entries.push(DisplayEntry {
                name: "..".to_string(),
                path: PathBuf::new(),
                size: 0,
                is_dir: true,
            });
        }

        if let Some(node) = self.get_current_node() {
            for child in &node.children {
                entries.push(DisplayEntry {
                    name: child.name.clone(),
                    path: child.path.clone(),
                    size: child.size,
                    is_dir: child.is_dir,
                });
            }
        }

        entries
    }

    /// Get current path for display
    pub fn get_current_path(&self) -> PathBuf {
        self.get_current_node()
            .map(|n| n.path.clone())
            .unwrap_or_else(|| self.root_path.clone())
    }

    /// Get total entries count
    pub fn entries_count(&self) -> usize {
        let base = self.get_current_node().map(|n| n.children.len()).unwrap_or(0);
        if self.nav_stack.is_empty() {
            base
        } else {
            base + 1 // +1 for ".." entry
        }
    }

    pub fn select_next(&mut self) {
        let count = self.entries_count();
        if count > 0 && self.selected_index < count - 1 {
            self.selected_index += 1;
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn adjust_scroll(&mut self, viewport_height: usize) {
        let usable_height = viewport_height.saturating_sub(2);

        if usable_height == 0 {
            return;
        }

        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }

        if self.selected_index >= self.scroll_offset + usable_height {
            self.scroll_offset = self.selected_index.saturating_sub(usable_height - 1);
        }
    }

    pub fn enter_directory(&mut self) {
        if self.tree.is_none() || self.is_scanning {
            return;
        }

        let entries = self.get_display_entries();
        if let Some(entry) = entries.get(self.selected_index) {
            if !entry.is_dir {
                return;
            }

            // Handle ".." entry
            if entry.name == ".." {
                self.go_parent();
                return;
            }

            // Find the index of this child in the current node
            if let Some(current) = self.get_current_node() {
                let child_idx = current.children.iter().position(|c| c.name == entry.name);
                if let Some(idx) = child_idx {
                    self.nav_stack.push(idx);
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                }
            }
        }
    }

    pub fn go_parent(&mut self) {
        if self.nav_stack.pop().is_some() {
            self.selected_index = 0;
            self.scroll_offset = 0;
        }
    }

    /// Remove a deleted entry from the tree and update sizes up the tree
    fn remove_entry_from_tree(&mut self, path: &std::path::Path) {
        let Some(tree) = self.tree.as_mut() else { return };

        // Navigate to the current node using nav_stack
        let mut node = tree;
        for &idx in &self.nav_stack {
            node = &mut node.children[idx];
        }

        // Find and remove the child with matching path
        if let Some(idx) = node.children.iter().position(|c| c.path == path) {
            let removed_size = node.children[idx].size;
            node.children.remove(idx);

            // Update selected_index if needed
            if self.selected_index >= node.children.len() && self.selected_index > 0 {
                // Account for ".." entry if present
                let offset = if self.nav_stack.is_empty() { 0 } else { 1 };
                let max_idx = node.children.len().saturating_sub(1) + offset;
                self.selected_index = max_idx;
            }

            // Subtract the removed size from all parents up to root
            if removed_size > 0 {
                let tree = self.tree.as_mut().unwrap();
                tree.size = tree.size.saturating_sub(removed_size);

                let mut parent = tree;
                for &idx in &self.nav_stack {
                    parent = &mut parent.children[idx];
                    parent.size = parent.size.saturating_sub(removed_size);
                }
            }
        }
    }

    /// Start full tree scan
    fn start_scan(&mut self) {
        // Cancel any existing scan
        if let Some(thread) = self.scan_thread.take() {
            let _ = thread.join();
        }
        self.scan_rx = None;

        let path = self.root_path.clone();
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            match scan_tree(&path, Some(tx.clone())) {
                Ok(tree) => {
                    let _ = tx.send(ScanProgress::Complete(tree));
                }
                Err(e) => {
                    let _ = tx.send(ScanProgress::Error(e));
                }
            }
        });

        self.scan_thread = Some(handle);
        self.scan_rx = Some(rx);
        self.is_scanning = true;
        self.scan_files_count = 0;
        self.scanning_path = None;
    }

    pub fn refresh(&mut self) {
        self.tree = None;
        self.nav_stack.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.start_scan();
    }

    pub fn update_scan_progress(&mut self) {
        if let Some(rx) = self.scan_rx.as_ref() {
            while let Ok(progress) = rx.try_recv() {
                match progress {
                    ScanProgress::Scanning { files_scanned, current_path } => {
                        self.scan_files_count = files_scanned;
                        self.scanning_path = Some(current_path);
                    }
                    ScanProgress::Complete(tree) => {
                        self.tree = Some(tree);
                        self.is_scanning = false;
                        self.scan_thread = None;
                        self.scan_rx = None;
                        self.scanning_path = None;
                        self.disk_space = platform::get_disk_space(&self.root_path);
                        break;
                    }
                    ScanProgress::Error(e) => {
                        self.is_scanning = false;
                        self.scan_thread = None;
                        self.scan_rx = None;
                        self.notification = Some(format!("✗ Scan error: {}", e));
                        self.notification_time = Some(Instant::now());
                        break;
                    }
                }
            }
        }
    }

    pub fn open_delete_modal(&mut self) {
        let entries = self.get_display_entries();
        if let Some(entry) = entries.get(self.selected_index) {
            if entry.name != ".." {
                self.modal = Some(Modal::confirm_delete(&entry.path, entry.size));
            }
        }
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn start_delete(&mut self, path: &std::path::Path) -> Result<(), String> {
        let path_clone = path.to_path_buf();
        let (tx, rx) = mpsc::channel();
        let start_time = Instant::now();

        let handle = thread::spawn(move || {
            match delete::delete_directory(&path_clone) {
                Ok(result) => {
                    let duration_ms = start_time.elapsed().as_millis() as u64;

                    let log = logger::DeleteLog {
                        timestamp: Local::now().to_rfc3339(),
                        action: "delete".to_string(),
                        path: path_clone.display().to_string(),
                        size_bytes: result.total_bytes,
                        dry_run: false,
                        status: "success".to_string(),
                        files_deleted: result.total_files,
                        duration_ms,
                        errors: if result.errors.is_empty() {
                            None
                        } else {
                            Some(result.errors)
                        },
                    };

                    let _ = logger::write_log(&log);

                    let _ = tx.send(DeleteProgressUpdate::Complete {
                        total_bytes: result.total_bytes,
                        total_files: result.total_files,
                    });
                    Ok(())
                }
                Err(e) => {
                    let log = logger::DeleteLog {
                        timestamp: Local::now().to_rfc3339(),
                        action: "delete".to_string(),
                        path: path_clone.display().to_string(),
                        size_bytes: 0,
                        dry_run: false,
                        status: "error".to_string(),
                        files_deleted: 0,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        errors: Some(vec![e.to_string()]),
                    };

                    let _ = logger::write_log(&log);
                    let _ = tx.send(DeleteProgressUpdate::Error(e.to_string()));
                    Err(e.to_string())
                }
            }
        });

        self.delete_thread = Some(handle);
        self.delete_rx = Some(rx);
        self.deleting_path = Some(path.to_path_buf());
        self.mode = AppMode::Deleting;
        self.delete_progress = Some(DeleteProgress {
            deleted_bytes: 0,
            total_bytes: 0,
            deleted_files: 0,
            total_files: 0,
            current_file: String::new(),
            status: "Starting deletion...".to_string(),
        });

        Ok(())
    }

    pub fn start_dry_run(&mut self, path: &PathBuf) -> Result<(), String> {
        match delete::dry_run_delete(path) {
            Ok(files) => {
                self.mode = AppMode::DryRun;

                let total_size: u64 = files
                    .iter()
                    .filter_map(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())
                    .sum();

                let msg = format!(
                    "Dry-run: Would delete {} files ({:.1} MB)",
                    files.len(),
                    total_size as f64 / 1_048_576.0
                );

                let log = logger::DeleteLog {
                    timestamp: Local::now().to_rfc3339(),
                    action: "dry-run".to_string(),
                    path: path.display().to_string(),
                    size_bytes: total_size,
                    dry_run: true,
                    status: "complete".to_string(),
                    files_deleted: files.len() as u64,
                    duration_ms: 0,
                    errors: None,
                };

                let _ = logger::write_log(&log);
                self.notification = Some(msg);
                self.notification_time = Some(Instant::now());
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn update_delete_progress(&mut self) {
        let mut updates = Vec::new();
        if let Some(rx) = self.delete_rx.as_mut() {
            while let Ok(update) = rx.try_recv() {
                updates.push(update);
            }
        }

        for update in updates {
            match update {
                DeleteProgressUpdate::Progress {
                    bytes_done,
                    bytes_total,
                    files_done,
                    files_total,
                    current_file,
                } => {
                    if let Some(progress) = &mut self.delete_progress {
                        progress.deleted_bytes = bytes_done;
                        progress.total_bytes = bytes_total;
                        progress.deleted_files = files_done;
                        progress.total_files = files_total;
                        progress.current_file = current_file;
                        progress.status = "Deleting...".to_string();
                    }
                }
                DeleteProgressUpdate::Complete {
                    total_bytes,
                    total_files,
                } => {
                    self.delete_progress = None;
                    self.delete_rx = None;
                    self.mode = AppMode::Browsing;
                    let msg = format!(
                        "✓ Deleted {} files ({:.1} MB)",
                        total_files,
                        total_bytes as f64 / 1_048_576.0
                    );
                    self.notification = Some(msg);
                    self.notification_time = Some(Instant::now());
                    self.disk_space = platform::get_disk_space(&self.root_path);
                    // Remove deleted entry from tree (no rescan needed!)
                    if let Some(path) = self.deleting_path.take() {
                        self.remove_entry_from_tree(&path);
                    }
                }
                DeleteProgressUpdate::Error(e) => {
                    self.delete_progress = None;
                    self.delete_rx = None;
                    self.mode = AppMode::Browsing;
                    let msg = format!("✗ Delete error: {}", e);
                    self.notification = Some(msg);
                    self.notification_time = Some(Instant::now());
                }
            }
        }
    }
}
