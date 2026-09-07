<script lang="ts">
  import {
    applyHunk,
    createRepository,
    getCommitDetail,
    getCommitFileDiff,
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
  import DiffTable from "./lib/DiffTable.svelte";
  // F1: component import (lazy-loaded at the mount anchor below; see REVIEW-PERF.md 2.3)
  import FileGroup from "./lib/FileGroup.svelte";
  import ReadonlyPane from "./lib/ReadonlyPane.svelte";
  import { parseDiffRows } from "./lib/diffRows";
  // F3: component import
  import type {
    CommitDetail,
    CommitFileChange,
    CommitNode,
    ConflictFile,
    FileDiff,
    FileStatus,
    GitAction,
    RepositoryState,
    Worktree,
  } from "./lib/types";
  // F2: component import

  type AppTab = {
    id: string;
    kind: "launchpad" | "repo";
    label: string;
    path?: string;
  };
  // F4: component import

  type GraphLane = {
    index: number;
    color: string;
    capStart: boolean;
    capEnd: boolean;
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

  type RecentRepo = { name: string; path: string };

  type Settings = {
    confirmRisky: boolean;
    clonePath: string;
    graphLimit: number;
    staleDays: number;
  };

  const defaultSettings: Settings = {
    confirmRisky: true,
    clonePath: "/Users/dillon/Documents/dev",
    graphLimit: 250,
    staleDays: 30,
  };

  const seedRecentRepos: RecentRepo[] = [
    { name: "gitc", path: "/Users/dillon/Documents/dev/gitc" },
    { name: "meetings", path: "/Users/dillon/Documents/dev/meetings" },
    { name: "data-layer", path: "/Users/dillon/Documents/dev/data-layer" },
    { name: "waas", path: "/Users/dillon/Documents/dev/waas" },
    { name: "nested", path: "/Users/dillon/Documents/dev/nested" },
    { name: "LandLocked", path: "/Users/dillon/Documents/dev/LandLocked" },
    { name: "RobertaRoyale", path: "/Users/dillon/Documents/dev/RobertaRoyale" },
    { name: "otc-api", path: "/Users/dillon/Documents/dev/otc-api" },
  ];

  let state: RepositoryState | null = null;
  let tabs: AppTab[] = [{ id: "launchpad", kind: "launchpad", label: "Launchpad" }];
  let activeTabId = "launchpad";
  let commits: CommitNode[] = [];
  let selectedCommit: CommitNode | null = null;
  let commitDetail: CommitDetail | null = null;
  let commitDetailBusy = false;
  let selectedFile: FileStatus | null = null;
  let selectedDiff: FileDiff | null = null;
  let diffContext: "worktree" | "commit" = "worktree";
  let commitFilePath = "";
  let centerMode: "graph" | "file" | "launchpad" | "compare" = "graph";
  let fileViewMode: "diff" | "file" | "blame" | "history" = "diff";
  let fileText = "";
  let splitDiff = false;
  let selectedHunk = 0;
  let conflict: ConflictFile | null = null;
  // F2: compare state
  let compareBase: string | null = null;
  let compareHead = "";
  let resolvedContent = "";
  let commitMessage = "";
  let commitDescription = "";
  let amendCommit = false;
  let commandTarget = "";
  let branchName = "";
  let resetMode = "mixed";
  // F4: rebase state
  let rebaseOpen = false;
  let rebaseMode: "interactive" | "plain" = "interactive";
  let rebaseBase: string | null = null;
  let rightTab: "path" | "tree" = "path";
  let searchOpen = false;
  let searchQuery = "";
  let sortAsc = true;
  let localOpen = true;
  let remoteOpen = false;
  let stashesOpen = true;
  let tagsOpen = false;
  let worktreesOpen = true;
  // F1: cleanup state
  let cleanupOpen = false;
  let unstagedOpen = true;
  let stagedOpen = true;
  let actionsOpen = false;
  let settingsOpen = false;
  let settings: Settings = loadSettings();
  let recentRepos: RecentRepo[] = loadRecentRepos();
  let busy = false;
  let error = "";
  let notice = "";
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;
  let filterInput: HTMLInputElement | null = null;
  // F3: clone dialog state
  let cloneOpen = false;

  // Success toasts dismiss themselves; errors stay until addressed.
  $: if (notice) {
    if (noticeTimer) clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ""), 3200);
  }

  const riskyActions = new Set([
    "discard",
    "reset",
    "forcePush",
    "stashDrop",
    "deleteBranch",
    "deleteBranchForce",
    "cleanUntracked",
    "rebase",
    "merge",
  ]);
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

  $: staged = grouped(state, "staged");
  $: unstaged = grouped(state, "unstaged");
  $: untracked = grouped(state, "untracked");
  $: conflicted = grouped(state, "conflicted");
  $: currentBranch = state?.currentBranch || "detached";
  $: totalChanges = state?.files.length ?? 0;
  $: repoName = state?.root.split("/").filter(Boolean).at(-1) ?? "gitc";
  $: accountName = state?.userName?.trim() || "Local";
  $: fullCommitMessage = commitDescription.trim()
    ? `${commitMessage.trim()}\n\n${commitDescription.trim()}`
    : commitMessage.trim();
  $: visibleUnstaged = sortFiles([...unstaged, ...untracked, ...conflicted]);
  $: visibleStaged = sortFiles(staged);
  $: diffRows = parseDiffRows(selectedDiff?.diff ?? "");
  $: hunkRows = diffRows.map((row, index) => ({ row, index })).filter((item) => item.row.kind === "hunk");
  $: graphRows = buildGraphRows(commits);
  $: visibleGraphRows = filterGraphRows(graphRows, searchOpen ? searchQuery : "");
  $: graphLaneCount = Math.max(8, ...graphRows.flatMap((row) => row.lanes.map((lane) => lane.index + 1)), 1);
  $: filteredBranches = (state?.branches ?? []).filter(
    (branch) => !searchQuery.trim() || branch.name.toLowerCase().includes(searchQuery.trim().toLowerCase()),
  );
  $: filteredRemoteBranches = (state?.remoteBranches ?? []).filter(
    (branch) => !searchQuery.trim() || branch.toLowerCase().includes(searchQuery.trim().toLowerCase()),
  );

  function loadSettings(): Settings {
    try {
      return { ...defaultSettings, ...JSON.parse(localStorage.getItem("gitc:settings") ?? "{}") };
    } catch {
      return { ...defaultSettings };
    }
  }

  function saveSettings() {
    settings.graphLimit = Math.min(1000, Math.max(25, Math.round(Number(settings.graphLimit)) || 250));
    settings.staleDays = Math.min(3650, Math.max(1, Math.round(Number(settings.staleDays)) || 30));
    try {
      localStorage.setItem("gitc:settings", JSON.stringify(settings));
    } catch {
      /* localStorage unavailable */
    }
    settingsOpen = false;
    notice = "Settings saved";
    void refresh();
  }

  function loadRecentRepos(): RecentRepo[] {
    try {
      const stored = JSON.parse(localStorage.getItem("gitc:recentRepos") ?? "null");
      if (Array.isArray(stored) && stored.length) return stored;
    } catch {
      /* fall through to seeds */
    }
    return seedRecentRepos;
  }

  function rememberRepo(path: string, name: string) {
    recentRepos = [{ name, path }, ...recentRepos.filter((repo) => repo.path !== path)].slice(0, 12);
    try {
      localStorage.setItem("gitc:recentRepos", JSON.stringify(recentRepos));
    } catch {
      /* localStorage unavailable */
    }
  }

  async function refresh() {
    busy = true;
    error = "";
    try {
      const [nextState, graph] = await Promise.all([getRepositoryState(), getCommitGraph(settings.graphLimit)]);
      state = nextState;
      syncRepoTab(nextState.root, nextState.root.split("/").filter(Boolean).at(-1) ?? "repo");
      commits = graph.commits;
      const kept = selectedCommit
        ? graph.commits.find((commit) => commit.hash === selectedCommit?.hash) ?? null
        : null;
      selectedCommit = kept;
      if (kept) {
        await loadCommitDetail(kept.hash);
      } else {
        commitDetail = null;
      }
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
    rememberRepo(path, label);
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
    if (
      riskyActions.has(action.kind) &&
      settings.confirmRisky &&
      !confirm(`${label} can rewrite or discard repository state. Continue?`)
    ) {
      return;
    }

    actionsOpen = false;
    busy = true;
    error = "";
    notice = "";
    try {
      const result = await runGitAction(action);
      // refresh() unconditionally clears `error` at its start, so a failure
      // message would otherwise be wiped the instant it's set. Re-apply it
      // after refresh() runs so the toast is actually shown.
      let failure = "";
      if (!result.ok) {
        failure = result.stderr || result.stdout || `${label} failed`;
      } else {
        notice = `${label} complete`;
      }
      if (result.refresh) await refresh();
      if (failure) error = failure;
      if (selectedFile && action.path === selectedFile.path && diffContext === "worktree") {
        const stillThere = state?.files.some(
          (file) => file.path === selectedFile?.path && normalizedGroup(file) === selectedFile?.group,
        );
        if (stillThere) await openFile(selectedFile);
        else closeFileView();
      }
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function loadCommitDetail(hash: string) {
    commitDetailBusy = true;
    try {
      commitDetail = await getCommitDetail(hash);
    } catch (err) {
      commitDetail = null;
      error = String(err);
    } finally {
      commitDetailBusy = false;
    }
  }

  async function selectCommit(commit: CommitNode | null) {
    selectedCommit = commit;
    commitDetail = null;
    if (!commit) return;
    await loadCommitDetail(commit.hash);
  }

  async function openCommitFile(change: CommitFileChange) {
    if (!commitDetail) return;
    busy = true;
    error = "";
    try {
      selectedDiff = await getCommitFileDiff(commitDetail.hash, change.path);
      diffContext = "commit";
      commitFilePath = change.path;
      selectedFile = null;
      selectedHunk = 0;
      fileViewMode = "diff";
      centerMode = "file";
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function closeFileView() {
    centerMode = "graph";
    selectedFile = null;
    selectedDiff = null;
    diffContext = "worktree";
    commitFilePath = "";
    compareHead = "";
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
    diffContext = "worktree";
    commitFilePath = "";
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

  async function openCreatePrompt() {
    const path = prompt("Create repository at path", `${settings.clonePath.replace(/\/$/, "")}/new-repo`);
    if (!path?.trim()) return;
    busy = true;
    error = "";
    try {
      state = await createRepository(path.trim());
      const graph = await getCommitGraph(settings.graphLimit);
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
  // F3: clone helpers
  async function afterClone(next: RepositoryState) {
    // Close first: once `cloneRepository` has succeeded the dialog's job is
    // done, and any failure below (e.g. the graph reload) should surface on
    // App's own error banner rather than reopen a "clone failed" state in a
    // dialog that already reported success.
    cloneOpen = false;
    state = next;
    try {
      const graph = await getCommitGraph(settings.graphLimit);
      commits = graph.commits;
      selectedFile = null;
      selectedDiff = null;
      centerMode = "graph";
      syncRepoTab(state.root, state.root.split("/").filter(Boolean).at(-1) ?? "repo");
      notice = `Cloned ${state.root}`;
    } catch (err) {
      error = String(err);
    }
  }

  async function openRepositoryPath(path: string, silent = false) {
    busy = true;
    error = "";
    notice = "";
    try {
      state = await setRepositoryPath(path);
      const graph = await getCommitGraph(settings.graphLimit);
      commits = graph.commits;
      selectedFile = null;
      selectedDiff = null;
      selectedCommit = null;
      commitDetail = null;
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
      .sort((a, b) => (sortAsc ? a.path.localeCompare(b.path) : b.path.localeCompare(a.path)));
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

  function laneColor(index: number) {
    return graphColors[index % graphColors.length];
  }

  function refLabels(commit: CommitNode) {
    return commit.refs
      .map((ref) => ref.replace(/^HEAD -> /, "").replace(/^tag: /, "tag:"))
      .filter((ref) => ref && !ref.includes("origin/HEAD"));
  }

  function authorInitials(author: string) {
    return (
      author
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 2)
        .map((part) => part[0]?.toUpperCase())
        .join("") || "G"
    );
  }

  function buildGraphRows(nodes: CommitNode[]): GraphRow[] {
    const lanes: string[] = [];
    const rows: GraphRow[] = [];

    for (const commit of nodes) {
      let lane = lanes.indexOf(commit.hash);
      let laneIsNew = false;
      if (lane === -1) {
        lane = lanes.findIndex((value) => value === "");
        if (lane === -1) lane = lanes.length;
        lanes[lane] = commit.hash;
        laneIsNew = true;
      }

      const firstParent = commit.parents[0] ?? "";
      const visibleLanes = lanes
        .map((hash, index) => ({ hash, index }))
        .filter((item) => item.hash)
        .map((item) => ({
          index: item.index,
          color: laneColor(item.index),
          // The topmost lane-0 commit stays uncapped so the WIP connector reaches its dot.
          capStart: item.index === lane && laneIsNew && !(rows.length === 0 && lane === 0),
          capEnd: item.index === lane && !firstParent,
        }));
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

  function filterGraphRows(rows: GraphRow[], query: string) {
    const trimmed = query.trim().toLowerCase();
    if (!trimmed) return rows;
    return rows.filter((row) => {
      const haystack = [
        row.commit.subject,
        row.commit.bodySummary,
        row.commit.author,
        row.commit.hash,
        row.commit.shortHash,
        ...row.labels,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(trimmed);
    });
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

  function createTagPrompt(target?: string) {
    const name = prompt("Tag name");
    if (!name?.trim()) return;
    execute({ kind: "createTag", branch: name.trim(), target: target ?? null }, `Tag ${name.trim()}`);
  }
  // F2: compare helpers

  function openCompare(base: string | null, head: string) {
    compareBase = base;
    compareHead = head;
    centerMode = "compare";
  }

  function commitRowTitle(row: GraphRow): string {
    if (selectedCommit && selectedCommit.hash !== row.commit.hash) {
      return `Shift-click to compare with ${selectedCommit.shortHash}`;
    }
    return row.labels.join("  ");
  }

  function handleCommitRowClick(event: MouseEvent, row: GraphRow, rowIndex: number) {
    if (event.shiftKey && selectedCommit && selectedCommit.hash !== row.commit.hash) {
      const selectedIndex = visibleGraphRows.findIndex((item) => item.commit.hash === selectedCommit?.hash);
      // Rows are newest-first, so a larger index means an older commit; the
      // older commit always becomes the base or every diff comes out reversed.
      const older = selectedIndex > rowIndex ? selectedCommit : row.commit;
      const newer = selectedIndex > rowIndex ? row.commit : selectedCommit;
      openCompare(older.hash, newer.hash);
      return;
    }
    selectCommit(row.commit);
  }

  function createBranchAtCommit(hash: string) {
    const name = prompt("Branch name for this commit");
    if (!name?.trim()) return;
    execute({ kind: "createBranch", branch: name.trim(), target: hash }, `Create ${name.trim()} at ${hash.slice(0, 8)}`);
  }

  function shortWorktreePath(path: string) {
    const parts = path.split("/").filter(Boolean);
    return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : path;
  }

  function worktreeLabel(worktree: Worktree) {
    return worktree.branch ?? `${worktree.head} (detached)`;
  }

  function addWorktreePrompt() {
    const branch = prompt("Branch for the new worktree (an existing name checks it out, a new name creates it)");
    if (!branch?.trim()) return;
    const name = branch.trim();
    const rootParts = (state?.root ?? "").split("/").filter(Boolean);
    const repoName = rootParts.at(-1) ?? "repo";
    const parent = rootParts.slice(0, -1).join("/");
    const suggested = `/${parent ? `${parent}/` : ""}${repoName}-${name.replace(/[^\w.-]+/g, "-")}`;
    const path = prompt("Create worktree at path", suggested);
    if (!path?.trim()) return;
    const exists = state?.branches.some((entry) => entry.name === name) ?? false;
    execute(
      { kind: "worktreeAdd", path: path.trim(), branch: name, mode: exists ? "checkout" : "new" },
      `Add worktree for ${name}`,
    );
  }

  async function removeWorktree(worktree: Worktree) {
    if (
      settings.confirmRisky &&
      !confirm(`Remove the worktree at ${worktree.path}? Its checkout directory will be deleted.`)
    ) {
      return;
    }
    busy = true;
    error = "";
    notice = "";
    try {
      let result = await runGitAction({ kind: "worktreeRemove", path: worktree.path });
      if (
        !result.ok &&
        /modified or untracked files|locked working tree|use --force/i.test(result.stderr) &&
        confirm(`${result.stderr.trim()}\n\nForce remove ${worktree.path}?`)
      ) {
        result = await runGitAction({ kind: "worktreeRemoveForce", path: worktree.path });
      }
      if (!result.ok) {
        error = result.stderr || result.stdout || "Remove worktree failed";
      } else {
        notice = `Removed worktree ${worktree.path}`;
      }
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
    await refresh();
  }
  // F1: cleanup helpers

  async function deleteBranch(name: string) {
    busy = true;
    error = "";
    notice = "";
    try {
      let result = await runGitAction({ kind: "deleteBranch", branch: name });
      if (
        !result.ok &&
        /not fully merged/i.test(result.stderr) &&
        confirm(`${result.stderr.trim()}\n\nDelete ${name} anyway? Its tip stays recoverable for about two weeks.`)
      ) {
        result = await runGitAction({ kind: "deleteBranchForce", branch: name });
      }
      if (!result.ok) {
        error = result.stderr || result.stdout || "Delete branch failed";
      } else {
        notice = `Deleted ${name}`;
      }
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
    await refresh();
  }

  // post-merge: open RebasePanel in plain mode
  function rebaseOnto(target: string) {
    if (settings.confirmRisky && !confirm(`Rebase ${currentBranch} onto ${target}?`)) return;
    void execute({ kind: "rebase", target }, `Rebase onto ${target}`);
  }

  async function copyHash(hash: string) {
    try {
      await navigator.clipboard.writeText(hash);
      notice = `Copied ${hash.slice(0, 8)}`;
    } catch {
      notice = hash;
    }
  }
  function openRebasePanel(rebaseModeArg: "interactive" | "plain", base: string | null = null) {
    rebaseMode = rebaseModeArg;
    rebaseBase = base;
    rebaseOpen = true;
    actionsOpen = false;
  }
  // F4: rebase helpers

  function showAddRepoNotice() {
    activeTabId = "launchpad";
    centerMode = "launchpad";
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
    if (mode === "discard" && settings.confirmRisky && !confirm("Discard hunk? This cannot be undone.")) return;
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
    actionsOpen = false;
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

  function statusLabel(status: string) {
    const map: Record<string, string> = { A: "added", M: "modified", D: "deleted", R: "renamed", C: "copied" };
    return map[status] ?? status;
  }

  refresh();
</script>

<svelte:window
  on:keydown={(event) => {
    if (event.key === "Escape") {
      settingsOpen = false;
      actionsOpen = false;
    }
    if ((event.metaKey || event.ctrlKey) && event.altKey && event.code === "KeyF") {
      event.preventDefault();
      filterInput?.focus();
      filterInput?.select();
    }
  }}
  on:click={(event) => {
    if (actionsOpen && !(event.target instanceof Element && event.target.closest(".search-actions"))) {
      actionsOpen = false;
    }
  }}
/>

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
      <button title="Settings" on:click={() => (settingsOpen = true)}>⚙</button>
      <strong>{accountName}</strong>
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
        <button title="Fetch" on:click={() => execute({ kind: "fetch" }, "Fetch")} disabled={busy}>⇣<span>Fetch</span></button>
        <button title="Pull" on:click={() => execute({ kind: "pull" }, "Pull")} disabled={busy}>⇩<span>Pull</span></button>
        <button title="Push" on:click={() => execute({ kind: "push" }, "Push")} disabled={busy}>⇧<span>Push</span></button>
        <button title="Branch" on:click={createBranchFromToolbar}>⑂<span>Branch</span></button>
        <button title="Compare refs (shift-click two commits in the graph)" on:click={() => openCompare(null, currentBranch)}>⇄<span>Compare</span></button>
        <button title="Stash" on:click={() => execute({ kind: "stashCreate", message: "gitc stash" }, "Create stash")} disabled={busy}>▤<span>Stash</span></button>
        <button title="Terminal" on:click={openRepoTerminal} disabled={busy}>⌁<span>Terminal</span></button>
      </div>
      <div class="search-actions">
        <button title="Actions" class:active={actionsOpen} on:click={() => (actionsOpen = !actionsOpen)}>☷<span>Actions</span></button>
        <button title="Search" on:click={() => (searchOpen = !searchOpen)}>⌕<span>Search</span></button>
        {#if actionsOpen}
          <div class="dropdown-menu" role="menu">
            <button on:click={() => execute({ kind: "fetchAll" }, "Fetch all remotes")}>Fetch All &amp; Prune</button>
            <button on:click={() => execute({ kind: "forcePush" }, "Force push")}>Force Push (with lease)</button>
            <button on:click={() => openRebasePanel("interactive")}>Interactive Rebase…</button>
            <button on:click={() => openRebasePanel("plain")}>Rebase onto…</button>
            <button on:click={() => createTagPrompt()}>Create Tag at HEAD…</button>
            <button
              on:click={() => {
                actionsOpen = false;
                const message = prompt("Stash message", "gitc stash");
                if (message != null) execute({ kind: "stashCreate", message: message.trim() || "gitc stash" }, "Create stash");
              }}
            >
              Stash With Message…
            </button>
            <button on:click={openRepoTerminal}>Open Terminal Here</button>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if centerMode !== "launchpad"}
    <aside class="left-panel">
      <div class="panel-header">
        <span class="panel-title">Repository</span>
        <span class="panel-count">{(state?.branches.length ?? 0) + (state?.remoteBranches.length ?? 0)} refs</span>
      </div>
      <div class="filter-block">
        <input
          aria-label="Filter refs"
          bind:this={filterInput}
          bind:value={searchQuery}
          placeholder="Filter refs (⌘ + Option + F)"
        />
      </div>

      <div class="nav-scroll">
        <section class="nav-section">
          <div class="section-head">
            <button class="nav-row" on:click={() => (worktreesOpen = !worktreesOpen)}>
              <span>{worktreesOpen ? "⌄" : "›"} ⧉ WORKTREES</span>
              <strong>{state?.worktrees.length ?? 0}</strong>
            </button>
            {#if (state?.worktrees ?? []).some((entry) => entry.prunable)}
              <button
                class="section-action"
                title="Prune worktrees whose directories are gone"
                on:click={() => execute({ kind: "worktreePrune" }, "Prune worktrees")}
                disabled={busy}
              >prune</button>
            {/if}
            <button class="section-action" title="Add worktree…" on:click={addWorktreePrompt} disabled={busy}>+</button>
          </div>
          {#if worktreesOpen}
            <div class="branch-list">
              {#each state?.worktrees ?? [] as worktree (worktree.path)}
                <div class="worktree-row" class:active={worktree.current} class:stale={worktree.prunable}>
                  <button
                    class="worktree-main"
                    title={worktree.current ? worktree.path : `Switch to ${worktree.path}`}
                    on:click={() => openRepositoryPath(worktree.path)}
                    disabled={busy || worktree.current}
                  >
                    <span class="wt-dot"></span>
                    <span class="wt-label">
                      <span class="wt-name">
                        <span class="wt-branch">{worktreeLabel(worktree)}</span>
                        {#if worktree.main}<em>main</em>{/if}
                        {#if worktree.locked}<em title={worktree.lockReason ?? "locked"}>locked</em>{/if}
                        {#if worktree.prunable}<em>stale</em>{/if}
                      </span>
                      <small class="wt-path">{shortWorktreePath(worktree.path)}</small>
                    </span>
                  </button>
                  {#if !worktree.main && !worktree.current}
                    <button
                      class="row-action danger"
                      title={`Remove worktree ${worktree.path}`}
                      on:click={() => removeWorktree(worktree)}
                      disabled={busy}
                    >×</button>
                  {/if}
                </div>
              {:else}
                <p class="empty">No worktrees</p>
              {/each}
            </div>
          {/if}
        </section>

        <section class="nav-section">
          <div class="section-head">
            <button class="nav-row" on:click={() => (localOpen = !localOpen)}>
              <span>{localOpen ? "⌄" : "›"} ⌂ LOCAL</span>
              <strong>{state?.branches.length ?? 0}</strong>
            </button>
            <button
              class="section-action"
              title="Delete merged, squash-merged and gone branches…"
              on:click={() => (cleanupOpen = true)}
              disabled={busy}
            >clean up</button>
          </div>
          {#if localOpen}
            <div class="branch-list">
              {#each filteredBranches as branch}
                <div class="branch-row" class:active={branch.current}>
                  <button
                    class="branch-name"
                    on:click={() => execute({ kind: "checkoutBranch", branch: branch.name }, `Checkout ${branch.name}`)}
                    disabled={busy || branch.current}
                  >
                    <span>{branch.current ? "✓" : " "} {branch.name}</span>
                    {#if branch.upstream || branch.upstreamGone}
                      <small>
                        {branch.upstreamGone ? `${branch.upstream ?? "upstream"} · gone` : branch.upstream}
                        {#if branch.ahead || branch.behind}↑{branch.ahead} ↓{branch.behind}{/if}
                      </small>
                    {/if}
                  </button>
                  {#if !branch.current}
                    <button
                      class="row-action"
                      title={`Rebase ${currentBranch} onto ${branch.name}`}
                      on:click={() => rebaseOnto(branch.name)}
                      disabled={busy}
                    >⤴</button>
                    <button
                      class="row-action danger"
                      title={`Delete ${branch.name}`}
                      on:click={() => deleteBranch(branch.name)}
                      disabled={busy}
                    >×</button>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </section>

        <section class="nav-section">
          <button class="nav-row" on:click={() => (remoteOpen = !remoteOpen)}>
            <span>{remoteOpen ? "⌄" : "›"} ☁ REMOTE</span>
            <strong>{state?.remoteBranches.length ?? 0}</strong>
          </button>
          {#if remoteOpen}
            <div class="branch-list">
              {#each filteredRemoteBranches as branch}
                <button
                  class="branch-name"
                  title={`Checkout tracking branch for ${branch}`}
                  on:click={() => execute({ kind: "checkoutRemote", target: branch }, `Checkout ${branch}`)}
                  disabled={busy}
                >
                  <span>☁ {branch}</span>
                </button>
              {:else}
                <p class="empty">No remote branches</p>
              {/each}
            </div>
          {/if}
        </section>

        <section class="nav-section">
          <button class="nav-row" on:click={() => (stashesOpen = !stashesOpen)}>
            <span>{stashesOpen ? "⌄" : "›"} ▤ STASHES</span>
            <strong>{state?.stashes.length ?? 0}</strong>
          </button>
          {#if stashesOpen}
            <div class="branch-list">
              {#each state?.stashes ?? [] as stash}
                <div class="stash-row">
                  <div class="stash-info" title={stash.message}>
                    <span>{stash.name}</span>
                    <small>{stash.message}</small>
                  </div>
                  <div class="stash-actions">
                    <button title="Apply stash" on:click={() => execute({ kind: "stashApply", target: stash.name }, `Apply ${stash.name}`)} disabled={busy}>Apply</button>
                    <button title="Pop stash" on:click={() => execute({ kind: "stashPop", target: stash.name }, `Pop ${stash.name}`)} disabled={busy}>Pop</button>
                    <button class="danger" title="Drop stash" on:click={() => execute({ kind: "stashDrop", target: stash.name }, `Drop ${stash.name}`)} disabled={busy}>×</button>
                  </div>
                </div>
              {:else}
                <p class="empty">No stashes</p>
              {/each}
            </div>
          {/if}
        </section>

        <section class="nav-section">
          <button class="nav-row" on:click={() => (tagsOpen = !tagsOpen)}>
            <span>{tagsOpen ? "⌄" : "›"} ⌖ TAGS</span>
            <strong>{state?.tags.length ?? 0}</strong>
          </button>
          {#if tagsOpen}
            <div class="branch-list">
              {#each state?.tags ?? [] as tag}
                <div class="branch-row">
                  <button
                    class="branch-name"
                    title={`Checkout ${tag} (detached)`}
                    on:click={() => execute({ kind: "checkoutCommit", target: tag }, `Checkout ${tag}`)}
                    disabled={busy}
                  >
                    <span>⌖ {tag}</span>
                  </button>
                  <button
                    class="row-action danger"
                    title={`Delete tag ${tag}`}
                    on:click={() => execute({ kind: "deleteTag", branch: tag }, `Delete tag ${tag}`)}
                    disabled={busy}
                  >×</button>
                </div>
              {:else}
                <p class="empty">No tags</p>
              {/each}
            </div>
          {/if}
        </section>

      </div>
    </aside>
  {/if}

  <section class="graph-area" style={`--lane-count:${graphLaneCount}`}>
    {#if busy}<div class="busy-bar" aria-hidden="true"></div>{/if}
    {#if error}
      <div class="message error" role="alert">
        <span>{error}</span>
        <button class="msg-close" title="Dismiss" on:click={() => (error = "")}>×</button>
      </div>
    {/if}
    {#if notice}
      <div class="message ok" role="status">
        <span>{notice}</span>
        <button class="msg-close" title="Dismiss" on:click={() => (notice = "")}>×</button>
      </div>
    {/if}
    {#if state?.merging || state?.rebasing}
      <div class="message conflict-banner">
        <span>
          {state.merging ? "Merge" : "Rebase"} in progress
          {#if conflicted.length}· {conflicted.length} conflicted {conflicted.length === 1 ? "file" : "files"} to resolve{/if}
        </span>
        <div class="banner-actions">
          <button
            on:click={() => execute({ kind: state?.merging ? "mergeContinue" : "rebaseContinue" }, "Continue")}
            disabled={busy || conflicted.length > 0}
          >
            Continue
          </button>
          <button
            class="danger"
            on:click={() => execute({ kind: state?.merging ? "mergeAbort" : "rebaseAbort" }, "Abort")}
            disabled={busy}
          >
            Abort
          </button>
        </div>
      </div>
    {/if}
    {#if searchOpen}
      <div class="message search-banner">
        <span>Search commits, files, and refs</span>
        <input aria-label="Search commits, files, and refs" bind:value={searchQuery} placeholder="Filter the graph by message, author, hash, or ref" />
      </div>
    {/if}

    {#if centerMode === "launchpad"}
      <div class="launchpad">
        <h1>Repositories</h1>
        <div class="launch-actions">
          <button on:click={switchRepository}>▰ Open</button>
          <button on:click={() => (cloneOpen = true)}>☁ Clone</button>
          <button on:click={openCreatePrompt}>⊞ Create</button>
        </div>
        <section class="recent-repos">
          <h2>Recent</h2>
          {#each recentRepos as repo}
            <button on:click={() => openRepositoryPath(repo.path)}>
              <strong>{repo.name}</strong>
              <span>{repo.path.replace(/^\/Users\/[^/]+/, "~")}</span>
            </button>
          {/each}
        </section>
      </div>
    <!-- F2: compare center mode -->
    {:else if centerMode === "compare"}
      {#await import("./lib/CompareView.svelte") then module}
        <svelte:component
          this={module.default}
          base={compareBase}
          head={compareHead}
          branches={state?.branches ?? []}
          remoteBranches={state?.remoteBranches ?? []}
          tags={state?.tags ?? []}
          onClose={closeFileView}
        />
      {/await}
    {:else if centerMode === "file" && diffContext === "commit"}
      <div class="file-diff-shell commit-context">
        <header class="file-diff-header">
          <div class="file-diff-name">
            <span>◉</span>
            <strong>{commitFilePath}</strong>
            <em class="at-commit">at {commitDetail?.shortHash ?? selectedDiff?.path ?? ""}</em>
          </div>
          <div class="file-diff-actions">
            <button title="Unified" class:active={!splitDiff} on:click={() => (splitDiff = false)}>▣</button>
            <button title="Split" class:active={splitDiff} on:click={() => (splitDiff = true)}>▥</button>
            <button title="Close commit diff" on:click={closeFileView}>×</button>
          </div>
        </header>
        {#if selectedDiff?.diff}
          <DiffTable rows={diffRows} split={splitDiff} />
        {:else}
          <p class="diff-empty main-empty">No text diff available.</p>
        {/if}
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
            <button title="Close file diff" on:click={closeFileView}>×</button>
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
            <button class:active={fileViewMode === "diff"} on:click={() => setFileViewMode("diff")}>Diff</button>
            <button class:active={fileViewMode === "file"} on:click={() => setFileViewMode("file")}>File</button>
            <button class:active={fileViewMode === "blame"} on:click={() => setFileViewMode("blame")}>Blame</button>
            <button class:active={fileViewMode === "history"} on:click={() => setFileViewMode("history")}>History</button>
          </div>
          <div class="diff-tool-group">
            <button title="Previous hunk" on:click={() => moveHunk(-1)}>↑</button>
            <button title="Next hunk" on:click={() => moveHunk(1)}>↓</button>
            <button title="Unified" class:active={!splitDiff} on:click={() => (splitDiff = false)}>▣</button>
            <button title="Split" class:active={splitDiff} on:click={() => (splitDiff = true)}>▥</button>
          </div>
        </div>

        <div class="hunk-toolbar">
          <span class="hunk-count">{hunkRows.length ? `Hunk ${selectedHunk + 1} of ${hunkRows.length}` : "No hunks"}</span>
          <button class="danger" on:click={() => applySelectedHunk("discard")} disabled={hunkRows.length === 0}>
            Discard Hunk
          </button>
          <button
            class="stage-file"
            on:click={() => applySelectedHunk(selectedFile?.group === "staged" ? "unstage" : "stage")}
            disabled={hunkRows.length === 0}
          >
            {selectedFile.group === "staged" ? "Unstage Hunk" : "Stage Hunk"}
          </button>
        </div>

        {#if fileViewMode === "diff" && selectedDiff?.diff}
          <DiffTable rows={diffRows} split={splitDiff} selectedHunkRow={hunkRows[selectedHunk]?.index ?? null} />
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
            <button class="wip-message" on:click={() => selectCommit(null)}>
              <strong>// WIP</strong>
            </button>
            <span class="wip-count" title={`${totalChanges} WIP file changes`}>
              <span class="wip-pencil">✎</span>
              <strong>{totalChanges}</strong>
            </span>
          </div>
        </div>
        {#each visibleGraphRows as row, rowIndex}
          <button
            class="commit-row"
            class:active={selectedCommit?.hash === row.commit.hash}
            title={commitRowTitle(row)}
            on:click={(event) => handleCommitRowClick(event, row, rowIndex)}
          >
            <span class="branch-cell" title={row.labels.join("  ")}>
              {#if row.labels.length}
                <span class="ref-pill" style={`--ref-color:${row.color}`}>{row.labels[0]}</span>
                {#if row.labels.length > 1}
                  <span class="ref-pill more-pill" style={`--ref-color:${row.color}`}>+{row.labels.length - 1}</span>
                {/if}
              {/if}
            </span>
            <span class="graph-cell" style={`--lane-count:${graphLaneCount}`}>
              {#each row.lanes as lane}
                <span
                  class="graph-rail"
                  class:rail-start={lane.capStart}
                  class:rail-end={lane.capEnd}
                  style={`--lane:${lane.index}; --lane-color:${lane.color}`}
                ></span>
              {/each}
              {#each row.edges as edge}
                <span
                  class="graph-edge"
                  class:edge-right={edge.to >= edge.from}
                  class:edge-left={edge.to < edge.from}
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
              <small>{row.commit.shortHash} · {row.commit.author} · {row.commit.relativeDate}</small>
            </span>
          </button>
        {:else}
          <p class="empty centered">{searchOpen && searchQuery.trim() ? "No commits match the search" : "No commits yet"}</p>
        {/each}
      </div>
    {/if}
  </section>

  {#if centerMode !== "launchpad"}
    <aside class="right-panel">
      {#if selectedCommit}
        <div class="commit-detail">
          <div class="changes-title">
            <button title="Copy hash" on:click={() => selectedCommit && copyHash(selectedCommit.hash)}>⧉</button>
            <strong>Commit <span>{selectedCommit.shortHash}</span></strong>
            <button title="Back to work in progress" on:click={() => selectCommit(null)}>×</button>
          </div>
          {#if commitDetailBusy}
            <p class="empty centered">Loading commit…</p>
          {:else if commitDetail}
            <div class="commit-detail-scroll">
              <div class="commit-meta">
                <h2>{commitDetail.subject}</h2>
                {#if commitDetail.body}<p class="commit-body">{commitDetail.body}</p>{/if}
                <div class="commit-meta-line">
                  <span class="author-badge">{authorInitials(commitDetail.author)}</span>
                  <div>
                    <strong>{commitDetail.author}</strong>
                    <small>{commitDetail.email}</small>
                  </div>
                </div>
                <small>{commitDetail.date} · {commitDetail.relativeDate}</small>
                {#if commitDetail.refs.length}
                  <div class="detail-refs">
                    {#each commitDetail.refs as ref}
                      <span class="ref-pill" style="--ref-color:#26c6da">{ref.replace(/^HEAD -> /, "")}</span>
                    {/each}
                  </div>
                {/if}
                {#if commitDetail.parents.length}
                  <small>
                    {commitDetail.parents.length === 1 ? "parent" : "parents"}
                    {commitDetail.parents.map((parent) => parent.slice(0, 8)).join(", ")}
                  </small>
                {/if}
              </div>

              <div class="commit-actions">
                <button on:click={() => commitDetail && execute({ kind: "checkoutCommit", target: commitDetail.hash }, `Checkout ${commitDetail.shortHash}`)} disabled={busy}>Checkout</button>
                <button on:click={() => commitDetail && createBranchAtCommit(commitDetail.hash)} disabled={busy}>Branch</button>
                <button on:click={() => commitDetail && createTagPrompt(commitDetail.hash)} disabled={busy}>Tag</button>
                <button on:click={() => commitDetail && execute({ kind: "cherryPick", target: commitDetail.hash }, `Cherry-pick ${commitDetail.shortHash}`)} disabled={busy}>Cherry-pick</button>
                <button on:click={() => commitDetail && execute({ kind: "revert", target: commitDetail.hash }, `Revert ${commitDetail.shortHash}`)} disabled={busy}>Revert</button>
                <button on:click={() => commitDetail && openRebasePanel("plain", commitDetail.hash)} disabled={busy}>Rebase onto this</button>
              </div>
              <div class="reset-mode-row">
                <label for="detail-reset-mode">reset mode</label>
                <select id="detail-reset-mode" bind:value={resetMode}>
                  <option value="soft">soft</option>
                  <option value="mixed">mixed</option>
                  <option value="hard">hard</option>
                </select>
                <button class="danger" on:click={() => commitDetail && execute({ kind: "reset", target: commitDetail.hash, mode: resetMode }, `Reset to ${commitDetail.shortHash}`)} disabled={busy}>
                  Reset here
                </button>
              </div>

              <div class="commit-files">
                <h3>{commitDetail.files.length} changed {commitDetail.files.length === 1 ? "file" : "files"}</h3>
                {#each commitDetail.files as change}
                  <button
                    class="commit-file-row"
                    class:active={commitFilePath === change.path && diffContext === "commit"}
                    title={`${statusLabel(change.status)}: ${change.path}`}
                    on:click={() => openCommitFile(change)}
                  >
                    <span class={`status-${change.status.toLowerCase()}`}>{change.status}</span>
                    <strong>{change.path}</strong>
                  </button>
                {:else}
                  <p class="empty">No file changes recorded</p>
                {/each}
              </div>
            </div>
          {:else}
            <p class="empty centered">Unable to load commit detail.</p>
          {/if}
        </div>
      {:else}
        <div class="changes-title">
          <button
            class="trash danger"
            title="Discard selected"
            on:click={() => selectedFile && execute({ kind: selectedFile.group === "untracked" ? "cleanUntracked" : "discard", path: selectedFile.path }, "Discard selected file")}
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
            <FileGroup title="Conflicted" files={visibleUnstaged.filter((file) => file.group === "conflicted")} open={openConflict} action={fileAction} tree={rightTab === "tree"} hideWhenEmpty />
            <FileGroup title="Unstaged" files={visibleUnstaged.filter((file) => file.group === "unstaged")} open={openFile} action={fileAction} selectedPath={selectedFile?.path} tree={rightTab === "tree"} hideWhenEmpty={untracked.length + conflicted.length > 0} />
            <FileGroup title="Untracked" files={visibleUnstaged.filter((file) => file.group === "untracked")} open={openFile} action={fileAction} selectedPath={selectedFile?.path} tree={rightTab === "tree"} hideWhenEmpty />
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
            <FileGroup title="Staged" files={visibleStaged} open={openFile} action={fileAction} selectedPath={selectedFile?.path} tree={rightTab === "tree"} />
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
            {amendCommit ? "Amend Previous Commit" : "Commit Staged Changes"}
          </button>
        </div>
      {/if}
    </aside>
  {/if}
  {#if rebaseOpen && state}
    {#await import("./lib/RebasePanel.svelte") then module}
      <svelte:component
        this={module.default}
        currentBranch={currentBranch}
        branches={state.branches}
        mode={rebaseMode}
        initialBase={rebaseBase}
        confirmRisky={settings.confirmRisky}
        onClose={() => (rebaseOpen = false)}
        onDone={async (result, label) => {
          rebaseOpen = false;
          if (result.ok) notice = label;
          if (result.refresh) await refresh();
        }}
      />
    {/await}
  {/if}
  <!-- F4: rebase panel mount -->

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
          <button title="Close merge editor" on:click={() => (conflict = null)}>×</button>
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
  <!-- F3: clone dialog mount -->
  {#if cloneOpen}
    {#await import("./lib/CloneDialog.svelte") then module}
      <svelte:component
        this={module.default}
        clonePath={settings.clonePath}
        onClose={() => (cloneOpen = false)}
        onCloned={afterClone}
      />
    {/await}
  {/if}

  {#if settingsOpen}
    <div
      class="modal-backdrop"
      role="presentation"
      on:click={(event) => event.target === event.currentTarget && (settingsOpen = false)}
    >
      <div class="modal" role="dialog" aria-label="Settings" tabindex="-1">
        <div class="panel-title">
          <h1>Settings</h1>
          <button title="Close settings" on:click={() => (settingsOpen = false)}>×</button>
        </div>
        <div class="modal-body">
          <label class="checkbox">
            <input type="checkbox" bind:checked={settings.confirmRisky} />
            Confirm destructive actions (discard, reset, force push, drop stash, remove worktree, delete branches)
          </label>
          <div class="field">
            <label for="settings-clone-path">Default clone / create directory</label>
            <input id="settings-clone-path" bind:value={settings.clonePath} placeholder="/path/to/dev" />
          </div>
          <div class="field">
            <label for="settings-graph-limit">Commits loaded in graph (25–1000)</label>
            <input id="settings-graph-limit" type="number" min="25" max="1000" bind:value={settings.graphLimit} />
          </div>
          <div class="field">
            <label for="settings-stale-days">Branch cleanup: stale after (1–3650 days)</label>
            <input id="settings-stale-days" type="number" min="1" max="3650" bind:value={settings.staleDays} />
          </div>
        </div>
        <div class="modal-footer">
          <button on:click={() => (settingsOpen = false)}>Cancel</button>
          <button class="stage-file" on:click={saveSettings}>Save Settings</button>
        </div>
      </div>
    </div>
  {/if}
  <!-- F1: cleanup panel mount -->
  {#if cleanupOpen && state}
    {#await import("./lib/CleanupPanel.svelte") then module}
      <svelte:component
        this={module.default}
        {state}
        staleDays={settings.staleDays}
        confirmRisky={settings.confirmRisky}
        onClose={() => (cleanupOpen = false)}
        onDone={async (s) => {
          notice = s;
          await refresh();
        }}
      />
    {/await}
  {/if}

  <footer class="status-bar">
    <span>⌁ {repoName}</span>
    <span>
      {busy
        ? "Running git command..."
        : state?.merging
          ? "Merge in progress"
          : state?.rebasing
            ? "Rebase in progress"
            : selectedCommit?.shortHash ?? "Ready"}
    </span>
    <span>{currentBranch}</span>
  </footer>
</main>
