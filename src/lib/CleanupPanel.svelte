<script lang="ts">
  import { onMount } from "svelte";
  import { getBranchCleanup, runGitAction } from "./git";
  import { trapFocus } from "./modal";
  import type { BranchAudit, BranchCleanupReport, RepositoryState, Worktree } from "./types";

  export let state: RepositoryState;
  export let staleDays: number;
  export let confirmRisky: boolean;
  export let onClose: () => void;
  export let onDone: (summary: string) => Promise<void> | void;

  type SectionKey = "merged" | "squashMerged" | "gone" | "stale" | "active" | "kept";
  type Section = { key: SectionKey; title: string; rows: BranchAudit[] };
  type Phase = "ready" | "confirm" | "running" | "result";

  type ResultLine = {
    kind: "branch" | "worktree";
    id: string;
    ok: boolean;
    detail: string;
    head?: string;
    restored?: boolean;
    canForceDelete?: boolean;
    canForceRemove?: boolean;
    confirmingForceRemove?: boolean;
  };

  let report: BranchCleanupReport | null = null;
  let base = guessBase(state.branches);
  let error = "";
  let busy = false;
  let loaded = false;
  let phase: Phase = "ready";
  let selected = new Set<string>();
  let worktreeSelected = new Set<string>();
  let lastClickedName: string | null = null;
  let results: ResultLine[] = [];

  onMount(() => {
    void loadReport(null);
  });

  function guessBase(branches: RepositoryState["branches"]): string {
    return (
      branches.find((b) => b.name === "main")?.name ??
      branches.find((b) => b.name === "master")?.name ??
      branches.find((b) => b.current)?.name ??
      branches[0]?.name ??
      "main"
    );
  }

  async function loadReport(nextBase: string | null) {
    error = "";
    busy = true;
    try {
      const next = await getBranchCleanup(nextBase, staleDays);
      report = next;
      base = next.base;
      const defaults = defaultSelection(next, state.worktrees);
      selected = defaults.branches;
      worktreeSelected = defaults.worktrees;
      loaded = true;
      phase = "ready";
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function deletableClassification(classification: string): boolean {
    return classification === "merged" || classification === "squashMerged" || classification === "gone";
  }

  function defaultSelection(r: BranchCleanupReport, worktrees: Worktree[]): { branches: Set<string>; worktrees: Set<string> } {
    const branchSel = new Set(r.branches.filter((b) => deletableClassification(b.classification)).map((b) => b.name));
    const worktreeSel = new Set<string>();
    for (const wt of worktrees) {
      if (wt.main || wt.current) continue;
      if (wt.prunable || (wt.branch && branchSel.has(wt.branch))) worktreeSel.add(wt.path);
    }
    return { branches: branchSel, worktrees: worktreeSel };
  }

  $: deletableNames = new Set((report?.branches ?? []).filter((b) => deletableClassification(b.classification)).map((b) => b.name));
  $: worktreeCandidates = (state.worktrees ?? []).filter(
    (w) => !w.main && !w.current && (w.prunable || (w.branch && deletableNames.has(w.branch))),
  );

  $: sections = buildSections(report, base, staleDays);

  function buildSections(r: BranchCleanupReport | null, activeBase: string, days: number): Section[] {
    if (!r) return [];
    const groups: Record<SectionKey, BranchAudit[]> = {
      merged: [],
      squashMerged: [],
      gone: [],
      stale: [],
      active: [],
      kept: [],
    };
    for (const audit of r.branches) {
      if (audit.classification === "current" || audit.classification === "base") groups.kept.push(audit);
      else if (audit.classification === "merged") groups.merged.push(audit);
      else if (audit.classification === "squashMerged") groups.squashMerged.push(audit);
      else if (audit.classification === "gone") groups.gone.push(audit);
      else if (audit.classification === "stale") groups.stale.push(audit);
      else groups.active.push(audit);
    }
    return [
      { key: "merged", title: `SAFE TO DELETE · merged into ${activeBase}`, rows: groups.merged },
      { key: "squashMerged", title: `SQUASH-MERGED · same changes are on ${activeBase}`, rows: groups.squashMerged },
      { key: "gone", title: "UPSTREAM GONE · remote branch was deleted", rows: groups.gone },
      { key: "stale", title: `STALE · no commits in ${days} days`, rows: groups.stale },
      { key: "active", title: "ACTIVE", rows: groups.active },
      { key: "kept", title: "KEPT", rows: groups.kept },
    ];
  }

  $: flatBranches = sections.flatMap((section) => section.rows);
  $: nothingRecommended =
    (sections.find((s) => s.key === "merged")?.rows.length ?? 0) +
      (sections.find((s) => s.key === "squashMerged")?.rows.length ?? 0) +
      (sections.find((s) => s.key === "gone")?.rows.length ?? 0) +
      (sections.find((s) => s.key === "stale")?.rows.length ?? 0) ===
    0;
  $: activeCount = sections.find((s) => s.key === "active")?.rows.length ?? 0;

  $: selectedAudits = flatBranches.filter((audit) => selected.has(audit.name));
  $: selectedWorktreeRows = worktreeCandidates.filter((w) => worktreeSelected.has(w.path));
  $: safeCount = selectedAudits.filter((audit) => audit.merged).length;
  $: forceCount = selectedAudits.length - safeCount;
  $: totalSelected = selectedAudits.length + selectedWorktreeRows.length;
  $: needsWorktreeConfirm = selectedWorktreeRows.some((w) => !w.prunable);
  $: mustConfirm = forceCount > 0 || needsWorktreeConfirm || confirmRisky;

  function branchWord(n: number): string {
    return n === 1 ? "Branch" : "Branches";
  }
  function worktreeWord(n: number): string {
    return n === 1 ? "Worktree" : "Worktrees";
  }

  function primaryLabel(): string {
    if (selectedAudits.length > 0 && selectedWorktreeRows.length > 0) {
      return `Delete ${selectedAudits.length} ${branchWord(selectedAudits.length)} and ${selectedWorktreeRows.length} ${worktreeWord(selectedWorktreeRows.length)}`;
    }
    if (selectedAudits.length > 0) return `Delete ${selectedAudits.length} ${branchWord(selectedAudits.length)}`;
    if (selectedWorktreeRows.length > 0) return `Remove ${selectedWorktreeRows.length} ${worktreeWord(selectedWorktreeRows.length)}`;
    return "Delete Selected";
  }

  function confirmCopy(): string {
    const forced = selectedAudits.filter((audit) => !audit.merged).map((audit) => audit.name);
    let text: string;
    if (forced.length === 0) {
      text = `Delete ${selectedAudits.length} merged ${branchWord(selectedAudits.length).toLowerCase()}? Their commits stay on ${base}; you can restore any of them from the result list.`;
    } else {
      text = `Delete ${selectedAudits.length} ${branchWord(selectedAudits.length).toLowerCase()}? ${forced.length} ${forced.length === 1 ? "is" : "are"} not merged into ${base} and will be force-deleted: ${forced.join(", ")}. Their tips are kept for restore below.`;
    }
    if (selectedWorktreeRows.length === 1) {
      text += ` The worktree at ${selectedWorktreeRows[0].path} will be removed.`;
    } else if (selectedWorktreeRows.length > 1) {
      text += ` ${selectedWorktreeRows.length} worktrees will be removed: ${selectedWorktreeRows.map((w) => w.path).join(", ")}.`;
    }
    return text;
  }

  function upstreamLine(audit: BranchAudit): string {
    if (!audit.upstream) return "no upstream";
    if (audit.ahead || audit.behind) return `${audit.upstream} · ↑${audit.ahead} ↓${audit.behind}`;
    return audit.upstream;
  }

  function aheadBehindText(ahead: number, behind: number): string {
    return ahead === 0 && behind === 0 ? "—" : `↑${ahead} ↓${behind}`;
  }

  function isoDate(unix: number): string {
    try {
      return new Date(unix * 1000).toISOString();
    } catch {
      return "";
    }
  }

  function shortSha(sha: string | undefined): string {
    return (sha ?? "").slice(0, 7);
  }

  function toggleBranch(name: string, event: MouseEvent) {
    const disabled = flatBranches.find((a) => a.name === name);
    if (!disabled || disabled.current || disabled.isBase) return;
    const willSelect = !selected.has(name);
    const next = new Set(selected);
    if (event.shiftKey && lastClickedName) {
      const names = flatBranches.map((a) => a.name);
      const from = names.indexOf(lastClickedName);
      const to = names.indexOf(name);
      if (from !== -1 && to !== -1) {
        const [start, end] = from < to ? [from, to] : [to, from];
        for (let i = start; i <= end; i += 1) {
          const audit = flatBranches[i];
          if (audit.current || audit.isBase) continue;
          if (willSelect) next.add(audit.name);
          else next.delete(audit.name);
        }
        selected = next;
        lastClickedName = name;
        return;
      }
    }
    if (willSelect) next.add(name);
    else next.delete(name);
    selected = next;
    lastClickedName = name;
  }

  function toggleWorktree(path: string) {
    const next = new Set(worktreeSelected);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    worktreeSelected = next;
  }

  function setSectionSelection(rows: BranchAudit[], value: boolean) {
    const next = new Set(selected);
    for (const row of rows) {
      if (row.current || row.isBase) continue;
      if (value) next.add(row.name);
      else next.delete(row.name);
    }
    selected = next;
  }

  function setWorktreeSelection(value: boolean) {
    worktreeSelected = value ? new Set(worktreeCandidates.map((w) => w.path)) : new Set();
  }

  function clickPrimary() {
    if (busy || totalSelected === 0) return;
    if (mustConfirm) phase = "confirm";
    else void runDeletion();
  }

  function cancelConfirm() {
    phase = "ready";
  }

  async function runDeletion() {
    const branchPlan = [...selectedAudits];
    const worktreePlan = [...selectedWorktreeRows];
    phase = "running";
    busy = true;
    error = "";
    const lines: ResultLine[] = [];

    if (worktreePlan.some((w) => w.prunable)) {
      await runGitAction({ kind: "worktreePrune" });
    }

    for (const wt of worktreePlan) {
      if (wt.prunable) {
        lines.push({ kind: "worktree", id: wt.path, ok: true, detail: "pruned" });
        continue;
      }
      const result = await runGitAction({ kind: "worktreeRemove", path: wt.path });
      if (result.ok) {
        lines.push({ kind: "worktree", id: wt.path, ok: true, detail: "removed" });
      } else {
        const dirty = /modified or untracked files|locked working tree|use --force/i.test(result.stderr);
        lines.push({
          kind: "worktree",
          id: wt.path,
          ok: false,
          detail: (result.stderr || result.stdout || "Remove failed").split("\n")[0],
          canForceRemove: dirty,
        });
      }
    }

    for (const audit of branchPlan) {
      const kind = audit.merged ? "deleteBranch" : "deleteBranchForce";
      const result = await runGitAction({ kind, branch: audit.name });
      if (result.ok) {
        lines.push({ kind: "branch", id: audit.name, ok: true, detail: `deleted (was ${shortSha(audit.head)})`, head: audit.head });
      } else {
        const notMerged = /not fully merged/i.test(result.stderr);
        lines.push({
          kind: "branch",
          id: audit.name,
          ok: false,
          detail: (result.stderr || result.stdout || "Delete failed").split("\n")[0],
          head: audit.head,
          canForceDelete: notMerged,
        });
      }
    }

    results = lines;
    busy = false;
    phase = "result";
  }

  async function restoreBranch(line: ResultLine) {
    const result = await runGitAction({ kind: "createBranch", branch: line.id, target: line.head });
    results = results.map((entry) =>
      entry !== line
        ? entry
        : result.ok
          ? { ...entry, restored: true }
          : { ...entry, detail: `${entry.detail} — restore failed: ${(result.stderr || result.stdout).split("\n")[0]}` },
    );
  }

  async function forceDeleteRetry(line: ResultLine) {
    const result = await runGitAction({ kind: "deleteBranchForce", branch: line.id });
    results = results.map((entry) =>
      entry !== line
        ? entry
        : result.ok
          ? { ...entry, ok: true, detail: `deleted (was ${shortSha(entry.head)})`, canForceDelete: false }
          : { ...entry, detail: (result.stderr || result.stdout || "Delete failed").split("\n")[0] },
    );
  }

  function requestForceRemove(line: ResultLine) {
    results = results.map((entry) => (entry === line ? { ...entry, confirmingForceRemove: true } : entry));
  }

  function cancelForceRemove(line: ResultLine) {
    results = results.map((entry) => (entry === line ? { ...entry, confirmingForceRemove: false } : entry));
  }

  async function forceRemoveConfirm(line: ResultLine) {
    const result = await runGitAction({ kind: "worktreeRemoveForce", path: line.id });
    results = results.map((entry) =>
      entry !== line
        ? entry
        : result.ok
          ? { ...entry, ok: true, detail: "removed", canForceRemove: false, confirmingForceRemove: false }
          : { ...entry, detail: (result.stderr || result.stdout || "Remove failed").split("\n")[0], confirmingForceRemove: false },
    );
  }

  function buildDoneSummary(lines: ResultLine[]): string {
    const branchesDeleted = lines.filter((l) => l.kind === "branch" && l.ok).length;
    const worktreesDone = lines.filter((l) => l.kind === "worktree" && l.ok).length;
    const failed = lines.filter((l) => !l.ok).length;
    const parts: string[] = [];
    if (branchesDeleted) parts.push(`deleted ${branchesDeleted} branch${branchesDeleted === 1 ? "" : "es"}`);
    if (worktreesDone) parts.push(`removed ${worktreesDone} worktree${worktreesDone === 1 ? "" : "s"}`);
    let text = parts.length ? parts.join(" and ") : "nothing changed";
    text = text.charAt(0).toUpperCase() + text.slice(1);
    if (failed) text += ` · ${failed} failed`;
    return text;
  }

  let finished = false;

  function requestClose() {
    if (busy) return;
    if (phase === "result" && !finished) {
      finished = true;
      void onDone(buildDoneSummary(results));
    }
    onClose();
  }

  function onKey(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    if (event.key === "Escape") {
      if (busy) {
        event.preventDefault();
        return;
      }
      event.preventDefault();
      if (phase === "confirm") {
        cancelConfirm();
        return;
      }
      requestClose();
      return;
    }
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      if (busy) return;
      if (phase === "ready") clickPrimary();
      else if (phase === "confirm") void runDeletion();
      else if (phase === "result") requestClose();
    }
  }
</script>

<svelte:window on:keydown={onKey} />

<div
  class="modal-backdrop"
  role="presentation"
  on:click={(event) => event.target === event.currentTarget && requestClose()}
>
  <div
    class="modal panel size-xl"
    role="dialog"
    aria-modal="true"
    aria-labelledby="cleanup-title"
    use:trapFocus={{ initial: "#cleanup-base" }}
  >
    <div class="panel-title">
      <h1 id="cleanup-title">Clean up branches</h1>
      <button type="button" title="Close" aria-label="Close" on:click={requestClose} disabled={busy}>×</button>
    </div>
    {#if busy}<div class="busy-bar" aria-hidden="true"></div>{/if}
    <div class="modal-body">
      {#if error}<p class="panel-error" role="alert">{error}</p>{/if}

      <div class="cleanup-head">
        <div class="field">
          <label for="cleanup-base">compared against</label>
          <select
            id="cleanup-base"
            value={base}
            disabled={busy}
            on:change={(event) => void loadReport((event.target as HTMLSelectElement).value)}
          >
            {#each state.branches as branch (branch.name)}
              <option value={branch.name}>{branch.name}</option>
            {/each}
          </select>
        </div>
        <p class="stale-note">stale after {staleDays} days · change in Settings</p>
      </div>

      {#if !report}
        <p class="empty">Auditing {state.branches.length} branch{state.branches.length === 1 ? "" : "es"} against {base}…</p>
      {:else if phase === "result"}
        <table class="cleanup-table result-table">
          <thead>
            <tr>
              <th scope="col"></th>
              <th scope="col">item</th>
              <th scope="col"></th>
            </tr>
          </thead>
          <tbody>
            {#each results as line (line.kind + ":" + line.id)}
              <tr>
                <td class="status-cell">{line.ok ? "✓" : "✗"}</td>
                <td>
                  <strong>{line.id}</strong>
                  <small>{line.detail}{line.restored ? " — restored" : ""}</small>
                </td>
                <td class="action-cell">
                  {#if line.ok && line.kind === "branch" && line.head && !line.restored}
                    <button type="button" on:click={() => restoreBranch(line)}>Restore</button>
                  {:else if !line.ok && line.canForceDelete}
                    <button type="button" class="danger-fill" on:click={() => forceDeleteRetry(line)}>Force delete</button>
                  {:else if !line.ok && line.canForceRemove && !line.confirmingForceRemove}
                    <button type="button" class="danger-fill" on:click={() => requestForceRemove(line)}>Force remove</button>
                  {:else if line.confirmingForceRemove}
                    <span class="inline-confirm">
                      has uncommitted changes that will be lost.
                      <button type="button" on:click={() => cancelForceRemove(line)}>Cancel</button>
                      <button type="button" class="danger-fill" on:click={() => forceRemoveConfirm(line)}>Remove anyway</button>
                    </span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if sections.length === 0 || flatBranches.length + worktreeCandidates.length === 0}
        <p class="empty">No branches to clean up — everything is current or the base.</p>
      {:else}
        {#if nothingRecommended}
          <p class="empty">
            {activeCount > 0 ? `No branches to clean up — all ${activeCount} are active.` : "No branches to clean up."}
          </p>
        {/if}
        <table class="cleanup-table">
          <thead>
            <tr>
              <th scope="col" class="col-check"></th>
              <th scope="col">branch</th>
              <th scope="col">flags</th>
              <th scope="col">vs {base}</th>
              <th scope="col">last commit</th>
            </tr>
          </thead>
          {#each sections as section (section.key)}
            {#if section.rows.length}
              <tbody>
                <tr class="section-row">
                  <th colspan="5" scope="colgroup">
                    <div class="section-row-inner">
                      <span>{section.title}</span>
                      <span class="section-count">{section.rows.length}</span>
                      {#if section.key !== "kept"}
                        <button
                          type="button"
                          class="section-action"
                          on:click={() => setSectionSelection(section.rows, true)}
                          disabled={busy}
                        >all</button>
                        <button
                          type="button"
                          class="section-action"
                          on:click={() => setSectionSelection(section.rows, false)}
                          disabled={busy}
                        >none</button>
                      {/if}
                    </div>
                  </th>
                </tr>
                {#each section.rows as audit (audit.name)}
                  <tr class="branch-row-line" class:dimmed={audit.current || audit.isBase}>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`Select ${audit.name}`}
                        checked={selected.has(audit.name)}
                        disabled={busy || audit.current || audit.isBase}
                        on:click={(event) => toggleBranch(audit.name, event)}
                      />
                    </td>
                    <td>
                      <div class="branch-cell">
                        <strong title={audit.name}>{audit.name}</strong>
                        <small>{upstreamLine(audit)}</small>
                      </div>
                    </td>
                    <td>
                      <div class="flag-cell">
                        {#if audit.classification === "current"}<span class="chip">current</span>{/if}
                        {#if audit.classification === "base"}<span class="chip">base</span>{/if}
                        {#if audit.upstreamGone && audit.classification !== "gone"}<span class="chip warn">upstream gone</span>{/if}
                        {#if audit.stale && audit.classification !== "stale"}<span class="chip warn">stale</span>{/if}
                        {#if audit.worktreePath}<span class="chip" title={audit.worktreePath}>in worktree</span>{/if}
                      </div>
                    </td>
                    <td class="mono-cell" title={`vs ${base}`}>{aheadBehindText(audit.aheadOfBase, audit.behindBase)}</td>
                    <td class="dim-cell" title={isoDate(audit.lastCommitUnix)}>{audit.lastCommitRelative}</td>
                  </tr>
                {/each}
              </tbody>
            {/if}
          {/each}
        </table>

        {#if worktreeCandidates.length}
          <table class="cleanup-table worktree-table">
            <thead>
              <tr>
                <th scope="col" class="col-check"></th>
                <th scope="col">worktree</th>
                <th scope="col">flags</th>
              </tr>
            </thead>
            <tbody>
              <tr class="section-row">
                <th colspan="3" scope="colgroup">
                  <div class="section-row-inner">
                    <span>WORKTREES</span>
                    <span class="section-count">{worktreeCandidates.length}</span>
                    <button type="button" class="section-action" on:click={() => setWorktreeSelection(true)} disabled={busy}>all</button>
                    <button type="button" class="section-action" on:click={() => setWorktreeSelection(false)} disabled={busy}>none</button>
                  </div>
                </th>
              </tr>
              {#each worktreeCandidates as wt (wt.path)}
                <tr class="branch-row-line">
                  <td>
                    <input
                      type="checkbox"
                      aria-label={`Select worktree ${wt.path}`}
                      checked={worktreeSelected.has(wt.path)}
                      disabled={busy}
                      on:click={() => toggleWorktree(wt.path)}
                    />
                  </td>
                  <td>
                    <div class="branch-cell">
                      <strong title={wt.path}>{wt.path}</strong>
                      <small>{wt.branch ?? `${wt.head} (detached)`}</small>
                    </div>
                  </td>
                  <td>
                    <div class="flag-cell">
                      {#if wt.prunable}<span class="chip warn">stale</span>{/if}
                      {#if wt.locked}<span class="chip" title={wt.lockReason ?? "locked"}>locked</span>{/if}
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/if}
    </div>
    <div class="modal-footer">
      {#if !report}
        <button type="button" on:click={requestClose}>Close</button>
      {:else if phase === "confirm"}
        <button type="button" on:click={cancelConfirm} disabled={busy}>Cancel</button>
        <p class="confirm-copy" role="alert">{confirmCopy()}</p>
        <button type="button" class="danger-fill" on:click={runDeletion} disabled={busy}>{primaryLabel()}</button>
      {:else if phase === "running"}
        <span class="footer-tertiary" aria-live="polite">Working…</span>
        <button type="button" class="danger-fill" disabled>Working…</button>
      {:else if phase === "result"}
        <span class="footer-tertiary" aria-live="polite">
          {results.filter((l) => l.ok).length} done{results.some((l) => !l.ok) ? ` · ${results.filter((l) => !l.ok).length} failed` : ""}
        </span>
        <button type="button" class="stage-file" on:click={requestClose}>Done</button>
      {:else}
        <span class="footer-tertiary" aria-live="polite">
          {totalSelected === 0
            ? "Nothing selected"
            : `${totalSelected} selected · ${safeCount} safe · ${forceCount} force${selectedWorktreeRows.length ? ` · ${selectedWorktreeRows.length} worktree${selectedWorktreeRows.length === 1 ? "" : "s"}` : ""}`}
        </span>
        <button type="button" on:click={requestClose} disabled={busy}>Cancel</button>
        <button type="button" class="danger-fill" on:click={clickPrimary} disabled={busy || totalSelected === 0}>
          {primaryLabel()}
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .cleanup-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px;
  }

  .cleanup-head .field {
    margin: 0;
  }

  .stale-note {
    margin: 0 0 6px;
    color: var(--ink-dim);
    font-size: 11px;
  }

  .cleanup-table {
    width: 100%;
    border-collapse: collapse;
  }

  .cleanup-table thead th {
    position: sticky;
    top: 0;
    z-index: 1;
    border-bottom: 1px solid var(--edge);
    background: rgba(26, 29, 36, 0.96);
    padding: 6px 8px;
    color: var(--ink-dim);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.05em;
    text-align: left;
    text-transform: uppercase;
  }

  .cleanup-table .col-check {
    width: 28px;
  }

  .section-row th {
    position: sticky;
    top: 28px;
    z-index: 1;
    background: rgba(26, 29, 36, 0.96);
    padding: 0;
  }

  .section-row-inner {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    padding: 0 8px;
    color: var(--ink-dim);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .section-count {
    color: var(--ink-dim);
    font-weight: 400;
    text-transform: none;
  }

  .section-row-inner .section-action {
    margin-left: 0;
  }

  .section-row-inner .section-action:last-child {
    margin-right: 0;
  }

  .branch-row-line {
    min-height: 44px;
  }

  .branch-row-line:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .branch-row-line td {
    border-bottom: 1px solid var(--edge);
    padding: 6px 8px;
    vertical-align: middle;
  }

  .branch-row-line.dimmed {
    opacity: 0.6;
  }

  .branch-cell {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .branch-cell strong {
    overflow: hidden;
    color: #e8ecf1;
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .branch-cell small {
    overflow: hidden;
    color: #aab2bd;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .flag-cell {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .mono-cell {
    color: #aab2bd;
    font-family: "SFMono-Regular", Consolas, monospace;
    font-size: 12px;
    white-space: nowrap;
  }

  .dim-cell {
    color: #aab2bd;
    font-size: 12px;
    white-space: nowrap;
  }

  .worktree-table {
    margin-top: 14px;
  }

  .result-table td {
    border-bottom: 1px solid var(--edge);
    padding: 6px 8px;
    vertical-align: top;
  }

  .status-cell {
    width: 24px;
    text-align: center;
  }

  .result-table strong {
    display: block;
    color: #e8ecf1;
    font-size: 13px;
    font-weight: 600;
  }

  .result-table small {
    color: #aab2bd;
    font-size: 11px;
  }

  .action-cell {
    text-align: right;
    white-space: nowrap;
  }

  .inline-confirm {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: #f1cf86;
    font-size: 11px;
  }

  .confirm-copy {
    flex: 1;
    margin: 0;
    color: #ffd8df;
    font-size: 12px;
  }
</style>
