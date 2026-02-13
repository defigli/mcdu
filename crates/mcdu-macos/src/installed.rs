use std::collections::HashSet;
use std::process::Command;

/// Discover all installed application bundle IDs using Spotlight (mdfind)
pub fn get_installed_bundle_ids() -> HashSet<String> {
    let mut ids = HashSet::new();

    // Use mdfind to find all .app bundles indexed by Spotlight
    let app_paths = match Command::new("mdfind")
        .arg("kMDItemContentType == 'com.apple.application-bundle'")
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).to_string()
        }
        _ => return ids,
    };

    for app_path in app_paths.lines() {
        let app_path = app_path.trim();
        if app_path.is_empty() {
            continue;
        }

        if let Some(bundle_id) = read_bundle_id(app_path) {
            ids.insert(bundle_id);
        }
    }

    ids
}

/// Read CFBundleIdentifier from an .app bundle's Info.plist
fn read_bundle_id(app_path: &str) -> Option<String> {
    let plist_path = format!("{}/Contents/Info.plist", app_path);

    let output = Command::new("defaults")
        .arg("read")
        .arg(&plist_path)
        .arg("CFBundleIdentifier")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Get bundle IDs of currently running processes via launchctl
/// Used to avoid flagging data for running services/daemons
pub fn get_running_bundle_ids() -> HashSet<String> {
    let mut ids = HashSet::new();

    let output = match Command::new("launchctl").arg("list").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return ids,
    };

    // launchctl list outputs: PID\tStatus\tLabel
    // Labels are often bundle IDs or bundle-ID-prefixed
    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let label = parts[2].trim();
            // Only include labels that look like bundle IDs (reverse-DNS)
            if label.contains('.') && !label.contains(' ') {
                ids.insert(label.to_string());
                // Also include the base bundle ID if this is a sub-service
                // e.g., "com.example.app.helper" → also insert "com.example.app"
                if let Some(base) = strip_service_suffix(label) {
                    ids.insert(base);
                }
            }
        }
    }

    ids
}

/// Strip common service suffixes to get the base bundle ID
fn strip_service_suffix(label: &str) -> Option<String> {
    let suffixes = [".helper", ".agent", ".daemon", ".service", ".launcher"];
    for suffix in &suffixes {
        if let Some(base) = label.strip_suffix(suffix) {
            return Some(base.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_service_suffixes() {
        assert_eq!(
            strip_service_suffix("com.example.app.helper"),
            Some("com.example.app".to_string())
        );
        assert_eq!(
            strip_service_suffix("com.example.app.agent"),
            Some("com.example.app".to_string())
        );
        assert_eq!(strip_service_suffix("com.example.app"), None);
    }

    #[test]
    fn installed_bundle_ids_runs_without_panic() {
        // Verifies the function runs without panicking.
        // On macOS: returns installed apps. On other platforms: returns empty set.
        let _ids = get_installed_bundle_ids();
    }
}
