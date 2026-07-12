# PLAN: .reesyncignore (clone-owned skip-list + TUI edit)

## Problem

reesync shows the full template->clone diff and the operator hand-unchecks the
project-owned files (`package.json`, `wrangler.jsonc`, `config/*.override.ts`,
`src/css/**`, ...) every run. That skip-list lives only in the operator's head:
re-decided each sync, not written down, not transferable. We want it to be a
versioned, inherited artifact - with sensible defaults seeded per clone and an
in-TUI way to grow/shrink it.

## Decisions (agreed)

1. **One checkbox + an `i` action key.** No second checkbox. The existing sync
   checkbox (`space`) is the per-run choice. Ignore-list membership sets the
   *default* checked state; `i` toggles membership. Avoids the contradictory
   "checked-to-sync AND ignored" state.
2. **Pattern format: glob via `globset`.** Patterns match the project-root-
   relative path the diff already produces (`strip_prefix(root)` in diff.rs).
   Expresses `src/css/**`, `config/*.override.ts`, exact files, etc.
3. **`i` applies immediately.** Writes `.reesyncignore` and re-filters the tree
   in-session (mirrors how `space` re-flattens). Ignore-edits persist on disk
   even if the operator `q`-quits without syncing - the list is a durable
   statement, separate from "did I sync this run".
4. **Seeded by the template, self-ignored.** reeweb ships a starter
   `.reesyncignore` at repo root; first untar seeds the clone. Because the walk
   already skips all dotfiles (diff.rs:47 `name.starts_with('.')`),
   `.reesyncignore` never appears in the diff and never syncs over itself - no
   special-casing. Existing clones keep their own copy; only new clones get an
   improved starter list (correct for a clone-owned file).

## Behavior

- An **ignored** file (matches a `.reesyncignore` glob) renders **visible but
  dimmed and pre-unchecked**. Full visibility preserved (no hidden skips - the
  anti-pattern we avoid). Operator can still `space`-check it for a one-off pull.
- **`i`** on the highlighted entry:
  - not yet ignored -> add a matching pattern to `.reesyncignore`, dim + uncheck
    the entry now.
  - already ignored (exact-path membership) -> remove that line, un-dim; leave
    checked state to the normal default (new/modified pre-checked).
  - `i` operates on the exact relative path of the highlighted file. It does not
    author globs - a human edits `.reesyncignore` directly for `**`/`*` patterns.
    (Glob patterns from the seed/file are honored for matching; `i` only
    adds/removes concrete file lines.)
- Folders: `i` on a folder is out of scope for v1 (operate on files only). A
  future step could add a whole subtree via a `dir/**` line.

## reesync changes (Rust)

Files: `src/main.rs` (keybinds, footer), `src/diff.rs` (load patterns, mark
ignored), `src/tree.rs` (carry an `ignored` flag + default-unchecked), and
`Cargo.toml` (+`globset`).

1. **Cargo.toml** - add `globset = "0.4"`.
2. **Ignore loader** - read `.reesyncignore` from the PROJECT dir (cwd / the
   dir reesync is run in, i.e. the target), one glob per line, `#` comments and
   blank lines skipped. Build a `GlobSet`. Missing file -> empty set (no error).
3. **DiffEntry / tree node** - add `ignored: bool`. Set it when the entry's
   relative path matches the GlobSet. Ignored entries default to **unchecked**
   (override the normal new/modified pre-check).
4. **Render** - dim ignored rows; show a marker (e.g. trailing `(ignored)`,
   like the existing `(deleted)`).
5. **`i` keybind** - in the `match key.code` block next to `Char(' ')`:
   - resolve highlighted path (skip the synthetic `/` root, like the others);
   - toggle its exact-path line in `.reesyncignore` (append or remove);
   - rebuild the GlobSet, re-mark entries, `flatten` again.
6. **Footer** - add `i ignore` to the hint line.
7. **Persistence** - write `.reesyncignore` atomically (write temp, rename) so a
   mid-session edit can't truncate the file.

## reeweb change

- Add `reeweb/.reesyncignore` (repo root) with the starter list:
  ```
  # Clone-owned files - reesync pre-unchecks these (see PLAN_reesyncignore).
  package.json
  wrangler.jsonc
  .env
  config/*.override.ts
  src/css/**
  ```
  Exact set TBD - confirm which files are genuinely clone-owned vs engine.
- Verify it ships in the tarball (release.ts / install.ts do not strip dotfiles).

## Decisions (resolved)

- **Q1 (a): `i` manages exact-path lines only.** On a file already ignored by a
  *glob* (e.g. `src/css/**`), `i` is a no-op that shows a status message
  ("ignored by pattern <glob> - edit .reesyncignore to change"). No negation
  authoring in the TUI; globs are edited by hand in the file.
- **Q2: cwd is the anchor.** reesync runs from inside the clone
  (`cd clone; reesync ../template`), so `.reesyncignore` is `./.reesyncignore`
  in the current working directory (the target).
- **Q3: seed list confirmed** as drafted (`package.json`, `wrangler.jsonc`,
  `.env`, `config/*.override.ts`, `src/css/**`). Adjust when building reeweb side
  if a listed path turns out to be engine-owned.

## Verification

- Unit (reesync): a GlobSet from sample patterns marks the right relative paths
  ignored; loader tolerates missing file / comments / blanks.
- Manual TUI: ignored files render dimmed+unchecked; `i` toggles a file and the
  row updates immediately; `.reesyncignore` on disk reflects the edit; quitting
  without sync keeps the edit.
- reeweb: fresh untar contains `.reesyncignore`; running reesync in the clone
  pre-unchecks the seeded files.
