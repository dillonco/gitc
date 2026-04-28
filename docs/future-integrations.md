# Future Integrations

These sections were intentionally removed from the current UI until they have real backing behavior.

## Agents

- Add an agent/workflow model for assisted commit summaries, conflict resolution, and review tasks.
- Show active/background jobs, status, logs, and retry/cancel controls.
- Keep all agent actions explicit before mutating git state.

## Pull Requests

- Add provider accounts for GitHub/GitLab/Bitbucket.
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
