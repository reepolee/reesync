# Spec: reesync

> Interactive template file sync tool — selectively copy files from an upstream template directory into your current project.

## Objective

**What:** A Rust CLI tool `reesync` that diffs two directories (your project and a template), shows the differing files in an interactive tree TUI with checkboxes, and copies the selected files from the template into your project.

**User:** Someone maintaining a project derived from a template repo (e.g., `example-project` from `reepolee/reepolee`). The template evolves upstream, and the user wants to selectively adopt changes — file by file or folder by folder — without git submodules, subtrees, or manual shell pipelines.

**Success:** One command — `reesync ../template-dir` — walks the user through a visual tree of differences, lets them toggle files/folders, and copies their selection. No more `git diff --name-only > changes.txt` pipelines.

## Tech Stack

| Layer | Choice | Why |
|-------|--------|-----|
| Language | Rust (edition 2024) | Match existing toolchain (reefmt, reesql, reemerge) |
| TUI framework | `ratatui` + `crossterm` | Standard Rust TUI ecosystem; mature, well-documented |
| Tree widget | `tui-tree-widget` | Renders collapsible tree, integrates with Ratatui |
| Checkbox | `tui-checkbox` | Toggleable checkbox widget for Ratatui |
| CLI args | Hand-parsed (`std::env::args()`) | Keep it simple — just a path arg and `--version`; avoid `clap` dependency for this scope |
| Error handling | `anyhow` | Same as reemerge; simple context-based errors |
| Rendering | `colored` | For terminal output outside the TUI (status messages, errors) |

**Dependencies (Cargo.toml):**

```toml
[package]
name = "reesync"
version = "0.1.0"
edition = "2024"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
tui-tree-widget = "0.4"
tui-checkbox = "0.3"
anyhow = "1"
colored = "2"
```

## Commands

```
Usage: reesync <TEMPLATE_DIR>

Arguments:
  <TEMPLATE_DIR>    Path to the template directory to sync from

Options:
  --version, -V     Print version and exit
  --help, -h        Print help

Examples:
  reesync ../reepolee        # Sync from template one directory up
  reesync /path/to/template  # Sync from absolute path
```

**Run inside the target project.** The tool diffs `<TEMPLATE_DIR>` against the current working directory. What's in `./` is the project,  `../template-dir/` is the source of truth.

## Project Structure

```
reesync/
├── src/
│   ├── main.rs              # Entry point, CLI arg parsing, orchestrator
│   ├── diff.rs              # File diffing logic (walk two dirs, compare)
│   ├── tree.rs              # Tree data structure and TUI rendering
│   └── sync.rs              # File copy logic (cp selected files)
├── Cargo.toml
├── build.sh                 # Native build + local install (from reemerge)
├── build.ps1                # Windows build + local install
├── install.sh               # curl-pipe install from GitHub Releases
├── install.ps1              # PowerShell install
├── release.sh               # Version bump, tag, gh release, upload
├── release.ps1              # Windows release
├── SPEC.md                  # This file
├── README.md                # User-facing docs
└── .gitignore
```

## Core Workflow (End-to-End)

```
reesync ../template-dir
```

