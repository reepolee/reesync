<img src="/github-reepolee.svg" style="margin-bottom:1rem; width:200px">

# reesync

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

An interactive tool for bringing a project up to date with a newer version of the starter it was generated from. Unpack a newly published starter archive, point reesync at it, and selectively copy files into your current project using a tree TUI with checkboxes.

## The Problem

Git can't selectively sync individual files from another directory. When a new starter is published and you want only some of its changes, the manual pipeline is tedious and error-prone - diff by hand, decide file by file, copy each one across, and re-decide the same skip-list next time.

reesync replaces this with a single command and an interactive file browser.

## Features

- **Directory diffing** - compares your project against the newer starter, finding new, modified, and deleted files by SHA-256 content hash
- **Tree view TUI** - navigate the diff with a collapsible tree, toggle files and folders with checkboxes
- **Smart defaults** - new/modified files pre-checked, deleted files shown as informational
- **`.reesyncignore`** - a versioned, project-owned skip-list so per-project files are never adopted by accident
- **Commit context** - when the source is a git repo, each row shows the subject of the last commit that touched the file
- **Batch copy** - copies selected files into your project with progress display

## Installation

### Quick install

**macOS / Linux:**

```bash
curl -fsSL https://raw.githubusercontent.com/reepolee/reesync/main/install.sh | bash
```

**Windows:**

```powershell
irm https://raw.githubusercontent.com/reepolee/reesync/main/install.ps1 | iex
```

The script detects your OS and architecture, downloads the correct binary from the latest GitHub Release, and adds it to your PATH - `~/.local/bin` on macOS and Linux, `~\bin` on Windows.

Or download a binary directly from the [latest release](https://github.com/reepolee/reesync/releases/latest).

### Build from source

Requires [Rust](https://rustup.rs/) (edition 2024).

```bash
cargo build --release
# Binary at ./target/release/reesync
```

`build.sh` produces a platform-named binary and installs it to `~/.local/bin/`. It runs on macOS and Linux:

```bash
bash build.sh
# Produces reesync-macos-arm64 (or -macos-x64 / -linux-x64 / -linux-arm64)
```

Pass `--no-install` to skip the local install step (useful for CI).

## Usage

A new starter archive was published - unpack it, then pull in what you want:

```bash
unzip ~/Downloads/reepolee-starter.zip -d /tmp/reepolee-new

cd my-project
reesync /tmp/reepolee-new
```

`<TEMPLATE_DIR>` is any directory to compare against. An unpacked starter archive is the common case, but a sibling checkout of the upstream repo works the same way. reesync only reads from it - your project is the only thing ever written to.

### Interactive workflow

1. **Diff** - reesync walks both directories and identifies differing files
2. **Browse tree** - navigate the file tree with arrow keys
3. **Toggle files** - use the top `GLOBAL` checkbox to select or clear every folder, or press space on an individual item. New/modified files are pre-checked; deleted files (`[─] (deleted)`) are informational only and can never be copied
4. **Ignore** - press `i` to add/remove the highlighted file in `.reesyncignore`
5. **Confirm** - press Enter to copy selected files into your project
6. **Done** - progress is shown per file

### Key bindings

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate tree items |
| `→` / `←` | Expand / collapse a folder |
| `Space` | Toggle the current item's checkbox |
| `i` | Add or remove the current file in `.reesyncignore` |
| `Enter` | Confirm selection and begin sync |
| `q` / `Esc` | Quit without syncing |

### Options

| Flag | Description |
|------|-------------|
| `--version`, `-v`, `-V` | Print the bare version number and exit |
| `--where` | Print the directory the binary lives in and exit |

### What gets compared

Two categories are skipped outright - they never appear in the tree and are never synced:

- **Hidden files and directories** (anything starting with `.`). This is why `.reesyncignore` never syncs over itself, but it also means dotfiles like `.gitignore` are not brought over - update those by hand.
- **Build/dependency dirs**: `.git`, `node_modules`, `target`, `vendor`, `vendors`, `dist`, `.next`, `.svelte-kit`, `.cache`, `.output`.

Everything else is compared by SHA-256 content hash. If nothing differs, reesync prints `No differences found` and exits without opening the tree.

### .reesyncignore

Files that are yours alone and should never be pulled in from a newer starter go in a `.reesyncignore` at your project root:

```
# One glob pattern per line; blank lines and # comments are ignored
package.json
src/css/**
*.local.json
```

Each line is a [globset](https://docs.rs/globset) pattern matched against the project-root-relative path. Matching files are **shown but pre-unchecked**, dimmed and marked `(ignored)` - never hidden, so you still see that the starter changed them. A missing file is not an error; an invalid pattern is skipped with a warning.

Press `i` to add/remove the highlighted file; on a folder, `i` toggles every file beneath it as a group. `i` manages exact-path lines only - a file matched by a broader glob reports which pattern matched, so you can edit it by hand.

## Design boundaries

Constraints the tool is built to, kept here because they are decisions rather than
anything you can read off the code:

**Always:**
- Validate `TEMPLATE_DIR` exists before entering the TUI
- Show a file count summary before confirming a sync
- Print per-file progress during sync, with clear success/failure
- Handle terminal resize events gracefully

**Ask first:**
- Changing from `cp` to another copy mechanism (`rsync`, hardlinks)
- Adding git awareness beyond read-only commit context (remotes, branches)
- Adding diff preview (showing content diffs in the TUI)
- Adding an undo/rollback feature
- Adding a config file (like `reefmt.jsonc`)

**Never:**
- Delete files from the project - deleted files are informational only
- Modify files outside the current working directory
- Run mutating git commands (commits, pushes, branch operations). Reading commit
  subjects via `git log` for tree context is the only git use
- Overwrite files without user confirmation - the checkbox is the confirmation
- Add network calls (clone, fetch, pull)

## Development

This is a Rust project. To test locally without releasing:

```bash
cargo build --release
cp target/release/reesync ~/.local/bin/   # macOS/Linux
```

### Release workflow

Releases are cut from a single macOS machine. `release.sh` cross-builds
**all six targets** and publishes them as one GitHub Release:

- macOS arm64/x64 (native `cargo build`)
- Linux x64/arm64 (`cargo zigbuild`)
- Windows x64/arm64 (`cargo xwin build`)

Version numbers are date-based: `YY.MM.PATCH`.

```bash
bash release.sh            # bump, tag, cross-build all targets, publish
bash release.sh --minor    # bump the month component instead of patch
bash release.sh --draft    # publish the release as a draft
bash release.sh --force    # release the current Cargo.toml version even if ahead of the tag
```

One-time setup on the Mac:

```bash
brew install zig
cargo install cargo-zigbuild cargo-xwin
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-pc-windows-msvc aarch64-pc-windows-msvc
gh auth login   # the gh CLI must be authenticated to publish
```
