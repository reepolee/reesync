use std::path::{Path, PathBuf};
use crate::diff::{DiffEntry, FileState};

/// A node in the diff tree — either a file or a folder.
#[derive(Debug, Clone)]
pub enum TreeNode {
    File(TreeFile),
    Folder(TreeFolder),
}

/// A file node.
#[derive(Debug, Clone)]
pub struct TreeFile {
    pub name: String,
    pub path: PathBuf,
    pub state: FileState,
    pub checked: bool,
    /// Matches a `.reesyncignore` pattern - rendered dimmed and pre-unchecked.
    pub ignored: bool,
    /// Last commit info from template repo: "abc1234 2026-06-15 Fix message"
    pub commit_info: Option<String>,
}

/// A folder node with children.
#[derive(Debug, Clone)]
pub struct TreeFolder {
    pub name: String,
    pub path: PathBuf,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
}

impl TreeNode {
    pub fn name(&self) -> &str {
        match self {
            TreeNode::File(f) => &f.name,
            TreeNode::Folder(d) => &d.name,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            TreeNode::File(f) => &f.path,
            TreeNode::Folder(d) => &d.path,
        }
    }

    pub fn is_folder(&self) -> bool {
        matches!(self, TreeNode::Folder(_))
    }



    /// Get the total count of items (files only, not intermediate nodes).
    pub fn file_count(&self) -> usize {
        match self {
            TreeNode::File(_) => 1,
            TreeNode::Folder(d) => d.children.iter().map(|c| c.file_count()).sum(),
        }
    }

    /// Get the number of files that can be selected for syncing.
    pub fn selectable_file_count(&self) -> usize {
        match self {
            TreeNode::File(f) => usize::from(f.state != FileState::Deleted),
            TreeNode::Folder(d) => {
                let children = d.children.iter();
                children.map(|c| c.selectable_file_count()).sum()
            }
        }
    }

    /// Get the count of checked files.
    pub fn checked_count(&self) -> usize {
        match self {
            TreeNode::File(f) => {
                if f.checked { 1 } else { 0 }
            }
            TreeNode::Folder(d) => d.children.iter().map(|c| c.checked_count()).sum(),
        }
    }

    /// Toggle this node's checked state.
    /// For files: toggle individual.
    /// For folders: toggle all children to match folder's desired state.
    pub fn toggle(&mut self) {
        match self {
            TreeNode::File(f) => {
                if f.state != FileState::Deleted {
                    f.checked = !f.checked;
                }
            }
            TreeNode::Folder(d) => {
                // Toggle folder: compute desired state based on current state
                let children = d.children.iter();
                let total = children.map(|c| c.selectable_file_count()).sum::<usize>();
                let checked = d.children.iter().map(|c| c.checked_count()).sum::<usize>();
                // If any are unchecked → check all; if all checked → uncheck all
                let new_checked = checked < total;
                for child in &mut d.children {
                    child.set_checked(new_checked);
                }
            }
        }
    }

    /// Recursively set checked state (for folder propagation).
    pub fn set_checked(&mut self, checked: bool) {
        match self {
            TreeNode::File(f) => {
                if f.state != FileState::Deleted {
                    f.checked = checked;
                }
            }
            TreeNode::Folder(d) => {
                for child in &mut d.children {
                    child.set_checked(checked);
                }
            }
        }
    }





    /// Record the collapsed folders (path -> not expanded) into `out`. Used to
    /// preserve expand/collapse state across a tree rebuild (e.g. after an `i`
    /// toggle re-diffs); folders default to expanded, so we only track the ones
    /// the user explicitly collapsed.
    pub fn collect_collapsed(&self, out: &mut Vec<PathBuf>) {
        if let TreeNode::Folder(d) = self {
            if !d.expanded {
                out.push(d.path.clone());
            }
            for child in &d.children {
                child.collect_collapsed(out);
            }
        }
    }

    /// Re-collapse the folders whose paths are in `collapsed`, restoring state
    /// captured by `collect_collapsed` after a rebuild.
    pub fn apply_collapsed(&mut self, collapsed: &[PathBuf]) {
        if let TreeNode::Folder(d) = self {
            if collapsed.iter().any(|p| p == &d.path) {
                d.expanded = false;
            }
            for child in &mut d.children {
                child.apply_collapsed(collapsed);
            }
        }
    }

