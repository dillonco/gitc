<script lang="ts">
  import {
    applyHunk,
    cloneRepository,
    createRepository,
    getCommitGraph,
    getConflictFile,
    getFileBlame,
    getFileContent,
    getFileDiff,
    getFileHistory,
    getRepositoryState,
    openTerminal,
    pickRepositoryFolder,
    runGitAction,
    saveConflictResolution,
    setRepositoryPath,
  } from "./lib/git";
  import FileGroup from "./lib/FileGroup.svelte";
  import ReadonlyPane from "./lib/ReadonlyPane.svelte";
  import type { CommitNode, ConflictFile, FileDiff, FileStatus, GitAction, RepositoryState } from "./lib/types";

  type AppTab = {
    id: string;
    kind: "launchpad" | "repo";
    label: string;
    path?: string;
  };

  type GraphLane = {
    index: number;
    color: string;
  };

  type GraphEdge = {
    from: number;
    to: number;
    color: string;
  };

  type GraphRow = {
    commit: CommitNode;
    lane: number;
    lanes: GraphLane[];
    edges: GraphEdge[];
    color: string;
    labels: string[];
  };

  let state: RepositoryState | null = null;
  let tabs: AppTab[] = [{ id: "launchpad", kind: "launchpad", label: "Launchpad" }];
  let activeTabId = "launchpad";
  let commits: CommitNode[] = [];
  let selectedCommit: CommitNode | null = null;
  let selectedFile: FileStatus | null = null;
  let selectedDiff: FileDiff | null = null;
  let centerMode: "graph" | "file" | "launchpad" = "graph";
  let fileViewMode: "diff" | "file" | "blame" | "history" = "diff";
  let fileText = "";
  let splitDiff = false;
  let selectedHunk = 0;
  let conflict: ConflictFile | null = null;
  let resolvedContent = "";
  let commitMessage = "";
  let commitDescription = "";
  let amendCommit = false;
  let commandTarget = "";
  let branchName = "";
  let resetMode = "mixed";
  let rightTab: "path" | "tree" = "path";
  let searchOpen = false;
  let searchQuery = "";
  let sortAsc = true;
  let localOpen = true;
  let remoteOpen = false;
  let worktreesOpen = false;
  let unstagedOpen = true;
  let stagedOpen = true;
  let busy = false;
  let error = "";
  let notice = "";

  const riskyActions = new Set(["discard", "reset", "forcePush", "stashDrop"]);
  const graphColors = [
    "#26c6da",
    "#2f80ed",
    "#c33cff",
    "#f33bd2",
    "#f94144",
    "#ff7a45",
    "#f5d547",
    "#8be34b",
    "#20d6a3",
    "#33b5e5",
    "#3167d9",
    "#9c27b0",
  ];
  const recentRepos = [
    { name: "gitc", path: "/Users/dillon/Documents/dev/gitc" },
    { name: "meetings", path: "/Users/dillon/Documents/dev/meetings" },
    { name: "data-layer", path: "/Users/dillon/Documents/dev/data-layer" },
    { name: "waas", path: "/Users/dillon/Documents/dev/waas" },
    { name: "nested", path: "/Users/dillon/Documents/dev/nested" },
    { name: "LandLocked", path: "/Users/dillon/Documents/dev/LandLocked" },
    { name: "RobertaRoyale", path: "/Users/dillon/Documents/dev/RobertaRoyale" },
    { name: "otc-api", path: "/Users/dillon/Documents/dev/otc-api" },
  ];

  $: staged = grouped(state, "staged");
  $: unstaged = grouped(state, "unstaged");
  $: untracked = grouped(state, "untracked");
  $: conflicted = grouped(state, "conflicted");
  $: currentBranch = state?.currentBranch || "detached";
  $: totalChanges = state?.files.length ?? 0;
  $: unstagedTotal = unstaged.length + untracked.length + conflicted.length;
  $: repoName = state?.root.split("/").filter(Boolean).at(-1) ?? "gitc";
  $: activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  $: fullCommitMessage = commitDescription.trim()
    ? `${commitMessage.trim()}\n\n${commitDescription.trim()}`
    : commitMessage.trim();
  $: visibleUnstaged = sortFiles([...unstaged, ...untracked, ...conflicted]);
  $: visibleStaged = sortFiles(staged);
  $: diffRows = parseDiffRows(selectedDiff?.diff ?? "");
  $: hunkRows = diffRows.map((row, index) => ({ row, index })).filter((item) => item.row.kind === "hunk");
  $: graphRows = buildGraphRows(commits);
  $: graphLaneCount = Math.max(8, ...graphRows.flatMap((row) => row.lanes.map((lane) => lane.index + 1)), 1);

  async function refresh() {
    busy = true;
    error = "";
    try {
      const [nextState, graph] = await Promise.all([getRepositoryState(), getCommitGraph(250)]);
      state = nextState;
      syncRepoTab(nextState.root, nextState.root.split("/").filter(Boolean).at(-1) ?? "repo");
      commits = graph.commits;
      selectedCommit = selectedCommit
        ? graph.commits.find((commit) => commit.hash === selectedCommit?.hash) ?? graph.commits[0] ?? null
        : graph.commits[0] ?? null;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function normalizedGroup(file: FileStatus): FileStatus["group"] {
    const rawGroup = String(file.group).toLowerCase();
    if (rawGroup === "staged" || rawGroup === "unstaged" || rawGroup === "untracked" || rawGroup === "conflicted") {
      return rawGroup;
    }
    if (file.index === "U" || file.worktree === "U") return "conflicted";
    if (file.index === "?" || file.worktree === "?") return "untracked";
    if (file.index && file.index !== ".") return "staged";
    return "unstaged";
  }

  function grouped(repositoryState: RepositoryState | null, group: FileStatus["group"]) {
    return repositoryState?.files.filter((file) => normalizedGroup(file) === group) ?? [];
  }

  function syncRepoTab(path: string, label: string) {
    const id = `repo:${path}`;
    const existing = tabs.find((tab) => tab.id === id);
    if (existing) {
      tabs = tabs.map((tab) => (tab.id === id ? { ...tab, label } : tab));
      activeTabId = id;
      return;
    }
    tabs = [...tabs, { id, kind: "repo", label, path }];
    activeTabId = id;
  }

  function switchToTab(tab: AppTab) {
    activeTabId = tab.id;
    if (tab.kind === "launchpad") {
      centerMode = "launchpad";
      return;
    }
    if (tab.path) {
      centerMode = "graph";
      void openRepositoryPath(tab.path, true);
    }
  }

  function closeTab(tab: AppTab) {
    if (tab.kind === "launchpad") return;
    const remaining = tabs.filter((item) => item.id !== tab.id);
    tabs = remaining.length ? remaining : [{ id: "launchpad", kind: "launchpad", label: "Launchpad" }];
    if (activeTabId === tab.id) {
      const nextActive = tabs.find((item) => item.kind === "repo") ?? tabs[0];
      activeTabId = nextActive.id;
      switchToTab(nextActive);
    }
  }

  async function execute(action: GitAction, label: string) {
    if (riskyActions.has(action.kind) && !confirm(`${label} can rewrite or discard repository state. Continue?`)) {
      return;
    }

    busy = true;
    error = "";
    notice = "";
    try {
      const result = await runGitAction(action);
      if (!result.ok) {
        error = result.stderr || result.stdout || `${label} failed`;
      } else {
        notice = `${label} complete`;
      }
      if (result.refresh) await refresh();
      if (selectedFile && action.path === selectedFile.path) await openFile(selectedFile);
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function openConflict(file: FileStatus) {
    selectedFile = file;
    selectedDiff = null;
    conflict = null;
    resolvedContent = "";
    if (file.group !== "conflicted") return;

    busy = true;
    error = "";
    try {
      conflict = await getConflictFile(file.path);
      resolvedContent = conflict.working;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function openFile(file: FileStatus) {
    selectedFile = file;
    selectedDiff = null;
    selectedHunk = 0;
    centerMode = "file";
    if (file.group === "conflicted") {
      await openConflict(file);
      return;
    }

    busy = true;
    error = "";
    try {
      selectedDiff = await getFileDiff(file.path, file.group === "staged");
      if (fileViewMode !== "diff") await loadFileAuxiliary(file);
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function saveResolution() {
    if (!conflict) return;
    busy = true;
    error = "";
    try {
      const result = await saveConflictResolution(conflict.path, resolvedContent);
      if (!result.ok) error = result.stderr || "Unable to save resolution";
      await refresh();
      notice = `Saved ${conflict.path}`;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function useVersion(content?: string | null) {
    if (content != null) resolvedContent = content;
  }

  function fileAction(file: FileStatus, kind: string, label: string) {
    execute({ kind, path: file.path }, label);
  }

  async function switchRepository() {
    const path = await pickRepositoryFolder();
    if (!path) return;
    await openRepositoryPath(path);
  }

  async function openClonePrompt() {
    const url = prompt("Repository URL to clone");
    if (!url?.trim()) return;
    const path = prompt("Clone into path", `/Users/dillon/Documents/dev/${url.split("/").pop()?.replace(/\.git$/, "") ?? "repo"}`);
    if (!path?.trim()) return;
    busy = true;
    error = "";
    try {
      state = await cloneRepository(url.trim(), path.trim());
      const graph = await getCommitGraph(250);
      commits = graph.commits;
      selectedFile = null;
      selectedDiff = null;
      centerMode = "graph";
      syncRepoTab(state.root, state.root.split("/").filter(Boolean).at(-1) ?? "repo");
      notice = `Cloned ${state.root}`;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function openCreatePrompt() {
    const path = prompt("Create repository at path", "/Users/dillon/Documents/dev/new-repo");
    if (!path?.trim()) return;
    busy = true;
    error = "";
    try {
      state = await createRepository(path.trim());
      const graph = await getCommitGraph(250);
      commits = graph.commits;
      selectedFile = null;
      selectedDiff = null;
      centerMode = "graph";
      syncRepoTab(state.root, state.root.split("/").filter(Boolean).at(-1) ?? "repo");
      notice = `Created ${state.root}`;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function openRepositoryPath(path: string, silent = false) {
    busy = true;
    error = "";
    notice = "";
    try {
      state = await setRepositoryPath(path);
      const graph = await getCommitGraph(250);
      commits = graph.commits;
      selectedFile = null;
      selectedDiff = null;
      centerMode = "graph";
      syncRepoTab(state.root, state.root.split("/").filter(Boolean).at(-1) ?? "repo");
      if (!silent) notice = `Opened ${state.root}`;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function sortFiles(files: FileStatus[]) {
    return files
      .filter((file) => !searchQuery.trim() || file.path.toLowerCase().includes(searchQuery.trim().toLowerCase()))
      .sort((a, b) => sortAsc ? a.path.localeCompare(b.path) : b.path.localeCompare(a.path));
  }

  async function loadFileAuxiliary(file = selectedFile) {
    if (!file) return;
    busy = true;
    error = "";
    try {
      if (fileViewMode === "file") fileText = await getFileContent(file.path, file.group === "staged");
      if (fileViewMode === "blame") fileText = await getFileBlame(file.path);
      if (fileViewMode === "history") fileText = await getFileHistory(file.path);
    } catch (err) {
      fileText = "";
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function setFileViewMode(mode: "diff" | "file" | "blame" | "history") {
    fileViewMode = mode;
    if (mode === "diff") return;
    await loadFileAuxiliary();
  }

  function parseDiffRows(diff: string) {
    let oldLine = 0;
    let newLine = 0;
    return diff
      .split("\n")
      .filter((line) => !line.startsWith("diff --git") && !line.startsWith("index ") && !line.startsWith("--- ") && !line.startsWith("+++ "))
      .map((line) => {
        const hunk = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
        if (hunk) {
          oldLine = Number(hunk[1]);
          newLine = Number(hunk[2]);
          return { kind: "hunk", oldNo: "", newNo: "", text: line };
        }
        if (line.startsWith("+")) return { kind: "add", oldNo: "", newNo: String(newLine++), text: line };
        if (line.startsWith("-")) return { kind: "del", oldNo: String(oldLine++), newNo: "", text: line };
        return { kind: "ctx", oldNo: oldLine ? String(oldLine++) : "", newNo: newLine ? String(newLine++) : "", text: line };
      });
  }

  function laneColor(index: number) {
    return graphColors[index % graphColors.length];
  }

  function refLabels(commit: CommitNode) {
    return commit.refs
      .map((ref) => ref.replace(/^HEAD -> /, "").replace(/^tag: /, "tag:"))
      .filter((ref) => ref && !ref.includes("origin/HEAD"));
  }

  function authorInitials(author: string) {
    return author
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase())
      .join("") || "G";
  }

  function buildGraphRows(nodes: CommitNode[]): GraphRow[] {
    const lanes: string[] = [];
    const rows: GraphRow[] = [];

    for (const commit of nodes) {
      let lane = lanes.indexOf(commit.hash);
      if (lane === -1) {
        lane = lanes.findIndex((value) => value === "");
        if (lane === -1) lane = lanes.length;
        lanes[lane] = commit.hash;
      }

      const visibleLanes = lanes
        .map((hash, index) => ({ hash, index }))
        .filter((item) => item.hash)
        .map((item) => ({ index: item.index, color: laneColor(item.index) }));

      const firstParent = commit.parents[0] ?? "";
      if (firstParent) {
        lanes[lane] = firstParent;
      } else {
        lanes[lane] = "";
      }

      const edges: GraphEdge[] = [];
      for (const parent of commit.parents.slice(1)) {
        let parentLane = lanes.indexOf(parent);
        if (parentLane === -1) {
          parentLane = lanes.findIndex((value) => value === "");
          if (parentLane === -1) parentLane = lanes.length;
          lanes[parentLane] = parent;
        }
        edges.push({ from: lane, to: parentLane, color: laneColor(parentLane) });
      }

      rows.push({
        commit,
        lane,
        lanes: visibleLanes,
        edges,
        color: laneColor(lane),
        labels: refLabels(commit),
      });
    }

    return rows;
  }

  function fileStageAction() {
    if (!selectedFile) return;
    if (selectedFile.group === "staged") {
      execute({ kind: "unstage", path: selectedFile.path }, "Unstage file");
    } else {
      execute({ kind: "stage", path: selectedFile.path }, "Stage file");
    }
  }

  function createBranchFromToolbar() {
    const name = prompt("New branch name");
    if (name?.trim()) execute({ kind: "createBranch", branch: name.trim() }, `Create ${name.trim()}`);
  }

  function showActionsMenu() {
    notice = "Use Commit options for merge, rebase, cherry-pick, and reset actions.";
  }

  function showAddRepoNotice() {
    activeTabId = "launchpad";
    centerMode = "launchpad";
  }

  async function openFromPicker() {
    const path = await pickRepositoryFolder();
    if (!path) return;
    await openRepositoryPath(path);
  }

  function moveHunk(direction: 1 | -1) {
    if (hunkRows.length === 0) return;
    selectedHunk = (selectedHunk + direction + hunkRows.length) % hunkRows.length;
  }

  function selectedHunkPatch() {
    if (!selectedDiff?.diff || hunkRows.length === 0) return "";
    const lines = selectedDiff.diff.split("\n");
    const header: string[] = [];
    let hunkNumber = -1;
    const hunk: string[] = [];

    for (const line of lines) {
      if (line.startsWith("@@ ")) {
        hunkNumber += 1;
        if (hunkNumber > selectedHunk) break;
      }
      if (hunkNumber < 0) header.push(line);
      if (hunkNumber === selectedHunk) hunk.push(line);
    }

    return [...header, ...hunk, ""].join("\n");
  }

  async function applySelectedHunk(mode: "stage" | "unstage" | "discard") {
    if (!selectedFile) return;
    const patch = selectedHunkPatch();
    if (!patch.trim()) {
      error = "No hunk selected";
      return;
    }
    busy = true;
    error = "";
    notice = "";
    try {
      const result = await applyHunk(patch, mode);
      if (!result.ok) error = result.stderr || result.stdout || "Unable to apply hunk";
      else notice = `${mode === "stage" ? "Staged" : mode === "unstage" ? "Unstaged" : "Discarded"} hunk`;
      await refresh();
      await openFile(selectedFile);
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function openRepoTerminal() {
    busy = true;
    error = "";
    try {
      const result = await openTerminal();
      if (!result.ok) error = result.stderr || "Unable to open terminal";
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  refresh();
</script>

<main class="shell" class:launchpad-mode={centerMode === "launchpad"}>
  <header class="app-header">
    <nav class="tabs" aria-label="Open repositories">
      {#each tabs as tab}
        <div class="tab-wrap">
          <button class="tab {tab.id === activeTabId ? 'active' : ''}" on:click={() => switchToTab(tab)}>
            <span>{tab.label}</span>
          </button>
          {#if tab.kind === "repo"}
            <button class="tab-close" on:click={() => closeTab(tab)} aria-label={`Close ${tab.label}`}>×</button>
          {/if}
        </div>
      {/each}
      <button class="tab add-tab" title="Open repository" on:click={showAddRepoNotice}>+</button>
    </nav>
    <div class="account-strip">
      <button title="Notifications" on:click={() => (notice = "No notifications")}>◔</button>
      <button title="Settings" on:click={() => (notice = "Settings are not implemented yet")}>⚙</button>
      <strong>Dillon</strong>
    </div>
  </header>

  {#if centerMode !== "launchpad"}
  <div class="repo-bar">
    <button class="repo-select" on:click={switchRepository}>
      <span>repository</span>
      <strong>{repoName}</strong>
    </button>
    <div class="branch-select">
      <span>branch</span>
      <strong>{currentBranch}</strong>
    </div>
    <div class="top-actions">
      <button title="Refresh" on:click={refresh} disabled={busy}>↻<span>Refresh</span></button>
      <button title="Pull" on:click={() => execute({ kind: "pull" }, "Pull")} disabled={busy}>⇩<span>Pull</span></button>
      <button title="Push" on:click={() => execute({ kind: "push" }, "Push")} disabled={busy}>⇧<span>Push</span></button>
      <button title="Branch" on:click={createBranchFromToolbar}>⑂<span>Branch</span></button>
      <button title="Stash" on:click={() => execute({ kind: "stashCreate", message: "gitc stash" }, "Create stash")} disabled={busy}>▤<span>Stash</span></button>
      <button title="Terminal" on:click={openRepoTerminal} disabled={busy}>⌁<span>Terminal</span></button>
    </div>
    <div class="search-actions">
      <button title="Actions" on:click={showActionsMenu}>☷<span>Actions</span></button>
      <button title="Search" on:click={() => (searchOpen = !searchOpen)}>⌕<span>Search</span></button>
    </div>
  </div>
  {/if}

  {#if centerMode !== "launchpad"}
  <aside class="left-panel">
    <div class="view-tabs single">
      <button class="active">☷ Repository</button>
    </div>
    <div class="filter-block">
      <span>Viewing <strong>{(state?.branches.length ?? 0) + (state?.remotes.length ?? 0)}</strong></span>
      <input aria-label="Filter refs" bind:value={searchQuery} placeholder="Filter (⌘ + Option + f)" />
    </div>

    <section class="nav-section">
      <button class="nav-row" on:click={() => (localOpen = !localOpen)}>
        <span>{localOpen ? "⌄" : "›"} ⌂ LOCAL</span>
        <strong>{state?.branches.length ?? 0}</strong>
      </button>
      {#if localOpen}
      <div class="branch-list">
        {#each (state?.branches ?? []).filter((branch) => !searchQuery.trim() || branch.name.toLowerCase().includes(searchQuery.trim().toLowerCase())) as branch}
          <button
            class:active={branch.current}
            on:click={() => execute({ kind: "checkoutBranch", branch: branch.name }, `Checkout ${branch.name}`)}
            disabled={busy || branch.current}
          >
            <span>{branch.current ? "✓" : " "} {branch.name}</span>
            {#if branch.upstream}<small>{branch.upstream}</small>{/if}
          </button>
        {/each}
      </div>
      {/if}
    </section>

    <section class="nav-section">
      <button class="nav-row" on:click={() => (remoteOpen = !remoteOpen)}>
        <span>{remoteOpen ? "⌄" : "›"} ☁ REMOTE</span>
        <strong>{state?.remotes.length ?? 0}</strong>
      </button>
      {#if remoteOpen}
        <div class="branch-list">
          {#each state?.remotes ?? [] as remote}
            <button on:click={() => (notice = `Remote ${remote}`)}><span>☁ {remote}</span></button>
          {/each}
        </div>
      {/if}
    </section>
    <section class="nav-section">
      <button class="nav-row" on:click={() => (worktreesOpen = !worktreesOpen)}>
        <span>{worktreesOpen ? "⌄" : "›"} ⎇ WORKTREES</span><strong>{state?.worktrees.length ?? 0}</strong>
      </button>
      {#if worktreesOpen}
        <div class="branch-list">
          {#each state?.worktrees ?? [] as worktree}
            <button on:click={() => openRepositoryPath(worktree)}><span>⎇ {worktree}</span></button>
          {/each}
        </div>
      {/if}
    </section>
  </aside>
  {/if}

  <section class="graph-area">
    {#if error}<div class="message error">{error}</div>{/if}
    {#if notice}<div class="message ok">{notice}</div>{/if}
    {#if searchOpen}
      <div class="message search-banner">
        <span>Search files and refs</span>
        <input aria-label="Search files and refs" bind:value={searchQuery} placeholder="Type to filter files, branches, and remotes" />
      </div>
    {/if}

    {#if centerMode === "launchpad"}
      <div class="launchpad">
        <h1>Repositories</h1>
        <div class="launch-actions">
          <button on:click={openFromPicker}>▰ Open</button>
          <button on:click={openClonePrompt}>☁ Clone</button>
          <button on:click={openCreatePrompt}>⊞ Create</button>
        </div>
        <section class="recent-repos">
          <h2>Recent</h2>
          {#each recentRepos as repo}
            <button on:click={() => openRepositoryPath(repo.path)}>
              <strong>{repo.name}</strong>
              <span>{repo.path.replace("/Users/dillon", "~")}</span>
            </button>
          {/each}
        </section>
      </div>
    {:else if centerMode === "file" && selectedFile}
      <div class="file-diff-shell">
        <header class="file-diff-header">
          <div class="file-diff-name">
            <span>✎</span>
            <strong>{selectedFile.path}</strong>
          </div>
          <div class="file-diff-actions">
            <span>UTF-8</span>
            <button class="stage-file" on:click={fileStageAction}>
              {selectedFile.group === "staged" ? "Unstage File" : "Stage File"}
            </button>
            <button title="Close file diff" on:click={() => (centerMode = "graph")}>×</button>
          </div>
        </header>

        <div class="file-diff-toolbar">
          <div class="segmented">
            <button class:active={selectedFile.group !== "staged"} on:click={() => selectedFile && openFile({ ...selectedFile, group: "unstaged" })}>
              Unstaged
            </button>
            <button class:active={selectedFile.group === "staged"} on:click={() => selectedFile && openFile({ ...selectedFile, group: "staged" })}>
              Staged
            </button>
          </div>
          <div class="segmented">
            <button class:active={fileViewMode === "file"} on:click={() => setFileViewMode("file")}>File View</button>
            <button class:active={fileViewMode === "diff"} on:click={() => setFileViewMode("diff")}>Diff View</button>
          </div>
          <div class="diff-tool-group">
            <button class:active={fileViewMode === "blame"} on:click={() => setFileViewMode("blame")}>Blame</button>
            <button class:active={fileViewMode === "history"} on:click={() => setFileViewMode("history")}>History</button>
            <button title="Previous hunk" on:click={() => moveHunk(-1)}>↑</button>
            <button title="Next hunk" on:click={() => moveHunk(1)}>↓</button>
            <button title="Unified" class:active={!splitDiff} on:click={() => (splitDiff = false)}>▣</button>
            <button title="Split" class:active={splitDiff} on:click={() => (splitDiff = true)}>▥</button>
          </div>
        </div>

        <div class="hunk-toolbar">
          <button class="danger" on:click={() => applySelectedHunk("discard")}>
            Discard Hunk
          </button>
          <button class="stage-file" on:click={() => applySelectedHunk(selectedFile?.group === "staged" ? "unstage" : "stage")}>
            {selectedFile.group === "staged" ? "Unstage Hunk" : "Stage Hunk"}
          </button>
        </div>

        {#if fileViewMode === "diff" && selectedDiff?.diff}
          <div class="diff-table" class:split-diff={splitDiff}>
            {#each diffRows as row}
              <div
                class:hunk-row={row.kind === "hunk"}
                class:selected-hunk={row.kind === "hunk" && hunkRows[selectedHunk]?.index === diffRows.indexOf(row)}
                class:add-row={row.kind === "add"}
                class:del-row={row.kind === "del"}
                class="diff-line"
              >
                <span class="line-no">{row.oldNo}</span>
                <span class="line-no">{row.newNo}</span>
                <code>{row.text}</code>
              </div>
            {/each}
          </div>
        {:else if fileViewMode !== "diff"}
          <pre class="file-text-view">{fileText || "No content available."}</pre>
        {:else}
          <p class="diff-empty main-empty">No text diff available.</p>
        {/if}
      </div>
    {:else}
      <div class="graph-head">
        <span>BRANCH / TAG</span>
        <span>GRAPH</span>
        <span>COMMIT MESSAGE</span>
      </div>
      <div class="graph-scroll">
        <div class="wip-row">
          <div class="branch-chip">✓ {currentBranch}</div>
          <div class="wip-graph">
            <span class="wip-rail"></span>
            <span class="wip-node"></span>
          </div>
          <div class="wip-summary">
            <button class="wip-message" on:click={() => (selectedCommit = null)}>
              <strong>// WIP</strong>
            </button>
            <span class="wip-count" title={`${totalChanges} WIP file changes`}>
              <span class="wip-pencil">✎</span>
              <strong>{totalChanges}</strong>
            </span>
          </div>
        </div>
        {#each graphRows as row}
          <button class="commit-row" class:active={selectedCommit?.hash === row.commit.hash} on:click={() => (selectedCommit = row.commit)}>
            <span class="branch-cell">
              {#each row.labels as label}
                <span class="ref-pill" style={`--ref-color:${row.color}`}>{label}</span>
              {/each}
            </span>
            <span class="graph-cell" style={`--lane-count:${graphLaneCount}`}>
              {#each row.lanes as lane}
                <span class="graph-rail" style={`--lane:${lane.index}; --lane-color:${lane.color}`}></span>
              {/each}
              {#each row.edges as edge}
                <span
                  class="graph-edge"
                  style={`--from:${Math.min(edge.from, edge.to)}; --span:${Math.abs(edge.to - edge.from) || 1}; --lane-color:${edge.color}`}
                ></span>
              {/each}
              <span class="commit-dot" style={`--lane:${row.lane}; --lane-color:${row.color}`}>{authorInitials(row.commit.author)}</span>
            </span>
            <span class="commit-main">
              <strong>
                <span>{row.commit.subject}</span>
                {#if row.commit.bodySummary}<em>{row.commit.bodySummary}</em>{/if}
              </strong>
              <small>{row.commit.shortHash} · {row.commit.author} · {row.commit.relativeDate}{row.labels.length ? ` · ${row.labels.join(" ")}` : ""}</small>
            </span>
            <span class="refs">{row.labels.join(" ")}</span>
          </button>
        {:else}
          <p class="empty centered">No commits yet</p>
        {/each}
      </div>
    {/if}
  </section>

  {#if centerMode !== "launchpad"}
  <aside class="right-panel">
    <div class="changes-title">
      <button
        class="trash danger"
        title="Discard selected"
        on:click={() => selectedFile && execute({ kind: "discard", path: selectedFile.path }, "Discard selected file")}
        disabled={!selectedFile}
      >⌫</button>
      <strong>{totalChanges} file changes on <span>{currentBranch}</span></strong>
      <button title="Refresh" on:click={refresh} disabled={busy}>✦</button>
    </div>
    <div class="changes-tools">
      <button on:click={() => (sortAsc = !sortAsc)}>↕ {sortAsc ? "A Z" : "Z A"}</button>
      <div class="segmented">
        <button class:active={rightTab === "path"} on:click={() => (rightTab = "path")}>☰ Path</button>
        <button class:active={rightTab === "tree"} on:click={() => (rightTab = "tree")}>⌘ Tree</button>
      </div>
    </div>

    <div class="change-list">
      <div class="change-group-head">
        <button class="section-toggle" on:click={() => (unstagedOpen = !unstagedOpen)}>
          <span>{unstagedOpen ? "⌄" : "›"} Unstaged Files ({unstaged.length + untracked.length + conflicted.length})</span>
        </button>
        <button on:click={() => execute({ kind: "stage", path: "." }, "Stage all changes")} disabled={busy || totalChanges === staged.length}>
          Stage All Changes
        </button>
      </div>
      {#if unstagedOpen}
        <FileGroup title="Conflicted" files={visibleUnstaged.filter((file) => file.group === "conflicted")} open={openConflict} action={fileAction} />
        <FileGroup title="Unstaged" files={visibleUnstaged.filter((file) => file.group === "unstaged")} open={openFile} action={fileAction} selectedPath={selectedFile?.path} />
        <FileGroup title="Untracked" files={visibleUnstaged.filter((file) => file.group === "untracked")} open={openFile} action={fileAction} selectedPath={selectedFile?.path} />
      {/if}
      <div class="change-group-head compact">
        <button class="section-toggle" on:click={() => (stagedOpen = !stagedOpen)}>
          <span>{stagedOpen ? "⌄" : "›"} Staged Files ({staged.length})</span>
        </button>
        <button on:click={() => execute({ kind: "unstage", path: "." }, "Unstage all changes")} disabled={busy || staged.length === 0}>
          Unstage All
        </button>
      </div>
      {#if stagedOpen}
        <FileGroup title="Staged" files={visibleStaged} open={openFile} action={fileAction} selectedPath={selectedFile?.path} />
      {/if}
    </div>

    <div class="commit-panel">
      <div class="commit-tab">⌁ Commit</div>
      <label class="checkbox"><input type="checkbox" bind:checked={amendCommit} /> Amend previous commit</label>
      <label class="commit-input" for="commit-message">
        <input id="commit-message" bind:value={commitMessage} maxlength="72" placeholder="Commit summary" />
        <small>{72 - commitMessage.length}</small>
      </label>
      <textarea class="description" bind:value={commitDescription} placeholder="Description"></textarea>
      <details>
        <summary>Commit options</summary>
        <div class="field">
          <label for="branch-name">Branch</label>
          <input id="branch-name" bind:value={branchName} placeholder="feature/name" />
        </div>
        <button on:click={() => execute({ kind: "createBranch", branch: branchName }, "Create branch")} disabled={busy || !branchName.trim()}>
          Create Branch
        </button>
        <div class="field">
          <label for="command-target">Target</label>
          <input id="command-target" bind:value={commandTarget} placeholder="branch, commit, stash@{0}" />
        </div>
        <select bind:value={resetMode} aria-label="Reset mode">
          <option value="soft">soft</option>
          <option value="mixed">mixed</option>
          <option value="hard">hard</option>
        </select>
        <div class="mini-actions">
          <button on:click={() => execute({ kind: "merge", target: commandTarget }, "Merge")} disabled={busy || !commandTarget.trim()}>Merge</button>
          <button on:click={() => execute({ kind: "rebase", target: commandTarget }, "Rebase")} disabled={busy || !commandTarget.trim()}>Rebase</button>
          <button on:click={() => execute({ kind: "cherryPick", target: commandTarget }, "Cherry-pick")} disabled={busy || !commandTarget.trim()}>
            Cherry-pick
          </button>
          <button class="danger" on:click={() => execute({ kind: "reset", target: commandTarget, mode: resetMode }, "Reset")} disabled={busy || !commandTarget.trim()}>
            Reset
          </button>
        </div>
      </details>
      <button
        class="commit-button"
        on:click={() => execute({ kind: amendCommit ? "commitAmend" : "commit", message: fullCommitMessage }, amendCommit ? "Amend commit" : "Commit")}
        disabled={busy || !commitMessage.trim() || staged.length === 0}
      >
        Commit Staged Changes
      </button>
    </div>
  </aside>
  {/if}

  {#if conflict}
    <section class="merge-editor">
      <div class="panel-title">
        <h1>Merge Editor: {conflict.path}</h1>
        <div class="button-row">
          <button on:click={() => useVersion(conflict?.ours)}>Use Ours</button>
          <button on:click={() => useVersion(conflict?.theirs)}>Use Theirs</button>
          <button on:click={() => useVersion(conflict?.base)}>Use Base</button>
          <button on:click={saveResolution} disabled={busy}>Save Resolution</button>
          <button on:click={() => execute({ kind: "markResolved", path: conflict?.path }, "Mark resolved")} disabled={busy}>
            Mark Resolved
          </button>
        </div>
      </div>
      <div class="merge-grid">
        <ReadonlyPane title="Base" content={conflict.base ?? ""} />
        <ReadonlyPane title="Ours" content={conflict.ours ?? ""} />
        <ReadonlyPane title="Theirs" content={conflict.theirs ?? ""} />
        <div class="merge-pane resolved">
          <h2>Resolved</h2>
          <textarea bind:value={resolvedContent} spellcheck="false"></textarea>
        </div>
      </div>
    </section>
  {/if}

  <footer class="status-bar">
    <span>⌁ {repoName}</span>
    <span>{busy ? "Running git command..." : selectedCommit?.shortHash ?? "Ready"}</span>
    <span>100%</span>
  </footer>
</main>
