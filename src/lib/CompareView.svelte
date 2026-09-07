<script lang="ts">
  import { onMount, tick } from "svelte";
  import { getRefCompare, getRefFileDiff } from "./git";
  import { parseDiffRows } from "./diffRows";
  import DiffTable from "./DiffTable.svelte";
  import type { Branch, CommitFileChange, FileDiff, RefCompare } from "./types";

  export let base: string | null;
  export let head: string;
  export let branches: Branch[] = [];
  export let remoteBranches: string[] = [];
  export let tags: string[] = [];
  export let onClose: () => void;

  // Compare is opened either from the top-bar button (base = null, meaning
  // "use the default base") or by shift-clicking two commits in the graph
  // (base already resolved). Only the former should steal focus into the
  // base input; the latter should land on the file list once it loads.
  const openedViaButton = base === null;
  let initialFocusDone = false;

  let baseInput = base ?? "";
  let headInput = head;
  let threeDot = true;
  let splitDiff = false;
  let leftCollapsed = false;

  let compare: RefCompare | null = null;
  let busy = false;
  let error = "";

  let selectedFile: CommitFileChange | null = null;
  let fileDiff: FileDiff | null = null;
  let fileBusy = false;
  let fileError = "";

  let baseInputEl: HTMLInputElement | null = null;
  let fileListEl: HTMLDivElement | null = null;

  let lastKey = "";
  $: {
    const key = `${base ?? ""}::${head}`;
    if (key !== lastKey) {
      lastKey = key;
      baseInput = base ?? "";
      headInput = head;
      selectedFile = null;
      fileDiff = null;
      void runCompare();
    }
  }

  $: diffRows = fileDiff?.diff ? parseDiffRows(fileDiff.diff) : [];
  $: addCount = diffRows.filter((row) => row.kind === "add").length;
  $: delCount = diffRows.filter((row) => row.kind === "del").length;
  $: isEmptyCompare = compare != null && compare.ahead === 0;
  $: summaryText = compare
    ? `${compare.ahead || compare.behind ? `↑${compare.ahead} ↓${compare.behind}` : "—"} · ${compare.commits.length} ${compare.commits.length === 1 ? "commit" : "commits"} · ${compare.files.length} ${compare.files.length === 1 ? "file" : "files"}`
    : "";

  onMount(() => {
    if (openedViaButton) baseInputEl?.focus();
  });

  async function runCompare() {
    busy = true;
    error = "";
    try {
      const result = await getRefCompare(baseInput.trim() || null, headInput.trim(), threeDot);
      compare = result;
      if (selectedFile && !result.files.some((entry) => entry.path === selectedFile?.path)) {
        selectedFile = null;
        fileDiff = null;
      } else if (selectedFile) {
        await loadFileDiff(selectedFile);
      }
    } catch (err) {
      // Keep the last good result visible; only the error banner reports
      // the problem (e.g. an unknown ref) so the view never blanks out.
      error = String(err);
    } finally {
      busy = false;
      if (!initialFocusDone) {
        initialFocusDone = true;
        await tick();
        if (!openedViaButton) fileListEl?.querySelector<HTMLButtonElement>("button")?.focus();
      }
    }
  }

  function setThreeDot(value: boolean) {
    if (threeDot === value) return;
    threeDot = value;
    void runCompare();
  }

  function swapRefs() {
    const effectiveBase = baseInput.trim() || compare?.base || "";
    if (!effectiveBase || !headInput.trim()) return;
    const nextHead = effectiveBase;
    const nextBase = headInput.trim();
    baseInput = nextBase;
    headInput = nextHead;
    void runCompare();
  }

  function onRefKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      void runCompare();
    }
  }

  async function loadFileDiff(change: CommitFileChange) {
    fileBusy = true;
    fileError = "";
    try {
      fileDiff = await getRefFileDiff(baseInput.trim() || null, headInput.trim(), change.path, threeDot);
    } catch (err) {
      fileDiff = null;
      fileError = String(err);
    } finally {
      fileBusy = false;
    }
  }

  function selectFile(change: CommitFileChange) {
    selectedFile = change;
    void loadFileDiff(change);
  }

  function statusLabel(status: string) {
    const map: Record<string, string> = { A: "added", M: "modified", D: "deleted", R: "renamed", C: "copied" };
    return map[status] ?? status;
  }

  function onFileRowKeydown(event: KeyboardEvent) {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    if (!fileListEl) return;
    const items = Array.from(fileListEl.querySelectorAll<HTMLButtonElement>("button.commit-file-row"));
    const currentIndex = items.findIndex((el) => el === event.currentTarget);
    if (currentIndex === -1) return;
    event.preventDefault();
    const nextIndex = event.key === "ArrowDown" ? Math.min(items.length - 1, currentIndex + 1) : Math.max(0, currentIndex - 1);
    items[nextIndex]?.focus();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window on:keydown={onWindowKeydown} />

<div class="compare-view" style={`--left-w:${leftCollapsed ? "28px" : "280px"}`}>
  <header class="compare-header">
    <div class="compare-refs">
      <input
        id="compare-base"
        list="compare-refs-list"
        aria-label="Base ref"
        placeholder={compare?.base ?? "default base"}
        bind:this={baseInputEl}
        bind:value={baseInput}
        on:keydown={onRefKeydown}
      />
      <button class="swap-btn" title="Swap base and head" aria-label="Swap base and head" on:click={swapRefs}>⇄</button>
      <input id="compare-head" list="compare-refs-list" aria-label="Head ref" bind:value={headInput} on:keydown={onRefKeydown} />
    </div>
    <datalist id="compare-refs-list">
      {#each branches as branch (branch.name)}<option value={branch.name}></option>{/each}
      {#each remoteBranches as branch (branch)}<option value={branch}></option>{/each}
      {#each tags as tag (tag)}<option value={tag}></option>{/each}
    </datalist>
    <div class="segmented" role="group" aria-label="Diff mode">
      <button
        class:active={threeDot}
        aria-pressed={threeDot}
        title={`${baseInput || compare?.base || "base"}...${headInput || "head"}`}
        on:click={() => setThreeDot(true)}
      >
        since merge base
      </button>
      <button
        class:active={!threeDot}
        aria-pressed={!threeDot}
        title={`${baseInput || compare?.base || "base"}..${headInput || "head"}`}
        on:click={() => setThreeDot(false)}
      >
        direct
      </button>
    </div>
    <button title="Close compare" aria-label="Close compare" on:click={onClose}>×</button>
  </header>

  <div class="compare-subbar">
    {#if error}
      <span class="compare-summary" aria-live="polite">{compare ? summaryText : "—"}</span>
    {:else if busy && !compare}
      <span class="compare-summary" aria-live="polite">Comparing {baseInput || "default base"}…{headInput}</span>
    {:else}
      <span class="compare-summary" aria-live="polite">
        {summaryText}
        {#if compare?.commitsTruncated}<em title="Only the most recent 1000 commits are shown">· truncated</em>{/if}
      </span>
    {/if}
    <div class="file-diff-actions">
      <button title="Unified" class:active={!splitDiff} aria-pressed={!splitDiff} on:click={() => (splitDiff = false)}>▣</button>
      <button title="Split" class:active={splitDiff} aria-pressed={splitDiff} on:click={() => (splitDiff = true)}>▥</button>
    </div>
  </div>

  {#if error}
    <p class="panel-error compare-error" role="alert">{error}</p>
  {/if}

  <div class="compare-left">
    {#if leftCollapsed}
      <button class="collapse-btn" title="Expand list" aria-label="Expand list" on:click={() => (leftCollapsed = false)}>›</button>
    {:else}
      <button class="collapse-btn" title="Collapse list" aria-label="Collapse list" on:click={() => (leftCollapsed = true)}>‹</button>
      {#if isEmptyCompare}
        <p class="empty compare-empty">No differences — {headInput || compare?.head} is up to date with {compare?.base}.</p>
      {:else}
        <div class="compare-group">
          <div class="compare-group-head">COMMITS ({compare?.commits.length ?? 0})</div>
          {#each compare?.commits ?? [] as commit (commit.hash)}
            <div class="compare-commit-row" title={`${commit.subject} — ${commit.shortHash}`}>
              <strong>{commit.subject}</strong>
              <small>{commit.shortHash} · {commit.author} · {commit.relativeDate}</small>
            </div>
          {:else}
            <p class="empty">{busy ? "Loading commits…" : "No commits"}</p>
          {/each}
        </div>
        <div class="compare-group compare-files" bind:this={fileListEl}>
          <div class="compare-group-head">FILES ({compare?.files.length ?? 0})</div>
          {#each compare?.files ?? [] as change (change.path)}
            <button
              class="commit-file-row"
              class:active={selectedFile?.path === change.path}
              title={`${statusLabel(change.status)}: ${change.path}`}
              aria-pressed={selectedFile?.path === change.path}
              on:click={() => selectFile(change)}
              on:keydown={onFileRowKeydown}
            >
              <span class={`status-${change.status.toLowerCase()}`}>{change.status}</span>
              <strong>{change.path}</strong>
            </button>
          {:else}
            <p class="empty">{busy ? "Loading files…" : "No differences"}</p>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  <div class="compare-right">
    {#if isEmptyCompare}
      <!-- Blank on purpose: nothing to review when head is up to date with base. -->
    {:else if !selectedFile}
      <p class="empty centered">Select a file to see its diff</p>
    {:else}
      <div class="compare-file-header">
        <span title={selectedFile.path}>{selectedFile.path}</span>
        {#if fileDiff && !fileDiff.binary && (addCount || delCount)}<em>+{addCount} −{delCount}</em>{/if}
      </div>
      {#if fileError}
        <p class="panel-error compare-error">{fileError}</p>
      {:else if fileBusy}
        <p class="empty centered">Loading diff…</p>
      {:else if fileDiff?.binary}
        <p class="diff-empty main-empty">No text diff available.</p>
      {:else if fileDiff?.diff}
        <DiffTable rows={diffRows} split={splitDiff} />
      {:else}
        <p class="diff-empty main-empty">No text diff available.</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .compare-view {
    position: relative;
    display: grid;
    grid-template-rows: 52px 34px minmax(0, 1fr);
    grid-template-columns: var(--left-w, 280px) minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .compare-header {
    grid-column: 1 / -1;
    grid-row: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    border-bottom: 1px solid var(--edge);
    background: rgba(255, 255, 255, 0.07);
    padding: 0 12px 0 28px;
  }

  .compare-refs {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }

  .compare-refs input {
    width: 200px;
    min-width: 0;
  }

  .swap-btn {
    flex: none;
    min-width: 28px;
    padding: 0;
  }

  .compare-subbar {
    grid-column: 1 / -1;
    grid-row: 2;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border-bottom: 1px solid var(--edge);
    background: rgba(255, 255, 255, 0.045);
    padding: 0 12px 0 28px;
    color: #aab2bd;
    font-size: 12px;
  }

  .compare-summary em {
    margin-left: 6px;
    color: #f1cf86;
    font-style: normal;
  }

  .compare-error {
    grid-column: 1 / -1;
    margin: 8px 12px 0;
  }

  .compare-left {
    grid-column: 1;
    grid-row: 3;
    position: relative;
    min-height: 0;
    overflow: auto;
    border-right: 1px solid var(--edge);
    padding-top: 28px;
  }

  .collapse-btn,
  .expand-btn {
    position: absolute;
    top: 2px;
    left: 2px;
    min-width: 24px;
    min-height: 24px;
    padding: 0;
    border-color: transparent;
  }

  .compare-empty {
    padding: 12px;
  }

  .compare-group {
    border-bottom: 1px solid var(--edge);
    padding-bottom: 6px;
  }

  .compare-group-head {
    position: sticky;
    top: 0;
    z-index: 1;
    height: 28px;
    display: flex;
    align-items: center;
    padding: 0 10px;
    background: rgba(26, 29, 36, 0.96);
    color: var(--ink-dim);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .compare-commit-row {
    display: grid;
    gap: 2px;
    min-height: 44px;
    padding: 6px 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }

  .compare-commit-row strong {
    overflow: hidden;
    color: #e8ecf1;
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .compare-commit-row small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }

  .compare-files {
    display: flex;
    flex-direction: column;
  }

  .compare-right {
    grid-column: 2;
    grid-row: 3;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .compare-file-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    height: 28px;
    flex: none;
    border-bottom: 1px solid var(--edge);
    padding: 0 12px;
    color: #aab2bd;
    font-size: 12px;
  }

  .compare-file-header span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .compare-file-header em {
    flex: none;
    color: var(--ink-dim);
    font-style: normal;
    font-family: "SFMono-Regular", Consolas, monospace;
  }

  .compare-right > :global(.diff-table) {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
  }

  .compare-right > .empty.centered,
  .compare-right > .diff-empty {
    flex: 1 1 auto;
  }
</style>
