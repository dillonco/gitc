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
  (create, checkout, delete), stashes (create, apply, pop, drop), and worktrees.
- Merge/rebase progress banner with continue/abort, and an explicit-save merge editor
  for conflicts (base/ours/theirs/resolved).
- Fetch, pull (ff-only), push, force-push with lease, and an actions menu; settings for
  destructive-action confirmation, default clone path, and graph size; recent
  repositories persist across launches.

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
