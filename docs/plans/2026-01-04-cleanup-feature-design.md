# mcdu cleanup - Devtools Cleanup Feature Design

## Overview

**mcdu cleanup** is a devtools cleanup feature that allows users to clean up build artifacts, caches, and run git maintenance - both via TUI and CLI.

**Main features:**
- **Three categories**: Dev Projects, App Caches, Package Manager Caches + Git Maintenance
- **Checkbox-based TUI**: Select categories, rules, or individual findings
- **Configurable**: TOML config with path variables, optional Starlark for advanced logic
- **Cross-platform**: macOS, Linux, Windows with platform-specific paths
- **Persistent state**: Remembers selections between sessions
- **Safe**: Dry-run, double confirmation, `--yes` requirement for CLI

## User Flow

### TUI Flow
1. Press `C` from main view
2. mcdu scans all enabled rules
3. Shows categories with checkboxes and sizes
4. User toggles selections with Space, expands with Enter
5. `d` for cleanup, `D` for dry-run
6. Double confirmation before deletion

### CLI Flow
```bash
mcdu cleanup                    # Opens TUI
mcdu cleanup --dry-run          # Show what would be deleted
mcdu cleanup --run --yes        # Run with remembered selections
mcdu cleanup ~/Repos --run --yes # Scan specific folder
mcdu cleanup --list-rules       # Show all available rules
mcdu cleanup --reset-state      # Reset to defaults
```

## Architecture

### New Modules

```
src/
├── cleanup/
│   ├── mod.rs           # Public API, re-exports
│   ├── config.rs        # TOML parsing, path variables
│   ├── scanner.rs       # Find candidates based on rules
│   ├── rules.rs         # Rule structs, matching logic
│   ├── categories.rs    # Category grouping, state
│   ├── executor.rs      # Actual deletion, dry-run
│   ├── git.rs           # Git maintenance operations
│   ├── platform.rs      # Cross-platform path resolution
│   ├── state.rs         # Persist checkbox state
│   └── starlark.rs      # Optional: Starlark runtime (feature-gated)
├── cleanup_ui.rs        # TUI for cleanup view
└── ... (existing files)
```

### Data Flow

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐
│ Config      │───▶│ Scanner      │───▶│ Candidates  │
│(TOML/Starlark)   │ (parallel)   │    │ (grouped)   │
└─────────────┘    └──────────────┘    └─────────────┘
                                              │
                                              ▼
┌─────────────┐    ┌──────────────┐    ┌─────────────┐
│ Result      │◀───│ Executor     │◀───│ TUI/CLI     │
│ (log, stats)│    │ (async)      │    │ (selection) │
└─────────────┘    └──────────────┘    └─────────────┘
```

### Threading Model

```
┌─────────────────────────────────────────────────────────┐
│ Main Thread (TUI)                                       │
│  - Event loop                                           │
│  - Rendering                                            │
│  - Input handling                                       │
│  - Progress updates via mpsc::channel                   │
└─────────────────────────────────────────────────────────┘
        │                              ▲
        │ spawn                        │ CleanupProgress
        ▼                              │
┌─────────────────────────────────────────────────────────┐
│ Scanner Thread                                          │
│  - Parallel directory walking (rayon)                   │
│  - Rule matching                                        │
│  - Size calculation                                     │
│  - Sender<ScanProgress>                                 │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│ Cleanup Thread (when user confirms)                     │
│  - Iterates through selected candidates                 │
│  - Deletes / runs git gc                                │
│  - Sender<CleanupProgress> { current, total, bytes }    │
└─────────────────────────────────────────────────────────┘
```

### Progress Types

```rust
enum CleanupProgress {
    Scanning { current: PathBuf, found: usize },
    Deleting { path: PathBuf, current: usize, total: usize },
    GitMaintenance { repo: PathBuf, current: usize, total: usize },
    Done { deleted_bytes: u64, duration: Duration },
    Error { path: PathBuf, error: String },
}
```

### Cargo Features

```toml
[features]
default = []
starlark = ["dep:starlark"]  # Optional Starlark support
```

## Configuration

### TOML Configuration

**Config file location:** `~/.config/mcdu/cleanup.toml`

#### Path Variables

| Variable | macOS | Linux | Windows |
|----------|-------|-------|---------|
| `${HOME}` | `~` | `~` | `%USERPROFILE%` |
| `${CACHE_DIR}` | `~/Library/Caches` | `~/.cache` | `%LOCALAPPDATA%` |
| `${CONFIG_DIR}` | `~/Library/Application Support` | `~/.config` | `%APPDATA%` |
| `${DATA_DIR}` | `~/Library/Application Support` | `~/.local/share` | `%APPDATA%` |

#### Example Config

```toml
[settings]
skip_if_accessed_within = "24h"
show_skipped_in_ui = true
default_scan_paths = ["~/Repos", "~/Projects"]

