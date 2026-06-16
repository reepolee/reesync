use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;

/// Copy selected files from the template directory to the project directory.
///
/// For each file:
/// 1. Ensures the parent directory exists in the project
/// 2. Copies the file from template to project
/// 3. Prints progress
///
/// Returns the number of files successfully copied.
pub fn sync_files(
    project: &Path,
    template: &Path,
    files: &[PathBuf],
) -> Result<usize> {
    let total = files.len();
    let mut succeeded = 0;
    let mut had_errors = false;

    for (i, file) in files.iter().enumerate() {
        let source = template.join(file);
        let dest = project.join(file);

        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("  [{}/{}] ✗ {}: failed to create directory — {}", i + 1, total, file.display(), e);
                had_errors = true;
                continue;
            }
        }

        // Copy the file
        match fs::copy(&source, &dest) {
            Ok(_) => {
                succeeded += 1;
                println!("  [{}/{}] ✓ {}", i + 1, total, file.display());
            }
            Err(e) => {
                eprintln!("  [{}/{}] ✗ {}: {}", i + 1, total, file.display(), e);
                had_errors = true;
            }
        }
    }

    if had_errors {
        eprintln!("  Warning: {} of {} file(s) had errors", total - succeeded, total);
    }

    Ok(succeeded)
}
