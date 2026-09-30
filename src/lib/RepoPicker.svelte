<script lang="ts">
  // Searchable repository dropdown for the clone dialog: every repo the gh
  // account can reach, grouped by owner, with a text filter and an owner
  // filter. The list lives in a popover so the dialog itself stays compact.
  import { tick } from "svelte";
  import {
    filterRepos,
    githubRepoRef,
    groupByOwner,
    highlightParts,
    languageColor,
    ownerAvatarUrl,
  } from "./cloneUtils";
  import type { GhRepo } from "./types";

  export let repos: GhRepo[] = [];
  export let self: string | null = null;
  export let loading = false;
  export let refreshing = false;
  export let error = "";
  export let selected: GhRepo | null = null;
  export let onDisk: Set<string> = new Set();
  export let disabled = false;
  export let showAvatars = true;
  export let onSelect: (repo: GhRepo) => void;
  export let onRefresh: () => void;
  export let onPasteRef: (ref: string) => void;

  let open = false;
  let query = "";
  let ownerFilter = "";
  let activeIndex = 0;
  let trigger: HTMLButtonElement;
  let popover: HTMLDivElement;
  let search: HTMLInputElement;
  let popoverStyle = "";
  let failedAvatars = new Set<string>();

  $: owners = groupByOwner(repos, self).map((group) => ({ owner: group.owner, count: group.repos.length }));
  $: if (ownerFilter && !owners.some((entry) => entry.owner === ownerFilter)) ownerFilter = "";
  $: matches = filterRepos(ownerFilter ? repos.filter((repo) => repo.owner === ownerFilter) : repos, query);
  $: groups = groupByOwner(matches, self);
  $: flat = groups.flatMap((group) => group.repos);
  $: if (activeIndex > flat.length - 1) activeIndex = Math.max(0, flat.length - 1);
  $: pastedRef = githubRepoRef(query);
  $: pastedMissing =
    Boolean(pastedRef) && !repos.some((repo) => repo.nameWithOwner.toLowerCase() === pastedRef?.toLowerCase());
  // A pasted URL narrows to one repo; highlighting its whole URL is noise.
  $: highlightQuery = pastedRef ? "" : query;

  function place() {
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const below = window.innerHeight - rect.bottom - 12;
    const above = rect.top - 12;
    const upward = below < 260 && above > below;
    const maxHeight = Math.min(440, upward ? above : below);
    popoverStyle = [
      `left: ${rect.left}px`,
      `width: ${rect.width}px`,
      upward ? `bottom: ${window.innerHeight - rect.top + 6}px` : `top: ${rect.bottom + 6}px`,
      `max-height: ${maxHeight}px`,
    ].join("; ");
  }

  export async function openPicker(initialQuery = "") {
    if (disabled) return;
    query = initialQuery;
    const index = selected ? flatIndexOf(selected) : -1;
    activeIndex = Math.max(0, index);
    open = true;
    place();
    await tick();
    search?.focus();
    if (initialQuery) search?.setSelectionRange(initialQuery.length, initialQuery.length);
    scrollActiveIntoView();
  }

  function flatIndexOf(repo: GhRepo) {
    return groupByOwner(ownerFilter ? repos.filter((entry) => entry.owner === ownerFilter) : repos, self)
      .flatMap((group) => group.repos)
      .findIndex((entry) => entry.nameWithOwner === repo.nameWithOwner);
  }

  function close(refocus = true) {
    open = false;
    if (refocus) trigger?.focus();
  }

  function choose(repo: GhRepo) {
    onSelect(repo);
    query = "";
    close();
  }

  function scrollActiveIntoView() {
    requestAnimationFrame(() =>
      popover?.querySelector(`[data-index="${activeIndex}"]`)?.scrollIntoView({ block: "nearest" }),
    );
  }

  function onTriggerKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      openPicker();
    } else if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
      // Typing on the closed picker starts a search.
      event.preventDefault();
      openPicker(event.key);
    }
  }

  function move(delta: number) {
    activeIndex = Math.max(0, Math.min(flat.length - 1, activeIndex + delta));
    scrollActiveIntoView();
  }

  function onSearchKeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        move(-1);
        break;
      case "PageDown":
        event.preventDefault();
        move(8);
        break;
      case "PageUp":
        event.preventDefault();
        move(-8);
        break;
      case "Enter":
        if (event.metaKey || event.ctrlKey) break;
        event.preventDefault();
        if (flat[activeIndex]) choose(flat[activeIndex]);
        else if (pastedRef && pastedMissing) {
          onPasteRef(pastedRef);
          close(false);
        }
        break;
      case "Escape":
        // Handled here so the dialog underneath stays open.
        event.preventDefault();
        close();
        break;
      case "Tab":
        close(false);
        break;
      default:
        break;
    }
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (!open) return;
    const target = event.target as Node;
    if (popover?.contains(target) || trigger?.contains(target)) return;
    close(false);
  }

  function avatarFailed(login: string) {
    failedAvatars = new Set(failedAvatars).add(login);
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
      if (Math.abs(duration) < amount) return rtf.format(Math.round(duration), unit);
      duration /= amount;
    }
    return "";
  }

  function kindLabel(repo: GhRepo) {
    if (repo.isArchived) return "Archived repository";
    if (repo.isFork) return "Fork";
    return repo.isPrivate ? "Private repository" : "Public repository";
  }

  // Index into `flat` for a repo inside `groups`, for the keyboard cursor.
  function indexOf(repo: GhRepo) {
    return flat.indexOf(repo);
  }
