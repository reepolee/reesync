<img src="/github-reepolee.svg" style="margin-bottom:1rem; width:200px">

# reesync

An interactive template file sync tool for projects derived from upstream templates. Selectively copy files from a template directory into your current project using a tree TUI with checkboxes.

test

## The Problem

Git can't selectively sync individual files from another repository. When you maintain a project derived from a template (e.g., `example-project` from `reepolee/reepolee`), the manual pipeline is tedious:

```
git diff --name-only template/main > changes.txt
# manually edit changes.txt
while read file; do
  git checkout template/main -- "$file"
done < changes.txt
```

reesync replaces this with a single command and an interactive file browser.

## Features

- **Directory diffing** — compares your project against a template directory, finding new, modified, and deleted files
- **Tree view TUI** — navigate the diff with a collapsible tree, toggle files and folders with checkboxes
- **Smart defaults** — new/modified files pre-checked, deleted files shown as informational
- **Batch copy** — copies selected files from template to project with progress display

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

The script detects your OS and architecture, downloads the correct binary from the latest GitHub Release, and adds it to your PATH.

Or download a binary directly from the [latest release](https://github.com/reepolee/reesync/releases/latest).

### Build from source

Requires [Rust](https://rustup.rs/) (edition 2024).

```bash
cargo build --release
# Binary at ./target/release/reesync
```

**macOS / Linux:**

```bash
./build.sh
# Produces reesync-macos-arm64 (or -x64 / -linux-x64 / -linux-arm64) and installs to ~/.local/bin/
```

**Windows:**

```powershell
.\build.ps1
# Produces reesync-windows-x64.exe and installs to ~\bin\
```

Pass `--no-install` / `-NoInstall` to skip the local install step (useful for CI).

## Usage

```bash
cd my-project
reesync ../path/to/template
```

### Interactive workflow

1. **Diff** — reesync walks both directories and identifies differing files
2. **Browse tree** — navigate the file tree with arrow keys
3. **Toggle files** — press space to check/uncheck files. New/modified files are pre-checked; deleted files are informational only
4. **Confirm** — press Enter to copy selected files from template into your project
5. **Done** — progress is shown per file

### Options

| Flag | Description |
|------|-------------|
| `--version`, `-V` | Print the version and exit |

### Example

```bash
# From inside example-project, sync selected files from the upstream reepolee template
reesync ../reepolee
```

## Development

This is a Rust project. To test locally without releasing:

```bash
cargo build --release
cp target/release/reesync ~/.local/bin/   # macOS/Linux
```

### Release workflow

Releases are cut from a single machine (the Mac mini). `release.sh` cross-builds
**all six targets** and publishes them as one GitHub Release:

- macOS arm64/x64 (native `cargo build`)
- Linux x64/arm64 (`cargo zigbuild`)
- Windows x64/arm64 (`cargo xwin build`)

```bash
bash release.sh            # bump, tag, cross-build all targets, publish
bash release.sh --minor    # bump the minor version instead of patch
bash release.sh --draft    # publish the release as a draft
```

One-time setup on the Mac:

```bash
brew install zig
cargo install cargo-zigbuild cargo-xwin
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-pc-windows-msvc aarch64-pc-windows-msvc
```
