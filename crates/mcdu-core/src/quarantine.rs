//! Quarantine system for safe deletion with undo capability
//!
//! Instead of permanently deleting files, moves them to ~/.mcdu/quarantine/
//! with metadata for restoration. Auto-purges after configurable TTL.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use uuid::Uuid;

/// Default quarantine directory
const QUARANTINE_SUBDIR: &str = "quarantine";

/// Default time-to-live for quarantined items (7 days)
const DEFAULT_TTL_DAYS: u64 = 7;

/// Default maximum quarantine size (10 GB)
const DEFAULT_MAX_SIZE_GB: u64 = 10;

#[derive(Error, Debug)]
pub enum QuarantineError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Quarantine entry not found: {0}")]
    NotFound(String),
    
    #[error("Original path already exists: {0}")]
    PathExists(PathBuf),
    
    #[error("Quarantine size limit exceeded")]
    SizeLimitExceeded,
}

/// A single quarantined item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineItem {
    /// Original absolute path
    pub original_path: PathBuf,
    /// Relative path within quarantine data directory
    pub quarantine_path: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// Category from cleanup rule
    pub category: String,
    /// Rule name that matched this item
    pub rule_name: String,
}

/// Manifest for a quarantine batch (one deletion operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineManifest {
    /// Unique identifier
    pub id: String,
    /// When the quarantine was created
    pub timestamp: SystemTime,
    /// Items in this batch
    pub items: Vec<QuarantineItem>,
    /// Total size of all items
    pub total_size_bytes: u64,
    /// When this quarantine expires
    pub expires_at: SystemTime,
    /// Whether items can still be restored
    pub can_restore: bool,
}

/// Quarantine settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineSettings {
    /// Time-to-live in days
    pub ttl_days: u64,
    /// Maximum total quarantine size in GB
    pub max_size_gb: u64,
    /// Categories to skip quarantine (delete directly)
    pub skip_categories: Vec<String>,
}

impl Default for QuarantineSettings {
    fn default() -> Self {
        Self {
            ttl_days: DEFAULT_TTL_DAYS,
            max_size_gb: DEFAULT_MAX_SIZE_GB,
            skip_categories: vec![
                "Browser Caches".to_string(),
                "IDE Caches".to_string(),
            ],
        }
    }
}

/// Quarantine manager
pub struct Quarantine {
    /// Base directory (~/.mcdu)
    base_dir: PathBuf,
    /// Settings
    settings: QuarantineSettings,
}

impl Quarantine {
    /// Create a new quarantine manager
    pub fn new(base_dir: PathBuf, settings: QuarantineSettings) -> Self {
        Self { base_dir, settings }
    }
    
    /// Get the quarantine directory
    fn quarantine_dir(&self) -> PathBuf {
        self.base_dir.join(QUARANTINE_SUBDIR)
    }
    
    /// Ensure quarantine directory exists
    fn ensure_dir(&self) -> io::Result<()> {
        fs::create_dir_all(self.quarantine_dir())
    }
    
    /// Check if a category should skip quarantine
    pub fn should_skip(&self, category: &str) -> bool {
        self.settings.skip_categories.iter().any(|c| c == category)
    }
    
    /// Get current quarantine size in bytes
    pub fn current_size(&self) -> io::Result<u64> {
        let mut total = 0u64;
        let qdir = self.quarantine_dir();
        
        if !qdir.exists() {
            return Ok(0);
        }
        
        for entry in fs::read_dir(&qdir)? {
            let entry = entry?;
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                if let Ok(content) = fs::read_to_string(&manifest_path) {
                    if let Ok(manifest) = serde_json::from_str::<QuarantineManifest>(&content) {
                        total += manifest.total_size_bytes;
                    }
                }
            }
        }
        