    /// Collect every descendant file path (regardless of checked state).
    pub fn collect_all_file_paths(&self, paths: &mut Vec<PathBuf>) {
        match self {
            TreeNode::File(f) => paths.push(f.path.clone()),
            TreeNode::Folder(d) => {
                for child in &d.children {
                    child.collect_all_file_paths(paths);
                }
            }
        }
    }

    /// All descendant file paths under the node at `path`. Used by folder-level
    /// `i`: collect every file under the highlighted folder so they can be
    /// ignored/un-ignored as a group. Empty if `path` is not found.
    pub fn file_paths_at(&self, path: &Path) -> Vec<PathBuf> {
        if self.path() == path {
            let mut out = Vec::new();
            self.collect_all_file_paths(&mut out);
            return out;
        }
        if let TreeNode::Folder(d) = self {
            for child in &d.children {
                let found = child.file_paths_at(path);
                if !found.is_empty() {
                    return found;
                }
            }
        }
        Vec::new()
    }

    /// Collect file paths that are checked, recursively.
    pub fn collect_checked_paths(&self, paths: &mut Vec<PathBuf>) {
        match self {
            TreeNode::File(f) => {
                if f.checked {
                    paths.push(f.path.clone());
                }
            }
            TreeNode::Folder(d) => {
                for child in &d.children {
                    child.collect_checked_paths(paths);
                }
            }
        }
    }

    /// Flatten the tree into a display list for the TUI list widget.
    /// Only includes visible items (folders that are expanded show their children).
    /// If this is the root node, skips it and flattens children directly.
    pub fn flatten(&self, depth: usize, result: &mut Vec<DisplayItem>) {
        match self {
            TreeNode::File(f) => {
                result.push(DisplayItem {
                    path: f.path.clone(),
                    depth,
                    label: f.name.clone(),
                    is_folder: false,
                    is_expanded: false,
                    state: f.state,
                    checked: f.checked,
                    ignored: f.ignored,
                    total_count: 0,
                    checked_count: 0,
                    commit_info: f.commit_info.clone(),
                });
            }
            TreeNode::Folder(d) => {
                // Render the artificial root node as the GLOBAL selection control.
                if d.path == Path::new("/") && depth == 0 {
                    let children = d.children.iter();
                    let total = children.map(|c| c.selectable_file_count()).sum::<usize>();
                    let checked = d.children.iter().map(|c| c.checked_count()).sum::<usize>();
                    result.push(DisplayItem {
                        path: d.path.clone(),
                        depth: 0,
                        label: "GLOBAL".to_string(),
                        is_folder: true,
                        is_expanded: false,
                        state: FileState::Modified,
                        checked: checked > 0,
                        ignored: false,
                        total_count: total,
                        checked_count: checked,
                        commit_info: None,
                    });
                    for child in &d.children {
                        child.flatten(depth + 1, result);
                    }
                    return;
                }

                let children = d.children.iter();
                let total = children.map(|c| c.selectable_file_count()).sum::<usize>();
                let checked = d.children.iter().map(|c| c.checked_count()).sum::<usize>();
                result.push(DisplayItem {
                    path: d.path.clone(),
                    depth,
                    label: d.name.clone(),
                    is_folder: true,
                    is_expanded: d.expanded,
                    state: FileState::Modified,
                    checked: checked > 0,
                    ignored: false,
                    total_count: total,
                    checked_count: checked,
                    commit_info: None,
                });
                if d.expanded {
                    for child in &d.children {
                        child.flatten(depth + 1, result);
                    }
                }
            }
        }
    }

    /// Navigate to a specific path and toggle its expanded state (for folders).
    pub fn toggle_expand_at(&mut self, path: &Path) -> bool {
        if self.path() == path {
            if let TreeNode::Folder(d) = self {
                d.expanded = !d.expanded;
                return true;
            }
        }
        if let TreeNode::Folder(d) = self {
            for child in &mut d.children {
                if child.toggle_expand_at(path) {
                    return true;
                }
            }
        }
        false
    }

    /// Toggle checked state at a specific path.
    pub fn toggle_at(&mut self, path: &Path) -> bool {
        if self.path() == path {
            self.toggle();
            return true;
        }
        if let TreeNode::Folder(d) = self {
            for child in &mut d.children {
                if child.toggle_at(path) {
                    return true;
                }
            }
        }
        false
    }
}

