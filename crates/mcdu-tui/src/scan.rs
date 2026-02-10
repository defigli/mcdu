use crate::cache::SizeCache;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use walkdir::WalkDir;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

/// Progress updates during directory scanning
#[derive(Clone, Debug)]
pub enum ScanProgress {
    Progress {
        current_name: String,
        scanned_count: usize,
        total_count: usize,
    },
}

/// Get actual disk usage for a file (handles sparse files correctly)
/// Returns blocks * 512 instead of apparent size
#[cfg(unix)]
fn disk_usage(metadata: &std::fs::Metadata) -> u64 {
    metadata.blocks() * 512
}

#[cfg(not(unix))]
fn disk_usage(metadata: &std::fs::Metadata) -> u64 {
    metadata.len()
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    #[allow(dead_code)]
    pub file_count: u64,
    pub size_change: Option<(i64, f32)>, // (delta_bytes, percent_of_directory)
    #[allow(dead_code)]
    pub is_new: bool, // True if this didn't exist before
}

pub fn scan_directory(
    path: &PathBuf,
    cache: &SizeCache,
    progress_tx: Option<&mpsc::Sender<ScanProgress>>,
) -> Result<Vec<DirEntry>, Box<dyn std::error::Error>> {
    // Scan immediate children (non-recursive)
    let children: Vec<_> = fs::read_dir(path)?.filter_map(|e| e.ok()).collect();

    let total_count = children.len();
    let mut scanned_count = 0;

    // Process sequentially - directory size calculation is I/O bound and parallel doesn't help much
    // Plus we need to keep responsiveness
    let entries: Vec<DirEntry> = children
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = entry.metadata().ok()?;
            let is_dir = metadata.is_dir();

            let name = path.file_name()?.to_str()?.to_string();

            // Fast size calculation - reuse metadata for files, use cache for dirs
            let size = if is_dir {
                // Send progress update for directories (skip files since they're fast)
                if let Some(tx) = progress_tx {
                    scanned_count += 1;
                    let _ = tx.send(ScanProgress::Progress {
                        current_name: name.clone(),
                        scanned_count,
                        total_count,
                    });
                }
                // Try cache first, fall back to scanning
                if let Some(cached_size) = cache.get(&path) {
                    cached_size
                } else {
                    let size = quick_dir_size(&path);
                    cache.set(path.clone(), size);
                    size
                }
            } else {
                disk_usage(&metadata)
            };

            Some(DirEntry {
                path,
                name,
                size,
                is_dir,
                file_count: 0,
                size_change: None,
                is_new: false,
            })
        })
        .collect();

    // Sort by size, largest first
    let mut sorted = entries;
    sorted.sort_by(|a, b| b.size.cmp(&a.size));

    Ok(sorted)
}

fn quick_dir_size(path: &std::path::Path) -> u64 {
    // Calculate total size of all files in directory
    // Stay on same filesystem to avoid counting mounted volumes (like ncdu does)
    let mut total = 0u64;

    for entry in WalkDir::new(path)
        .same_file_system(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                total += disk_usage(&metadata);
            }
        }
    }

    total
}
