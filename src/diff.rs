use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;
use sha2::Digest;

/// The state of a file relative to the template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// File exists in template but not in project.
    New,
    /// File exists in both but content differs.
    Modified,
    /// File exists in project but not in template.
    Deleted,
}

/// A single file difference between project and template.
#[derive(Debug, Clone)]
pub struct DiffEntry {
    /// Relative path from the project/template root.
    pub relative_path: PathBuf,
    /// Whether this file is new, modified, or deleted.
    pub state: FileState,
    /// Last commit info from the template repo (hash, date, subject).
    pub commit_info: Option<String>,
}

/// Directories to skip when walking (common noise).
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "vendor", "vendors", "dist", ".next", ".svelte-kit", ".cache", ".output"];

/// Walk a directory recursively, collecting all file paths relative to the root.
/// Skips hidden directories and common noise directories.
fn walk_files(root: &Path) -> Result<HashSet<PathBuf>> {
    let mut files = HashSet::new();

    if !root.is_dir() {
        return Ok(files);
    }

    let walker = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() > 0 {
                let name = entry.file_name().to_string_lossy();
                // Skip hidden files and directories
                if name.starts_with('.') {
                    return false;
                }
                // Skip common noise directories
                if entry.file_type().is_dir() && SKIP_DIRS.contains(&name.as_ref()) {
                    return false;
                }
            }
            true
        });

    for entry in walker {
        match entry {
            Ok(entry) => {
                // Skip symlinks
                if entry.file_type().is_symlink() {
                    continue;
                }
                if entry.file_type().is_file() {
                    if let Ok(relative) = entry.path().strip_prefix(root) {
                        files.insert(relative.to_path_buf());
                    }
                }
            }
            Err(e) => {
                eprintln!("  Warning: error walking directory: {}", e);
            }
        }
    }

    Ok(files)
}

/// Compute the SHA256 hash of a file's contents.
fn hash_file(path: &Path) -> Result<Vec<u8>> {
    let data = fs::read(path)?;
    let hash = sha2::Sha256::digest(&data);
    Ok(hash.to_vec())
}

/// Diff two directories (project and template), returning a list of differences.
///
/// Walk both directories, compare files by path + content hash.
/// Returns entries for:
/// - Files in template but not project (New)
/// - Files in both with different content (Modified)
/// - Files in project but not template (Deleted)
pub fn diff_directories(project: &Path, template: &Path) -> Result<Vec<DiffEntry>> {
    let project_files = walk_files(project)?;
    let template_files = walk_files(template)?;

    // Cache hashes for files in both directories
    let mut hashes_project: std::collections::HashMap<PathBuf, Vec<u8>> = std::collections::HashMap::new();
    let mut hashes_template: std::collections::HashMap<PathBuf, Vec<u8>> = std::collections::HashMap::new();

    let both: HashSet<PathBuf> = project_files.intersection(&template_files).cloned().collect();

    // Compute hashes for the intersection
    for path in &both {
        let project_hash = hash_file(&project.join(path)).unwrap_or_default();
        let template_hash = hash_file(&template.join(path)).unwrap_or_default();
        hashes_project.insert(path.clone(), project_hash);
        hashes_template.insert(path.clone(), template_hash);
    }

    let mut diffs = Vec::new();

    // Files only in template → New
    for path in template_files.difference(&project_files) {
        diffs.push(DiffEntry {
            relative_path: path.clone(),
            state: FileState::New,
            commit_info: None,
        });
    }

    // Files only in project → Deleted
    for path in project_files.difference(&template_files) {
        diffs.push(DiffEntry {
            relative_path: path.clone(),
            state: FileState::Deleted,
            commit_info: None,
        });
    }

    // Files in both → check if Modified
    for path in &both {
        let p_hash = hashes_project.get(path);
        let t_hash = hashes_template.get(path);
        if p_hash != t_hash {
            diffs.push(DiffEntry {
                relative_path: path.clone(),
                state: FileState::Modified,
                commit_info: None,
            });
        }
    }

    // Sort by path for consistent output
    diffs.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(diffs)
}

