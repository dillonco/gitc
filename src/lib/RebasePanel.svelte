<script lang="ts">
  // Rebase UX: "plain" mode is the existing single-shot `git rebase <base>`
  // (still shown as a commit list first, per REVIEW-UX F4-4, so every
  // rebase entry point shows what will be replayed before doing it).
  // "interactive" mode is the narrow first cut: pick / reword / squash /
  // fixup / drop, plus reorder. No edit, no break, no user exec, no
  // autosquash, no --onto with a different upstream. See PLAN.md section 5
  // and REVIEW-UX.md section 6 for the full spec this implements.
  import { onMount } from "svelte";
  import { getRebasePlan, runGitAction, runInteractiveRebase } from "./git";
  import { trapFocus } from "./modal";
  import type { Branch, CommitNode, GitResult, RebasePlan, RebaseStep } from "./types";

  export let currentBranch: string;
  export let branches: Branch[] = [];
  export let mode: "interactive" | "plain" = "interactive";
  export let initialBase: string | null = null;
  export let confirmRisky: boolean;
  export let onClose: () => void;
  export let onDone: (result: GitResult, label: string) => Promise<void> | void;

  type RowAction = "pick" | "reword" | "squash" | "fixup" | "drop";
  type Row = {
    hash: string;
    shortHash: string;
    originalSubject: string;
    originalBodySummary: string;
    action: RowAction;
    message: string;
  };

  let loading = true;
  let busy = false;
  let error = "";
  let plan: RebasePlan | null = null;
  let base = initialBase ?? "";
  let rows: Row[] = [];
  let activeRowIndex = 0;

  let confirmingStart = false;
  let confirmingUndo = false;
  let runResult: GitResult | null = null;
  let resultLabel = "";

  $: branchNames = branches.map((branch) => branch.name);
  $: title = mode === "interactive" ? "Interactive Rebase" : "Rebase";
  $: firstLiveIndex = rows.findIndex((row) => row.action !== "drop");
  $: planIssue = (() => {
    if (firstLiveIndex !== -1 && (rows[firstLiveIndex].action === "squash" || rows[firstLiveIndex].action === "fixup")) {
      return "first commit can't be squashed";
    }
    if (rows.some((row) => row.action === "reword" && !row.message.trim())) {
      return "reword needs a message";
    }
    return "";
  })();
  $: primaryLabel =
    mode === "interactive"
      ? `Rewrite ${rows.length} Commit${rows.length === 1 ? "" : "s"}`
      : `Rebase onto ${base || "…"}`;
  $: busyLabel = mode === "interactive" ? "Rewriting…" : "Rebasing…";
  $: canStart =
    !busy &&
    !loading &&
    Boolean(plan) &&
    plan?.inProgress === false &&
    plan?.clean === true &&
    (plan?.commits.length ?? 0) > 0 &&
    !planIssue;

  onMount(() => {
    void loadPlan();
  });

  function buildRows(commits: CommitNode[]): Row[] {
    return commits.map((commit) => ({
      hash: commit.hash,
      shortHash: commit.shortHash,
      originalSubject: commit.subject,
      originalBodySummary: commit.bodySummary,
      action: "pick",
      message: commit.bodySummary ? `${commit.subject}\n\n${commit.bodySummary}` : commit.subject,
    }));
  }

  async function loadPlan() {
    loading = true;
    error = "";
    try {
      const next = await getRebasePlan(base.trim() ? base : null);
      plan = next;
      base = next.base;
      rows = buildRows(next.commits);
      activeRowIndex = 0;
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  }

  async function stashChanges() {
    busy = true;
    error = "";
    try {
      const result = await runGitAction({ kind: "stashCreate", message: "gitc: before rebase" });
      if (!result.ok) {
        error = result.stderr || result.stdout || "Could not stash changes";
      } else {
        await loadPlan();
      }
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function setAction(index: number, action: RowAction) {
    if ((action === "squash" || action === "fixup") && index === firstLiveIndex) return;
    rows = rows.map((row, i) => (i === index ? { ...row, action } : row));
  }

  function moveRow(index: number, direction: -1 | 1) {
    const target = index + direction;
    if (target < 0 || target >= rows.length) return;
    const next = rows.slice();
    [next[index], next[target]] = [next[target], next[index]];
    rows = next;
    activeRowIndex = target;
    queueMicrotask(() => focusRow(target));
  }

  function focusRow(index: number) {
    const el = document.querySelectorAll<HTMLElement>(".rebase-row")[index];
    el?.focus();
  }

  function focusRowSelect(index: number) {
    const el = document.querySelectorAll<HTMLElement>(".rebase-row")[index];
    el?.querySelector("select")?.focus();
  }

  function onRowFocus(index: number) {
    activeRowIndex = index;
  }

  function onRowKeydown(event: KeyboardEvent, index: number) {
    if (event.altKey || event.metaKey || event.ctrlKey) return;
    if (event.key === "ArrowUp" && index > 0) {
      event.preventDefault();
      activeRowIndex = index - 1;
      focusRow(activeRowIndex);
    } else if (event.key === "ArrowDown" && index < rows.length - 1) {
      event.preventDefault();
      activeRowIndex = index + 1;
      focusRow(activeRowIndex);
    } else if (event.key === "Enter") {
      event.preventDefault();
      focusRowSelect(index);
    }
  }

  function stepsPayload(): RebaseStep[] {
    return rows.map((row) => ({
      action: row.action,
      hash: row.hash,
      message:
        row.action === "reword" || (row.action === "squash" && row.message.trim())
          ? row.message
          : null,
    }));
  }

  function requestStart() {
    if (!canStart || busy) return;
    if (mode === "interactive" || confirmRisky) {
      confirmingStart = true;
    } else {
      void start();
    }
  }

  async function start() {
    if (!plan) return;
    busy = true;
    error = "";
    const label = primaryLabel;
    try {
      const result =
        mode === "interactive"
          ? await runInteractiveRebase(base, stepsPayload())
          : await runGitAction({ kind: "rebase", target: base });

      if (result.ok) {
        runResult = result;
        resultLabel = `Rebased ${plan.commits.length} commit${plan.commits.length === 1 ? "" : "s"} onto ${base}`;
      } else {
        // A real conflict leaves `state.rebasing` true; the existing
        // merge/rebase banner + continue/abort own that path, and this
        // panel must not linger over a conflicted tree. Anything else
        // (a hard failure that didn't actually start a rebase) stays
        // in-panel as an inline error so the user can fix and retry.
        const check = await getRebasePlan(base).catch(() => null);
        if (check?.inProgress) {
          await onDone(result, label);
          return;
        }
        error = result.stderr || result.stdout || `${label} failed`;
      }
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
      confirmingStart = false;
    }
  }

  async function undo() {
    if (!runResult) return;
    busy = true;
    try {
      const result = await runGitAction({ kind: "reset", target: "ORIG_HEAD", mode: "hard" });
      await onDone(result, `Restored ${currentBranch}`);
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
      confirmingUndo = false;
    }
  }

  async function done() {
    if (!runResult) return;
    await onDone(runResult, resultLabel);
  }

  function primaryAction() {
    if (runResult) {
      void done();
    } else if (confirmingStart) {
      void start();
    } else if (confirmingUndo) {
      void undo();
    } else {
      requestStart();
    }
  }

  function onKey(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    if (event.key === "Escape") {
      if (busy) {
        event.preventDefault();
        return;
      }
      if (confirmingStart) {
        confirmingStart = false;
        event.preventDefault();
        return;
      }
      if (confirmingUndo) {
        confirmingUndo = false;
        event.preventDefault();
        return;
      }
      const active = document.activeElement;
      if (active instanceof HTMLTextAreaElement) {
        active.blur();
        event.preventDefault();
        return;
      }
      event.preventDefault();
      onClose();
      return;
    }

    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      primaryAction();
      return;
    }
    if (event.key === "Enter" && event.target instanceof HTMLInputElement) {
      event.preventDefault();
      primaryAction();
      return;
    }

    if (mode === "interactive" && rows.length && !runResult) {
      const active = document.activeElement;
      const typing =
        active instanceof HTMLTextAreaElement || active instanceof HTMLInputElement || active instanceof HTMLSelectElement;
      if (!typing) {
        if (event.altKey && event.key === "ArrowUp") {
          event.preventDefault();
          moveRow(activeRowIndex, -1);
          return;
        }
        if (event.altKey && event.key === "ArrowDown") {
          event.preventDefault();
          moveRow(activeRowIndex, 1);
          return;
        }
        const letterActions: Record<string, RowAction> = { p: "pick", r: "reword", s: "squash", f: "fixup", d: "drop" };
        const letter = letterActions[event.key.toLowerCase()];
        if (letter && !event.metaKey && !event.ctrlKey) {
          event.preventDefault();
          setAction(activeRowIndex, letter);
        }
      }
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
    class="modal panel size-l"
    role="dialog"
    aria-modal="true"
    aria-labelledby="rebase-title"
    use:trapFocus={{ initial: "#rebase-base" }}
  >
    <div class="panel-title">
      <h1 id="rebase-title">{title}</h1>
      {#if mode === "interactive"}
        <span
          class="chip warn"
          title="Runs git rebase -i with a generated todo; pick/reword/squash/fixup/drop and reorder only."
        >experimental</span>
      {/if}
      <button type="button" title="Close" aria-label="Close" on:click={onClose} disabled={busy}>×</button>
    </div>
    {#if busy}<div class="busy-bar" aria-hidden="true"></div>{/if}

    <div class="modal-body">
      {#if error}<p class="panel-error" role="alert">{error}</p>{/if}

      {#if !runResult}
        <div class="field">
          <label for="rebase-base">rebase onto</label>
          <select id="rebase-base" bind:value={base} on:change={loadPlan} disabled={busy || loading}>
            {#each branchNames as name}
              <option value={name}>{name}</option>
            {/each}
            {#if base && !branchNames.includes(base)}
              <option value={base}>{base.slice(0, 8)}</option>
            {/if}
          </select>
        </div>

        <div aria-live="polite">
          {#if loading}
            <p class="empty">Loading commits since {base || "…"}…</p>
          {:else if plan?.inProgress}
            <div class="preflight-banner" role="status">
              A rebase or merge is already in progress — finish or abort it first.
            </div>
          {:else if plan && !plan.clean}
            <div class="preflight-banner" role="status">
              Commit or stash your changes before rebasing.
              <button type="button" on:click={stashChanges} disabled={busy}>Stash Changes</button>
            </div>
          {:else if plan && plan.commits.length === 0}
            <p class="empty">{currentBranch} has no commits that aren't already on {base}.</p>
          {/if}
        </div>

        {#if plan && plan.commits.length > 0 && !plan.inProgress && plan.clean}
          <p class="rebase-hint">Oldest first — applied top to bottom onto {base}</p>
          <div class="rebase-list" role="list">
            {#each rows as row, index (row.hash)}
              <!-- svelte-ignore a11y_no_noninteractive_tabindex -- roving tabindex over a role="list"/"listitem" composite (REVIEW-UX.md 6/2.11) is a standard ARIA pattern the linter doesn't recognize. -->
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <div
                class="rebase-row"
                class:is-drop={row.action === "drop"}
                class:is-fold={row.action === "squash" || row.action === "fixup"}
                role="listitem"
                tabindex={index === activeRowIndex ? 0 : -1}
                aria-label={`Commit ${index + 1} of ${rows.length}: ${row.action}, ${row.originalSubject}`}
                on:focus={() => onRowFocus(index)}
                on:keydown={(event) => onRowKeydown(event, index)}
              >
                <span class="rebase-index">{index + 1}</span>
                {#if mode === "interactive"}
                  <select
                    aria-label={`Action for ${row.shortHash}`}
                    value={row.action}
                    on:change={(event) => setAction(index, (event.currentTarget as HTMLSelectElement).value as RowAction)}
                  >
                    <option value="pick">pick</option>
                    <option value="reword">reword</option>
                    <option value="squash" disabled={index === firstLiveIndex} title="squash: fold into the commit above and edit the message">squash</option>
                    <option value="fixup" disabled={index === firstLiveIndex} title="fixup: fold into the commit above, discard this message">fixup</option>
                    <option value="drop">drop</option>
                  </select>
                {/if}
                <span class="rebase-hash">{row.shortHash}</span>
                <span class="rebase-subject" title={row.originalSubject}>{row.originalSubject}</span>
                {#if mode === "interactive"}
                  <button
                    type="button"
                    class="row-move"
                    aria-label="Move up"
                    title="Move up (⌥↑)"
                    disabled={index === 0}
                    on:click={() => moveRow(index, -1)}
                  >↑</button>
                  <button
                    type="button"
                    class="row-move"
                    aria-label="Move down"
                    title="Move down (⌥↓)"
                    disabled={index === rows.length - 1}
                    on:click={() => moveRow(index, 1)}
                  >↓</button>
                {/if}
              </div>
              {#if mode === "interactive" && (row.action === "reword" || row.action === "squash")}
                <textarea
                  class="rebase-message"
                  aria-label={`Message for ${row.shortHash}`}
                  placeholder={row.action === "squash" ? "Combined message (optional)" : undefined}
                  bind:value={row.message}
                ></textarea>
              {/if}
            {/each}
          </div>
          {#if planIssue}
            <p class="rebase-warn">{planIssue}</p>
          {/if}
        {/if}
      {:else}
        <div class="rebase-result">
          <p>{resultLabel}.</p>
          {#if plan?.upstream}
            <p class="hint">
              {plan.currentBranch} now diverges from {plan.upstream} — use Force Push (with lease) to update it.
            </p>
          {/if}
        </div>
      {/if}
    </div>

    <div class="modal-footer">
      {#if runResult}
        {#if confirmingUndo}
          <span class="footer-tertiary">Restore {currentBranch} to where it was before this rebase?</span>
          <button type="button" on:click={() => (confirmingUndo = false)} disabled={busy}>Cancel</button>
          <button type="button" class="danger-fill" on:click={undo} disabled={busy}>{busy ? "Restoring…" : "Undo"}</button>
        {:else}
          <button
            type="button"
            class="footer-tertiary"
            on:click={() => (confirmingUndo = true)}
            disabled={busy}
          >Undo</button>
          <button type="button" class="stage-file" on:click={done} disabled={busy}>Done</button>
        {/if}
      {:else if confirmingStart}
        <span class="footer-tertiary">
          {mode === "interactive"
            ? `Rewrite ${rows.length} commits on ${currentBranch}? History changes; the previous state stays available as ORIG_HEAD until your next rebase, merge or reset.`
            : `Rebase ${currentBranch} onto ${base}? Commits are replayed on top of ${base}.`}
        </span>
        <button type="button" on:click={() => (confirmingStart = false)} disabled={busy}>Cancel</button>
        <button
          type="button"
          class={mode === "interactive" ? "danger-fill" : "stage-file"}
          on:click={start}
          disabled={busy}
        >{busy ? busyLabel : primaryLabel}</button>
      {:else}
        <button type="button" on:click={onClose} disabled={busy}>Cancel</button>
        <button
          type="button"
          class={mode === "interactive" ? "danger-fill" : "stage-file"}
          on:click={requestStart}
          disabled={!canStart}
        >{busy ? busyLabel : primaryLabel}</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .preflight-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid #8a6b31;
    border-radius: 10px;
    background: #2d2517;
    color: #ffe9bd;
    padding: 8px 10px;
    font-size: 13px;
  }

  .rebase-hint {
    margin: 0;
    color: var(--ink-dim);
    font-size: 11px;
  }

  .rebase-list {
    display: grid;
    gap: 2px;
  }

  .rebase-row {
    display: grid;
    grid-template-columns: 22px 96px 64px minmax(0, 1fr) 28px 28px;
    align-items: center;
    gap: 8px;
    min-height: 36px;
    padding: 0 6px;
    border-radius: 6px;
  }

  .rebase-row:hover,
  .rebase-row:focus-visible {
    background: rgba(255, 255, 255, 0.05);
  }

  .rebase-row.is-drop {
    opacity: 0.5;
  }

  .rebase-row.is-drop .rebase-subject {
    text-decoration: line-through;
  }

  .rebase-row.is-fold {
    margin-left: 20px;
    padding-left: 8px;
    border-left: 2px solid var(--edge-hi);
  }

  .rebase-index {
    color: var(--ink-dim);
    font-family: "SFMono-Regular", Consolas, monospace;
    font-size: 12px;
  }

  .rebase-hash {
    overflow: hidden;
    color: #aab2bd;
    font-family: "SFMono-Regular", Consolas, monospace;
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rebase-subject {
    overflow: hidden;
    color: #e8ecf1;
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-move {
    opacity: 0;
    width: 28px;
    height: 28px;
    padding: 0;
  }

  .rebase-row:hover .row-move,
  .rebase-row:focus-within .row-move,
  .row-move:focus-visible {
    opacity: 1;
  }

  .rebase-message {
    box-sizing: border-box;
    width: 100%;
    min-height: 72px;
    margin: 2px 0 6px;
  }

  .rebase-warn {
    margin: 0;
    color: #f1cf86;
    font-size: 11px;
  }

  .rebase-result p {
    margin: 0 0 6px;
    color: #e8ecf1;
    font-size: 13px;
  }

  .rebase-result .hint {
    color: #aab2bd;
    font-size: 12px;
  }
</style>
