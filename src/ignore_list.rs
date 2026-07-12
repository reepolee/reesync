//! `.reesyncignore` support.
//!
//! A clone-owned skip-list living at `<project>/.reesyncignore`. Each non-blank,
//! non-`#` line is a glob pattern (globset syntax) matched against the
//! project-root-relative path of each diff entry. Matched files are shown but
//! pre-unchecked and dimmed - never hidden (visible skips only).
//!
//! The file is seeded by the template on first untar and is self-ignored during
//! the diff walk (dotfiles are skipped), so it never syncs over itself.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};

pub const IGNORE_FILE: &str = ".reesyncignore";

/// Compiled ignore patterns plus the raw lines (kept so the TUI can add/remove
/// exact-path lines without losing comments or ordering).
pub struct IgnoreList {
    path: PathBuf,
    lines: Vec<String>,
    set: GlobSet,
}

impl IgnoreList {
    /// Load `<project_dir>/.reesyncignore`. A missing file yields an empty list
    /// (no error). Invalid glob lines are skipped with a warning so one typo
    /// cannot break the whole sync.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let path = project_dir.join(IGNORE_FILE);
        let lines = if path.is_file() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            text.lines().map(|l| l.to_string()).collect()
        } else {
            Vec::new()
        };
        let set = build_set(&lines);
        Ok(Self { path, lines, set })
    }

    /// Whether `rel_path` (project-root-relative) matches any ignore pattern.
    pub fn is_ignored(&self, rel_path: &Path) -> bool {
        self.set.is_match(rel_path)
    }

    /// Whether `rel_path` is present as an EXACT line (as opposed to matched by a
    /// broader glob). `i` can only toggle exact lines; a glob-matched-but-not-
    /// exact file is reported via `matching_glob` instead.
    pub fn has_exact(&self, rel_path: &Path) -> bool {
        let needle = to_pattern(rel_path);
        self.lines.iter().any(|l| l.trim() == needle)
    }

    /// The first non-exact glob line that matches `rel_path`, if any. Used to
    /// tell the user "ignored by pattern <glob> - edit .reesyncignore" (Q1a).
    pub fn matching_glob(&self, rel_path: &Path) -> Option<String> {
        if self.has_exact(rel_path) {
            return None;
        }
        for line in &self.lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Ok(glob) = Glob::new(trimmed) {
                if glob.compile_matcher().is_match(rel_path) {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    /// Add an exact-path line for `rel_path` and persist. No-op if already exact.
    pub fn add_exact(&mut self, rel_path: &Path) -> Result<()> {
        if self.has_exact(rel_path) {
            return Ok(());
        }
        self.lines.push(to_pattern(rel_path));
        self.persist()?;
        self.set = build_set(&self.lines);
        Ok(())
    }

    /// Remove the exact-path line for `rel_path` and persist. No-op if absent.
    pub fn remove_exact(&mut self, rel_path: &Path) -> Result<()> {
        let needle = to_pattern(rel_path);
        let before = self.lines.len();
        self.lines.retain(|l| l.trim() != needle);
        if self.lines.len() != before {
            self.persist()?;
            self.set = build_set(&self.lines);
        }
        Ok(())
    }

    /// Atomically write the ignore file (temp + rename) so a mid-session edit
    /// cannot truncate it on failure. Removes the file when the list is empty.
    fn persist(&self) -> Result<()> {
        let has_content = self.lines.iter().any(|l| !l.trim().is_empty());
        if !has_content {
            if self.path.exists() {
                std::fs::remove_file(&self.path)
                    .with_context(|| format!("removing {}", self.path.display()))?;
            }
            return Ok(());
        }

        let mut body = self.lines.join("\n");
        body.push('\n');

        let tmp = self.path.with_extension("reesyncignore.tmp");
        std::fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("renaming into {}", self.path.display()))?;
        Ok(())
    }
}

/// Normalize a relative path to a forward-slash pattern (globset matches on
/// `/`-separated paths; Windows walk output uses `\`).
fn to_pattern(rel_path: &Path) -> String {
    rel_path.to_string_lossy().replace('\\', "/")
}

/// Build a GlobSet from raw lines, skipping blanks, `#` comments, and any line
/// that is not a valid glob (with a stderr warning).
fn build_set(lines: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match Glob::new(trimmed) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(e) => {
                eprintln!("  Warning: invalid .reesyncignore pattern '{}': {}", trimmed, e);
            }
        }
    }
    builder.build().unwrap_or_else(|_| GlobSet::empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    fn write_ignore(dir: &Path, body: &str) {
        std::fs::write(dir.join(IGNORE_FILE), body).unwrap();
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempdir().unwrap();
        let list = IgnoreList::load(dir.path()).unwrap();
        assert!(!list.is_ignored(Path::new("package.json")));
    }

    #[test]
    fn exact_and_glob_matching() {
        let dir = tempdir().unwrap();
        write_ignore(dir.path(), "# comment\npackage.json\nsrc/css/**\nconfig/*.override.ts\n\n");
        let list = IgnoreList::load(dir.path()).unwrap();

        assert!(list.is_ignored(Path::new("package.json")));
        assert!(list.is_ignored(Path::new("src/css/style.css")));
        assert!(list.is_ignored(Path::new("config/supported_languages.override.ts")));
        assert!(!list.is_ignored(Path::new("src/lib/helper.ts")));
    }

    #[test]
    fn exact_vs_glob_distinction() {
        let dir = tempdir().unwrap();
        write_ignore(dir.path(), "package.json\nsrc/css/**\n");
        let list = IgnoreList::load(dir.path()).unwrap();

        // package.json is an exact line -> toggleable, no matching_glob.
        assert!(list.has_exact(Path::new("package.json")));
        assert_eq!(list.matching_glob(Path::new("package.json")), None);

        // style.css is matched only by a glob -> not exact, reports the glob.
        assert!(!list.has_exact(Path::new("src/css/style.css")));
        assert_eq!(
            list.matching_glob(Path::new("src/css/style.css")),
            Some("src/css/**".to_string())
        );
    }

    #[test]
    fn add_and_remove_exact_roundtrip() {
        let dir = tempdir().unwrap();
        let mut list = IgnoreList::load(dir.path()).unwrap();

        list.add_exact(Path::new("wrangler.jsonc")).unwrap();
        assert!(list.is_ignored(Path::new("wrangler.jsonc")));
        // Persisted to disk.
        let reloaded = IgnoreList::load(dir.path()).unwrap();
        assert!(reloaded.is_ignored(Path::new("wrangler.jsonc")));

        list.remove_exact(Path::new("wrangler.jsonc")).unwrap();
        assert!(!list.is_ignored(Path::new("wrangler.jsonc")));
        // File removed when empty.
        assert!(!dir.path().join(IGNORE_FILE).exists());
    }
}
