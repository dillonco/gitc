# gitc

A small-footprint desktop git client built with Tauri, Svelte, TypeScript, and Rust.

The app opens this repository by default and can switch to another local git repository
by path. Rust owns all git operations through the installed Git CLI, while the Svelte UI
provides a dense desktop workflow:

- Visual commit graph with lanes, merge edges, ref pills, and search (message, author,
  hash, or ref).
- Commit detail panel: metadata, changed files with per-file diffs, and actions
  (checkout, branch, tag, cherry-pick, revert, reset).
- Staging workflow with path and tree views, hunk staging/unstaging/discard, and
  unified or side-by-side diffs, plus file view, blame, and history.
- Branches (checkout, create, delete), remote branches (checkout tracking), tags
  (create, checkout, delete), and stashes (create, apply, pop, drop).
- Worktrees as first-class sidebar entries: one-click switching between checkouts,
  add (existing, new, or detached branch), remove with a force fallback, and prune
  for stale entries — with branch, path, locked, and stale state on every row.
- Merge/rebase progress banner with continue/abort, and an explicit-save merge editor
  for conflicts (base/ours/theirs/resolved).
- Fetch, pull (ff-only), push, force-push with lease, and an actions menu; settings for
  destructive-action confirmation, default clone path, graph size, and cleanup staleness;
  recent repositories persist across launches.
- Branch & worktree cleanup panel: audits every local branch against a chosen base
  (merged, squash-merged, upstream-gone, or stale), bulk-deletes the safe ones with a
  force fallback for the rest, folds in prunable worktrees, and can restore a branch
  right after deleting it.
- Ref-to-ref compare view: pick any two branches, remote branches, tags, or commits (or
  shift-click two commits in the graph), toggle since-merge-base vs. direct diffing, and
  review the ahead/behind commit list alongside per-file diffs.
- Clone from GitHub through the installed `gh` CLI: browse and filter your repositories
  (or any owner/org) and clone in one click; falls back to a manual URL/path form when
  `gh` isn't installed or isn't signed in. gitc never handles or stores GitHub
  credentials itself — it only shells out to your existing `gh` login.
- Interactive rebase (**experimental**): reorder, reword, squash, fixup, or drop commits
  onto a chosen base, plus a plain "rebase onto" action from the Actions menu, a commit's
  detail panel, or a branch's sidebar row. Scope is intentionally narrow: no `edit` or
  `break` steps, no user `exec` commands, no autosquash, and it refuses to start unless
  the working tree is clean.

Provider-backed sections that need accounts or cloud services are tracked in
[Future Integrations](docs/future-integrations.md) and are not shown in the current UI.

## Commands

```sh
npm install
npm run dev
npm run check
npm run build
```

`npm run dev:ui` serves the UI in a plain browser with an in-memory demo repository
(no Rust build needed), which is useful for design work and UI testing.

`npm run build` produces a macOS app bundle at:

```text
src-tauri/target/release/bundle/macos/gitc.app
```