1. **Diff** — Walk both directories recursively. For each file:
   - Compare relative paths
   - If path exists in both but content differs → **Modified** (pre-checked)
   - If path exists only in template → **New** (pre-checked)
   - If path exists only in project → **Deleted** (shown, unchecked)
   - Ignore `.gitignore`d files (respect each dir's `.gitignore` if present)
   - Skip common noise: `.git/`, `node_modules/`, `target/`, `vendor/`

2. **Tree TUI** — Render the differing files as a tree.
   - Folders are collapsible (expand/collapse with arrow keys)
   - Each file has a checkbox: `[x]` or `[ ]`
   - **Modified/New** files: `[x]` (pre-checked)
   - **Deleted** files: `[ ]` with a dimmed/gray label (unchecked by default)
   - **Folder checkboxes**: toggling a folder toggles all its children
   - Status bar at bottom showing: `3 / 12 files selected | [Enter] confirm  [q] quit  [↑↓] navigate  [space] toggle`

3. **Sync** — After confirmation, for each checked file:
   - Ensure parent directory exists in project
   - `cp <template_dir>/<relative_path> ./<relative_path>`
   - Print progress: `[1/5] ✓ lib/helpers.ts`
   - On completion: `Done! Synced 5 files from template.`

4. **Error handling:**
   - If `TEMPLATE_DIR` doesn't exist → print error, exit 1
   - If no differences found → print "No differences found.", exit 0
   - If a file can't be copied → print warning, continue with remaining files
   - If directory has no read permission → skip with warning

## Code Style

Follow the same patterns as reemerge:

```rust
// Main entry with version flag
fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("reesync {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Parse template dir from args
    let template_dir = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("Usage: reesync <TEMPLATE_DIR>"))?;

    let template_path = std::path::Path::new(&template_dir);
    if !template_path.is_dir() {
        return Err(anyhow!("Template directory not found: {}", template_dir));
    }

    // ... orchestrate diff → TUI → sync ...
}
```

**Conventions:**
- `camelCase` for enum variants, `snake_case` for functions/variables, `PascalCase` for types
- Derive `Debug, Clone` on data structs
- Use `Result<T>` with `anyhow::Error` throughout
- Color terminal output with `colored` (same as reemerge)
- One blank line between function definitions
- 4-space indentation (same as existing Rust projects)

## TUI Design (Detail)

### Layout

```
┌─────────────────────────────────────────────────┐
│  reesync — Select files to sync from template   │  Header
│  Template: ../reepolee                          │
├─────────────────────────────────────────────────┤
│                                                 │
│  📁 lib/                                    [─] │
│    [x] 📄 lib/helpers.ts                        │
│    [ ] 📄 lib/utils.ts    (deleted)             │  Tree
│  📁 src/                                    [+] │
│    [x] 📄 src/main.ts                           │
│  📄 package.json            (new)               │
│                                                 │
├─────────────────────────────────────────────────┤
│  3 / 12 files selected  │  ↑↓ nav  space toggle │  Status
│  [Enter] sync           │  q quit               │  Bar
└─────────────────────────────────────────────────┘
```

### Key Bindings

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate tree items |
| `→` | Expand collapsed folder |
| `←` | Collapse expanded folder |
| `Space` | Toggle current item's checkbox |
| `Enter` | Confirm selection and begin sync |
| `q` / `Esc` | Quit without syncing |

### Behavior Rules

1. **Folder toggling**: Toggling a folder checks/unchecks all its visible (not filtered) children recursively. Folders show `[~]` when some but not all children are checked (partial state).
2. **Scroll**: If the tree is taller than the terminal, the viewport scrolls.
3. **Terminal resize**: The TUI re-renders on resize events.
4. **Deleted files**: Rendered with `(deleted)` suffix in dimmed/gray style. Pre-unchecked. Can still be checked if the user wants to copy even though the file doesn't exist in template — actually, deleted means file doesn't exist in template but exists in project, so it can't be copied. Maybe just show them as informational only, non-toggleable, with a `[~]` dash instead of checkbox.

Actually, let me reconsider the "deleted" case:
- **Deleted** = file exists in project but NOT in template. Can't copy from template. Should show but be non-toggleable. Mark with `[─]` to indicate it's informational.

Let me fix that:

| State | Checkbox | Default | Toggleable? |
|-------|----------|---------|-------------|
| Modified | `[x]` | Checked | Yes |
| New | `[x]` | Checked | Yes |
| Deleted | `[─]` | N/A (informational) | No |

## Testing Strategy

| Level | What | How |
|-------|------|-----|
| Unit | `diff.rs` — walk dirs, compare files, classify states | Create temp dirs with known file structures, assert correct diff output |
| Unit | `tree.rs` — tree construction, toggle logic, parent-child propagation | Unit tests on data structures without rendering |
| Unit | `sync.rs` — copy file, create parent dirs | Temp dirs, assert file contents match after copy |
| Integration | Full flow — setup two temp dirs, run CLI, verify copies | Build binary, run against temp dirs, assert files landed correctly |

**Run tests:** `cargo test`

## Boundaries

**Always do:**
- Validate `TEMPLATE_DIR` exists before entering TUI
- Show file count summary before confirming sync
- Print progress during sync with clear success/failure per file
- Respect `.gitignore` when walking directories
- Handle terminal resize events gracefully

**Ask first:**
- Changing from `cp` to another copy mechanism (e.g., `rsync`, hardlinks)
- Adding git awareness (remotes, branches)
- Adding diff preview (showing content diffs in the TUI)
- Adding an undo/rollback feature
- Adding a config file (like `reefmt.jsonc`)

**Never do:**
- Delete files from the project (deleted files are informational only)
- Modify files outside the current working directory
- Run git commands (commits, pushes, branch operations)
- Overwrite files without user confirmation (the checkbox is the confirmation)
- Add network calls (clone, fetch, pull)

## Open Questions

1. Should we respect `.gitignore` of the template dir, the project dir, or both? Both seems safest — skip anything either side ignores.
2. Symlinks — follow them or skip them? Skip by default, add flag later if needed.
3. Binary files — when comparing content for "modified" status, should we use byte comparison or line-by-line diff? Byte comparison is simplest and correct for all file types.
4. Large directory trees — should we add a spinner or progress indicator during the initial diff walk? Yes, a simple "Walking directories..." message before the TUI opens.
5. What about files that exist in both but are identical? They shouldn't appear in the tree at all — only differing files are shown.

## Success Criteria

- [ ] `reesync ../template-dir` shows a tree of differing files between the two directories
- [ ] Modified and new files are pre-checked; deleted files are shown as informational
- [ ] Space toggles checkboxes; folder toggles propagate to children
- [ ] Enter confirms and copies checked files via `cp`
- [ ] Copy progress displayed per-file
- [ ] No errors for empty diff (prints "No differences found.")
- [ ] Graceful error if template dir missing
- [ ] `--version` prints version
- [ ] All tests pass via `cargo test`
- [ ] Build scripts (`build.sh`, `release.sh`, `install.sh`) work for macOS/Linux
