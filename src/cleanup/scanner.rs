use crate::cleanup::config::CleanupConfig;
use crate::cleanup::platform::PlatformPaths;
use crate::cleanup::rules::{Candidate, Rule};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScanProgress {
    pub current_path: Option<PathBuf>,
    pub found_count: u64,
    pub total_size: u64,
}

pub fn scan(
    config: &CleanupConfig,
    platform_paths: &PlatformPaths,
    progress_tx: Option<mpsc::Sender<ScanProgress>>,
    now: SystemTime,
) -> Vec<Candidate> {
    let mut results = Vec::new();
    let mut found_count = 0u64;
    let mut total_size = 0u64;
    let scan_paths: Vec<PathBuf> = config
        .scan_paths
        .iter()
        .filter_map(|p| platform_paths.resolve_path(p))
        .collect();

    for rule in &config.rules {
        scan_rule(
            rule,
            platform_paths,
            progress_tx.as_ref(),
            now,
            &mut found_count,
            &mut total_size,
            &mut results,
            &scan_paths,
        );
    }

    results
}

fn scan_rule(
    rule: &Rule,
    platform_paths: &PlatformPaths,
    progress_tx: Option<&mpsc::Sender<ScanProgress>>,
    now: SystemTime,
    found_count: &mut u64,
    total_size: &mut u64,
    results: &mut Vec<Candidate>,
    scan_paths: &[PathBuf],
) {
    let base_path = match rule.base_path(platform_paths) {
        Some(path) => path,
        None => return,
    };

    if !scan_paths.is_empty()
        && !scan_paths.iter().any(|p| base_path.starts_with(p) || p.starts_with(&base_path))
    {
        return;
    }

    if !base_path.exists() {
        return;
    }

    for entry in WalkDir::new(&base_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if !metadata.is_file() {
            continue;
        }

        if !rule.matches(platform_paths, path, &metadata, now) {
            continue;
        }

        let size = file_size(&metadata);
        let last_accessed = metadata.accessed().ok();
        let is_active = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .map(|duration| duration < std::time::Duration::from_secs(48 * 3600))
            .unwrap_or(false);

        results.push(Candidate::new(
            path.to_path_buf(),
            rule.name.clone(),
            rule.pattern.clone(),
            size,
            last_accessed,
            is_active,
        ));

        *found_count += 1;
        *total_size += size;

        if let Some(tx) = progress_tx {
            let _ = tx.send(ScanProgress {
                current_path: Some(path.to_path_buf()),
                found_count: *found_count,
                total_size: *total_size,
            });
        }
    }
}

fn file_size(metadata: &std::fs::Metadata) -> u64 {
    if metadata.is_file() {
        metadata.len()
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleanup::config::CleanupConfig;
    use crate::cleanup::platform::PlatformPaths;
    use crate::cleanup::rules::Rule;
    use std::fs;
    use std::io::Write;
    use std::time::Duration;
    use tempfile::tempdir;

    fn platform_paths(tmp: &tempfile::TempDir) -> PlatformPaths {
        PlatformPaths {
            home_dir: tmp.path().join("home"),
            cache_dir: tmp.path().join("cache"),
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
        }
    }

    fn rule_for_cache() -> Rule {
        Rule {
            name: "logs".into(),
            category: "cache".into(),
            pattern: "**/*.log".into(),
            path: "${CACHE_DIR}".into(),
            signature: None,
            min_age_hours: Some(1),
            min_size_bytes: None,
            risky: false,
            enabled: true,
        }
    }

    #[test]
    fn scan_collects_matching_candidates() {
        let tmp = tempdir().unwrap();
        let paths = platform_paths(&tmp);
        let rule = rule_for_cache();
        let config = CleanupConfig {
            scan_paths: vec!["${CACHE_DIR}".into()],
            rules: vec![rule.clone()],
        };

        let target_dir = paths.cache_dir.join("nested");
        fs::create_dir_all(&target_dir).unwrap();
        let file_path = target_dir.join("file.log");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(f, "hello").unwrap();
        let metadata = fs::metadata(&file_path).unwrap();
        let now = metadata
            .modified()
            .unwrap()
            .checked_add(Duration::from_secs(7200))
            .unwrap();

        let results = scan(&config, &paths, None, now);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, file_path);
        assert_eq!(results[0].rule_name, rule.name);
        assert_eq!(results[0].rule_pattern, rule.pattern);
    }

    #[test]
    fn scan_reports_progress() {
        let tmp = tempdir().unwrap();
        let paths = platform_paths(&tmp);
        let rule = rule_for_cache();
        let config = CleanupConfig {
            scan_paths: vec!["${CACHE_DIR}".into()],
            rules: vec![rule],
        };

        let file_path = paths.cache_dir.join("file.log");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, "hello").unwrap();
        let metadata = fs::metadata(&file_path).unwrap();
        let now = metadata
            .modified()
            .unwrap()
            .checked_add(Duration::from_secs(7200))
            .unwrap();

        let (tx, rx) = mpsc::channel();
        let results = scan(&config, &paths, Some(tx), now);
        assert!(!results.is_empty());
        let progress: Vec<_> = rx.try_iter().collect();
        assert!(!progress.is_empty());
    }
}