# Custom rule
[[rules]]
name = "rust-target"
category = "dev-projects"
pattern = "**/target"
signature = ["CACHEDIR.TAG", "../Cargo.toml"]
enabled = true
risky = false

[[rules]]
name = "node-modules"
category = "dev-projects"
pattern = "**/node_modules"
signature = ["../package.json"]
enabled = true

[[rules]]
name = "vscode-cache"
category = "app-caches"
path = "${CACHE_DIR}/Code/"
enabled = true
min_age = "0s"

[[rules]]
name = "old-downloads"
category = "custom"
path = "~/Downloads"
pattern = "**/*"
min_age = "30d"
min_size = "100MB"
risky = true
enabled = false

[git]
enabled = true
gc_prune = true
gc_aggressive = false
reflog_expire = false
remote_prune = true
```

### Starlark Configuration

**Activation:** `cargo build --features starlark`

**Config file:** `~/.config/mcdu/cleanup.star` (used *instead of* TOML if present)

#### Available Functions

```python
# Path resolving
resolve_path("${CACHE_DIR}/Code/")  # -> "/Users/you/Library/Caches/Code/"
home_dir()                           # -> "/Users/you"
cache_dir()                          # -> "/Users/you/Library/Caches"
config_dir()                         # -> "/Users/you/.config"
platform()                           # -> "macos" | "linux" | "windows"

# File checks (for advanced conditions)
file_exists(path)                    # -> bool
dir_exists(path)                     # -> bool
file_age_hours(path)                 # -> int
file_size_bytes(path)                # -> int
parent_contains(path, filename)      # -> bool
git_has_uncommitted(repo_path)       # -> bool
git_last_commit_age_days(repo_path)  # -> int
```

#### Example cleanup.star

```python
# Settings
settings(
    skip_if_accessed_within_hours = 24,
    show_skipped_in_ui = True,
    default_scan_paths = ["~/Repos", "~/Projects"],
)

# Standard dev rules
rule(
    name = "rust-target",
    category = "dev-projects",
    pattern = "**/target",
    signature = ["CACHEDIR.TAG", "../Cargo.toml"],
)

rule(
    name = "node-modules",
    category = "dev-projects",
    pattern = "**/node_modules",
    signature = ["../package.json"],
)

# Advanced rule with custom logic
def should_clean_gradle(path):
    """Don't delete if project has uncommitted changes."""
    project_root = parent_dir(path)
    if git_has_uncommitted(project_root):
        return False
    if file_age_hours(path) < 24:
        return False
    return True

rule(
    name = "gradle-build",
    category = "dev-projects",
    pattern = "**/build",
    signature = ["../build.gradle", "../build.gradle.kts"],
    condition = should_clean_gradle,
)

# Conditional rule based on platform
if platform() == "macos":
    rule(
        name = "xcode-derived-data",
        category = "dev-projects",
        path = "~/Library/Developer/Xcode/DerivedData",
    )

# Dynamically generated rules
for ide in ["IntelliJIdea", "PyCharm", "WebStorm", "CLion", "GoLand"]:
    rule(
        name = "jetbrains-{}-cache".format(ide.lower()),
        category = "app-caches",
        path = resolve_path("${CACHE_DIR}/JetBrains/{}*".format(ide)),
    )
