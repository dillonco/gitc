# kGit

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
- Clone from GitHub through the installed `gh` CLI: choose where to clone to, then pick
  from every repository your account can reach (yours, your organizations', and ones you
  collaborate on) in a searchable dropdown grouped by owner, with an owner filter. Paste a
  GitHub URL to jump to it. Clones show live progress and can be stopped; the destination
  is checked first (an existing checkout of the same repo opens instead). Options cover
  branch (a dropdown of the remote's branches), shallow clone, and submodules, and forks
  get an `upstream` remote. The URL tab takes any git URL or `owner/repo` shorthand.
  kGit never handles or stores GitHub credentials itself — it only shells out to your
  existing `gh` login (including as the HTTPS credential helper for the clone).
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
src-tauri/target/release/bundle/macos/kGit.app
```

## Releases

kGit ships outside the Mac App Store: it runs your own `git` and `gh` against folders
anywhere on disk, which the App Store sandbox doesn't allow. Releases are signed with a
Developer ID certificate, notarized by Apple, and published as GitHub Releases. Installed
copies check the latest release on launch and offer to restart into it.

To cut a release, bump `version` in `package.json` on `main`, then run the **Release**
workflow from the Actions tab (or `gh workflow run release.yml`). It builds a universal
app, signs and notarizes it, and publishes `v<version>` with a `.dmg` for new installs
and the signed update bundle plus `latest.json` for the updater.

The workflow needs these repository secrets:

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | Developer ID Application certificate and key, exported as `.p12`, base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | Password chosen when exporting the `.p12` |
| `APPLE_API_ISSUER` | App Store Connect API issuer ID (Users and Access → Integrations) |
| `APPLE_API_KEY_ID` | App Store Connect API key ID |
| `APPLE_API_PRIVATE_KEY` | Contents of that key's `AuthKey_<id>.p8` |
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `~/.tauri/kgit-updater.key`, which signs update bundles |

The updater's public key is in `src-tauri/tauri.conf.json`. Keep the private key: losing
it means installed copies can no longer verify updates and have to be reinstalled by hand.

## License

Proprietary — copyright (c) 2026 Dillon, all rights reserved. The source is
published for reference only; see [LICENSE](LICENSE).
