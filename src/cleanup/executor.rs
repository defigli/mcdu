use crate::cleanup::rules::Candidate;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CleanupProgress {
    pub path: PathBuf,
    pub current: u64,
    pub total: u64,
    pub freed_bytes: u64,
}

#[derive(Debug, Default, Clone)]
pub struct CleanupResult {
    pub freed_bytes: u64,
    pub errors: Vec<(PathBuf, String)>,
}

pub fn execute_async(
    candidates: Vec<Candidate>,
    progress_tx: Option<mpsc::Sender<CleanupProgress>>,
) -> thread::JoinHandle<CleanupResult> {
    thread::spawn(move || execute(candidates, progress_tx))
}

pub fn execute(
    candidates: Vec<Candidate>,
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
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn candidate(path: PathBuf, size: u64) -> Candidate {
        Candidate::new(path, "rule".into(), "**/*".into(), size, None, false)
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
        let result = execute(candidates, Some(tx));

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

        let result = execute(candidates, None);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.freed_bytes, 2);
        assert!(!existing.exists());
    }
}