        Ok(total)
    }
    
    /// List all quarantine entries
    pub fn list(&self) -> io::Result<Vec<QuarantineManifest>> {
        let mut manifests = Vec::new();
        let qdir = self.quarantine_dir();
        
        if !qdir.exists() {
            return Ok(manifests);
        }
        
        for entry in fs::read_dir(&qdir)? {
            let entry = entry?;
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                if let Ok(content) = fs::read_to_string(&manifest_path) {
                    if let Ok(manifest) = serde_json::from_str::<QuarantineManifest>(&content) {
                        manifests.push(manifest);
                    }
                }
            }
        }
        
        // Sort by timestamp, newest first
        manifests.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(manifests)
    }
    
    /// Quarantine items (move to quarantine directory)
    pub fn quarantine(&self, items: Vec<(PathBuf, String, String, u64)>) -> Result<QuarantineManifest, QuarantineError> {
        self.ensure_dir()?;
        
        // Check size limit
        let current_size = self.current_size()?;
        let new_size: u64 = items.iter().map(|(_, _, _, size)| size).sum();
        let max_bytes = self.settings.max_size_gb * 1024 * 1024 * 1024;
        
        if current_size + new_size > max_bytes {
            // Try to purge expired entries first
            self.purge_expired()?;
            
            // Check again
            let current_size = self.current_size()?;
            if current_size + new_size > max_bytes {
                return Err(QuarantineError::SizeLimitExceeded);
            }
        }
        
        let id = Uuid::new_v4().to_string();
        let now = SystemTime::now();
        let expires_at = now + Duration::from_secs(self.settings.ttl_days * 24 * 3600);
        
        let batch_dir = self.quarantine_dir().join(&id);
        let data_dir = batch_dir.join("data");
        fs::create_dir_all(&data_dir)?;
        
        let mut quarantine_items = Vec::new();
        let mut total_size = 0u64;
        
        for (idx, (path, category, rule_name, size)) in items.into_iter().enumerate() {
            let quarantine_path = format!("{}", idx);
            let dest = data_dir.join(&quarantine_path);
            
            // Move the file/directory
            if let Err(_e) = fs::rename(&path, &dest) {
                // If rename fails (cross-device), try copy + delete
                if path.is_dir() {
                    copy_dir_all(&path, &dest)?;
                    fs::remove_dir_all(&path)?;
                } else {
                    fs::copy(&path, &dest)?;
                    fs::remove_file(&path)?;
                }
            }
            
            quarantine_items.push(QuarantineItem {
                original_path: path,
                quarantine_path,
                size_bytes: size,
                category,
                rule_name,
            });
            
            total_size += size;
        }
        
        let manifest = QuarantineManifest {
            id: id.clone(),
            timestamp: now,
            items: quarantine_items,
            total_size_bytes: total_size,
            expires_at,
            can_restore: true,
        };
        
        // Write manifest
        let manifest_path = batch_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_json)?;
        
        Ok(manifest)
    }
    
    /// Restore a quarantine entry
    pub fn restore(&self, id: &str) -> Result<Vec<PathBuf>, QuarantineError> {
        let batch_dir = self.quarantine_dir().join(id);
        let manifest_path = batch_dir.join("manifest.json");
        
        if !manifest_path.exists() {
            return Err(QuarantineError::NotFound(id.to_string()));
        }
        
        let content = fs::read_to_string(&manifest_path)?;
        let manifest: QuarantineManifest = serde_json::from_str(&content)?;
        
        if !manifest.can_restore {
            return Err(QuarantineError::NotFound(id.to_string()));
        }
        
        let data_dir = batch_dir.join("data");
        let mut restored = Vec::new();
        
        for item in &manifest.items {
            let source = data_dir.join(&item.quarantine_path);
            let dest = &item.original_path;
            
            // Check if destination already exists
            if dest.exists() {
                return Err(QuarantineError::PathExists(dest.clone()));
            }
            
            // Ensure parent directory exists
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            
            // Move back
            if let Err(_) = fs::rename(&source, dest) {
                if source.is_dir() {
                    copy_dir_all(&source, dest)?;
                    fs::remove_dir_all(&source)?;
                } else {
                    fs::copy(&source, dest)?;
                    fs::remove_file(&source)?;
                }
            }
            
            restored.push(dest.clone());
        }
        
        // Remove quarantine entry
        fs::remove_dir_all(&batch_dir)?;
        
        Ok(restored)
    }
    
    /// Permanently delete a quarantine entry
    pub fn purge(&self, id: &str) -> Result<u64, QuarantineError> {
        let batch_dir = self.quarantine_dir().join(id);
        let manifest_path = batch_dir.join("manifest.json");
        
        if !manifest_path.exists() {
            return Err(QuarantineError::NotFound(id.to_string()));
        }
        
        let content = fs::read_to_string(&manifest_path)?;
        let manifest: QuarantineManifest = serde_json::from_str(&content)?;
        
        let size = manifest.total_size_bytes;
        fs::remove_dir_all(&batch_dir)?;
        
        Ok(size)
    }
    
    /// Purge all expired entries
    pub fn purge_expired(&self) -> Result<u64, QuarantineError> {
        let now = SystemTime::now();
        let manifests = self.list()?;
        let mut purged_size = 0u64;
        
        for manifest in manifests {
            if manifest.expires_at <= now {
                purged_size += self.purge(&manifest.id)?;
            }
        }
        
        Ok(purged_size)
    }
    
    /// Purge all entries
    pub fn purge_all(&self) -> Result<u64, QuarantineError> {
        let manifests = self.list()?;
        let mut purged_size = 0u64;
        
        for manifest in manifests {
            purged_size += self.purge(&manifest.id)?;
        }
        
        Ok(purged_size)
    }
    
    /// Get quarantine statistics
    pub fn stats(&self) -> io::Result<QuarantineStats> {
        let manifests = self.list()?;
        let now = SystemTime::now();
        
        let total_entries = manifests.len();
        let total_items: usize = manifests.iter().map(|m| m.items.len()).sum();
        let total_size: u64 = manifests.iter().map(|m| m.total_size_bytes).sum();
        let expired_count = manifests.iter().filter(|m| m.expires_at <= now).count();
        
        let oldest = manifests.last().map(|m| m.timestamp);
        let newest = manifests.first().map(|m| m.timestamp);
        
        Ok(QuarantineStats {
            total_entries,
            total_items,
            total_size_bytes: total_size,
            expired_count,
            oldest_entry: oldest,
            newest_entry: newest,
            max_size_bytes: self.settings.max_size_gb * 1024 * 1024 * 1024,
        })
    }
}

