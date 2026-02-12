// Platform-specific features
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dirs::{cache_dir, config_dir, data_dir, home_dir};

// ==== Platform Paths (path resolution for rules) ====

#[derive(Debug, Clone)]
pub struct PlatformPaths {
    pub home_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Platform {
    MacOs,
    Linux,
    Windows,
    Unknown,
}

impl PlatformPaths {
    pub fn detect() -> Option<Self> {
        let home = home_dir()?;
        let cache = cache_dir().unwrap_or_else(|| home.join(".cache"));
        let config = config_dir().unwrap_or_else(|| home.join(".config"));
        let data = data_dir().unwrap_or_else(|| home.join(".local/share"));

        Some(PlatformPaths {
            home_dir: home,
            cache_dir: cache,
            config_dir: config,
            data_dir: data,
        })
    }

    pub fn resolve_path(&self, template: &str) -> Option<PathBuf> {
        let mut resolved = template.to_string();

        let tokens: HashMap<&str, &PathBuf> = HashMap::from([
            ("${CACHE_DIR}", &self.cache_dir),
            ("${HOME}", &self.home_dir),
            ("${CONFIG_DIR}", &self.config_dir),
            ("${DATA_DIR}", &self.data_dir),
        ]);

        for (token, path) in tokens {
            if resolved.contains(token) {
                resolved = resolved.replace(token, &path.to_string_lossy());
            }
        }

        if let Some(stripped) = resolved.strip_prefix('~') {
            let trimmed = stripped.strip_prefix('/').unwrap_or(stripped);
            return Some(self.home_dir.join(trimmed));
        }

        Some(PathBuf::from(resolved))
    }
}

#[allow(dead_code)]
pub fn resolve_path(template: &str) -> Option<PathBuf> {
    let paths = PlatformPaths::detect()?;
    paths.resolve_path(template)
}

#[allow(dead_code)]
pub fn current_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(target_os = "linux") {
        Platform::Linux
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Unknown
    }
}

// ==== Disk Space (for TUI display) ====

#[derive(Debug, Clone)]
pub struct DiskSpace {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

/// Get disk space information for the filesystem containing the given path
/// Works on both macOS (APFS, HFS+, etc.) and Linux (ext4, btrfs, xfs, etc.)
#[cfg(unix)]
pub fn get_disk_space(path: &Path) -> Option<DiskSpace> {
    use nix::sys::statvfs::statvfs;

    match statvfs(path) {
        Ok(stat) => {
            // IMPORTANT: Use fragment_size() (f_frsize), NOT block_size() (f_bsize)!
            // f_frsize is the fundamental filesystem block size (usually 4KB)
            // f_bsize is the preferred I/O block size (can be 1MB on APFS, giving wrong results!)
            let fragment_size = stat.fragment_size();
            // These casts are needed on macOS (returns i64) but are no-ops on Linux (u64)
            #[allow(clippy::unnecessary_cast)]
            let total_blocks = stat.blocks() as u64;
            #[allow(clippy::unnecessary_cast)]
            let free_blocks = stat.blocks_free() as u64;
            #[allow(clippy::unnecessary_cast)]
            let available_blocks = stat.blocks_available() as u64;

            let total_bytes = total_blocks * fragment_size;
            let free_bytes = free_blocks * fragment_size;
            let available_bytes = available_blocks * fragment_size;
            // Use free_bytes (not available_bytes) for accurate "used" calculation
            // This correctly accounts for reserved space (e.g., 5% reserved for root on ext4)
            let used_bytes = total_bytes.saturating_sub(free_bytes);

            Some(DiskSpace {
                total_bytes,
                available_bytes,
                used_bytes,
            })
        }
        Err(_) => None,
    }
}

#[cfg(not(unix))]
pub fn get_disk_space(_path: &Path) -> Option<DiskSpace> {
    // Windows support could be added here using GetDiskFreeSpaceEx
    None
}

#[cfg(target_os = "macos")]
pub mod _macos {
    // TODO: Future macOS-specific features:
    // - APFS snapshot removal
    // - Extended attributes (xattr) cleanup
    // - ACL handling
}

#[cfg(target_os = "linux")]
pub mod _linux {
    // TODO: Future Linux-specific features:
    // - Immutable flag removal (chattr -i)
    // - SELinux context handling
    // - Extended attributes cleanup
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resolves_tokens_and_tilde() {
        let tmp = tempdir().unwrap();
        let paths = PlatformPaths {
            home_dir: tmp.path().join("home"),
            cache_dir: tmp.path().join("cache"),
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
        };

        let cache_path = paths.resolve_path("${CACHE_DIR}/files").unwrap();
        assert_eq!(cache_path, paths.cache_dir.join("files"));

        let home_path = paths.resolve_path("~/downloads").unwrap();
        assert_eq!(home_path, paths.home_dir.join("downloads"));

        let config_path = paths.resolve_path("${CONFIG_DIR}").unwrap();
        assert_eq!(config_path, paths.config_dir);
    }

    #[test]
    fn detect_returns_some_path_info() {
        let detected = PlatformPaths::detect();
        assert!(detected.is_some());
    }
}