/// A display-ready item for the TUI list.
#[derive(Debug, Clone)]
pub struct DisplayItem {
    pub path: PathBuf,
    pub depth: usize,
    pub label: String,
    pub is_folder: bool,
    pub is_expanded: bool,
    pub state: FileState,
    pub checked: bool,
    /// Matches a `.reesyncignore` pattern (files only; always false for folders).
    pub ignored: bool,
    /// For folders: total number of descendant files
    pub total_count: usize,
    /// For folders: number of checked descendant files
    pub checked_count: usize,
    /// Last commit info from template repo: "abc1234 2026-06-15 Fix message"
    pub commit_info: Option<String>,
}

/// Build a tree from a list of diff entries.
///
/// Groups entries by their parent directory, creating intermediate folder nodes.
/// Only Modified files start pre-checked. New files (template-only) start unchecked
/// since they may be files the user intentionally deleted from their project.
/// Deleted files are always unchecked and not toggleable.
pub fn build_tree(entries: &[DiffEntry]) -> TreeNode {
    // Group entries by parent directory segments
    let mut root_children: Vec<TreeNode> = Vec::new();

    for entry in entries {
        insert_entry(&mut root_children, &entry.relative_path, entry.state, entry.ignored, entry.commit_info.clone());
    }

    // Sort: folders first, then by name
    sort_children(&mut root_children);

    TreeNode::Folder(TreeFolder {
        name: "/".to_string(),
        path: PathBuf::from("/"),
        children: root_children,
        expanded: true,
    })
}

/// Insert a single diff entry into the tree, creating intermediate folder nodes as needed.
/// Preserves the full relative path in file nodes, even for nested files.
fn insert_entry(children: &mut Vec<TreeNode>, full_path: &Path, state: FileState, ignored: bool, commit_info: Option<String>) {
    let components: Vec<_> = full_path.components().collect();
    if components.is_empty() {
        return;
    }

    // Work through path components from innermost to outermost.
    // We start from the last component (file name) and wrap it in folders
    // for each parent directory, then insert the whole chain.
    let file_name = components.last().unwrap().as_os_str().to_string_lossy().to_string();
    // Pre-check only Modified files (exist in both, content differs).
    // New files (template-only) start unchecked — they may be files
    // the user intentionally deleted from their project.
    // An ignored file is always pre-unchecked, whatever its state.
    let checked = state == FileState::Modified && !ignored;
    let file_node = TreeNode::File(TreeFile {
        name: file_name,
        path: full_path.to_path_buf(),
        state,
        checked,
        ignored,
        commit_info,
    });

    if components.len() == 1 {
        // Root-level file
        children.push(file_node);
    } else {
        // Nested — wrap the file node in folder nodes with full-qualified paths
        let wrapped = wrap_in_folders(file_node, &components[..components.len() - 1]);
        merge_into(children, wrapped);
    }
}

/// Wrap a tree node in nested folder nodes, from outermost to innermost.
/// Each folder node stores its FULL parent-qualified path (e.g., `"a/b/c"` not just `"c"`),
/// so 
/// `toggle_at` and `toggle_expand_at` correctly identify the right folder even when
/// same-named folders exist at different depths.
fn wrap_in_folders(node: TreeNode, folder_components: &[std::path::Component]) -> TreeNode {
    let mut current = node;
    let total = folder_components.len();
    for (i, comp) in folder_components.iter().rev().enumerate() {
        let name = comp.as_os_str().to_string_lossy().to_string();
        // Build the full path: components[0 .. total-i]  (e.g., for "a/b/c",
        // outer iteration = "a/b/c", middle = "a/b", innermost = "a")
        let folder_path: PathBuf = folder_components[..total - i]
            .iter()
            .map(|c| c.as_os_str())
            .collect();
        current = TreeNode::Folder(TreeFolder {
            name,
            path: folder_path,
            children: vec![current],
            expanded: true,
        });
    }
    current
}

/// Merge a tree node (or branch) into an existing children list,
/// joining at shared folder boundaries.
fn merge_into(children: &mut Vec<TreeNode>, node: TreeNode) {
    match node {
        TreeNode::Folder(new_folder) => {
            // Look for an existing folder with the same name
            if let Some(existing) = children.iter_mut().find(|c| {
                c.name() == new_folder.name && c.is_folder()
            }) {
                // Recurse into it with the new folder's children
                if let TreeNode::Folder(existing_f) = existing {
                    for child in new_folder.children {
                        merge_into(&mut existing_f.children, child);
                    }
                }
            } else {
                children.push(TreeNode::Folder(new_folder));
            }
        }
        other => {
            // Check if a file with same name exists; if so, skip (shouldn't happen)
            if !children.iter().any(|c| c.name() == other.name()) {
                children.push(other);
            }
        }
    }
}