```

## TUI Design

### Layout

```
┌─ Cleanup Categories ─────────────────────────────────────┐
│                                                          │
│ [x] Dev Projects                          12.4 GB        │
│     [x] Rust target/              (8 funn)    4.2 GB     │
│     [x] Node node_modules/        (23 funn)   6.1 GB     │
│     [x] Python venv/              (3 funn)    1.8 GB     │
│     [ ] Dart .dart_tool/          (5 funn)    0.3 GB     │
│                                                          │
│ [x] App Caches                             3.2 GB        │
│     [x] Chrome                             1.4 GB        │
│     [x] VSCode                             0.8 GB        │
│     [ ] Discord                            0.6 GB        │
│     [ ] Spotify                            0.4 GB        │
│                                                          │
│ [ ] Package Manager Caches                 8.7 GB        │
│     [ ] cargo registry                     3.2 GB        │
│     [ ] npm cache                          2.1 GB        │
│     [ ] Homebrew                           2.0 GB        │
│     [ ] pip cache                          1.4 GB        │
│                                                          │
│ [ ] Git Maintenance                        (45 repos)    │
│                                                          │
├──────────────────────────────────────────────────────────┤
│ Selected: 15.6 GB    [Space] Toggle  [a] All  [n] None   │
│                      [d] Clean  [D] Dry-run  [q] Back    │
└──────────────────────────────────────────────────────────┘
```

### Keybindings

| Key | Action |
|-----|--------|
| `C` | Open cleanup view (from main view) |
| `↑/k`, `↓/j` | Navigate |
| `Space` | Toggle checkbox |
| `Enter/l` | Expand/collapse category |
| `h/Backspace` | Collapse / go back |
| `a` | Select all |
| `n` | Deselect all |
| `d` | Start cleanup (selected items) |
| `D` | Dry-run (selected items) |
| `r` | Rescan |
| `q/Esc` | Back to main view |
| `?` | Help |

### States

#### Scanning
```
┌─ Cleanup: Scanning ──────────────────────────────────────┐
│                                                          │
│  ⟳ Scanning for cleanup candidates...                   │
│                                                          │
│  Checking: ~/Repos/some-project/node_modules             │
│  Found: 47 candidates (8.3 GB)                           │
│                                                          │
│  [Press q to cancel]                                     │
└──────────────────────────────────────────────────────────┘
```

#### Deleting
```
┌─ Cleanup: Deleting ──────────────────────────────────────┐
│                                                          │
│  🗑  Deleting selected items...                          │
│                                                          │
│  [████████░░░░░░░░░░░░] 12/31 items                      │
│                                                          │
│  Current: ~/Repos/old-app/node_modules                   │
│  Freed: 2.4 GB                                           │
│                                                          │
│  [Runs in background - press q to hide]                  │
└──────────────────────────────────────────────────────────┘
```

### Colors

| Element | Color |
|---------|-------|
| Category header | Bold cyan |
| Selected item | Normal |
| Grayed out (active) | Dark gray + "(active)" |
| Risky items | Yellow + "(risky)" |
| Sizes > 1GB | Red |
| Sizes > 100MB | Yellow |
| Sizes < 100MB | Green |

### Active Project Protection

| Setting | Default | Description |
|---------|---------|-------------|
| `skip_if_accessed_within` | `24h` | Skip if mtime/atime < 24 hours |
| `show_skipped` | `true` | Show in TUI but grayed out with "(active)" |
| `allow_override` | `true` | Can manually select even if active |

Display:
```
│ [x] Rust target/              (8 funn)    4.2 GB     │
│     [x] ~/Repos/old-project/target        1.2 GB     │
│     [x] ~/Repos/archived/target           0.8 GB     │
│     [ ] ~/Repos/mcdu/target    (active)   0.4 GB     │  ← grayed out
```

## Default Rules

### Dev Projects (enabled by default)

| Name | Pattern | Signature |
|------|---------|-----------|
| `rust-target` | `**/target` | `CACHEDIR.TAG` \| `../Cargo.toml` |
| `node-modules` | `**/node_modules` | `../package.json` |
| `python-venv` | `**/.venv`, `**/venv` | `../pyproject.toml` \| `../setup.py` |
| `python-cache` | `**/__pycache__`, `**/.pytest_cache`, `**/.mypy_cache`, `**/.ruff_cache` | - |
| `elixir-build` | `**/_build`, `**/deps` | `../mix.exs` |
| `dotnet-bin` | `**/bin`, `**/obj` | `../*.csproj` \| `../*.fsproj` |
| `dart-flutter` | `**/.dart_tool`, `**/build` | `../pubspec.yaml` |
| `java-maven` | `**/target` | `../pom.xml` |
| `java-gradle` | `**/build`, `**/.gradle` | `../build.gradle*` |
| `go-vendor` | `**/vendor` | `../go.mod` (risky) |
| `ruby-bundle` | `**/vendor/bundle` | `../Gemfile` |
| `php-vendor` | `**/vendor` | `../composer.json` |
| `swift-derived` | `**/DerivedData` | - |
| `haskell-stack` | `**/.stack-work`, `**/dist-newstyle` | `../stack.yaml` \| `../*.cabal` |
| `zig-cache` | `**/zig-cache`, `**/zig-out` | `../build.zig` |
| `scala-build` | `**/target`, `**/.bloop`, `**/.metals` | `../build.sbt` |
| `cmake-build` | `**/build`, `**/cmake-build-*` | `CMakeCache.txt` (inside dir) |

### App Caches (enabled by default)

| Name | macOS | Linux | Windows |
|------|-------|-------|---------|
| `chrome-cache` | `~/Library/Caches/Google/Chrome/` | `~/.cache/google-chrome/` | `%LOCALAPPDATA%\Google\Chrome\...` |
| `chromium-cache` | `~/Library/Caches/Chromium/` | `~/.cache/chromium/` | `%LOCALAPPDATA%\Chromium\...` |
| `edge-cache` | `~/Library/Caches/Microsoft Edge/` | `~/.cache/microsoft-edge/` | `%LOCALAPPDATA%\Microsoft\Edge\...` |
| `firefox-cache` | `~/Library/Caches/Firefox/` | `~/.cache/mozilla/firefox/` | `%LOCALAPPDATA%\Mozilla\Firefox\...` |
| `safari-cache` | `~/Library/Caches/com.apple.Safari/` | - | - |
| `vscode-cache` | `~/Library/Caches/Code/` | `~/.cache/Code/` | `%APPDATA%\Code\Cache\` |
| `vscode-insiders-cache` | `~/Library/Caches/Code - Insiders/` | `~/.cache/Code - Insiders/` | `%APPDATA%\Code - Insiders\...` |
| `slack-cache` | `~/Library/Caches/com.tinyspeck.slackmacgap/` | `~/.cache/Slack/` | `%APPDATA%\Slack\Cache\` |
| `discord-cache` | `~/Library/Caches/discord/` | `~/.cache/discord/` | `%APPDATA%\discord\Cache\` |
| `spotify-cache` | `~/Library/Caches/com.spotify.client/` | `~/.cache/spotify/` | `%LOCALAPPDATA%\Spotify\Data\` |
| `jetbrains-cache` | `~/Library/Caches/JetBrains/` | `~/.cache/JetBrains/` | `%LOCALAPPDATA%\JetBrains\` |

### Package Manager Caches (disabled by default)

| Name | Path |
|------|------|
| `npm-cache` | `~/.npm/_cacache/` |
| `yarn-cache` | `${CACHE_DIR}/Yarn/` |
| `pnpm-store` | `~/.pnpm-store/` |
| `pip-cache` | `${CACHE_DIR}/pip/` |
| `cargo-cache` | `~/.cargo/registry/cache/` |
| `homebrew-cache` | `${CACHE_DIR}/Homebrew/` |
| `pub-cache` | `~/.pub-cache/` |
| `gradle-cache` | `~/.gradle/caches/` |
| `maven-cache` | `~/.m2/repository/` |
| `cocoapods-cache` | `${CACHE_DIR}/CocoaPods/` |
| `nuget-cache` | `~/.nuget/packages/` |

### Git Maintenance

| Operation | Description | Default |
|-----------|-------------|---------|
| `git gc --prune=now` | Compress and clean git objects | On |
| `git gc --aggressive` | More thorough (slower) | Off |
| `git reflog expire --expire=now --all` | Delete reflog | Off |
| `git remote prune origin` | Remove stale remote branches | On |

## State Persistence

**File:** `~/.config/mcdu/cleanup-state.toml`

```toml
# Auto-generated - remembers user's selections between sessions

