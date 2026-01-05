use crate::cleanup::platform::PlatformPaths;
use glob::Pattern;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub name: String,
    pub category: String,
    pub pattern: String,
    pub path: String,
    pub signature: Option<String>,
    pub min_age_hours: Option<u64>,
    pub min_size_bytes: Option<u64>,
    pub risky: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub rule_name: String,
    pub rule_pattern: String,
    pub size_bytes: u64,
    pub last_accessed: Option<SystemTime>,
    pub is_active: bool,
}

impl Rule {
    pub fn glob(&self) -> Result<Pattern, glob::PatternError> {
        Pattern::new(&self.pattern)
    }

    pub fn base_path(&self, platform_paths: &PlatformPaths) -> Option<PathBuf> {
        platform_paths.resolve_path(&self.path)
    }

    pub fn matches(
        &self,
        platform_paths: &PlatformPaths,
        candidate_path: &Path,
        metadata: &fs::Metadata,
        now: SystemTime,
    ) -> bool {
        if !self.enabled {
            return false;
        }

        let base_path = match self.base_path(platform_paths) {
            Some(path) => path,
            None => return false,
        };

        if let Some(signature) = &self.signature {
            if !base_path.join(signature).exists() {
                return false;
            }
        }

        let pattern = match self.glob() {
            Ok(p) => p,
            Err(_) => return false,
        };

        let relative = candidate_path.strip_prefix(&base_path).unwrap_or(candidate_path);
        if !pattern.matches_path(relative) {
            return false;
        }

        if let Some(min_size) = self.min_size_bytes {
            if metadata.len() < min_size {
                return false;
            }
        }

        if let Some(min_age_hours) = self.min_age_hours {
            if let Ok(modified) = metadata.modified() {
                let min_age = Duration::from_secs(min_age_hours * 3600);
                if let Ok(age) = now.duration_since(modified) {
                    if age < min_age {
                        return false;
                    }
                } else {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }
}

impl Candidate {
    pub fn new(
        path: PathBuf,
        rule_name: String,
        rule_pattern: String,
        size_bytes: u64,
        last_accessed: Option<SystemTime>,
        is_active: bool,
    ) -> Self {
        Candidate {
            path,
            rule_name,
            rule_pattern,
            size_bytes,
            last_accessed,
            is_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;
    use tempfile::tempdir;

    fn build_rule() -> Rule {
        Rule {
            name: "old-logs".to_string(),
            category: "logs".to_string(),
            pattern: "**/*.log".to_string(),
            path: "${CACHE_DIR}".to_string(),
            signature: None,
            min_age_hours: Some(1),
            min_size_bytes: Some(1),
            risky: false,
            enabled: true,
        }
    }

    fn platform_paths(tmp: &tempfile::TempDir) -> PlatformPaths {
        PlatformPaths {
            home_dir: tmp.path().join("home"),
            cache_dir: tmp.path().join("cache"),
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
        }
    }

    #[test]
    fn matches_glob_and_age_and_size() {
        let tmp = tempdir().unwrap();
        let paths = platform_paths(&tmp);
        let rule = build_rule();

        let target_dir = paths.cache_dir.join("nested");
        std::fs::create_dir_all(&target_dir).unwrap();
        let target_file = target_dir.join("file.log");
        let mut file = std::fs::File::create(&target_file).unwrap();
        writeln!(file, "hello").unwrap();

        let metadata = std::fs::metadata(&target_file).unwrap();
        let now = metadata
            .modified()
            .unwrap()
            .checked_add(Duration::from_secs(7200))
            .unwrap();

        assert!(rule.matches(&paths, &target_file, &metadata, now));
    }

    #[test]
    fn fails_when_signature_missing() {
        let tmp = tempdir().unwrap();
        let mut rule = build_rule();
        rule.signature = Some("marker.txt".to_string());

        let paths = platform_paths(&tmp);
        let target_file = paths.cache_dir.join("file.log");
        std::fs::create_dir_all(target_file.parent().unwrap()).unwrap();
        std::fs::write(&target_file, "hi").unwrap();
        let metadata = std::fs::metadata(&target_file).unwrap();
        let now = metadata.modified().unwrap();

        assert!(!rule.matches(&paths, &target_file, &metadata, now));
    }

    #[test]
    fn disabled_rule_never_matches() {
        let tmp = tempdir().unwrap();
        let mut rule = build_rule();
        rule.enabled = false;

        let paths = platform_paths(&tmp);
        let target_file = paths.cache_dir.join("file.log");
        std::fs::create_dir_all(target_file.parent().unwrap()).unwrap();
        std::fs::write(&target_file, "hi").unwrap();
        let metadata = std::fs::metadata(&target_file).unwrap();
        let now = metadata.modified().unwrap();

        assert!(!rule.matches(&paths, &target_file, &metadata, now));
    }
}
