# gitc

A small-footprint desktop git client built with Tauri, Svelte, TypeScript, and Rust.

The app opens this repository by default and can switch to another local git repository
by path. Rust owns all git operations through the installed Git CLI, while the Svelte UI
provides a dense desktop workflow for graph/status visualization, branch actions, stash
operations, rebase/merge controls, reset/cherry-pick, hunk staging, and explicit-save
conflict resolution.

Provider-backed sections that need accounts or cloud services are tracked in
[Future Integrations](docs/future-integrations.md) and are not shown in the current UI.

## Commands

```sh
npm install
npm run dev
npm run check
npm run build
```

`npm run build` produces a macOS app bundle at:

```text
src-tauri/target/release/bundle/macos/gitc.app
```
