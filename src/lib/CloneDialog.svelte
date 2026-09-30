<script lang="ts">
  // Clone dialog: pick where to clone to, then a repository from every repo
  // the `gh` account can reach (grouped by owner, searchable), or any URL.
  // The clone streams progress and can be stopped; the destination is
  // checked before the Clone button is offered.
  import { tick } from "svelte";
  import {
    cancelClone,
    cloneRepository,
    existingCheckouts,
    ghRepoList,
    ghRepoParent,
    ghStatus,
    inspectCloneTarget,
    listRemoteBranches,
    openTerminal,
    pickRepositoryFolder,
  } from "./git";
  import { trapFocus } from "./modal";
  import {
    cacheRepoList,
    cachedRepoList,
    githubCloneUrl,
    joinPath,
    lastCloneParent,
    rememberCloneParent,
    resolveCloneUrl,
    sameRemote,
    splitPath,
    suggestClonePath,
  } from "./cloneUtils";
  import RepoPicker from "./RepoPicker.svelte";
  import type { CloneProgress, CloneTarget, GhRepo, GhStatus, RemoteBranches, RepositoryState } from "./types";

  export let clonePath: string;
  export let onClose: () => void;
  export let onCloned: (state: RepositoryState) => void | Promise<void>;
  export let onOpenExisting: (path: string) => void | Promise<void>;
  export let showAvatars = true;

  // `gh_status` and `gh_repo_list` are fired together so the repo list fetch
  // does not delay the quick status check. The tab defaults to GitHub
  // optimistically; it flips to URL once, when status resolves to "not
  // installed" or "not signed in", and never auto-switches after that.
  let activeTab: "github" | "url" = "github";
  let statusLoading = true;
  let status: GhStatus | null = null;

  let repos: GhRepo[] = cachedRepoList() ?? [];
  let reposLoading = repos.length === 0;
  let reposRefreshing = false;
  let reposError = "";
  let onDisk = new Set<string>();

  let selectedRepo: GhRepo | null = null;
  let urlValue = "";

  let parentDir = lastCloneParent() || clonePath;
  let folderName = "";
  let lastSource = "";

  let optionsOpen = false;
  let branch = "";
  let shallow = false;
  let submodules = false;
  let addUpstream = true;
  let forkParent: string | null = null;
  let remoteBranches: RemoteBranches | null = null;
  let branchesFor = "";
  let branchesLoading = false;
  let branchesError = "";
  let branchTimer: ReturnType<typeof setTimeout> | undefined;

  let target: CloneTarget | null = null;
  let targetFor = "";
  let targetRequest = 0;
  let targetTimer: ReturnType<typeof setTimeout> | undefined;
  let onDiskTimer: ReturnType<typeof setTimeout> | undefined;

  let busy = false;
  let stopping = false;
  let progress: CloneProgress | null = null;
  let cloneError = "";
  let cloneErrorTarget = "";
  let cloneNotice = "";

  loadStatus();
  loadRepos();

  function loadStatus() {
    statusLoading = true;
    ghStatus()
      .then((result) => {
        status = result;
        if (!(result.installed && result.authenticated)) switchToUrlTab();
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
        switchToUrlTab();
      })
      .finally(() => {
        statusLoading = false;
      });
  }

  // The picker had focus; move it somewhere that still exists.
  async function switchToUrlTab() {
    activeTab = "url";
    await tick();
    const active = document.activeElement;
    if (!active || active === document.body || !document.querySelector(".clone-dialog")?.contains(active)) {
      document.getElementById("clone-url")?.focus();
    }
  }

  function loadRepos() {
    reposError = "";
    if (repos.length) reposRefreshing = true;
    else reposLoading = true;
    ghRepoList()
      .then((result) => {
        repos = result;
        cacheRepoList(result);
        if (selectedRepo) {
          selectedRepo = result.find((repo) => repo.nameWithOwner === selectedRepo?.nameWithOwner) ?? selectedRepo;
        }
      })
      .catch((err) => {
        reposError = String(err);
      })
      .finally(() => {
        reposLoading = false;
        reposRefreshing = false;
      });
  }

  function retry() {
    loadStatus();
    loadRepos();
  }

  async function handleOpenTerminal() {
    try {
      await openTerminal();
    } catch {
      // Best-effort convenience affordance; nothing meaningful to show if
      // the platform has no terminal to open (e.g. the browser demo).
    }
  }

  // Mark repos that already have a checkout under the chosen folder.
  $: scheduleOnDisk(repos, parentDir);

  function scheduleOnDisk(list: GhRepo[], parent: string) {
    clearTimeout(onDiskTimer);
    onDiskTimer = setTimeout(() => {
      const paths = list.map((repo) => joinPath(parent, repo.name));
      existingCheckouts(paths)
        .then((found) => {
          const existing = new Set(found);
          onDisk = new Set(list.filter((_, i) => existing.has(paths[i])).map((repo) => repo.nameWithOwner));
        })
        .catch(() => {});
    }, 200);
  }

  $: resolvedUrl = resolveCloneUrl(urlValue, status?.protocol);
  $: sourceUrl =
    activeTab === "github"
      ? selectedRepo
        ? status?.protocol === "ssh"
          ? selectedRepo.sshUrl
          : selectedRepo.url
        : ""
      : resolvedUrl;
  $: sourceLabel = activeTab === "github" ? (selectedRepo?.nameWithOwner ?? "") : resolvedUrl;

  // The folder name follows the chosen repository; choosing a different
  // repository resets it (and the per-repo branch). Edits stick until then.
  $: onSourceChange(sourceUrl);

  function onSourceChange(url: string) {
    if (url === lastSource) return;
    lastSource = url;
    folderName = url ? splitPath(suggestClonePath(url, "/")).name : "";
    branch = "";
    remoteBranches = null;
    branchesFor = "";
    branchesError = "";
  }

  $: currentPath = joinPath(parentDir, folderName);

  // Forks: ask GitHub which repository this one came from.
  $: loadForkParent(activeTab === "github" ? selectedRepo : null);

  function loadForkParent(repo: GhRepo | null) {
    forkParent = null;
    if (!repo?.isFork) return;
    const key = repo.nameWithOwner;
    ghRepoParent(key)
      .then((parent) => {
        if (selectedRepo?.nameWithOwner === key) forkParent = parent;
      })
      .catch(() => {});
  }

  // Branches load only once Options is open, so browsing costs nothing.
  $: if (optionsOpen) scheduleBranches(sourceUrl);

  function scheduleBranches(url: string) {
    clearTimeout(branchTimer);
    if (!url || url === branchesFor) return;
    branchTimer = setTimeout(
      () => {
        branchesFor = url;
        branchesLoading = true;
        branchesError = "";
        listRemoteBranches(url)
          .then((result) => {
            if (branchesFor === url) remoteBranches = result;
          })
          .catch((err) => {
            if (branchesFor === url) {
              remoteBranches = null;
              branchesError = String(err).replace(/^Error: /, "");
            }
          })
          .finally(() => {
            if (branchesFor === url) branchesLoading = false;
          });
      },
      activeTab === "url" ? 450 : 0,
    );
  }

  // Check the destination shortly after it stops changing.
  $: scheduleTargetCheck(currentPath);

  function scheduleTargetCheck(path: string) {
    clearTimeout(targetTimer);
    const request = ++targetRequest;
    if (!path) {
      target = null;
      targetFor = "";
      return;
    }
    targetTimer = setTimeout(() => {
      inspectCloneTarget(path)
        .then((result) => {
          if (request !== targetRequest) return;
          target = result;
          targetFor = path;
        })
        .catch(() => {
          if (request !== targetRequest) return;
          target = null;
          targetFor = "";
        });
    }, 150);
  }

  $: targetKnown = Boolean(target) && targetFor === currentPath;
  $: targetBlocked = targetKnown && Boolean(target?.exists) && !(target?.isDir && target?.isEmpty);
  $: targetIsSameRepo = targetBlocked && Boolean(target?.isRepo) && sameRemote(target?.originUrl, sourceUrl);

  $: canClone =
    !busy &&
    !targetBlocked &&
    Boolean(currentPath) &&
    Boolean(sourceUrl) &&
    (activeTab === "url" || Boolean(status?.authenticated));
  $: primaryEnabled = targetIsSameRepo ? !busy : canClone;
  $: primaryLabel = busy ? "Cloning…" : targetIsSameRepo ? "Open Repository" : "Clone";

  function selectRepo(repo: GhRepo) {
    selectedRepo = repo;
  }

  async function useInUrlTab(ref: string) {
    urlValue = ref;
    activeTab = "url";
    await tick();
    document.getElementById("clone-url")?.focus();
  }

  async function chooseFolder() {
    if (busy) return;
    const picked = await pickRepositoryFolder("Choose where to clone to").catch(() => null);
    if (!picked) return;
    parentDir = picked.replace(/\/+$/, "") || "/";
  }

  function primaryAction() {
    if (!primaryEnabled) return;
    if (targetIsSameRepo) {
      void onOpenExisting(targetFor);
      return;
    }
    const upstreamUrl = forkParent && addUpstream ? githubCloneUrl(forkParent, status?.protocol) : null;
    performClone(sourceUrl, currentPath, sourceLabel, upstreamUrl);
  }

  async function performClone(url: string, path: string, label: string, upstreamUrl: string | null) {
    busy = true;
    stopping = false;
    cloneError = "";
    cloneNotice = "";
    cloneErrorTarget = label;
    progress = { phase: "Starting", percent: null, line: `Cloning ${label} into ${path}` };
    try {
      const nextState = await cloneRepository(
        {
          url,
          path,
          branch: branch.trim() || null,
          depth: shallow ? 1 : null,
          recurseSubmodules: submodules,
          upstreamUrl,
        },
        (update) => (progress = update),
      );
      rememberCloneParent(parentDir);
      await onCloned(nextState);
    } catch (err) {
      if (stopping) cloneNotice = "Clone stopped. Nothing was left behind.";
      else cloneError = String(err).replace(/^Error: /, "");
      // The folder may have been created and removed; re-check it.
      scheduleTargetCheck(currentPath);
    } finally {
      busy = false;
      stopping = false;
      progress = null;
    }
  }

  async function stopClone() {
    if (!busy || stopping) return;
    stopping = true;
    try {
      await cancelClone();
    } catch {
      stopping = false;
    }
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
      const el = event.target as HTMLElement | null;
      if (el && el.tagName === "INPUT" && (el as HTMLInputElement).type === "text") {
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

  $: githubReady = !statusLoading && Boolean(status?.installed && status?.authenticated);
  $: branchPlaceholder = remoteBranches?.defaultBranch ?? selectedRepo?.defaultBranch ?? null;
  $: optionsSummary = [branch.trim() && `branch ${branch.trim()}`, shallow && "shallow", submodules && "submodules"]
    .filter(Boolean)
    .join(" · ");
</script>

<svelte:window on:keydown={onKey} />

<div
  class="modal-backdrop"
  role="presentation"
  on:click={(event) => event.target === event.currentTarget && !busy && onClose()}
>
  <div
    class="modal panel size-m clone-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="clone-title"
    use:trapFocus={{ initial: "#repo-picker" }}
  >
    <div class="panel-title">
      <h1 id="clone-title">Clone Repository</h1>
      <button type="button" title="Close" aria-label="Close" on:click={onClose} disabled={busy}>×</button>
    </div>
    <div class="modal-body">
      <div class="segmented clone-tabs" role="tablist" aria-label="Clone source">
        <button
          type="button"
          role="tab"
          id="tab-github"
          aria-selected={activeTab === "github"}
          aria-controls="clone-form"
          class:active={activeTab === "github"}
          disabled={busy}
          on:click={() => (activeTab = "github")}
        >
          GitHub
        </button>
        <button
          type="button"
          role="tab"
          id="tab-url"
          aria-selected={activeTab === "url"}
          aria-controls="clone-form"
          class:active={activeTab === "url"}
          disabled={busy}
          on:click={() => (activeTab = "url")}
        >
          URL
        </button>
      </div>

      <div id="clone-form" class="clone-form" role="tabpanel" class:busy-lock={busy}>
        <label for="clone-parent">Where to clone to</label>
        <div class="row">
          <input id="clone-parent" type="text" bind:value={parentDir} placeholder="~/dev" spellcheck="false" />
          <button type="button" class="btn" on:click={chooseFolder}>Browse…</button>
        </div>

        {#if activeTab === "github"}
          <span class="label">Repository to clone</span>
          {#if status && !status.installed}
            <div class="note">
              GitHub CLI not found. Install it with <code>brew install gh</code>, then
              <button type="button" class="link" on:click={retry}>retry</button>, or use the URL tab.
            </div>
          {:else if status && !status.authenticated}
            <div class="note">
              GitHub CLI isn't signed in. Run <code>gh auth login</code>
              (<button type="button" class="link" on:click={handleOpenTerminal}>open Terminal</button>), then
              <button type="button" class="link" on:click={retry}>retry</button>.
            </div>
          {:else}
            <RepoPicker
              {repos}
              self={status?.login ?? null}
              loading={reposLoading || (statusLoading && !repos.length)}
              refreshing={reposRefreshing}
              error={reposError}
              selected={selectedRepo}
              {onDisk}
              {showAvatars}
              disabled={busy}
              onSelect={selectRepo}
              onRefresh={loadRepos}
              onPasteRef={useInUrlTab}
            />
          {/if}
        {:else}
          <label for="clone-url">Repository URL</label>
          <input
            id="clone-url"
            type="text"
            placeholder="owner/repo, https://…, or git@host:owner/repo.git"
            autocomplete="off"
            spellcheck="false"
            bind:value={urlValue}
          />
        {/if}

        <label for="clone-name">Folder name</label>
        <input
          id="clone-name"
          type="text"
          bind:value={folderName}
          placeholder={activeTab === "github" ? "set by the repository" : "set by the URL"}
          spellcheck="false"
          autocomplete="off"
        />

        <span class="label" aria-hidden="true"></span>
        <p class="hint" class:ok={targetIsSameRepo} class:warn={targetBlocked && !targetIsSameRepo} role="status">
          {#if !currentPath || !sourceUrl}
            Full path: <code>{currentPath || joinPath(parentDir, "…")}</code>
          {:else if targetIsSameRepo}
            Already cloned at <code>{targetFor}</code>, so this opens it instead.
          {:else if targetBlocked && target?.isRepo}
            <code>{targetFor}</code> holds a different repository. Pick another folder name.
          {:else if targetBlocked}
            <code>{targetFor}</code> already exists and isn't empty. Pick another folder name.
          {:else}
            Full path: <code>{currentPath}</code>
            {#if activeTab === "url" && resolvedUrl !== urlValue.trim()}
              · from <code>{resolvedUrl}</code>
            {/if}
          {/if}
        </p>
      </div>

      <details class="clone-options" bind:open={optionsOpen} class:busy-lock={busy}>
        <summary>
          Options
          {#if optionsSummary}<span class="summary-note">{optionsSummary}</span>{/if}
          {#if forkParent && addUpstream && !optionsOpen}<span class="summary-note">+ upstream remote</span>{/if}
        </summary>
        <div class="options-grid">
          <label for="clone-branch">Branch</label>
          {#if remoteBranches && remoteBranches.branches.length}
            <select id="clone-branch" bind:value={branch}>
              <option value="">Default{remoteBranches.defaultBranch ? ` (${remoteBranches.defaultBranch})` : ""}</option>
              {#each remoteBranches.branches.filter((name) => name !== remoteBranches?.defaultBranch) as name (name)}
                <option value={name}>{name}</option>
              {/each}
            </select>
          {:else}
            <input
              id="clone-branch"
              type="text"
              bind:value={branch}
              placeholder={branchesLoading
                ? "Loading branches…"
                : branchPlaceholder
                  ? `${branchPlaceholder} (default)`
                  : "default branch"}
              title={branchesError || undefined}
              autocomplete="off"
              spellcheck="false"
            />
          {/if}
          <span class="label" aria-hidden="true"></span>
          <div class="checks">
            <label class="checkbox"><input type="checkbox" bind:checked={shallow} /> Shallow (latest commit only)</label>
            <label class="checkbox"><input type="checkbox" bind:checked={submodules} /> Include submodules</label>
            {#if forkParent}
              <label class="checkbox">
                <input type="checkbox" bind:checked={addUpstream} /> Add <code>upstream</code> remote for {forkParent}
              </label>
            {/if}
          </div>
        </div>
      </details>

      {#if progress}
        <div class="clone-progress" role="status" aria-live="polite">
          <div class="clone-progress-head">
            <span>{stopping ? "Stopping…" : progress.phase}</span>
            {#if progress.percent != null && !stopping}<span>{progress.percent}%</span>{/if}
          </div>
          <div
            class="clone-meter"
            class:indeterminate={progress.percent == null || stopping}
            role="progressbar"
            aria-label="Clone progress"
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={progress.percent ?? undefined}
          >
            {#key progress.phase}
              <div style={`width: ${progress.percent ?? 0}%`}></div>
            {/key}
          </div>
          <p class="clone-progress-line">{progress.line}</p>
        </div>
      {/if}

      {#if cloneNotice}
        <p class="hint" role="status">{cloneNotice}</p>
      {/if}
      {#if cloneError}
        <p class="panel-error" role="alert">Could not clone {cloneErrorTarget}&#10;{cloneError}</p>
      {/if}
    </div>
    <div class="modal-footer">
      {#if activeTab === "github" && githubReady}
        <span class="footer-tertiary footer-note">
          {status?.login} · via {status?.protocol === "ssh" ? "SSH" : "HTTPS"}
        </span>
      {/if}
      {#if busy}
        <button type="button" class="btn danger" on:click={stopClone} disabled={stopping}>
          {stopping ? "Stopping…" : "Stop"}
        </button>
      {:else}
        <button type="button" class="btn" on:click={onClose}>Cancel</button>
      {/if}
      <button type="button" class="btn btn-primary" on:click={primaryAction} disabled={!primaryEnabled}>
        {#if busy}
          <span class="btn-spinner" aria-hidden="true"></span>
        {:else if !targetIsSameRepo}
          <svg viewBox="0 0 15 15" aria-hidden="true"><path d="M7.5 2v8M4 7l3.5 3.5L11 7M2.5 13h10" /></svg>
        {/if}
        {primaryLabel}
      </button>
    </div>
  </div>
</div>

<style>
  .clone-dialog .modal-body {
    gap: 12px;
  }

  .clone-tabs {
    /* `.modal-body` is `display: grid` with implicit `auto` rows and
       `.segmented` sets `overflow: hidden`, which zeroes a grid item's
       automatic minimum size and collapses this row. Pin a real height. */
    flex-shrink: 0;
    justify-self: start;
    min-height: 30px;
  }

  .clone-form,
  .options-grid {
    display: grid;
    grid-template-columns: 132px minmax(0, 1fr);
    align-items: center;
    gap: 10px 12px;
  }

  .clone-form > label,
  .clone-form > .label,
  .options-grid > label,
  .options-grid > .label {
    color: #aab2bd;
    font-size: 12px;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .row input {
    flex: 1;
    min-width: 0;
  }

  /* ---- fields: inputs and selects match the buttons' height and corners ---- */

  .clone-dialog :global(input[type="text"]),
  .clone-dialog :global(select) {
    height: 32px;
    border-color: #3d4048;
    border-radius: 6px;
    transition:
      border-color 120ms ease,
      box-shadow 120ms ease;
  }

  .clone-dialog :global(input[type="text"]:hover),
  .clone-dialog :global(select:hover) {
    border-color: #555a63;
  }

  .clone-dialog :global(input[type="text"]:focus),
  .clone-dialog :global(select:focus) {
    border-color: var(--teal);
    box-shadow: 0 0 0 3px rgb(20 160 191 / 0.18);
  }

  /* ---- buttons: the look comes from the app-wide button system in
     styles.css; only the dialog's sizing lives here ---- */

  .clone-dialog button {
    font-family: inherit;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .btn {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-width: 88px;
    height: 32px;
    padding: 0 14px;
    font-size: 12.5px;
  }

  .btn-primary {
    min-width: 112px;
  }

  .btn-primary svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 1.7;
  }

  .btn-spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgb(255 255 255 / 0.35);
    border-top-color: #fff;
    border-radius: 50%;
    animation: btn-spin 0.8s linear infinite;
  }

  @keyframes btn-spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* Plain toggles; the look comes from `.segmented` in styles.css. */
  .clone-tabs button {
    height: 28px;
    width: auto;
    padding: 0 12px;
    font-size: 12.5px;
  }

  .note {
    margin: 0;
    color: #aab2bd;
    font-size: 12px;
    line-height: 1.5;
  }

  .link {
    min-height: 0;
    padding: 0;
    border: 0;
    color: var(--teal);
    text-decoration: underline;
  }

  .link:hover:not(:disabled) {
    border: 0;
    background: none;
    color: #5fd0e8;
  }

  .hint {
    margin: -4px 0 0;
    overflow: hidden;
    color: #8b93a0;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint.ok {
    color: #bfeee2;
  }

  .hint.warn {
    color: #f1cf86;
  }

  .hint code,
  .note code {
    font-size: 10.5px;
  }

  .clone-options {
    font-size: 12px;
  }

  .clone-options summary {
    width: fit-content;
    height: 24px;
    color: #aab2bd;
  }

  .clone-options summary::before {
    content: "";
    width: 5px;
    height: 5px;
    margin: 0 9px 0 3px;
    border-right: 1.5px solid currentColor;
    border-bottom: 1.5px solid currentColor;
    transform: rotate(-45deg);
    transition: transform 0.12s ease;
  }

  .clone-options[open] summary::before {
    transform: rotate(45deg);
  }

  .summary-note {
    margin-left: 8px;
    color: #8b93a0;
    font-size: 11px;
  }

  .options-grid {
    padding-top: 8px;
  }

  .checks {
    display: grid;
  }

  .checks .checkbox {
    height: 24px;
    font-size: 12px;
  }

  .clone-progress {
    display: grid;
    gap: 5px;
    padding: 10px;
    border: 1px solid var(--edge);
    border-radius: 8px;
  }

  .clone-progress-head {
    display: flex;
    justify-content: space-between;
    color: #e8ecf1;
    font-size: 12px;
    font-weight: 600;
  }

  .clone-meter {
    position: relative;
    height: 4px;
    overflow: hidden;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.08);
  }

  .clone-meter > div {
    height: 100%;
    border-radius: 2px;
    background: var(--teal);
    transition: width 0.15s linear;
  }

  .clone-meter.indeterminate > div {
    position: absolute;
    width: 35% !important;
    animation: clone-slide 1.1s ease-in-out infinite;
  }

  @keyframes clone-slide {
    from {
      left: -35%;
    }
    to {
      left: 100%;
    }
  }

  .clone-progress-line {
    margin: 0;
    overflow: hidden;
    color: #8b93a0;
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 10.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .footer-note {
    align-self: center;
    color: #8b93a0;
    font-size: 11px;
  }

  .clone-dialog :global(.panel-title button) {
    border-radius: 6px;
    color: #aab2bd;
    font-size: 16px;
  }

  .clone-dialog :global(.panel-title button:hover:not(:disabled)) {
    color: #fff;
  }

  .busy-lock {
    opacity: 0.55;
    pointer-events: none;
  }

  @media (max-width: 560px) {
    .clone-form,
    .options-grid {
      grid-template-columns: minmax(0, 1fr);
      gap: 4px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .btn-spinner,
    .clone-meter.indeterminate > div {
      animation-duration: 2.4s;
    }
  }
</style>