/// Sort children: folders first, then files, alphabetically within each group.
fn sort_children(children: &mut Vec<TreeNode>) {
    children.sort_by(|a, b| {
        match (a.is_folder(), b.is_folder()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name().cmp(b.name()),
        }
    });

    for child in children.iter_mut() {
        if let TreeNode::Folder(f) = child {
            sort_children(&mut f.children);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::FileState;

    fn entry(path: &str, state: FileState) -> DiffEntry {
        DiffEntry {
            relative_path: PathBuf::from(path),
            state,
            commit_info: None,
            ignored: false,
        }
    }

    #[test]
    fn test_build_single_file() {
        let entries = vec![entry("file.txt", FileState::New)];
        let tree = build_tree(&entries);
        assert_eq!(tree.file_count(), 1);
        // New files start unchecked
        assert_eq!(tree.checked_count(), 0);
    }

    #[test]
    fn test_collapsed_state_roundtrip() {
        // Collapse one folder, snapshot, rebuild, restore -> that folder stays
        // collapsed while others stay expanded (the i-toggle rebuild path).
        let entries = vec![
            entry("config/a.ts", FileState::Modified),
            entry("src/lib/b.ts", FileState::New),
        ];
        let mut tree = build_tree(&entries);
        tree.toggle_expand_at(Path::new("config"));

        let mut collapsed = Vec::new();
        tree.collect_collapsed(&mut collapsed);
        assert_eq!(collapsed, vec![PathBuf::from("config")]);

        // Fresh rebuild (all expanded), then restore.
        let mut rebuilt = build_tree(&entries);
        let mut before = Vec::new();
        rebuilt.collect_collapsed(&mut before);
        assert!(before.is_empty(), "fresh build should have nothing collapsed");

        rebuilt.apply_collapsed(&collapsed);
        let mut after = Vec::new();
        rebuilt.collect_collapsed(&mut after);
        assert_eq!(after, vec![PathBuf::from("config")]);
    }

    #[test]
    fn test_file_paths_at_folder() {
        // A folder with two files plus an unrelated root file. Folder `i` should
        // collect exactly the two files under the folder, not the root one.
        let entries = vec![
            entry("config/a.ts", FileState::Modified),
            entry("config/nested/b.ts", FileState::New),
            entry("root.txt", FileState::Modified),
        ];
        let tree = build_tree(&entries);

        let mut under_config = tree.file_paths_at(Path::new("config"));
        under_config.sort();
        assert_eq!(
            under_config,
            vec![PathBuf::from("config/a.ts"), PathBuf::from("config/nested/b.ts")]
        );

        // A path that does not exist yields no files.
        assert!(tree.file_paths_at(Path::new("does/not/exist")).is_empty());
    }

    #[test]
    fn test_build_nested() {
        let entries = vec![
            entry("src/lib/helper.ts", FileState::Modified),
            entry("src/main.ts", FileState::New),
            entry("package.json", FileState::Deleted),
        ];
        let tree = build_tree(&entries);
        assert_eq!(tree.file_count(), 3);
        // Only Modified files are pre-checked
        assert_eq!(tree.checked_count(), 1);
    }

    #[test]
    fn test_toggle_folder() {
        let entries = vec![
            entry("src/a.ts", FileState::Modified),
            entry("src/b.ts", FileState::New),
            entry("readme.md", FileState::Modified),
        ];
        let mut tree = build_tree(&entries);
        // Only Modified files are pre-checked (a.ts, readme.md)
        assert_eq!(tree.checked_count(), 2);

        // Toggle the "src" folder:
        // - a.ts checked, b.ts unchecked → 1 of 2 → new_checked = true → check all
        let src_path = PathBuf::from("src");
        tree.toggle_at(&src_path);
        assert_eq!(tree.checked_count(), 3); // a.ts + b.ts + readme.md

        // Toggle again — all 2 checked → 2 < 2 = false → uncheck all
        tree.toggle_at(&src_path);
        assert_eq!(tree.checked_count(), 1); // only readme.md remains checked
    }

    #[test]
    fn test_deleted_not_checkable() {
        let entries = vec![entry("gone.txt", FileState::Deleted)];
        let tree = build_tree(&entries);
        // Root is a folder; its first child is the deleted file
        assert!(tree.is_folder());
        let checked = tree.checked_count();
        assert_eq!(checked, 0);
    }

    #[test]
    fn test_flatten() {
        let entries = vec![
            entry("src/a.ts", FileState::Modified),
            entry("src/b.ts", FileState::New),
            entry("readme.md", FileState::Deleted),
        ];
        let tree = build_tree(&entries);
        let mut items = Vec::new();
        tree.flatten(0, &mut items);
        // GLOBAL comes first, followed by src, a.ts, b.ts, and readme.md.
        assert_eq!(items.len(), 5);
        assert_eq!(items[0].label, "GLOBAL");
        assert!(items[1].is_folder);
        assert_eq!(items[1].label, "src");
        assert_eq!(items[1].depth, 1);
        // Then files inside src
        assert_eq!(items[2].label, "a.ts");
        assert_eq!(items[3].label, "b.ts");
        // Then root-level file
        assert_eq!(items[4].label, "readme.md");
    }

    #[test]
    fn test_deeply_nested_paths() {
        // Regression test: file paths 3+ levels deep must preserve the full relative path.
        // The original bug truncated nested paths to just the filename.
        let entries = vec![
            entry("a/b/c/deep.txt", FileState::Modified),
            entry("a/b/shallow.txt", FileState::New),
            entry("x/y/z/w/deepest.ts", FileState::Modified),
            entry("root.txt", FileState::Modified),
        ];
        let tree = build_tree(&entries);

        // File counts — only Modified files are pre-checked
        assert_eq!(tree.file_count(), 4);
        assert_eq!(tree.checked_count(), 3);

        // Flatten and verify structure
        let mut items = Vec::new();
        tree.flatten(0, &mut items);

        // GLOBAL comes first, followed by the tree contents.
        assert_eq!(items.len(), 12);

        // Verify folder nesting: a/b/c/deep.txt
        assert_eq!(items[1].label, "a");
        assert!(items[1].is_folder);
        assert_eq!(items[1].depth, 1);

        assert_eq!(items[2].label, "b");
        assert!(items[2].is_folder);
        assert_eq!(items[2].depth, 2);

        assert_eq!(items[3].label, "c");
        assert!(items[3].is_folder);
        assert_eq!(items[3].depth, 3);

        assert_eq!(items[4].label, "deep.txt");
        assert!(!items[4].is_folder);
        assert_eq!(items[4].depth, 4);

        assert_eq!(items[5].label, "shallow.txt");
        assert!(!items[5].is_folder);
        assert_eq!(items[5].depth, 3);

        // 4 levels deep: x/y/z/w/deepest.ts
        assert_eq!(items[6].label, "x");
        assert!(items[6].is_folder);
        assert_eq!(items[6].depth, 1);

        assert_eq!(items[7].label, "y");
        assert!(items[7].is_folder);
        assert_eq!(items[7].depth, 2);

        assert_eq!(items[8].label, "z");
        assert!(items[8].is_folder);
        assert_eq!(items[8].depth, 3);

        assert_eq!(items[9].label, "w");
        assert!(items[9].is_folder);
        assert_eq!(items[9].depth, 4);

        assert_eq!(items[10].label, "deepest.ts");
        assert!(!items[10].is_folder);
        assert_eq!(items[10].depth, 5);

        // Root-level file
        assert_eq!(items[11].label, "root.txt");
        assert!(!items[11].is_folder);
        assert_eq!(items[11].depth, 1);

        // CRITICAL: Verify that collect_checked_paths returns the FULL relative paths,
        // not just the filenames (this was the original path regression bug).
        // Only Modified files are pre-checked (deep.txt, deepest.ts, root.txt).
        // New file (shallow.txt) starts unchecked.
        let mut paths = Vec::new();
        tree.collect_checked_paths(&mut paths);
        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&PathBuf::from("a/b/c/deep.txt")));
        assert!(paths.contains(&PathBuf::from("x/y/z/w/deepest.ts")));
        assert!(paths.contains(&PathBuf::from("root.txt")));
    }

    #[test]
    fn test_sort_order() {
        let entries = vec![
            entry("z_file.txt", FileState::Modified),
            entry("a_folder/file.txt", FileState::Modified),
        ];
        let tree = build_tree(&entries);
        let mut items = Vec::new();
        tree.flatten(0, &mut items);
        // GLOBAL is first, then a_folder (folder), then z_file.txt (file).
        assert_eq!(items[0].label, "GLOBAL");
        assert!(items[1].is_folder, "Expected items[1] to be a folder, got: {}", items[1].label);
        assert!(!items[2].is_folder, "Expected items[2] to be a file, got: {}", items[2].label);
    }
}
