use std::path::Path;

/// Directories under ~/Library that contain app-specific data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryDir {
    ApplicationSupport,
    Caches,
    Containers,
    SavedApplicationState,
    HttpStorageData,
    WebKit,
    Preferences,
}

impl LibraryDir {
    /// All directories we scan for orphaned data
    pub fn all() -> &'static [LibraryDir] {
        &[
            LibraryDir::ApplicationSupport,
            LibraryDir::Caches,
            LibraryDir::Containers,
            LibraryDir::SavedApplicationState,
            LibraryDir::HttpStorageData,
            LibraryDir::WebKit,
            LibraryDir::Preferences,
        ]
    }

    /// Relative path from ~/Library
    pub fn relative_path(&self) -> &'static str {
        match self {
            LibraryDir::ApplicationSupport => "Application Support",
            LibraryDir::Caches => "Caches",
            LibraryDir::Containers => "Containers",
            LibraryDir::SavedApplicationState => "Saved Application State",
            LibraryDir::HttpStorageData => "HTTPStorageData",
            LibraryDir::WebKit => "WebKit",
            LibraryDir::Preferences => "Preferences",
        }
    }

    /// Whether entries in this directory are files (not directories)
    pub fn entries_are_files(&self) -> bool {
        matches!(self, LibraryDir::Preferences)
    }

    /// Suffix to strip from entry names to get the bundle ID
    pub fn entry_suffix(&self) -> Option<&'static str> {
        match self {
            LibraryDir::SavedApplicationState => Some(".savedState"),
            LibraryDir::Preferences => Some(".plist"),
            _ => None,
        }
    }
}

/// Extract a bundle ID from a directory/file entry name, stripping any suffix
pub fn extract_bundle_id(entry_name: &str, dir_type: LibraryDir) -> Option<String> {
    let id = if let Some(suffix) = dir_type.entry_suffix() {
        entry_name.strip_suffix(suffix)?
    } else {
        entry_name
    };

    // Bundle IDs use reverse-DNS notation (at least two dots typically)
    // But some are short like "MobileDevice" — we accept anything that
    // isn't obviously not a bundle ID
    if id.is_empty() || id.starts_with('.') {
        return None;
    }

    Some(id.to_string())
}

/// Returns true for Apple system bundles that should never be flagged as orphans
pub fn is_system_bundle(bundle_id: &str) -> bool {
    bundle_id.starts_with("com.apple.")
}

/// Check if a name looks like a reverse-DNS bundle identifier
pub fn looks_like_bundle_id(name: &str) -> bool {
    // Must contain at least one dot and no spaces
    name.contains('.') && !name.contains(' ')
}

/// Build the full path for a library directory
pub fn library_dir_path(home: &Path, dir_type: LibraryDir) -> std::path::PathBuf {
    home.join("Library").join(dir_type.relative_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plain_bundle_id() {
        let id = extract_bundle_id("com.example.app", LibraryDir::Caches);
        assert_eq!(id, Some("com.example.app".to_string()));
    }

    #[test]
    fn extract_saved_state_bundle_id() {
        let id = extract_bundle_id(
            "com.example.app.savedState",
            LibraryDir::SavedApplicationState,
        );
        assert_eq!(id, Some("com.example.app".to_string()));
    }

    #[test]
    fn extract_plist_bundle_id() {
        let id = extract_bundle_id("com.example.app.plist", LibraryDir::Preferences);
        assert_eq!(id, Some("com.example.app".to_string()));
    }

    #[test]
    fn rejects_hidden_entries() {
        let id = extract_bundle_id(".hidden", LibraryDir::Caches);
        assert_eq!(id, None);
    }

    #[test]
    fn saved_state_without_suffix_returns_none() {
        let id = extract_bundle_id("com.example.app", LibraryDir::SavedApplicationState);
        assert_eq!(id, None);
    }

    #[test]
    fn system_bundle_detection() {
        assert!(is_system_bundle("com.apple.Safari"));
        assert!(is_system_bundle("com.apple.dock"));
        assert!(!is_system_bundle("com.example.app"));
    }

    #[test]
    fn bundle_id_heuristic() {
        assert!(looks_like_bundle_id("com.example.app"));
        assert!(looks_like_bundle_id("org.mozilla.firefox"));
        assert!(!looks_like_bundle_id("SomeFolder"));
        assert!(!looks_like_bundle_id("My App Support"));
    }
}
