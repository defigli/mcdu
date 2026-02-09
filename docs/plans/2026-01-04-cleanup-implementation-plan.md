# mcdu cleanup - Implementation Plan

High-level implementation plan for the cleanup feature. Reference the design document for details.

## Phase 1: Core Infrastructure

### 1.1 Create cleanup module structure
```
src/cleanup/
├── mod.rs
├── config.rs
├── rules.rs
├── platform.rs
└── scanner.rs
```

### 1.2 Platform path resolution (`platform.rs`)
- Implement `resolve_path()` with `${CACHE_DIR}`, `${HOME}`, `${CONFIG_DIR}`, `${DATA_DIR}`
- Detect platform and return correct paths
- Handle `~` expansion

### 1.3 Rule definitions (`rules.rs`)
- `struct Rule { name, category, pattern, path, signature, min_age, min_size, risky, enabled }`
- `struct Candidate { path, rule_name, size_bytes, last_accessed, is_active }`
- Pattern matching with glob crate

### 1.4 Config parsing (`config.rs`)
- Parse TOML from `~/.config/mcdu/cleanup.toml`
- Merge with embedded defaults
- Load state from `~/.config/mcdu/cleanup-state.toml`

## Phase 2: Scanner

### 2.1 Scanner implementation (`scanner.rs`)
- Walk configured scan paths
- Match against rules (pattern + signature validation)
- Calculate sizes (reuse existing `dir_size_bytes` logic)
- Check `mtime` for active project protection
- Return grouped candidates by category

### 2.2 Threading
- Run scanner in background thread (like existing scan)
- `mpsc::channel` for progress updates
- `ScanProgress { current_path, found_count, total_size }`

## Phase 3: Executor

### 3.1 Create executor (`executor.rs`)
- Take list of selected candidates
- Delete in background thread
- Progress via channel: `CleanupProgress { path, current, total, freed_bytes }`
- Error handling per-item (continue on failure)

### 3.2 Git maintenance (`git.rs`)
- Find git repos in scan paths
- Run `git gc --prune=now`
- Optional: `--aggressive`, `reflog expire`, `remote prune`
- Progress per repo

## Phase 4: TUI

### 4.1 Cleanup view state
- Add to `App`: `cleanup_mode: bool`, `cleanup_state: CleanupState`
- `CleanupState { categories, candidates, selected, scanning, deleting }`

### 4.2 Cleanup UI (`cleanup_ui.rs`)
- Render category tree with checkboxes
- Handle expand/collapse
- Show sizes, counts, "(active)", "(risky)" markers
- Progress overlays for scanning/deleting

### 4.3 Keybindings
- `C` from main view enters cleanup mode
- `Space` toggle, `Enter` expand, `a` all, `n` none
- `d` delete, `D` dry-run, `q` back

### 4.4 State persistence
- Save selections to `cleanup-state.toml` on exit
- Load on startup

## Phase 5: CLI

### 5.1 Add clap subcommand
```rust
#[derive(Subcommand)]
enum Commands {
    Cleanup {
        path: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        run: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        list_rules: bool,
        #[arg(long)]
        reset_state: bool,
    }
}
```

### 5.2 CLI execution
- `--list-rules`: print rules and exit
- `--dry-run`: scan and print candidates
- `--run --yes`: execute with saved state
- No flags: launch TUI cleanup view

## Phase 6: Starlark (feature-gated)

### 6.1 Add feature and dependency
```toml
[features]
starlark = ["dep:starlark"]
```

### 6.2 Starlark runtime (`starlark.rs`)
- Parse `cleanup.star` if exists
- Register functions: `rule()`, `settings()`, `resolve_path()`, `platform()`, etc.
- File check functions: `file_exists()`, `file_age_hours()`, `git_has_uncommitted()`
- Custom `condition` function support

## Implementation Order

1. `platform.rs` - path resolution (can test standalone)
2. `rules.rs` - rule structs and matching
3. `config.rs` - TOML parsing with defaults
4. `scanner.rs` - find candidates
5. `executor.rs` - delete logic
6. `cleanup_ui.rs` - TUI view
7. CLI integration
8. State persistence
9. Git maintenance
10. Starlark (last, feature-gated)

## Default Rules File

Create `src/cleanup/defaults.toml` embedded with `include_str!()` containing all the default rules from the design doc.

## Testing Strategy

- Unit tests for path resolution per platform
- Unit tests for rule matching
- Integration test with temp directories
- Test TOML parsing with various configs

## Dependencies to Add

```toml
[dependencies]
glob = "0.3"           # Pattern matching
toml = "0.8"           # Config parsing
dirs = "5.0"           # XDG paths

[dependencies.starlark]
version = "0.12"
optional = true
```
