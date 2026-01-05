use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GitProgress {
    pub repo: PathBuf,
    pub completed: bool,
    pub message: Option<String>,
}

pub fn find_git_repos(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut repos = Vec::new();

    for root in paths {
        if !root.exists() {
            continue;
        }

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir() && is_git_dir(path) {
                if let Some(repo_root) = path.parent() {
                    repos.push(repo_root.to_path_buf());
                }
            }
        }
    }

    repos.sort();
    repos.dedup();
    repos
}

pub fn run_git_gc(repo: &Path) -> std::io::Result<()> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("gc")
        .arg("--prune=now")
        .arg("--quiet")
        .status()
        .map(|_| ())
}

fn is_git_dir(path: &Path) -> bool {
    path.file_name().map(|n| n == ".git").unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn finds_git_repositories() {
        let tmp = tempdir().unwrap();
        let repo_path = tmp.path().join("project");
        std::fs::create_dir_all(repo_path.join(".git")).unwrap();

        let repos = find_git_repos(&[tmp.path().to_path_buf()]);
        assert_eq!(repos, vec![repo_path]);
    }

    #[test]
    fn runs_git_gc_on_repo() {
        let tmp = tempdir().unwrap();
        let repo_path = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        // initialize repository
        let status = Command::new("git")
            .arg("-C")
            .arg(&repo_path)
            .arg("init")
            .status()
            .unwrap();
        assert!(status.success());

        let result = run_git_gc(&repo_path);
        assert!(result.is_ok());
    }
}