/// Quarantine statistics
#[derive(Debug, Clone)]
pub struct QuarantineStats {
    pub total_entries: usize,
    pub total_items: usize,
    pub total_size_bytes: u64,
    pub expired_count: usize,
    pub oldest_entry: Option<SystemTime>,
    pub newest_entry: Option<SystemTime>,
    pub max_size_bytes: u64,
}

/// Recursively copy a directory
fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        
        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn quarantine_and_restore() {
        let tmp = tempdir().unwrap();
        let base_dir = tmp.path().join(".mcdu");
        let source_dir = tmp.path().join("source");
        
        // Create test file
        fs::create_dir_all(&source_dir).unwrap();
        let test_file = source_dir.join("test.txt");
        fs::write(&test_file, "hello world").unwrap();
        
        let quarantine = Quarantine::new(base_dir, QuarantineSettings::default());
        
        // Quarantine the file
        let manifest = quarantine.quarantine(vec![
            (test_file.clone(), "Test".to_string(), "test-rule".to_string(), 11),
        ]).unwrap();
        
        assert_eq!(manifest.items.len(), 1);
        assert!(!test_file.exists()); // File should be moved
        
        // Restore
        let restored = quarantine.restore(&manifest.id).unwrap();
        assert_eq!(restored.len(), 1);
        assert!(test_file.exists()); // File should be back
        assert_eq!(fs::read_to_string(&test_file).unwrap(), "hello world");
    }
    
    #[test]
    fn quarantine_directory() {
        let tmp = tempdir().unwrap();
        let base_dir = tmp.path().join(".mcdu");
        let source_dir = tmp.path().join("source");
        
        // Create test directory with files
        let target_dir = source_dir.join("target");
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(target_dir.join("a.txt"), "a").unwrap();
        fs::write(target_dir.join("b.txt"), "b").unwrap();
        
        let quarantine = Quarantine::new(base_dir, QuarantineSettings::default());
        
        // Quarantine the directory
        let manifest = quarantine.quarantine(vec![
            (target_dir.clone(), "Rust".to_string(), "target".to_string(), 2),
        ]).unwrap();
        
        assert!(!target_dir.exists());
        
        // Restore
        quarantine.restore(&manifest.id).unwrap();
        assert!(target_dir.exists());
        assert!(target_dir.join("a.txt").exists());
        assert!(target_dir.join("b.txt").exists());
    }
    
    #[test]
    fn list_and_purge() {
        let tmp = tempdir().unwrap();
        let base_dir = tmp.path().join(".mcdu");
        let source_dir = tmp.path().join("source");
        fs::create_dir_all(&source_dir).unwrap();
        
        let quarantine = Quarantine::new(base_dir, QuarantineSettings::default());
        
        // Create and quarantine multiple items
        for i in 0..3 {
            let file = source_dir.join(format!("file{}.txt", i));
            fs::write(&file, format!("content {}", i)).unwrap();
            quarantine.quarantine(vec![
                (file, "Test".to_string(), "test".to_string(), 10),
            ]).unwrap();
        }
        
        let list = quarantine.list().unwrap();
        assert_eq!(list.len(), 3);
        
        // Purge one
        quarantine.purge(&list[0].id).unwrap();
        
        let list = quarantine.list().unwrap();
        assert_eq!(list.len(), 2);
        
        // Purge all
        quarantine.purge_all().unwrap();
        
        let list = quarantine.list().unwrap();
        assert_eq!(list.len(), 0);
    }
    
    #[test]
    fn stats() {
        let tmp = tempdir().unwrap();
        let base_dir = tmp.path().join(".mcdu");
        let source_dir = tmp.path().join("source");
        fs::create_dir_all(&source_dir).unwrap();
        
        let quarantine = Quarantine::new(base_dir, QuarantineSettings::default());
        
        let file = source_dir.join("test.txt");
        fs::write(&file, "test content").unwrap();
        quarantine.quarantine(vec![
            (file, "Test".to_string(), "test".to_string(), 12),
        ]).unwrap();
        
        let stats = quarantine.stats().unwrap();
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.total_items, 1);
        assert_eq!(stats.total_size_bytes, 12);
        assert_eq!(stats.expired_count, 0);
    }
}
