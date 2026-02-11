use crate::app::DeleteProgressUpdate;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use walkdir::WalkDir;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

/// Get actual disk usage for a file (handles sparse files correctly)
#[cfg(unix)]
fn disk_usage(metadata: &std::fs::Metadata) -> u64 {
    metadata.blocks() * 512
}

#[cfg(not(unix))]
fn disk_usage(metadata: &std::fs::Metadata) -> u64 {
    metadata.len()
}

pub struct DeleteResult {
    pub total_bytes: u64,
    pub total_files: u64,
    pub errors: Vec<String>,
}

/// Delete a directory with optional progress updates sent to UI
///
/// # Arguments
/// * `path` - Path to delete
/// * `progress_tx` - Optional channel to send progress updates to UI
pub fn delete_directory(
    path: &PathBuf,
    progress_tx: Option<mpsc::Sender<DeleteProgressUpdate>>,
) -> Result<DeleteResult, Box<dyn std::error::Error>> {
    let mut total_bytes = 0u64;
    let mut total_files = 0u64;
    let mut errors = Vec::new();

    // Optimized: Single walk, collect entries with metadata to avoid re-stating
    struct EntryWithMetadata {
        path: PathBuf,
        size: u64,
        is_file: bool,
    }

    // Phase 1: Collect all entries
    let entries: Vec<EntryWithMetadata> = WalkDir::new(path)
        .same_file_system(true) // Don't cross into mounted volumes
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let entry_path = entry.path();
            if entry_path == path {
                return None; // Skip root for now
            }

            // Get metadata once and store it
            entry.metadata().ok().map(|metadata| EntryWithMetadata {
                path: entry_path.to_path_buf(),
                size: disk_usage(&metadata),
                is_file: metadata.is_file(),
            })
        })
        .collect();

    // Phase 2: Send total count to UI (so user sees what's coming)
    let total_count = entries.len() as u64;
    let total_size_bytes: u64 = entries.iter().map(|e| e.size).sum();

    if let Some(ref tx) = progress_tx {
        let _ = tx.send(DeleteProgressUpdate::Progress {
            bytes_done: 0,
            bytes_total: total_size_bytes,
            files_done: 0,
            files_total: total_count + 1, // +1 for root directory
            current_file: format!(
                "Found {} files, {:.1} MB",
                total_count,
                total_size_bytes as f64 / 1_048_576.0
            ),
        });
    }

    // Phase 3: Delete in reverse order (files first, then directories)
    let mut files_deleted = 0u64;
    let mut bytes_deleted = 0u64;
    let progress_interval = std::cmp::max(1, total_count / 20); // Send update every ~5% or 1 file

    for (idx, entry) in entries.iter().rev().enumerate() {
        if entry.is_file {
            if fs::remove_file(&entry.path).is_ok() {
                total_bytes += entry.size;
                total_files += 1;
                files_deleted += 1;
                bytes_deleted += entry.size;
            } else {
                errors.push(format!("Failed to delete {}", entry.path.display()));
            }
        } else if fs::remove_dir(&entry.path).is_ok() {
            total_files += 1;
        } else {
            errors.push(format!("Failed to delete {}", entry.path.display()));
        }

        // Send periodic progress updates
        if idx % progress_interval as usize == 0 || idx == entries.len() - 1 {
            if let Some(ref tx) = progress_tx {
                // Check if channel is still connected (receiver hasn't dropped)
                match tx.send(DeleteProgressUpdate::Progress {
                    bytes_done: bytes_deleted,
                    bytes_total: total_size_bytes,
                    files_done: files_deleted,
                    files_total: total_count + 1,
                    current_file: entry
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("<unknown>")
                        .to_string(),
                }) {
                    Ok(_) => {}
                    Err(_) => {
                        // Channel disconnected, UI is gone - abort early
                        break;
                    }
                }
            }
        }
    }

    // Phase 4: Finally, remove root directory itself
    if let Err(e) = fs::remove_dir(path) {
        errors.push(format!("Failed to remove root directory: {}", e));
    } else {
        total_files += 1;
    }

    // Phase 5: Send final progress (NOT Complete - let app.rs handle that)
    if let Some(ref tx) = progress_tx {
        let _ = tx.send(DeleteProgressUpdate::Progress {
            bytes_done: total_bytes,
            bytes_total: total_size_bytes,
            files_done: total_files,
            files_total: total_count + 1,
            current_file: "Complete".to_string(),
        });
    }

    Ok(DeleteResult {
        total_bytes,
        total_files,
        errors,
    })
}

pub fn dry_run_delete(path: &PathBuf) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(path)
        .same_file_system(true) // Don't cross into mounted volumes
        .into_iter()
        .filter_map(|e| e.ok())
    {
        files.push(entry.path().to_path_buf());
    }

    files.push(path.clone());
    Ok(files)
}
