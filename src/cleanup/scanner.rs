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

#[derive(Debug, Clone)]
pub struct CategoryGroup {
    pub name: String,
    pub candidates: Vec<Candidate>,
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
            rule.category.clone(),
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

pub fn group_by_category(candidates: Vec<Candidate>) -> Vec<CategoryGroup> {
    let mut grouped: std::collections::BTreeMap<String, Vec<Candidate>> = std::collections::BTreeMap::new();
    for cand in candidates {
        grouped
            .entry(cand.rule_category.clone())
            .or_default()
            .push(cand);
    }

    grouped
        .into_iter()
        .map(|(name, candidates)| CategoryGroup { name, candidates })
        .collect()
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
        assert_eq!(results[0].rule_category, rule.category);
        assert_eq!(results[0].rule_pattern, rule.pattern);
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

    #[test]
    fn groups_candidates_by_category() {
        let tmp = tempdir().unwrap();
        let paths = platform_paths(&tmp);
        let mut rule_a = rule_for_cache();
        rule_a.category = "cat-a".into();
        rule_a.min_age_hours = None;
        rule_a.min_size_bytes = None;
        let mut rule_b = rule_for_cache();
        rule_b.category = "cat-b".into();
        rule_b.pattern = "**/*.txt".into();
        rule_b.min_age_hours = None;
        rule_b.min_size_bytes = None;

        let config = CleanupConfig {
            scan_paths: vec!["${CACHE_DIR}".into()],
            rules: vec![rule_a.clone(), rule_b.clone()],
        };

        let file_a = paths.cache_dir.join("one.log");
        let file_b = paths.cache_dir.join("two.txt");
        fs::create_dir_all(paths.cache_dir.clone()).unwrap();
        fs::write(&file_a, "a").unwrap();
        fs::write(&file_b, "b").unwrap();
        let now = std::time::SystemTime::now();

        let grouped = group_by_category(scan(&config, &paths, None, now));
        let mut cats: Vec<_> = grouped.iter().map(|g| g.name.as_str()).collect();
        cats.sort();
        assert_eq!(cats, vec!["cat-a", "cat-b"]);
    }
}
