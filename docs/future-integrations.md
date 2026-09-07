# Future Integrations

These sections were intentionally removed from the current UI until they have real backing behavior.

**GitHub clone is no longer future** — gitc can browse and clone your GitHub repositories
today by shelling out to the installed `gh` CLI (see the README). That deliberately covers
only clone: it reuses whatever account `gh auth login` already has signed in, and gitc
itself never performs OAuth, never requests a token, and never stores credentials. Every
section below is a *provider account* integration — PRs, Issues, Teams, and Cloud Patches
all need gitc to hold and act on behalf of a signed-in account (list/act on PRs, look up
issues, resolve collaborators, sync patches), which is a materially bigger commitment than
"run a local CLI the user already authenticated" and remains genuinely future work.

## Agents

- Add an agent/workflow model for assisted commit summaries, conflict resolution, and review tasks.
- Show active/background jobs, status, logs, and retry/cancel controls.
- Keep all agent actions explicit before mutating git state.

## Pull Requests

- Add provider accounts for GitHub/GitLab/Bitbucket (beyond the unauthenticated-clone use
  of `gh` described above).
- List PRs for the active repository and current branch.
- Support checkout, open in browser, review comments, CI status, and merge readiness.

## Issues

- Add issue search and branch-from-issue workflows.
- Link commits and branches to issue IDs.
- Provide provider-specific filters without blocking local git workflows.

## Teams

- Add account/team context from connected providers.
- Show repository collaborators, reviewers, and ownership metadata.
- Avoid showing this section when no provider account is connected.

## Cloud Patches

- Define a patch sharing model before exposing UI.
- Support export/import of patch files as the local-first baseline.
- Add cloud sync only after authentication and conflict handling are designed.