/// Run `git log -1 --format="%h %as %s"` for each differing file that exists
/// in the template, and store the result in `commit_info`.
///
/// Skips files that are Deleted (don't exist in template).
/// Gracefully handles non-git repos, missing files, and git errors.
pub fn enrich_commit_info(template_dir: &Path, entries: &mut [DiffEntry]) {
    // Quick check: is the template dir a git repo?
    let is_git = std::process::Command::new("git")
        .args(["-C", &template_dir.to_string_lossy(), "rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_git {
        return;
    }

    for entry in entries.iter_mut() {
        if entry.state == FileState::Deleted {
            continue;
        }

        let path_str = entry.relative_path.to_string_lossy();
        let output = std::process::Command::new("git")
            .args([
                "-C",
                &template_dir.to_string_lossy(),
                "log",
                "-1",
                "--format=%h %as %s",
                "--",
                &path_str,
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !line.is_empty() {
                    entry.commit_info = Some(line);
                }
            }
            _ => {
                // Git command failed or other error — leave as None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn setup_temp_dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let project = dir.path().join("project");
        let template = dir.path().join("template");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&template).unwrap();
        (dir, project, template)
    }

    fn create_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_empty_dirs_no_diff() {
        let (_dir, project, template) = setup_temp_dirs();
        let diffs = diff_directories(&project, &template).unwrap();
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_new_file_in_template() {
        let (_dir, project, template) = setup_temp_dirs();
        create_file(&template.join("new.txt"), "hello");
        let diffs = diff_directories(&project, &template).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].relative_path, PathBuf::from("new.txt"));
        assert_eq!(diffs[0].state, FileState::New);
    }

    #[test]
    fn test_deleted_file_in_project() {
        let (_dir, project, template) = setup_temp_dirs();
        create_file(&project.join("old.txt"), "bye");
        let diffs = diff_directories(&project, &template).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].relative_path, PathBuf::from("old.txt"));
        assert_eq!(diffs[0].state, FileState::Deleted);
    }

    #[test]
    fn test_modified_file() {
        let (_dir, project, template) = setup_temp_dirs();
        create_file(&project.join("file.txt"), "original");
        create_file(&template.join("file.txt"), "changed");
        let diffs = diff_directories(&project, &template).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].relative_path, PathBuf::from("file.txt"));
        assert_eq!(diffs[0].state, FileState::Modified);
    }

    #[test]
    fn test_identical_file_no_diff() {
        let (_dir, project, template) = setup_temp_dirs();
        create_file(&project.join("file.txt"), "same");
        create_file(&template.join("file.txt"), "same");
        let diffs = diff_directories(&project, &template).unwrap();
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_nested_paths() {
        let (_dir, project, template) = setup_temp_dirs();
        create_file(&project.join("src/lib/helper.ts"), "project version");
        create_file(&template.join("src/lib/helper.ts"), "template version");
        let diffs = diff_directories(&project, &template).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].relative_path, PathBuf::from("src/lib/helper.ts"));
        assert_eq!(diffs[0].state, FileState::Modified);
    }

    #[test]
    fn test_skips_git_dir() {
        let (_dir, project, template) = setup_temp_dirs();
        create_file(&template.join(".git/config"), "git config");
        create_file(&template.join("actual.txt"), "real file");
        let diffs = diff_directories(&project, &template).unwrap();
        // Should only find actual.txt, not .git/config
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].relative_path, PathBuf::from("actual.txt"));
    }

    #[test]
    fn test_multiple_changes() {
        let (_dir, project, template) = setup_temp_dirs();
        // New in template
        create_file(&template.join("new.txt"), "new");
        // Deleted in project
        create_file(&project.join("deleted.txt"), "gone");
        // Modified
        create_file(&project.join("changed.txt"), "old");
        create_file(&template.join("changed.txt"), "new");
        // Identical (no diff)
        create_file(&project.join("same.txt"), "same");
        create_file(&template.join("same.txt"), "same");

        let diffs = diff_directories(&project, &template).unwrap();
        assert_eq!(diffs.len(), 3);

        let states: Vec<FileState> = diffs.iter().map(|d| d.state).collect();
        assert!(states.contains(&FileState::New));
        assert!(states.contains(&FileState::Deleted));
        assert!(states.contains(&FileState::Modified));
    }
}
