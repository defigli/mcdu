# mcdu User Guide

## Installation

```sh
cargo install mcdu
```

## Usage

### Disk Usage Browser (ncdu mode)

```sh
mcdu              # browse current directory
mcdu /path/to    # browse specific directory
```

**Keybindings:**

| Key | Action |
|-----|--------|
| `j/k` or arrows | Navigate |
| `Enter/l` | Enter directory |
| `Backspace/h` | Go up |
| `d` | Delete selected |
| `r` | Rescan selected |
| `R` | Rescan all |
| `C` | Switch to cleanup mode |
| `?` | Help |
| `q` | Quit |

### Developer Cleanup (CCleaner mode)

```sh
mcdu cleanup            # scan default paths
mcdu cleanup ~/repos    # scan specific path
```

Scans for build artifacts, caches, and other reclaimable disk space across your development tools.

**Keybindings:**

| Key | Action |
|-----|--------|
| `Tab` / `1-4` | Switch tabs (Overview, Categories, Files, Quarantine) |
| `j/k` or arrows | Navigate |
| `Space` | Toggle selection |
| `a` / `n` | Select all / none |
| `d` | Delete selected |
| `D` | Dry run |
| `C` | Rescan |
| `q` | Back to disk browser |

### Supported Categories

Rust/Cargo, Node.js, Python, Go, Java/JVM, Elixir/Erlang, Ruby, PHP, .NET, Zig, Deno, Swift/iOS, Docker, Kubernetes, Terraform, IDE caches, browser caches, system caches (Homebrew, Xcode, Trash).

### Default Scan Paths

`~/Downloads`, `~/Projects`, `~/Code`, `~/Developer`, `~/repos`, `~/dev`, `~/src`, `~/workspace`

### Custom Configuration

Place a config file at `~/.config/mcdu/cleanup.toml` (Linux/macOS):

```toml
scan_paths = ["~/myprojects", "~/work"]

[[rules]]
name = "custom-cache"
category = "Custom"
pattern = "**/.cache"
path = "${HOME}/myprojects"
match_type = "directory"
```

Rules from the config are merged with built-in defaults.
