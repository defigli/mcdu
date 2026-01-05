use crate::cleanup::config::{default_config_paths, load_config, load_state, save_state};
use crate::cleanup::platform::PlatformPaths;
use crate::cleanup::scanner;
use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about = "Cleanup utilities for mcdu")]
pub struct CleanupCommand {
    /// Optional override path to scan
    pub path: Option<PathBuf>,
    /// Print available rules
    #[arg(long)]
    pub list_rules: bool,
    /// Perform scan without deletion
    #[arg(long)]
    pub dry_run: bool,
    /// Execute deletion immediately
    #[arg(long)]
    pub run: bool,
    /// Assume yes to prompts
    #[arg(long)]
    pub yes: bool,
    /// Reset stored state
    #[arg(long)]
    pub reset_state: bool,
}

pub fn run_command(cmd: CleanupCommand) -> Result<(), String> {
    let platform_paths = PlatformPaths::detect().ok_or("Unable to resolve platform paths")?;
    run_command_with_paths(cmd, platform_paths)
}

pub fn run_command_with_paths(
    cmd: CleanupCommand,
    platform_paths: PlatformPaths,
) -> Result<(), String> {
    let config_paths = default_config_paths(&platform_paths);

    if cmd.reset_state {
        let empty = crate::cleanup::config::CleanupState::default();
        save_state(&config_paths, &empty).map_err(|e| e.to_string())?;
    }

    let mut config = load_config(&config_paths).map_err(|e| e.to_string())?;
    let state = load_state(&config_paths).map_err(|e| e.to_string())?;

    if let Some(path) = cmd.path {
        config.scan_paths = vec![path.to_string_lossy().to_string()];
    }

    if cmd.list_rules {
        for rule in &config.rules {
            println!("{} [{}] {}", rule.name, rule.category, rule.path);
        }
        return Ok(());
    }

    let now = std::time::SystemTime::now();
    let candidates = scanner::scan(&config, &platform_paths, None, now);

    if cmd.dry_run || !cmd.run {
        for cand in &candidates {
            println!("{}\t{} bytes", cand.path.display(), cand.size_bytes);
        }
        return Ok(());
    }

    if candidates.is_empty() {
        println!("No candidates found");
        return Ok(());
    }

    if !cmd.yes {
        println!("{} items would be deleted. Re-run with --yes to confirm.", candidates.len());
        return Ok(());
    }

    let result = crate::cleanup::executor::execute(candidates, None);
    save_state(&config_paths, &state).map_err(|e| e.to_string())?;
    if !result.errors.is_empty() {
        for (path, err) in result.errors {
            eprintln!("Failed {}: {}", path.display(), err);
        }
    }
    println!("Freed {} bytes", result.freed_bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleanup::config::CleanupState;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parses_cleanup_command_flags() {
        let args = ["cleanup", "--list-rules", "--dry-run"];
        let cmd = CleanupCommand::parse_from(args);
        assert!(cmd.list_rules);
        assert!(cmd.dry_run);
    }

    #[test]
    fn dry_run_does_not_delete_files() {
        let tmp = tempdir().unwrap();
        let platform_paths = PlatformPaths {
            home_dir: tmp.path().join("home"),
            cache_dir: tmp.path().join("cache"),
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
        };

        let config_dir = platform_paths.config_dir.join("mcdu");
        fs::create_dir_all(&config_dir).unwrap();
        let config = r#"
scan_paths = ["${CACHE_DIR}"]

[[rules]]
name = "all"
category = "test"
path = "${CACHE_DIR}"
pattern = "**/*"
enabled = true
risky = false
"#;
        fs::write(config_dir.join("cleanup.toml"), config).unwrap();

        let file_path = platform_paths.cache_dir.join("file.tmp");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, "hi").unwrap();

        let cmd = CleanupCommand {
            path: None,
            list_rules: false,
            dry_run: true,
            run: false,
            yes: false,
            reset_state: true,
        };

        run_command_with_paths(cmd, platform_paths.clone()).unwrap();
        assert!(file_path.exists());

        // state file should be present after reset_state
        let state_path = default_config_paths(&platform_paths).state_file;
        let contents = fs::read_to_string(state_path).unwrap();
        let saved: CleanupState = toml::from_str(&contents).unwrap();
        assert!(saved.selected.is_empty());
    }
}