</script>

<svelte:window on:pointerdown={onWindowPointerDown} on:resize={() => open && place()} />

<!-- Owner avatar with an initial as the fallback when avatars are off or fail. -->
{#snippet avatar(login: string, isOrg: boolean, size: number)}
  <span class="avatar" class:org={isOrg} style={`width: ${size}px; height: ${size}px`} aria-hidden="true">
    {#if showAvatars && !failedAvatars.has(login)}
      <img src={ownerAvatarUrl(login, size * 2)} alt="" on:error={() => avatarFailed(login)} />
    {:else}
      {login.slice(0, 1).toUpperCase()}
    {/if}
  </span>
{/snippet}

{#snippet repoIcon(repo: GhRepo)}
  <svg class="repo-icon" viewBox="0 0 15 15" aria-hidden="true">
    {#if repo.isArchived}
      <rect x="2" y="3" width="11" height="3" rx="0.8" />
      <path d="M3 6v6.5a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1V6M6 8.5h3" />
    {:else if repo.isFork}
      <circle cx="4.5" cy="3.5" r="1.5" />
      <circle cx="10.5" cy="3.5" r="1.5" />
      <circle cx="7.5" cy="12" r="1.5" />
      <path d="M4.5 5v.8a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2V5M7.5 7.8v2.7" />
    {:else if repo.isPrivate}
      <rect x="3" y="6.8" width="9" height="6.7" rx="1.2" />
      <path d="M5 6.8V5a2.5 2.5 0 0 1 5 0v1.8" />
    {:else}
      <path d="M3.5 1.8h8v9.4h-8a1.1 1.1 0 0 0-1.1 1.1V2.9a1.1 1.1 0 0 1 1.1-1.1Z" />
      <path d="M2.4 12.3a1.1 1.1 0 0 0 1.1 1.1h8" />
    {/if}
  </svg>
{/snippet}

<button
  bind:this={trigger}
  id="repo-picker"
  type="button"
  class="picker-trigger"
  class:open
  class:filled={Boolean(selected)}
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-controls="repo-picker-list"
  {disabled}
  on:click={() => (open ? close() : openPicker())}
  on:keydown={onTriggerKeydown}
>
  {#if selected}
    {@render avatar(selected.owner, Boolean(selected.ownerIsOrg), 18)}
    <span class="trigger-name"><span class="dim">{selected.owner} /</span> {selected.name}</span>
    <span class="trigger-kind" title={kindLabel(selected)}>{@render repoIcon(selected)}</span>
  {:else if loading && !repos.length}
    <span class="placeholder">Loading repositories…</span>
  {:else if error && !repos.length}
    <span class="placeholder">Couldn't load repositories</span>
  {:else}
    <svg class="search-glyph" viewBox="0 0 15 15" aria-hidden="true">
      <circle cx="6.5" cy="6.5" r="4.2" />
      <path d="m9.6 9.6 3.4 3.4" />
    </svg>
    <span class="placeholder">Choose from {repos.length} repositories…</span>
  {/if}
  <span class="caret" aria-hidden="true"></span>
</button>

{#if open}
  <div class="picker-popover" bind:this={popover} style={popoverStyle}>
    <div class="picker-search">
      <div class="search-field">
        <svg class="search-glyph" viewBox="0 0 15 15" aria-hidden="true">
          <circle cx="6.5" cy="6.5" r="4.2" />
          <path d="m9.6 9.6 3.4 3.4" />
        </svg>
        <input
          bind:this={search}
          type="text"
          placeholder="Search or paste a URL"
          aria-label="Search repositories"
          aria-controls="repo-picker-list"
          aria-activedescendant={flat[activeIndex] ? `repo-option-${activeIndex}` : undefined}
          autocomplete="off"
          spellcheck="false"
          bind:value={query}
          on:input={() => (activeIndex = 0)}
          on:keydown={onSearchKeydown}
        />
      </div>
      <select
        aria-label="Owner"
        bind:value={ownerFilter}
        on:change={() => {
          activeIndex = 0;
          search?.focus();
        }}
      >
        <option value="">All owners</option>
        {#each owners as entry (entry.owner)}
          <option value={entry.owner}>{entry.owner} ({entry.count})</option>
        {/each}
      </select>
      <button
        type="button"
        class="btn picker-refresh"
        title="Refresh from GitHub"
        aria-label="Refresh repositories from GitHub"
        disabled={refreshing || loading}
        on:click={() => {
          onRefresh();
          search?.focus();
        }}
      >
        <svg class:spin={refreshing || loading} viewBox="0 0 15 15" aria-hidden="true">
          <path d="M12.5 7.5a5 5 0 1 1-1.5-3.6" />
          <path d="M11.4 1.6v2.6H8.8" />
        </svg>
      </button>
    </div>

    <div id="repo-picker-list" class="picker-list" role="listbox" aria-label="Repositories">
      {#if loading && !repos.length}
        {#each Array(5) as _, i (i)}
          <div class="picker-row skeleton" aria-hidden="true">
            <span class="bone icon"></span>
            <span class="bone" style={`width: ${34 + ((i * 17) % 30)}%`}></span>
            <span></span>
            <span class="bone thin" style={`width: ${52 + ((i * 11) % 34)}%`}></span>
          </div>
        {/each}
      {:else if error && !repos.length}
        <p class="picker-empty" role="alert">{error}</p>
      {:else}
        {#each groups as group (group.owner)}
          {@const first = group.repos[0]}
          <div class="picker-group" role="presentation">
            {@render avatar(group.owner, Boolean(first?.ownerIsOrg), 18)}
            <span class="group-name">{group.owner}</span>
            {#if self && group.owner.toLowerCase() === self.toLowerCase()}
              <span class="tag">You</span>
            {:else if first?.ownerIsOrg}
              <span class="tag">Org</span>
            {/if}
            <span class="count">{group.repos.length}</span>
          </div>
          {#each group.repos as repo (repo.nameWithOwner)}
            {@const index = indexOf(repo)}
            {@const isCurrent = selected?.nameWithOwner === repo.nameWithOwner}
            <div
              id={`repo-option-${index}`}
              data-index={index}
              role="option"
              tabindex="-1"
              aria-selected={isCurrent}
              class="picker-row"
              class:active={index === activeIndex}
              class:current={isCurrent}
              class:archived={repo.isArchived}
              title={`${repo.nameWithOwner} · ${kindLabel(repo)}`}
              on:pointermove={() => {
                if (activeIndex !== index) activeIndex = index;
              }}
              on:click={() => choose(repo)}
              on:keydown={() => {}}
            >
              <span class="row-icon">{@render repoIcon(repo)}</span>
              <span class="row-name">
                {#each highlightParts(repo.name, highlightQuery) as part, partIndex (partIndex)}
                  {#if part.match}<mark>{part.text}</mark>{:else}{part.text}{/if}
                {/each}
                {#if onDisk.has(repo.nameWithOwner)}<span class="chip ok">cloned</span>{/if}
                {#if repo.isArchived}<span class="chip warn">archived</span>{/if}
              </span>
              <span class="row-meta">
                {#if repo.language}
                  <span class="lang"><span class="dot" style={`background: ${languageColor(repo.language)}`}></span>{repo.language}</span>
                {/if}
                {#if repo.pushedAt}<span class="when" title={repo.pushedAt}>{formatPushed(repo.pushedAt)}</span>{/if}
                {#if isCurrent}
                  <svg class="check" viewBox="0 0 15 15" aria-hidden="true"><path d="M3 7.8 6 10.8 12 4.6" /></svg>
                {/if}
              </span>
              {#if repo.description}
                <span class="row-desc">
                  {#each highlightParts(repo.description, highlightQuery) as part, partIndex (partIndex)}
                    {#if part.match}<mark>{part.text}</mark>{:else}{part.text}{/if}
                  {/each}
                </span>
              {/if}
            </div>
          {/each}
        {:else}
          {#if !(pastedRef && pastedMissing)}
            <div class="picker-empty">
              <strong>No matches</strong>
              <span>Nothing in your {repos.length} repositories matches “{query.trim()}”.</span>
            </div>
          {/if}
        {/each}
        {#if pastedRef && pastedMissing}
          <button
            type="button"
            class="picker-paste"
            on:click={() => {
              if (pastedRef) onPasteRef(pastedRef);
              close(false);
            }}
          >
            <span>Clone <strong>{pastedRef}</strong> by URL</span>
            <span aria-hidden="true">→</span>
          </button>
        {/if}
      {/if}
    </div>
    <div class="picker-foot" class:warn={Boolean(error && repos.length)}>
      {#if error && repos.length}
        Showing the last list; refresh failed.
      {:else}
        <span>{flat.length} of {repos.length}</span>
        <span class="keys"><kbd>↑</kbd><kbd>↓</kbd> move <kbd>⏎</kbd> select <kbd>esc</kbd> close</span>
      {/if}
    </div>
  </div>
{/if}

<style>
  svg {
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 1.3;
  }

  /* ---- trigger ---- */

  .picker-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-width: 0;
    height: 32px;
    padding: 0 10px;
    border: 1px solid #3d4048;
    border-radius: 6px;
    background: var(--input-bg);
    color: var(--ink);
    font: inherit;
    font-size: 13px;
    text-align: left;
    transition:
      border-color 120ms ease,
      background-color 120ms ease,
      box-shadow 120ms ease;
  }

  .picker-trigger:hover:not(:disabled) {
    border-color: #6a6e76;
    background: #252930;
  }

  .picker-trigger.open,
  .picker-trigger:focus-visible {
    border-color: var(--teal);
    outline: none;
    box-shadow: 0 0 0 3px rgb(20 160 191 / 0.18);
  }

  .trigger-name {
    overflow: hidden;
    min-width: 0;
    color: #eef1f5;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dim {
    color: #9aa2ad;
    font-weight: 400;
  }

  .trigger-kind {
    display: inline-flex;
    color: #8b93a0;
  }

  .placeholder {
    overflow: hidden;
    color: #8b93a0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .search-glyph {
    flex: none;
    width: 14px;
    height: 14px;
    color: #8b93a0;
  }

  .caret {
    flex: none;
    width: 6px;
    height: 6px;
    margin-left: auto;
    border-right: 1.5px solid #aab2bd;
    border-bottom: 1.5px solid #aab2bd;
    transform: translateY(-2px) rotate(45deg);
    transition: transform 150ms ease;
  }

  .open .caret {
    transform: translateY(1px) rotate(225deg);
  }

  /* ---- avatars & icons ---- */

  .avatar {
    display: inline-grid;
    flex: none;
    place-items: center;
    overflow: hidden;
    border-radius: 50%;
    background: #3a4050;
    color: #dfe5ee;
    font-size: 10px;
    font-weight: 600;
  }

  .avatar.org {
    border-radius: 4px;
  }

  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .repo-icon {
    width: 15px;
    height: 15px;
  }

  /* ---- popover ---- */

  .picker-popover {
    position: fixed;
    z-index: 60;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    border: 1px solid var(--edge-hi);
    border-radius: 8px;
    background: #23262d;
    box-shadow:
      0 20px 48px rgb(0 0 0 / 0.55),
      0 0 0 1px rgb(0 0 0 / 0.25);
    animation: pop-in 120ms ease-out;
  }

  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  .picker-search {
    display: flex;
    flex: none;
    gap: 6px;
    padding: 8px;
    border-bottom: 1px solid var(--edge);
    background: #272a32;
  }

  .search-field {
    position: relative;
    display: flex;
    flex: 1;
    min-width: 0;
    align-items: center;
  }

  .search-field .search-glyph {
    position: absolute;
    left: 9px;
    pointer-events: none;
  }

  .search-field input {
    width: 100%;
    min-width: 0;
    padding-left: 29px;
  }

  .picker-search select {
    flex: none;
    width: 32%;
    min-width: 0;
  }

  .picker-refresh {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
  }

  .picker-refresh svg {
    width: 14px;
    height: 14px;
  }

  .spin {
    animation: picker-spin 0.9s linear infinite;
  }

  @keyframes picker-spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* ---- list ---- */

  .picker-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 0 6px;
  }

  /* The thumb only shows while the pointer is over the list. */
  .picker-list::-webkit-scrollbar-thumb {
    background: transparent;
  }

  .picker-list:hover::-webkit-scrollbar-thumb {
    background: #484a51;
    background-clip: padding-box;
  }

  .picker-group {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px 5px;
    background: #23262d;
    color: #c5ccd6;
    font-size: 12px;
    font-weight: 600;
  }

  .group-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag {
    padding: 1px 5px;
    border-radius: 3px;
    background: rgb(255 255 255 / 0.07);
    color: #9aa2ad;
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .count {
    margin-left: auto;
    color: #7e8792;
    font-size: 11px;
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }

  .picker-row {
    position: relative;
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr) auto;
    column-gap: 10px;
    row-gap: 1px;
    align-items: center;
    margin: 1px 6px;
    padding: 7px 10px;
    border-radius: 6px;
    cursor: pointer;
    transition: background-color 80ms ease;
  }

  .picker-row.active {
    background: rgb(255 255 255 / 0.06);
  }

  .picker-row.current {
    background: rgb(47 164 76 / 0.14);
  }

  .picker-row.current.active {
    background: rgb(47 164 76 / 0.2);
  }

  .picker-row.archived .row-name {
    color: #aab2bd;
  }

  .row-icon {
    display: inline-flex;
    color: #8b93a0;
  }

  .picker-row.active .row-icon,
  .picker-row.current .row-icon {
    color: #c9d1d9;
  }

  .row-name {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    min-width: 0;
    color: #eef1f5;
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
  }

  mark {
    border-radius: 2px;
    background: rgb(20 160 191 / 0.28);
    color: #fff;
  }

  .row-meta {
    display: flex;
    align-items: center;
    gap: 10px;
    color: #8b93a0;
    font-size: 11px;
    white-space: nowrap;
  }

  .lang {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.12);
  }

  .check {
    width: 14px;
    height: 14px;
    color: #4ade80;
    stroke-width: 1.8;
  }

  .row-desc {
    grid-column: 2 / 4;
    overflow: hidden;
    color: #9aa2ad;
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skeleton {
    cursor: default;
  }

  .bone {
    display: block;
    height: 9px;
    border-radius: 3px;
    background: rgb(255 255 255 / 0.07);
    animation: picker-pulse 1.2s ease-in-out infinite alternate;
  }

  .bone.icon {
    width: 14px;
    height: 14px;
    border-radius: 4px;
  }

  .bone.thin {
    grid-column: 2 / 4;
    height: 7px;
    margin-top: 5px;
  }

  @keyframes picker-pulse {
    to {
      opacity: 0.45;
    }
  }

  .picker-empty {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 22px 16px;
    color: #8b93a0;
    font-size: 12px;
    text-align: center;
    white-space: pre-wrap;
  }

  .picker-empty strong {
    color: #c5ccd6;
    font-size: 13px;
  }

  .picker-paste {
    display: flex;
    justify-content: space-between;
    width: calc(100% - 12px);
    margin: 4px 6px 0;
    padding: 9px 10px;
    border: 1px dashed rgb(20 160 191 / 0.45);
    border-radius: 6px;
    color: #7fd3e6;
    font-size: 12px;
    font-weight: 600;
    text-align: left;
  }

  .picker-paste:hover:not(:disabled) {
    border-color: var(--teal);
    background: rgb(20 160 191 / 0.1);
  }

  .picker-foot {
    display: flex;
    flex: none;
    justify-content: space-between;
    padding: 6px 12px;
    border-top: 1px solid var(--edge);
    background: #272a32;
    color: #7e8792;
    font-size: 11px;
  }

  .picker-foot.warn {
    color: #f1cf86;
  }

  .keys {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  kbd {
    min-width: 16px;
    padding: 0 4px;
    border: 1px solid var(--edge-hi);
    border-bottom-width: 2px;
    border-radius: 3px;
    color: #aab2bd;
    font-family: inherit;
    font-size: 10px;
    text-align: center;
  }

  kbd + kbd {
    margin-left: -1px;
  }

  @media (prefers-reduced-motion: reduce) {
    .picker-popover {
      animation: none;
    }

    .spin,
    .bone {
      animation-duration: 2.4s;
    }
  }
</style>
