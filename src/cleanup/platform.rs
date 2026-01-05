use std::path::PathBuf;
use std::collections::HashMap;

use dirs::{cache_dir, config_dir, data_dir, home_dir};

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

pub fn resolve_path(template: &str) -> Option<PathBuf> {
    let paths = PlatformPaths::detect()?;
    paths.resolve_path(template)
}

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
