<script lang="ts">
  // Stream F3 — clone dialog: browse the user's GitHub repos via `gh` (with a
  // manual URL fallback). See PLAN.md section 4 and REVIEW-UX.md section 5
  // for the full design this implements.
  import { cloneRepository, ghRepoList, ghStatus, openTerminal } from "./git";
  import { trapFocus } from "./modal";
  import { suggestClonePath } from "./cloneUtils";
  import type { GhRepo, GhStatus, RepositoryState } from "./types";

  export let clonePath: string;
  export let onClose: () => void;
  export let onCloned: (state: RepositoryState) => void | Promise<void>;

  const REPO_FETCH_LIMIT = 100;

  // `gh_status` and `gh_repo_list` are fired together (REVIEW-PERF 2.4) so
  // the ~2.1-2.2s repo list fetch does not delay the ~245ms status check.
  // The tab defaults to GitHub optimistically (so `#gh-filter` is always the
  // initial-focus target); it flips to URL exactly once, when status
  // resolves to "not installed" or "not signed in", and never auto-switches
  // again after that.
  let activeTab: "github" | "url" = "github";
  let statusLoading = true;
  let status: GhStatus | null = null;

  let repos: GhRepo[] = [];
  let reposLoading = true;
  let reposError = "";

  let filterQuery = "";
  let owner = "";
  let highlightedIndex = 0;
  let lastPathRepoKey: string | null = null;
  let ghClonePath = "";

  let urlValue = "";
  let urlPathValue = "";
  let pathTouched = false;

  let busy = false;
  let busyLabel = "";
  let cloneError = "";
  let cloneErrorTarget = "";

  loadStatus();
  loadRepos();

  function loadStatus() {
    statusLoading = true;
    ghStatus()
      .then((result) => {
        status = result;
        if (!(result.installed && result.authenticated)) activeTab = "url";
      })
      .catch((err) => {
        status = {
          installed: false,
          authenticated: false,
          login: null,
          host: "github.com",
          protocol: "https",
          message: String(err),
        };
        activeTab = "url";
      })
      .finally(() => {
        statusLoading = false;
      });
  }

  function loadRepos(forOwner: string | null = null) {
    reposLoading = true;
    reposError = "";
    ghRepoList(forOwner, REPO_FETCH_LIMIT)
      .then((result) => {
        repos = result;
        highlightedIndex = 0;
      })
      .catch((err) => {
        reposError = String(err);
      })
      .finally(() => {
        reposLoading = false;
      });
  }

  function retry() {
    loadStatus();
    loadRepos(owner.trim() || null);
  }

  async function handleOpenTerminal() {
    try {
      await openTerminal();
    } catch {
      // Best-effort convenience affordance; nothing meaningful to show if
      // the platform has no terminal to open (e.g. the browser demo).
    }
  }

  $: query = filterQuery.trim().toLowerCase();
  $: filteredRepos = query
    ? repos.filter(
        (repo) =>
          repo.name.toLowerCase().includes(query) ||
          repo.owner.toLowerCase().includes(query) ||
          (repo.description ?? "").toLowerCase().includes(query),
      )
    : repos;
  $: if (highlightedIndex > filteredRepos.length - 1) highlightedIndex = Math.max(0, filteredRepos.length - 1);
  $: activeRepo = filteredRepos[highlightedIndex] ?? null;
  $: activeOptionId = activeRepo ? `gh-repo-${highlightedIndex}` : undefined;

  // The path preview follows the highlighted row live (arrowing through the
  // list re-suggests the path); editing it while the same repo stays
  // highlighted keeps the edit.
  $: if (activeRepo && activeRepo.nameWithOwner !== lastPathRepoKey) {
    ghClonePath = suggestClonePath(activeRepo.url, clonePath);
    lastPathRepoKey = activeRepo.nameWithOwner;
  } else if (!activeRepo) {
    ghClonePath = "";
    lastPathRepoKey = null;
  }

  // URL tab: auto-suggest the path from the URL until the user edits it.
  $: if (!pathTouched) urlPathValue = urlValue.trim() ? suggestClonePath(urlValue, clonePath) : "";

  $: primaryEnabled =
    !busy &&
    (activeTab === "github"
      ? Boolean(status?.authenticated) && Boolean(activeRepo) && Boolean(ghClonePath.trim())
      : Boolean(urlValue.trim()) && Boolean(urlPathValue.trim()));

  function onListNav(event: KeyboardEvent) {
    if (busy) return;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        highlightedIndex = Math.min(filteredRepos.length - 1, highlightedIndex + 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        highlightedIndex = Math.max(0, highlightedIndex - 1);
        break;
      case "Home":
        event.preventDefault();
        highlightedIndex = 0;
        break;
      case "End":
        event.preventDefault();
        highlightedIndex = Math.max(0, filteredRepos.length - 1);
        break;
      case "Enter":
        if (!(event.metaKey || event.ctrlKey)) {
          // The highlighted row is already the live selection (see the
          // reactive block above); a bare Enter confirms it without
          // cloning. Only Cmd/Ctrl+Enter, the Clone button, or a
          // double-click actually clone — cloning is uncancellable, so a
          // stray Enter must never trigger it.
          event.preventDefault();
        }
        break;
      default:
        break;
    }
  }

  function onOwnerKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !(event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      loadRepos(owner.trim() || null);
    }
  }

  function onRowClick(index: number) {
    if (busy) return;
    highlightedIndex = index;
  }

  function onRowDblClick(repo: GhRepo, index: number) {
    if (busy) return;
    highlightedIndex = index;
    cloneFromGithub(repo);
  }

  function onUrlFieldKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !(event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      cloneFromUrl();
    }
  }

  function cloneFromGithub(repoOverride?: GhRepo) {
    const repo = repoOverride ?? activeRepo;
    if (!repo || busy) return;
    const path = repoOverride ? suggestClonePath(repo.url, clonePath) : ghClonePath;
    if (!path.trim()) return;
    const url = status?.protocol === "ssh" ? repo.sshUrl : repo.url;
    performClone(url, path, repo.nameWithOwner);
  }

  function cloneFromUrl() {
    if (busy || !urlValue.trim() || !urlPathValue.trim()) return;
    performClone(urlValue.trim(), urlPathValue.trim(), urlValue.trim());
  }

  function primaryAction() {
    if (!primaryEnabled) return;
    if (activeTab === "github") cloneFromGithub();
    else cloneFromUrl();
  }

  async function performClone(url: string, path: string, label: string) {
    busy = true;
    cloneError = "";
    busyLabel = `Cloning ${label} into ${path} — this can take a while.`;
    cloneErrorTarget = label;
    try {
      const nextState = await cloneRepository(url, path);
      await onCloned(nextState);
    } catch (err) {
      cloneError = String(err);
    } finally {
      busy = false;
    }
  }

  function formatPushed(pushedAt?: string | null): string {
    if (!pushedAt) return "";
    const date = new Date(pushedAt);
    if (Number.isNaN(date.getTime())) return "";
    const rtf = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
    const divisions: [number, Intl.RelativeTimeFormatUnit][] = [
      [60, "seconds"],
      [60, "minutes"],
      [24, "hours"],
      [7, "days"],
      [4.34524, "weeks"],
      [12, "months"],
      [Number.POSITIVE_INFINITY, "years"],
    ];
    let duration = (date.getTime() - Date.now()) / 1000;
    for (const [amount, unit] of divisions) {
      if (Math.abs(duration) < amount) return `pushed ${rtf.format(Math.round(duration), unit)}`;
      duration /= amount;
    }
    return "";
  }

  function onKey(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    const primaryChord = (event.metaKey || event.ctrlKey) && event.key === "Enter";
    if (primaryChord) {
      event.preventDefault();
      primaryAction();
      return;
    }
    if (event.key === "Enter") {
      const target = event.target as HTMLElement | null;
      if (target && target.tagName === "INPUT" && target.id !== "gh-filter" && target.id !== "gh-owner") {
        event.preventDefault();
        primaryAction();
      }
      return;
    }
    if (event.key === "Escape") {
      if (busy) return;
      event.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window on:keydown={onKey} />

<div
  class="modal-backdrop"
  role="presentation"
  on:click={(event) => event.target === event.currentTarget && !busy && onClose()}
>
  <div
    class="modal panel size-m"
    role="dialog"
    aria-modal="true"
    aria-labelledby="clone-title"
    use:trapFocus={{ initial: "#gh-filter" }}
  >
    <div class="panel-title">
      <h1 id="clone-title">Clone Repository</h1>
      <button type="button" title="Close" aria-label="Close" on:click={onClose} disabled={busy}>×</button>
    </div>
    {#if busy}<div class="busy-bar" aria-hidden="true"></div>{/if}
    <div class="modal-body">
      {#if busy}
        <p class="gh-status-line" role="status">{busyLabel}</p>
      {/if}

      <div class="segmented gh-tabs" role="tablist" aria-label="Clone source">
        <button
          type="button"
          role="tab"
          id="tab-github"
          aria-selected={activeTab === "github"}
          aria-controls="panel-github"
          class:active={activeTab === "github"}
          disabled={statusLoading || busy}
          on:click={() => (activeTab = "github")}
        >
          GitHub
        </button>
        <button
          type="button"
          role="tab"
          id="tab-url"
          aria-selected={activeTab === "url"}
          aria-controls="panel-url"
          class:active={activeTab === "url"}
          disabled={statusLoading || busy}
          on:click={() => (activeTab = "url")}
        >
          URL
        </button>
      </div>

      {#if activeTab === "github"}
        <div id="panel-github" role="tabpanel" aria-labelledby="tab-github" class="gh-panel" class:busy-lock={busy}>
          <div class="gh-filter-row">
            <input
              id="gh-filter"
              type="text"
              placeholder="Filter by name, owner, or description"
              aria-label="Filter by name, owner, or description"
              aria-activedescendant={activeOptionId}
              aria-controls="gh-repo-list"
              autocomplete="off"
              bind:value={filterQuery}
              on:keydown={onListNav}
              disabled={busy}
            />
            <input
              id="gh-owner"
              class="gh-owner"
              type="text"
              placeholder="owner or org"
              aria-label="Owner or organization"
              bind:value={owner}
              on:keydown={onOwnerKeydown}
              disabled={statusLoading || !status?.authenticated || busy}
            />
          </div>

          {#if statusLoading}
            <p class="empty">Checking for GitHub CLI…</p>
          {:else if !status?.installed}
            <p class="empty">GitHub CLI not found.<br />Install it with <code>brew install gh</code>, then Retry.</p>
            <div class="gh-actions">
              <button type="button" on:click={retry} disabled={busy}>Retry</button>
            </div>
          {:else if !status?.authenticated}
            <p class="empty">GitHub CLI is not signed in.<br />Run <code>gh auth login</code> in a terminal, then Retry.</p>
            <div class="gh-actions">
              <button type="button" on:click={retry} disabled={busy}>Retry</button>
              <button type="button" on:click={handleOpenTerminal} disabled={busy}>Open Terminal</button>
            </div>
          {:else}
            <p class="gh-status-line" aria-live="polite">
              {#if reposLoading}
                Loading repositories for {status?.login}…
              {:else if !reposError}
                {status?.login} · {repos.length} repositor{repos.length === 1 ? "y" : "ies"} · via {status?.protocol === "ssh" ? "SSH" : "HTTPS"}
              {/if}
            </p>
            {#if reposError}
              <p class="panel-error" role="alert">Could not load repositories&#10;{reposError}</p>
            {:else}
              <div
                id="gh-repo-list"
                class="gh-list"
                role="listbox"
                aria-label="Repositories"
                tabindex="0"
                aria-activedescendant={activeOptionId}
                on:keydown={onListNav}
              >
                {#each filteredRepos as repo, i (repo.nameWithOwner)}
                  <div
                    id={`gh-repo-${i}`}
                    role="option"
                    tabindex="-1"
                    aria-selected={i === highlightedIndex}
                    class="gh-row"
                    class:selected={i === highlightedIndex}
                    title={repo.nameWithOwner}
                    on:click={() => onRowClick(i)}
                    on:dblclick={() => onRowDblClick(repo, i)}
                    on:keydown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault();
                        onRowClick(i);
                      }
                    }}
                  >
                    <div class="gh-row-line1">
                      <span class="gh-name"><span class="gh-owner-name">{repo.owner}/</span>{repo.name}</span>
                      <span class="gh-chips">
                        {#if repo.isPrivate}<span class="chip">private</span>{/if}
                        {#if repo.isFork}<span class="chip">fork</span>{/if}
                        {#if repo.isArchived}<span class="chip warn">archived</span>{/if}
                      </span>
                    </div>
                    <div class="gh-row-line2">
                      <span class="gh-desc">{repo.description ?? "—"}</span>
                      <span class="gh-meta" title={repo.pushedAt ?? undefined}>
                        {[repo.language, formatPushed(repo.pushedAt)].filter(Boolean).join(" · ")}
                      </span>
                    </div>
                  </div>
                {:else}
                  <p class="empty gh-empty">
                    {query
                      ? `No repositories match "${filterQuery.trim()}"`
                      : `No repositories found for ${owner.trim() || status?.login}`}
                  </p>
                {/each}
              </div>
              {#if repos.length >= REPO_FETCH_LIMIT}
                <p class="gh-footnote">Showing the {REPO_FETCH_LIMIT} most recently pushed</p>
              {/if}
              <div class="field">
                <label for="clone-path">clone into</label>
                <input id="clone-path" type="text" bind:value={ghClonePath} placeholder="/path/to/dev/repo" disabled={busy} />
              </div>
            {/if}
          {/if}
        </div>
      {:else}
        <div id="panel-url" role="tabpanel" aria-labelledby="tab-url" class:busy-lock={busy}>
          <div class="field">
            <label for="clone-url">repository url</label>
            <input
              id="clone-url"
              type="text"
              placeholder="https://github.com/owner/repo.git or git@github.com:owner/repo.git"
              bind:value={urlValue}
              on:keydown={onUrlFieldKeydown}
              disabled={busy}
            />
          </div>
          <div class="field">
            <label for="clone-url-path">clone into</label>
            <input
              id="clone-url-path"
              type="text"
              placeholder="/path/to/dev/repo"
              bind:value={urlPathValue}
              on:input={() => (pathTouched = true)}
              on:keydown={onUrlFieldKeydown}
              disabled={busy}
            />
          </div>
        </div>
      {/if}

      {#if cloneError}
        <p class="panel-error" role="alert">Could not clone {cloneErrorTarget}&#10;{cloneError}</p>
      {/if}
    </div>
    <div class="modal-footer">
      <button type="button" on:click={onClose} disabled={busy}>Cancel</button>
      <button type="button" class="stage-file" on:click={primaryAction} disabled={!primaryEnabled}>
        {busy ? "Cloning…" : "Clone"}
      </button>
    </div>
  </div>
</div>

<style>
  .gh-tabs {
    /* `.modal-body` (frozen, shared) is `display: grid` with implicit
       `auto`-sized rows; `.segmented` (frozen, shared) sets
       `overflow: hidden`, which per the flex/grid spec zeroes out a flex
       container's *automatic* minimum size. As a direct grid-item child of
       `.modal-body` that collapses this row to a couple of pixels instead
       of the height its button children actually need. Pin a real height
       here (in our own scoped style, not the frozen files) rather than
       touching either shared rule. */
    flex-shrink: 0;
    min-height: 30px;
    margin-bottom: 10px;
  }

  .gh-filter-row {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
  }

  .gh-filter-row input:first-child {
    flex: 1;
    min-width: 0;
  }

  .gh-owner {
    width: 140px;
    flex-shrink: 0;
  }

  .gh-status-line {
    margin: 0 0 8px;
    color: #8b93a0;
    font-size: 11px;
  }

  .gh-actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }

  .gh-list {
    max-height: 46vh;
    overflow: auto;
    border: 1px solid var(--edge);
    border-radius: 8px;
  }

  .gh-list:focus-visible {
    outline: 2px solid var(--teal);
    outline-offset: -2px;
  }

  .gh-row {
    display: grid;
    grid-template-rows: auto auto;
    gap: 2px;
    min-height: 44px;
    padding: 6px 10px;
    cursor: pointer;
  }

  .gh-row + .gh-row {
    border-top: 1px solid var(--edge);
  }

  .gh-row:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .gh-row.selected {
    background: rgba(110, 168, 254, 0.13);
    box-shadow: inset 3px 0 0 #4f83d6;
  }

  .gh-row-line1 {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }

  .gh-name {
    overflow: hidden;
    color: #e8ecf1;
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gh-owner-name {
    color: #aab2bd;
    font-weight: 400;
  }

  .gh-chips {
    display: flex;
    flex-shrink: 0;
    gap: 4px;
  }

  .gh-row-line2 {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    color: #aab2bd;
    font-size: 11px;
  }

  .gh-desc {
    overflow: hidden;
    min-width: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gh-meta {
    flex-shrink: 0;
    white-space: nowrap;
  }

  .gh-empty {
    margin: 0;
    padding: 14px 10px;
  }

  .gh-footnote {
    margin: 6px 0 0;
    color: #8b93a0;
    font-size: 11px;
  }

  .busy-lock {
    opacity: 0.55;
    pointer-events: none;
  }
</style>
