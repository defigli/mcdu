use crate::cleanup::rules::Candidate;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub struct CleanupViewState {
    pub candidates: Vec<Candidate>,
    pub selected: HashSet<PathBuf>,
    pub scanning: bool,
    pub deleting: bool,
}

impl CleanupViewState {
    pub fn new(candidates: Vec<Candidate>) -> Self {
        CleanupViewState {
            candidates,
            selected: HashSet::new(),
            scanning: false,
            deleting: false,
        }
    }

    pub fn toggle_selection(&mut self, path: &PathBuf) {
        if self.selected.contains(path) {
            self.selected.remove(path);
        } else {
            self.selected.insert(path.clone());
        }
    }

    pub fn is_selected(&self, path: &PathBuf) -> bool {
        self.selected.contains(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(path: &str) -> Candidate {
        Candidate::new(
            PathBuf::from(path),
            "rule".into(),
            "category".into(),
            "**/*".into(),
            1,
            None,
            false,
        )
    }

    #[test]
    fn toggles_selection_state() {
        let mut state = CleanupViewState::new(vec![candidate("/tmp/a")]);
        let path = PathBuf::from("/tmp/a");
        assert!(!state.is_selected(&path));
        state.toggle_selection(&path);
        assert!(state.is_selected(&path));
        state.toggle_selection(&path);
        assert!(!state.is_selected(&path));
    }
}
