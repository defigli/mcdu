use crate::cleanup::git;
use crate::cleanup::rules::Candidate;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

#[derive(Debug, Clone, Copy)]
pub enum CleanupStage {
    Files,
    Git,
}

#[derive(Debug, Clone)]
pub struct CleanupProgress {
    pub path: PathBuf,
    pub current: u64,
    pub total: u64,
    pub freed_bytes: u64,
    pub stage: CleanupStage,
}

#[derive(Debug, Default, Clone)]
pub struct CleanupResult {
    pub freed_bytes: u64,
    pub errors: Vec<(PathBuf, String)>,
}

pub fn execute_async(
    candidates: Vec<Candidate>,
    run_git: bool,
    git_roots: Vec<PathBuf>,
    progress_tx: Option<mpsc::Sender<CleanupProgress>>,
) -> thread::JoinHandle<CleanupResult> {
    thread::spawn(move || execute(candidates, run_git, git_roots, progress_tx))
}

pub fn execute(
    candidates: Vec<Candidate>,
    run_git: bool,
    git_roots: Vec<PathBuf>,
    progress_tx: Option<mpsc::Sender<CleanupProgress>>,
) -> CleanupResult {
    let total = candidates.len() as u64;
    let mut result = CleanupResult::default();

    for (idx, candidate) in candidates.into_iter().enumerate() {
        let path = candidate.path.clone();
        let size = candidate.size_bytes;
        let deletion = if path.is_file() {
            fs::remove_file(&path)
        } else {
            fs::remove_dir_all(&path)
        };

        if deletion.is_ok() {
            result.freed_bytes += size;
        } else if let Err(err) = deletion {
            result.errors.push((path.clone(), err.to_string()));
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(CleanupProgress {
                path,
                current: (idx as u64) + 1,
                total,
                freed_bytes: result.freed_bytes,
                stage: CleanupStage::Files,
            });
        }
    }

    if run_git {
        let repos = git::find_git_repos(&git_roots);
        let total_git = repos.len() as u64;
        for (idx, repo) in repos.into_iter().enumerate() {
            let _ = git::run_git_gc(&repo);
            if let Some(tx) = &progress_tx {
                let _ = tx.send(CleanupProgress {
                    path: repo,
                    current: (idx as u64) + 1,
                    total: total_git,
                    freed_bytes: result.freed_bytes,
                    stage: CleanupStage::Git,
                });
            }
        }
    }

    result
}

pub fn dry_run(candidates: Vec<Candidate>) -> CleanupResult {
    let total: u64 = candidates.iter().map(|c| c.size_bytes).sum();
    CleanupResult {
        freed_bytes: total,
        errors: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn candidate(path: PathBuf, size: u64) -> Candidate {
        Candidate::new(
            path,
            "rule".into(),
            "category".into(),
            "**/*".into(),
            size,
            None,
            false,
        )
    }

    #[test]
    fn deletes_files_and_reports_progress() {
        let tmp = tempdir().unwrap();
        let file_a = tmp.path().join("a.txt");
        let file_b = tmp.path().join("b.txt");
        let mut fa = std::fs::File::create(&file_a).unwrap();
        writeln!(fa, "hello").unwrap();
        let mut fb = std::fs::File::create(&file_b).unwrap();
        writeln!(fb, "world!").unwrap();

        let candidates = vec![candidate(file_a.clone(), 6), candidate(file_b.clone(), 7)];
        let (tx, rx) = mpsc::channel();
        let result = execute(candidates, false, Vec::new(), Some(tx));

        assert!(!file_a.exists());
        assert!(!file_b.exists());
        assert_eq!(result.freed_bytes, 13);

        let progress: Vec<_> = rx.try_iter().collect();
        assert_eq!(progress.len(), 2);
        assert_eq!(progress.last().unwrap().current, 2);
    }

    #[test]
    fn continues_on_errors() {
        let tmp = tempdir().unwrap();
        let missing = tmp.path().join("missing.txt");
        let existing = tmp.path().join("exists.txt");
        std::fs::write(&existing, "hi").unwrap();

        let candidates = vec![
            candidate(missing.clone(), 5),
            candidate(existing.clone(), 2),
        ];

        let result = execute(candidates, false, Vec::new(), None);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.freed_bytes, 2);
        assert!(!existing.exists());
    }

    #[test]
    fn dry_run_sums_sizes_without_deleting() {
        let tmp = tempdir().unwrap();
        let file_a = tmp.path().join("a.txt");
        std::fs::write(&file_a, "hello").unwrap();
        let metadata = std::fs::metadata(&file_a).unwrap();
        let size = metadata.len();
        let candidates = vec![candidate(file_a.clone(), size)];
        let result = dry_run(candidates);
        assert_eq!(result.freed_bytes, size);
        assert!(file_a.exists());
    }
}
