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
  import FileGroup from "./lib/FileGroup.svelte";
  import ReadonlyPane from "./lib/ReadonlyPane.svelte";
  import { parseDiffRows } from "./lib/diffRows";
  import type {
    Branch,
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

  type AppTab = {
    id: string;
    kind: "launchpad" | "repo";
    label: string;
    path?: string;
  };

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
  let compareBase: string | null = null;
  let compareHead = "";
  let resolvedContent = "";
  let commitMessage = "";
  let commitDescription = "";
  let amendCommit = false;
  let commandTarget = "";
  let branchName = "";
  let resetMode = "mixed";
  let rebaseOpen = false;
  let rebaseMode: "interactive" | "plain" = "interactive";
  let rebaseBase: string | null = null;
  let rightTab: "path" | "tree" = "path";
  let searchOpen = false;
  let searchQuery = "";
  let sortAsc = true;
  let localOpen = false;
  let remoteOpen = false;
  let stashesOpen = false;
  let tagsOpen = false;
  let worktreesOpen = false;
  let collapsedLocalDirs = new Set<string>();
  let collapsedRemoteDirs = new Set<string>();
  let collapsedCommitDirs = new Set<string>();
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
    "#14a0bf",
    "#036af7",
    "#8e00c2",
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
  $: wipModified = unstaged.length;
  $: wipAdded = untracked.length + staged.filter((file) => file.index === "A").length;
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
  // No artificial floor: a linear repo has one lane, and the graph column
  // should be sized for the lanes the data actually has, not padded for
  // lanes that don't exist (was Math.max(3, ...), wasting column width).
  $: graphLaneCount = Math.max(1, ...graphRows.flatMap((row) => row.lanes.map((lane) => lane.index + 1)));
  $: filteredBranches = (state?.branches ?? []).filter(
    (branch) => !searchQuery.trim() || branch.name.toLowerCase().includes(searchQuery.trim().toLowerCase()),
  );
  $: commitFileSummaryParts = commitDetail
    ? (
        [
          { key: "modified", cls: "st-mod", glyph: "✎", count: commitDetail.files.filter((f) => !/^[AD?]/.test(f.status)).length },
          { key: "added", cls: "st-add", glyph: "+", count: commitDetail.files.filter((f) => /^[A?]/.test(f.status)).length },
          { key: "deleted", cls: "st-del", glyph: "−", count: commitDetail.files.filter((f) => /^D/.test(f.status)).length },
        ] as const
      ).filter((part) => part.count > 0)
    : [];
  $: sortedCommitFiles = commitDetail ? sortByPath(commitDetail.files, sortAsc) : [];
  $: commitFileTree = commitDetail ? buildRefTree(sortedCommitFiles, (file) => file.path, collapsedCommitDirs) : [];
  $: localTree = buildRefTree(filteredBranches, (branch) => branch.name, collapsedLocalDirs);
  $: remoteTree = buildRefTree(filteredRemoteBranches, (name) => name, collapsedRemoteDirs);
  // Sections after the last expanded one stack at the sidebar's bottom edge
  // (matching the reference), so the empty space collects between the open
  // section's content and the collapsed remainder instead of below everything.
  $: lastOpenNavIndex = [localOpen, remoteOpen, worktreesOpen, stashesOpen, tagsOpen].reduce(
    (last, open, index) => (open ? index : last),
    -1,
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
    const parts = author.trim().split(/\s+/).filter(Boolean);
    if (parts.length >= 2) {
      return parts
        .slice(0, 2)
        .map((part) => part[0]?.toUpperCase())
        .join("");
    }
    const word = parts[0];
    if (!word) return "G";
    // A single-word author (the common case for a solo repo) still gets two
    // letters, e.g. "Dillon" -> "Di", instead of a single dot-everywhere "D".
    return word.length > 1 ? word[0].toUpperCase() + word[1].toLowerCase() : word[0].toUpperCase();
  }

  // Git wraps a manually-authored commit body at ~72 columns for terminal
  // reading; rendering those hard breaks verbatim inside a ~320px-wide panel
  // re-wraps them again and shreds the paragraph into short fragments. This
  // reflows each paragraph's wrapped lines back together (a line is a
  // continuation unless it starts a bullet/numbered item or looks like a
  // trailer such as "Co-Authored-By: ..."), so the browser can wrap the
  // result naturally at the panel's real width. Blank lines still separate
  // paragraphs; bullets and trailers still get their own line.
  function reflowCommitBody(raw: string) {
    if (!raw) return "";
    return raw
      .replace(/\r\n/g, "\n")
      .split(/\n{2,}/)
      .map((paragraph) => {
        const groups: string[] = [];
        for (const rawLine of paragraph.split("\n")) {
          const line = rawLine.trim();
          if (!line) continue;
          const isBullet = /^([-*•]|\d+[.)])\s+/.test(line);
          const isTrailer = /^[A-Za-z][\w .()/'-]*:\s/.test(line);
          if (groups.length && !isBullet && !isTrailer) {
            groups[groups.length - 1] += ` ${line}`;
          } else {
            groups.push(line);
          }
        }
        return groups.join("\n");
      })
      .join("\n\n");
  }

  type RefTreeDirRow = { key: string; depth: number; kind: "dir"; label: string };
  type RefTreeLeafRow<T> = { key: string; depth: number; kind: "leaf"; label: string; item: T };
  type RefTreeRow<T> = RefTreeDirRow | RefTreeLeafRow<T>;

  // Branch names like "style/graph-sidebar-match" render as a folder tree
  // (split on "/"), matching the reference — not a flat list of full names.
  function buildRefTree<T>(items: T[], pathOf: (item: T) => string, collapsed: Set<string>): RefTreeRow<T>[] {
    type Node = { dirs: Map<string, Node>; leaves: T[] };
    const root: Node = { dirs: new Map(), leaves: [] };
    for (const item of items) {
      const parts = pathOf(item).split("/");
      let node = root;
      for (const part of parts.slice(0, -1)) {
        if (!node.dirs.has(part)) node.dirs.set(part, { dirs: new Map(), leaves: [] });
        node = node.dirs.get(part)!;
      }
      node.leaves.push(item);
    }
    const rows: RefTreeRow<T>[] = [];
    const walk = (node: Node, prefix: string, depth: number) => {
      for (const [segment, child] of [...node.dirs.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
        const key = prefix ? `${prefix}/${segment}` : segment;
        rows.push({ key, depth, kind: "dir", label: segment });
        if (!collapsed.has(key)) walk(child, key, depth + 1);
      }
      for (const item of [...node.leaves].sort((a, b) => pathOf(a).localeCompare(pathOf(b)))) {
        const path = pathOf(item);
        const segment = path.split("/").at(-1) ?? path;
        rows.push({ key: `${path}:leaf`, depth, kind: "leaf", label: segment, item });
      }
    };
    walk(root, "", 0);
    return rows;
  }

  function toggleRefDir(collapsed: Set<string>, key: string) {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    return next;
  }

  function commitStatusGlyph(status: string): { glyph: string; cls: string } {
    const code = status[0]?.toUpperCase();
    if (code === "A" || code === "?") return { glyph: "+", cls: "st-add" };
    if (code === "D") return { glyph: "−", cls: "st-del" };
    if (code === "R" || code === "C") return { glyph: "⇝", cls: "st-mod" };
    return { glyph: "✎", cls: "st-mod" };
  }

  function sortByPath<T extends { path: string }>(items: T[], asc: boolean) {
    const sorted = [...items].sort((a, b) => a.path.localeCompare(b.path));
    return asc ? sorted : sorted.reverse();
  }

  // Upstream/ahead-behind detail moved off the row (it was rendering as a
  // second line, making tree rows ragged and uneven height) and into this
  // tooltip instead — every row stays a single, uniform-height line.
  function branchTooltip(branch: Branch) {
    const parts = [branch.current ? `${branch.name} (checked out)` : `Checkout ${branch.name}`];
    if (branch.upstreamGone) parts.push(`${branch.upstream ?? "upstream"} · gone`);
    else if (branch.upstream) parts.push(branch.upstream);
    if (branch.ahead || branch.behind) parts.push(`↑${branch.ahead} ↓${branch.behind}`);
    return parts.join(" — ");
  }

  function isHeadRow(row: GraphRow) {
    return !!state && row.commit.shortHash === state.head;
  }

  function isWorktreeBranch(name: string) {
    return (state?.worktrees ?? []).some((worktree) => worktree.branch === name);
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
        <button title="Refresh" on:click={refresh} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M23 4v6h-6" /><path d="M1 20v-6h6" /><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10" /><path d="M1 14l4.64 4.36A9 9 0 0 0 20.49 15" /></svg>
          <span>Refresh</span>
        </button>
        <button title="Fetch" on:click={() => execute({ kind: "fetch" }, "Fetch")} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" /></svg>
          <span>Fetch</span>
        </button>
        <button title="Pull" on:click={() => execute({ kind: "pull" }, "Pull")} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19" /><polyline points="19 12 12 19 5 12" /></svg>
          <span>Pull</span>
        </button>
        <button title="Push" on:click={() => execute({ kind: "push" }, "Push")} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5" /><polyline points="5 12 12 5 19 12" /></svg>
          <span>Push</span>
        </button>
        <button title="Branch" on:click={createBranchFromToolbar}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" /></svg>
          <span>Branch</span>
        </button>
        <button title="Compare refs (shift-click two commits in the graph)" on:click={() => openCompare(null, currentBranch)}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="17 1 21 5 17 9" /><path d="M3 11V9a4 4 0 0 1 4-4h14" /><polyline points="7 23 3 19 7 15" /><path d="M21 13v2a4 4 0 0 1-4 4H3" /></svg>
          <span>Compare</span>
        </button>
        <button title="Stash" on:click={() => execute({ kind: "stashCreate", message: "gitc stash" }, "Create stash")} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="21 8 21 21 3 21 3 8" /><rect x="1" y="3" width="22" height="5" /><line x1="10" y1="12" x2="14" y2="12" /></svg>
          <span>Stash</span>
        </button>
        <button title="Terminal" on:click={openRepoTerminal} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5" /><line x1="12" y1="19" x2="20" y2="19" /></svg>
          <span>Terminal</span>
        </button>
      </div>
      <div class="toolbar-spacer"></div>
      <div class="search-actions">
        <button title="Actions" class:active={actionsOpen} on:click={() => (actionsOpen = !actionsOpen)}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" y1="21" x2="4" y2="14" /><line x1="4" y1="10" x2="4" y2="3" /><line x1="12" y1="21" x2="12" y2="12" /><line x1="12" y1="8" x2="12" y2="3" /><line x1="20" y1="21" x2="20" y2="16" /><line x1="20" y1="12" x2="20" y2="3" /><line x1="1" y1="14" x2="7" y2="14" /><line x1="9" y1="8" x2="15" y2="8" /><line x1="17" y1="16" x2="23" y2="16" /></svg>
          <span>Actions</span>
        </button>
        <button title="Search" on:click={() => (searchOpen = !searchOpen)}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" /></svg>
          <span>Search</span>
        </button>
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
      <div class="viewing-row">Viewing <strong>{filteredBranches.length + filteredRemoteBranches.length}</strong></div>
      <div class="filter-block">
        <div class="filter-input-wrap">
          <input
            aria-label="Filter refs"
            bind:this={filterInput}
            bind:value={searchQuery}
            placeholder="Filter (⌘ + Option + f)"
          />
          <span class="filter-icon">⌕</span>
        </div>
      </div>

      <div class="nav-scroll">
        <section class="nav-section">
          <div class="section-head">
            <button class="nav-row" on:click={() => (localOpen = !localOpen)}>
              <span class="nav-chevron">{localOpen ? "⌄" : "›"}</span>
              <i class="nav-icon">⌂</i>
              <span class="nav-label">Local</span>
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
              {#each localTree as row (row.key)}
                {#if row.kind === "dir"}
                  <button class="ref-tree-dir" style={`--depth:${row.depth}`} on:click={() => (collapsedLocalDirs = toggleRefDir(collapsedLocalDirs, row.key))}>
                    <svg class="tree-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" /></svg>
                    <span>{row.label}</span>
                  </button>
                {:else}
                  <div class="branch-row" class:active={row.item.current} style={`--depth:${row.depth}`}>
                    <button
                      class="branch-name"
                      title={branchTooltip(row.item)}
                      on:click={() => execute({ kind: "checkoutBranch", branch: row.item.name }, `Checkout ${row.item.name}`)}
                      disabled={busy || row.item.current}
                    >
                      <span class="branch-name-line">
                        {#if row.item.current}<i class="branch-check">✓</i>{/if}
                        <svg class="tree-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" /></svg>
                        <span>{row.label}</span>
                      </span>
                    </button>
                    {#if !row.item.current}
                      <button
                        class="row-action"
                        title={`Rebase ${currentBranch} onto ${row.item.name}`}
                        on:click={() => rebaseOnto(row.item.name)}
                        disabled={busy}
                      >⤴</button>
                      <button
                        class="row-action danger"
                        title={`Delete ${row.item.name}`}
                        on:click={() => deleteBranch(row.item.name)}
                        disabled={busy}
                      >×</button>
                    {/if}
                  </div>
                {/if}
              {:else}
                <p class="empty">No local branches</p>
              {/each}
            </div>
          {/if}
        </section>
        {#if lastOpenNavIndex === 0}<div class="nav-spacer"></div>{/if}

        <section class="nav-section">
          <button class="nav-row" on:click={() => (remoteOpen = !remoteOpen)}>
            <span class="nav-chevron">{remoteOpen ? "⌄" : "›"}</span>
            <i class="nav-icon">☁</i>
            <span class="nav-label">Remote</span>
            <strong>{state?.remoteBranches.length ?? 0}</strong>
          </button>
          {#if remoteOpen}
            <div class="branch-list">
              {#each remoteTree as row (row.key)}
                {#if row.kind === "dir"}
                  <button class="ref-tree-dir" style={`--depth:${row.depth}`} on:click={() => (collapsedRemoteDirs = toggleRefDir(collapsedRemoteDirs, row.key))}>
                    <svg class="tree-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" /></svg>
                    <span>{row.label}</span>
                  </button>
                {:else}
                  <button
                    class="branch-name tree-leaf"
                    style={`--depth:${row.depth}`}
                    title={`Checkout tracking branch for ${row.item}`}
                    on:click={() => execute({ kind: "checkoutRemote", target: row.item }, `Checkout ${row.item}`)}
                    disabled={busy}
                  >
                    <span class="branch-name-line"><svg class="tree-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" /></svg> <span>{row.label}</span></span>
                  </button>
                {/if}
              {:else}
                <p class="empty">No remote branches</p>
              {/each}
            </div>
          {/if}
        </section>
        {#if lastOpenNavIndex === 1}<div class="nav-spacer"></div>{/if}

        <section class="nav-section">
          <div class="section-head">
            <button class="nav-row" on:click={() => (worktreesOpen = !worktreesOpen)}>
              <span class="nav-chevron">{worktreesOpen ? "⌄" : "›"}</span>
              <i class="nav-icon">⧉</i>
              <span class="nav-label">Worktrees</span>
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
        {#if lastOpenNavIndex === 2}<div class="nav-spacer"></div>{/if}

        <section class="nav-section">
          <button class="nav-row" on:click={() => (stashesOpen = !stashesOpen)}>
            <span class="nav-chevron">{stashesOpen ? "⌄" : "›"}</span>
            <i class="nav-icon">▤</i>
            <span class="nav-label">Stashes</span>
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
        {#if lastOpenNavIndex === 3}<div class="nav-spacer"></div>{/if}

        <section class="nav-section">
          <button class="nav-row" on:click={() => (tagsOpen = !tagsOpen)}>
            <span class="nav-chevron">{tagsOpen ? "⌄" : "›"}</span>
            <i class="nav-icon">⌖</i>
            <span class="nav-label">Tags</span>
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
        <span>Branch / Tag</span>
        <span>Graph</span>
        <span>
          Commit Message
          <button class="graph-settings" title="Settings" on:click={() => (settingsOpen = true)}>⚙</button>
        </span>
      </div>
      <div class="graph-scroll">
        {#if totalChanges > 0}
          <div class="wip-row">
            <div class="branch-cell"></div>
            <div class="wip-graph">
              <span class="wip-rail"></span>
              <span class="wip-node"></span>
            </div>
            <div class="wip-summary">
              <button class="wip-message" on:click={() => selectCommit(null)}>
                <strong>// WIP</strong>
              </button>
              {#if wipModified || wipAdded}
                <span class="wip-count" title={`${totalChanges} WIP file changes`}>
                  {#if wipModified}
                    <span class="wip-pencil">✎</span>
                    <strong>{wipModified}</strong>
                  {/if}
                  {#if wipAdded}
                    <span class="wip-added">+</span>
                    <strong>{wipAdded}</strong>
                  {/if}
                </span>
              {/if}
            </div>
          </div>
        {/if}
        {#each visibleGraphRows as row, rowIndex}
          <button
            class="commit-row"
            class:active={selectedCommit?.hash === row.commit.hash}
            title={commitRowTitle(row)}
            on:click={(event) => handleCommitRowClick(event, row, rowIndex)}
          >
            <span class="branch-cell" title={row.labels.join("  ")}>
              {#if row.labels.length}
                <span class="ref-pill" class:head={isHeadRow(row) && row.labels[0] === currentBranch} style={`--ref-color:${row.color}`}>
                  {#if isHeadRow(row) && row.labels[0] === currentBranch}<i class="pill-check">✓</i>{/if}
                  <span class="pill-label">{row.labels[0]}</span>
                  {#if isWorktreeBranch(row.labels[0])}<i class="pill-worktree">💻</i>{:else}<i class="pill-branch">⑂</i>{/if}
                </span>
                {#if row.labels.length > 1}
                  <span class="ref-pill more-pill" style={`--ref-color:${row.color}`}>+{row.labels.length - 1}</span>
                {/if}
              {/if}
            </span>
            <span class="graph-cell" style={`--lane-count:${graphLaneCount}`}>
              <span class="graph-tint" style={`--lane:${row.lane}; --lane-color:${row.color}`}></span>
              {#if row.labels.length}
                <span class="ref-link" class:head={isHeadRow(row)} style={`--lane:${row.lane}; --lane-color:${row.color}`}></span>
              {/if}
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
              <span class="commit-dot" style={`--lane:${row.lane}; --lane-color:${row.color}`}>
                {#if row.commit.parents.length > 1}<i class="merge-glyph">≡</i>{:else}{authorInitials(row.commit.author)}{/if}
              </span>
            </span>
            <span class="commit-main">
              <strong>
                <span>{row.commit.subject}</span>
                {#if row.commit.bodySummary}
                  <span class="commit-summary">
                    <em>{row.commit.bodySummary}</em>
                  </span>
                {/if}
                {#if rowIndex > 0 && visibleGraphRows[rowIndex - 1].commit.relativeDate !== row.commit.relativeDate}
                  <span class="date-marker">{row.commit.relativeDate}</span>
                {/if}
              </strong>
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
          <div class="commit-detail-head">
            <button class="commit-hash-btn" title="Copy hash" on:click={() => selectedCommit && copyHash(selectedCommit.hash)}>
              commit: <strong>{selectedCommit.shortHash}</strong>
            </button>
            <button class="commit-detail-close" title="Back to work in progress" on:click={() => selectCommit(null)}>×</button>
          </div>
          {#if commitDetailBusy}
            <p class="empty centered">Loading commit…</p>
          {:else if commitDetail}
            <div class="commit-detail-scroll">
              <div class="commit-message-card">
                <h2>{commitDetail.subject}</h2>
                {#if commitDetail.body}
                  <div class="commit-body-scroll">
                    <p class="commit-body">{reflowCommitBody(commitDetail.body)}</p>
                  </div>
                {/if}
              </div>
              <div class="split-handle"></div>

              <div class="commit-author-row">
                <span class="author-badge">{authorInitials(commitDetail.author)}</span>
                <div class="commit-author-main">
                  <strong>{commitDetail.author}</strong>
                  <small>authored {commitDetail.date}</small>
                </div>
                {#if commitDetail.parents.length}
                  <div class="commit-parent">
                    <span>{commitDetail.parents.length === 1 ? "parent" : "parents"}:</span>
                    <strong>{commitDetail.parents.map((parent) => parent.slice(0, 7)).join(", ")}</strong>
                  </div>
                {/if}
              </div>

              {#if commitDetail.refs.length}
                <div class="detail-refs">
                  <span class="detail-refs-label">refs</span>
                  {commitDetail.refs.map((ref) => ref.replace(/^HEAD -> /, "")).join(", ")}
                </div>
              {/if}

              {#if commitFileSummaryParts.length}
                <div class="commit-change-summary">{#each commitFileSummaryParts as part, index (part.key)}{#if index > 0}<span class="summary-sep">,</span>{/if}<span class={part.cls}>{part.glyph}</span> {part.count} {part.key}{/each}</div>
              {/if}

              <div class="changes-tools">
                <button class="sort-btn" title={sortAsc ? "Sorted A to Z" : "Sorted Z to A"} on:click={() => (sortAsc = !sortAsc)}>⇅<i>{sortAsc ? "AZ" : "ZA"}</i></button>
                <div class="segmented">
                  <button class:active={rightTab === "path"} on:click={() => (rightTab = "path")}>☰ Path</button>
                  <button class:active={rightTab === "tree"} on:click={() => (rightTab = "tree")}>⌘ Tree</button>
                </div>
              </div>

              <div class="commit-files">
                {#if rightTab === "tree"}
                  {#each commitFileTree as row (row.key)}
                    {#if row.kind === "dir"}
                      <button class="tree-dir" style={`--depth:${row.depth}`} on:click={() => (collapsedCommitDirs = toggleRefDir(collapsedCommitDirs, row.key))}>
                        <span>{collapsedCommitDirs.has(row.key) ? "›" : "⌄"}</span>
                        <strong>{row.label}</strong>
                      </button>
                    {:else}
                      <button
                        class="commit-file-row tree-row"
                        class:active={commitFilePath === row.item.path && diffContext === "commit"}
                        style={`--depth:${row.depth}`}
                        title={`${statusLabel(row.item.status)}: ${row.item.path}`}
                        on:click={() => openCommitFile(row.item)}
                      >
                        <span class={commitStatusGlyph(row.item.status).cls}>{commitStatusGlyph(row.item.status).glyph}</span>
                        <strong>{row.label}</strong>
                      </button>
                    {/if}
                  {:else}
                    <p class="empty">No file changes recorded</p>
                  {/each}
                {:else}
                  {#each sortedCommitFiles as change (change.path)}
                    <button
                      class="commit-file-row"
                      class:active={commitFilePath === change.path && diffContext === "commit"}
                      title={`${statusLabel(change.status)}: ${change.path}`}
                      on:click={() => openCommitFile(change)}
                    >
                      <span class={commitStatusGlyph(change.status).cls}>{commitStatusGlyph(change.status).glyph}</span>
                      <strong>{change.path}</strong>
                    </button>
                  {:else}
                    <p class="empty">No file changes recorded</p>
                  {/each}
                {/if}
              </div>

              <details class="commit-actions-disclosure">
                <summary>
                  <span class="options-toggle"><i>›</i> Commit actions</span>
                </summary>
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
              </details>
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
          <strong><span class="changes-title-label">{totalChanges} file changes on</span> <span class="changes-title-branch">{currentBranch}</span></strong>
          <button class="refresh-btn" title="Refresh" on:click={refresh} disabled={busy}>↻</button>
        </div>
        <div class="changes-tools">
          <button class="sort-btn" title={sortAsc ? "Sorted A to Z" : "Sorted Z to A"} on:click={() => (sortAsc = !sortAsc)}>⇅<i>{sortAsc ? "AZ" : "ZA"}</i></button>
          <div class="segmented">
            <button class:active={rightTab === "path"} on:click={() => (rightTab = "path")}>☰ Path</button>
            <button class:active={rightTab === "tree"} on:click={() => (rightTab = "tree")}>⌘ Tree</button>
          </div>
        </div>

        <div class="change-list">
          <div class="change-group-head">
            <button class="section-toggle" on:click={() => (unstagedOpen = !unstagedOpen)}>
              <span class="chevron">{unstagedOpen ? "▾" : "▸"}</span>
              <span>Unstaged Files ({unstaged.length + untracked.length + conflicted.length})</span>
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
              <span class="chevron">{stagedOpen ? "▾" : "▸"}</span>
              <span>Staged Files ({staged.length})</span>
            </button>
            <button on:click={() => execute({ kind: "unstage", path: "." }, "Unstage all changes")} disabled={busy || staged.length === 0}>
              Unstage All Changes
            </button>
          </div>
          {#if stagedOpen}
            <FileGroup title="Staged" files={visibleStaged} open={openFile} action={fileAction} selectedPath={selectedFile?.path} tree={rightTab === "tree"} />
          {/if}
        </div>

        <div class="commit-panel">
          <div class="split-handle"></div>
          <div class="commit-tabs">
            <button class="commit-tab">-o- Commit</button>
          </div>
          <label class="checkbox"><input type="checkbox" bind:checked={amendCommit} /> Amend previous commit</label>
          <div class="commit-box">
            <label class="commit-input" for="commit-message">
              <input id="commit-message" bind:value={commitMessage} maxlength="72" placeholder="Commit summary" />
              <small>{72 - commitMessage.length}</small>
            </label>
            <textarea class="description" bind:value={commitDescription} placeholder="Description"></textarea>
          </div>
          <details>
            <summary>
              <span class="options-toggle"><i>›</i> Commit options</span>
            </summary>
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
            {commitMessage.trim() ? (amendCommit ? "Amend Previous Commit" : "Commit Staged Changes") : "-o- Type a Message to Commit"}
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
            : "Ready"}
    </span>
    <span>{currentBranch}</span>
  </footer>
</main>