[categories]
dev-projects = true
app-caches = true
package-manager-caches = false
git-maintenance = true

[rules]
rust-target = true
node-modules = true
python-venv = true
dart-flutter = true
go-vendor = false
vscode-cache = true
npm-cache = false
cargo-cache = false

[git]
gc_prune = true
gc_aggressive = false
reflog_expire = false
remote_prune = true

[last_run]
timestamp = "2026-01-04T15:30:00Z"
scan_paths = ["~/Repos"]
deleted_bytes = 4823947234
deleted_items = 47
```

**Logic:**
- On first run: use defaults from `cleanup.toml`
- On subsequent runs: merge state with config (state wins)
- `mcdu cleanup --reset-state` to reset to defaults

## Rule Conditions

All supported conditions:

| Condition | Description | Example |
|-----------|-------------|---------|
| Pattern matching | Glob patterns | `**/node_modules` |
| Signature validation | Required files inside or in parent | `["../Cargo.toml"]` |
| Age | Minimum age since last access | `min_age = "30d"` |
| Size threshold | Minimum size | `min_size = "100MB"` |
| Context check | Check for presence of files | `.keep` file |
| Git status | Check for uncommitted changes | Starlark only |

## Summary

| Component | Description |
|-----------|-------------|
| **Categories** | Dev Projects, App Caches, Package Manager Caches, Git Maintenance |
| **TUI** | Checkbox list, expand/collapse, select all/none, dry-run |
| **CLI** | `mcdu cleanup [--dry-run\|--run --yes] [path]` |
| **Config** | TOML default, Starlark via feature flag |
| **Path variables** | `${CACHE_DIR}`, `${HOME}`, etc. - cross-platform |
| **Conditions** | Pattern, signature, age, size, git-status, context |
| **Active protection** | Skip if accessed < 24h |
| **Threading** | Scanner + Cleaner in separate threads, non-blocking TUI |
| **State** | Remembers checkbox selections between sessions |
| **Git** | gc, prune, reflog expire, remote prune |
