<script lang="ts">
  import {
    applyHunk,
    createRepository,
    getCommitDetail,
    getCommitFileDiff,
    getCommitGraph,
    getCommitTree,
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
  import { avatarUrl, coAuthorsOf } from "./lib/avatar";
  import { hoverExpand } from "./lib/hoverExpand";
  import { buildGraphRows, githubOwnerAvatar, isGithubUrl, type GraphRow } from "./lib/graph";
  import { blockedReason, undoableKinds, undoEntryFor, undoItem as toUndoItem, type UndoItem } from "./lib/undo";
  import type {
    Branch,
    CommitDetail,
    CommitFileChange,
    CommitNode,
    ConflictFile,
    FileDiff,
    FileStatus,
    GitAction,
    GitResult,
    RepositoryState,
    Worktree,
  } from "./lib/types";

  type AppTab = {
    id: string;
    kind: "launchpad" | "repo";
    label: string;
    path?: string;
  };

  type RecentRepo = { name: string; path: string };

  type Settings = {
    confirmRisky: boolean;
    clonePath: string;
    graphLimit: number;
    staleDays: number;
    showAvatars: boolean;
    pullMode: string;
  };

  const defaultSettings: Settings = {
    confirmRisky: true,
    clonePath: "/Users/dillon/Documents/dev",
    graphLimit: 250,
    staleDays: 30,
    showAvatars: true,
    pullMode: "pull",
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
  let pullMenuOpen = false;

  const pullOptions = [
    { kind: "pullMerge", label: "Pull (fast-forward if possible)" },
    { kind: "pull", label: "Pull (fast-forward only)" },
    { kind: "pullRebase", label: "Pull (rebase)" },
  ];

  // Undo and redo stacks per repository, kept for the session.
  let undoHistory: Record<string, { undo: UndoItem[]; redo: UndoItem[] }> = {};
  $: repoHistory = (state && undoHistory[state.root]) || { undo: [], redo: [] };
  $: undoItem = repoHistory.undo.at(-1) ?? null;
  $: redoItem = repoHistory.redo.at(-1) ?? null;

  function recordUndo(action: GitAction, before: RepositoryState | null, result: GitResult) {
    if (!before || !state || before.root !== state.root) return;
    const entry = undoEntryFor(action, before, state, result);
    if (!entry) return;
    const current = undoHistory[state.root] ?? { undo: [], redo: [] };
    undoHistory = {
      ...undoHistory,
      [state.root]: { undo: [...current.undo, toUndoItem(entry, state)].slice(-50), redo: [] },
    };
  }

  async function replay(direction: "undo" | "redo") {
    if (!state || busy) return;
    const root = state.root;
    const current = undoHistory[root] ?? { undo: [], redo: [] };
    const item = current[direction].at(-1);
    if (!item) return;
    const verb = direction === "undo" ? "Undo" : "Redo";
    busy = true;
    error = "";
    notice = "";
    let failure = "";
    try {
      const blocked = blockedReason(item, await getRepositoryState());
      if (blocked === "dirty") {
        failure = `${verb} ${item.entry.label} would discard uncommitted changes. Commit or stash them first.`;
      } else if (blocked === "moved") {
        // Every entry beneath it is older still, so none of them can apply either.
        undoHistory = { ...undoHistory, [root]: { ...current, [direction]: [] } };
        failure = `Can't ${direction} ${item.entry.label}: the repository has changed since.`;
      } else {
        for (const step of item.entry[direction]) {
          const result = await runGitAction(step);
          if (!result.ok) {
            failure = result.stderr || result.stdout || `${verb} ${item.entry.label} failed`;
            break;
          }
        }
        await refresh();
        const next = { undo: [...current.undo], redo: [...current.redo] };
        next[direction].pop();
        if (!failure && state) {
          next[direction === "undo" ? "redo" : "undo"].push(toUndoItem(item.entry, state));
          notice = `${direction === "undo" ? "Undid" : "Redid"} ${item.entry.label}`;
        }
        undoHistory = { ...undoHistory, [root]: next };
      }
    } catch (err) {
      failure = String(err);
    } finally {
      busy = false;
    }
    if (failure) error = failure;
  }

  function setPullMode(kind: string) {
    settings = { ...settings, pullMode: kind };
    try {
      localStorage.setItem("gitc:settings", JSON.stringify(settings));
    } catch {
      /* localStorage unavailable */
    }
  }

  function runPullMenu(action: GitAction, label: string) {
    pullMenuOpen = false;
    void execute(action, label);
  }

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
  $: graphRows = buildGraphRows(commits, { hasWip: totalChanges > 0, remotes: state?.remotes ?? [] });
  $: visibleGraphRows = filterGraphRows(graphRows, searchOpen ? searchQuery : "");
  // The column grows past its 130px floor (styles.css, --graph-column) only
  // when the history actually has the lanes for it.
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
  $: commitFileEntries = commitDetail
    ? withUnchangedFiles(commitDetail.files, viewAllFiles && commitTree?.hash === commitDetail.hash ? commitTree.paths : null)
    : [];
  $: sortedCommitFiles = sortByPath(commitFileEntries, sortAsc);
  $: coAuthors = commitDetail ? coAuthorsOf(commitDetail.body) : [];
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
      const shown = kept ?? cleanHeadCommit(nextState, graph.commits);
      selectedCommit = shown;
      if (shown) {
        await loadCommitDetail(shown.hash);
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
    pullMenuOpen = false;
    busy = true;
    error = "";
    notice = "";
    try {
      // Read fresh rather than trusting the last refresh: undo returns to this HEAD.
      const before = undoableKinds.has(action.kind) ? await getRepositoryState().catch(() => null) : null;
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
      if (result.refresh) {
        await refresh();
        recordUndo(action, before, result);
      }
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
      await selectCommit(cleanHeadCommit(state, graph.commits));
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
      await selectCommit(cleanHeadCommit(state, graph.commits));
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
      await selectCommit(cleanHeadCommit(state, graph.commits));
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

  // An avatar that fails to load (no Gravatar, offline) stays on initials for
  // the session rather than being re-requested every time the rows render.
  let failedAvatars = new Set<string>();
  $: detailAvatar = commitDetail ? avatarFor(commitDetail.email, settings.showAvatars, failedAvatars) : null;

  function avatarFor(email: string, enabled: boolean, failed: Set<string>) {
    if (!enabled) return null;
    const url = avatarUrl(email);
    return url && !failed.has(url) ? url : null;
  }

  // The pill for a branch on a GitHub remote carries the remote account's
  // avatar; it falls back to the GitHub mark when that can't be loaded.
  function remoteAvatarFor(url: string | null | undefined, enabled: boolean, failed: Set<string>) {
    if (!enabled) return null;
    const avatar = githubOwnerAvatar(url);
    return avatar && !failed.has(avatar) ? avatar : null;
  }

  function avatarFailed(url: string) {
    failedAvatars = new Set(failedAvatars).add(url);
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
    if (!status) return { glyph: "", cls: "" };
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

  // With nothing to commit there is no WIP to show, so the right panel opens
  // on the checked-out commit instead of an empty change list.
  function cleanHeadCommit(repo: RepositoryState | null, nodes: CommitNode[]) {
    if (!repo || repo.files.length > 0) return null;
    return nodes.find((commit) => commit.refs.some((ref) => ref === "HEAD" || ref.startsWith("HEAD -> "))) ?? null;
  }

  // "View all files": the commit's whole tree, with its changes marked.
  let viewAllFiles = false;
  let commitTree: { hash: string; paths: string[] } | null = null;
  let commitTreePending = "";
  $: if (viewAllFiles && commitDetail && commitTree?.hash !== commitDetail.hash && commitTreePending !== commitDetail.hash) {
    void loadCommitTree(commitDetail.hash);
  }

  async function loadCommitTree(hash: string) {
    commitTreePending = hash;
    try {
      const paths = await getCommitTree(hash);
      commitTree = { hash, paths };
    } catch (err) {
      error = String(err);
      viewAllFiles = false;
    } finally {
      commitTreePending = "";
    }
  }

  function withUnchangedFiles(changes: CommitFileChange[], paths: string[] | null): CommitFileChange[] {
    if (!paths) return changes;
    const byPath = new Map(changes.map((change) => [change.path, change]));
    const inTree = new Set(paths);
    // Deleted files are gone from the tree but are still part of the change.
    return [
      ...paths.map((path) => byPath.get(path) ?? { status: "", path }),
      ...changes.filter((change) => !inTree.has(change.path)),
    ];
  }

  function splitPath(path: string) {
    const cut = path.lastIndexOf("/") + 1;
    return { dir: path.slice(0, cut), name: path.slice(cut) };
  }

  // Checked out in some other worktree. The current checkout's own branch is
  // just a local branch (the HEAD pill already says it is checked out here).
  function isWorktreeBranch(name: string) {
    return (state?.worktrees ?? []).some((worktree) => worktree.branch === name && !worktree.current);
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
    let action: GitAction = { kind: "deleteBranch", branch: name };
    let before: RepositoryState | null = null;
    let result: GitResult | null = null;
    try {
      before = await getRepositoryState().catch(() => null);
      result = await runGitAction(action);
      if (
        !result.ok &&
        /not fully merged/i.test(result.stderr) &&
        confirm(`${result.stderr.trim()}\n\nDelete ${name} anyway? Its tip stays recoverable for about two weeks.`)
      ) {
        action = { kind: "deleteBranchForce", branch: name };
        result = await runGitAction(action);
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
    // refresh() clears `error`, which used to swallow a failed delete's message.
    const failure = error;
    await refresh();
    if (failure) error = failure;
    if (result) recordUndo(action, before, result);
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
      pullMenuOpen = false;
    }
    // ⌘Z / ⇧⌘Z (or ⌘Y) undo and redo git actions, but never while typing,
    // where they belong to the text field.
    const typing =
      event.target instanceof HTMLElement &&
      (event.target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName));
    const key = event.key.toLowerCase();
    if ((event.metaKey || event.ctrlKey) && !event.altKey && !typing && (key === "z" || key === "y")) {
      if (centerMode !== "launchpad" && !settingsOpen && !conflict) {
        event.preventDefault();
        void replay(key === "z" && !event.shiftKey ? "undo" : "redo");
      }
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
    if (pullMenuOpen && !(event.target instanceof Element && event.target.closest(".pull-split"))) {
      pullMenuOpen = false;
    }
  }}
/>

<main class="shell" class:launchpad-mode={centerMode === "launchpad"}>
  <!-- Dragging the empty chrome moves the window: with an overlay title
       bar there is no system title bar left to grab. Tauri checks the
       event target itself, not its ancestors, so every container that
       covers empty header space carries the attribute; the tabs and
       buttons inside still take clicks. -->
  <header class="app-header" data-tauri-drag-region>
    <nav class="tabs" aria-label="Open repositories" data-tauri-drag-region>
      {#each tabs as tab}
        <div class="tab-wrap">
          <button class="tab {tab.id === activeTabId ? 'active' : ''}" on:click={() => switchToTab(tab)}>
            {#if tab.kind === "repo"}
              <svg class="tab-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" /></svg>
            {/if}
            <span>{tab.label}</span>
          </button>
          {#if tab.kind === "repo"}
            <button class="tab-close" on:click={() => closeTab(tab)} aria-label={`Close ${tab.label}`}>×</button>
          {/if}
        </div>
      {/each}
      <button class="tab add-tab" title="Open repository" on:click={showAddRepoNotice}>+</button>
    </nav>
    <div class="account-strip" data-tauri-drag-region>
      <button title="Notifications" on:click={() => (notice = "No notifications")}>◔</button>
      <button title="Settings" on:click={() => (settingsOpen = true)}>⚙</button>
      <strong data-tauri-drag-region>{accountName}</strong>
    </div>
  </header>

  {#if centerMode !== "launchpad"}
    <div class="repo-bar">
      <div class="repo-group">
        <button class="repo-select" on:click={switchRepository}>
          <span>repository</span>
          <strong>{repoName}</strong>
        </button>
        <div class="branch-select">
          <span>branch</span>
          <strong>{currentBranch}</strong>
        </div>
      </div>
      <div class="top-actions">
        <button title={undoItem ? `Undo ${undoItem.entry.label} (⌘Z)` : "Nothing to undo"} on:click={() => replay("undo")} disabled={busy || !undoItem}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" /></svg>
          <span>Undo</span>
        </button>
        <button title={redoItem ? `Redo ${redoItem.entry.label} (⇧⌘Z)` : "Nothing to redo"} on:click={() => replay("redo")} disabled={busy || !redoItem}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" /><path d="M21 3v5h-5" /></svg>
          <span>Redo</span>
        </button>
        <div class="pull-split">
          <button title={pullOptions.find((option) => option.kind === settings.pullMode)?.label ?? "Pull"} on:click={() => execute({ kind: settings.pullMode }, "Pull")} disabled={busy}>
            <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v13" /><path d="m6 10 6 6 6-6" /><path d="M5 21h14" /></svg>
            <span>Pull</span>
          </button>
          <button class="pull-caret" title="Pull options" aria-haspopup="menu" class:active={pullMenuOpen} on:click={() => (pullMenuOpen = !pullMenuOpen)} disabled={busy}>
            <svg class="caret-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M5 9h14l-7 8z" /></svg>
          </button>
          {#if pullMenuOpen}
            <div class="dropdown-menu pull-menu" role="menu">
              <button class="menu-plain" on:click={() => runPullMenu({ kind: "fetchAll" }, "Fetch all remotes")}>Fetch All</button>
              {#each pullOptions as option (option.kind)}
                <div class="menu-row">
                  <button
                    class="menu-default"
                    class:checked={settings.pullMode === option.kind}
                    title="Use for the Pull button"
                    aria-label={`Use ${option.label} for the Pull button`}
                    on:click={() => setPullMode(option.kind)}
                  ></button>
                  <button on:click={() => runPullMenu({ kind: option.kind }, option.label)}>{option.label}</button>
                </div>
              {/each}
            </div>
          {/if}
        </div>
        <button title="Push" on:click={() => execute({ kind: "push" }, "Push")} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 16V3" /><path d="m6 9 6-6 6 6" /><path d="M5 21h14" /></svg>
          <span>Push</span>
        </button>
        <button title="Branch" on:click={createBranchFromToolbar} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" /></svg>
          <span>Branch</span>
        </button>
        <button title={totalChanges ? "Stash all changes" : "Nothing to stash"} on:click={() => execute({ kind: "stashCreate", message: "gitc stash" }, "Create stash")} disabled={busy || totalChanges === 0}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v9" /><path d="m8 8 4 4 4-4" /><path d="M3 14h5l1.5 2.5h5L16 14h5v7H3z" /></svg>
          <span>Stash</span>
        </button>
        <button title={state?.stashes.length ? `Pop ${state.stashes[0].message}` : "No stashes"} on:click={() => execute({ kind: "stashPop", target: "stash@{0}" }, "Pop stash")} disabled={busy || !state?.stashes.length}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 12V3" /><path d="m8 7 4-4 4 4" /><path d="M3 14h5l1.5 2.5h5L16 14h5v7H3z" /></svg>
          <span>Pop</span>
        </button>
        <span class="toolbar-sep" aria-hidden="true"></span>
        <button title="Terminal" on:click={openRepoTerminal} disabled={busy}>
          <svg class="toolbar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5" /><line x1="12" y1="19" x2="20" y2="19" /></svg>
          <span>Terminal</span>
        </button>
      </div>
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
            <button on:click={() => { actionsOpen = false; void refresh(); }} disabled={busy}>Refresh</button>
            <button on:click={() => { actionsOpen = false; openCompare(null, currentBranch); }}>Compare Refs…</button>
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
            <span class="section-actions">
              <button
                class="section-action"
                title="Delete merged, squash-merged and gone branches…"
                on:click={() => (cleanupOpen = true)}
                disabled={busy}
              >clean up</button>
            </span>
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
                        <span use:hoverExpand={{ text: row.label }}>{row.label}</span>
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
                    <span class="branch-name-line"><svg class="tree-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" /></svg> <span use:hoverExpand={{ text: row.label }}>{row.label}</span></span>
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
            <span class="section-actions">
              {#if (state?.worktrees ?? []).some((entry) => entry.prunable)}
                <button
                  class="section-action"
                  title="Prune worktrees whose directories are gone"
                  on:click={() => execute({ kind: "worktreePrune" }, "Prune worktrees")}
                  disabled={busy}
                >prune</button>
              {/if}
              <button class="section-action" title="Add worktree…" on:click={addWorktreePrompt} disabled={busy}>+</button>
            </span>
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
                        <span class="wt-branch" use:hoverExpand={{ text: worktreeLabel(worktree) }}>{worktreeLabel(worktree)}</span>
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
                    <small use:hoverExpand={{ text: stash.message }}>{stash.message}</small>
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
                    <span use:hoverExpand={{ text: `⌖ ${tag}` }}>⌖ {tag}</span>
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
              <svg class="wip-node" viewBox="0 0 22 22" aria-hidden="true">
                <circle cx="11" cy="11" r="9.6" pathLength="128" />
              </svg>
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
            class:head={row.head}
            class:date-break={rowIndex > 0 && visibleGraphRows[rowIndex - 1].dateBucket !== row.dateBucket}
            style={`--row-color:${row.color}`}
            title={commitRowTitle(row)}
            on:click={(event) => handleCommitRowClick(event, row, rowIndex)}
          >
            <span
              class="branch-cell"
              class:linked={row.refs.length > 0}
              class:head={row.head}
              style={`--ref-color:${row.color}`}
              title={row.labels.join("  ")}
            >
              {#if row.refs.length}
                {@const ref = row.refs[0]}
                {@const githubRemote = ref.remotes.find((remote) => isGithubUrl(state?.remoteUrls?.[remote]))}
                <span class="ref-pill" class:head={ref.head} style={`--ref-color:${row.color}`}>
                  {#if ref.head}<i class="pill-check">✓</i>{/if}
                  <span class="pill-label" use:hoverExpand={{ text: ref.name }}>{ref.name}</span>
                  {#if ref.tag}
                    <svg class="pill-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-label="tag"><path d="M12.6 2.6A2 2 0 0 0 11.2 2H4a2 2 0 0 0-2 2v7.2a2 2 0 0 0 .6 1.4l8.7 8.7a2.4 2.4 0 0 0 3.4 0l6.6-6.6a2.4 2.4 0 0 0 0-3.4z" /><circle cx="7.5" cy="7.5" r="1" fill="currentColor" /></svg>
                  {:else if ref.local && isWorktreeBranch(ref.name)}
                    <svg class="pill-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-label="checked out in another worktree"><path d="m17 14 3 3.3a1 1 0 0 1-.7 1.7H4.7a1 1 0 0 1-.7-1.7L7 14h-.3a1 1 0 0 1-.7-1.7L9 9h-.2A1 1 0 0 1 8 7.3L12 3l4 4.3a1 1 0 0 1-.8 1.7H15l3 3.3a1 1 0 0 1-.7 1.7H17Z" /><path d="M12 22v-3" /></svg>
                  {:else if ref.local}
                    <svg class="pill-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-label="local"><rect x="4" y="5" width="16" height="11" rx="1.5" /><path d="M2 19.5h20" /></svg>
                  {/if}
                  {#if githubRemote}
                    {@const ownerAvatar = remoteAvatarFor(state?.remoteUrls?.[githubRemote], settings.showAvatars, failedAvatars)}
                    {#if ownerAvatar}
                      <img class="pill-avatar" src={ownerAvatar} alt="GitHub" loading="lazy" on:error={() => avatarFailed(ownerAvatar)} />
                    {:else}
                      <svg class="pill-icon" viewBox="0 0 16 16" fill="currentColor" aria-label="GitHub"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z" /></svg>
                    {/if}
                  {/if}
                  {#if ref.remotes.some((remote) => !isGithubUrl(state?.remoteUrls?.[remote]))}
                    <svg class="pill-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-label={ref.remotes.join(", ")}><path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" /></svg>
                  {/if}
                </span>
                {#if row.refs.length > 1}
                  <span class="ref-pill more-pill" style={`--ref-color:${row.color}`}>+{row.refs.length - 1}</span>
                {/if}
              {/if}
            </span>
            <span class="graph-cell" style={`--lane-count:${graphLaneCount}`}>
              <span class="graph-tint" style={`--lane:${row.lane}; --lane-color:${row.color}`}></span>
              {#if row.refs.length}
                <span class="ref-link" class:head={row.head} style={`--lane:${row.lane}; --lane-color:${row.color}`}></span>
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
              {#each row.joins as join}
                <span class="graph-join" style={`--from:${join.to}; --span:${join.from - join.to}; --lane-color:${join.color}`}></span>
              {/each}
              {#if row.commit.parents.length > 1}
                <span class="commit-dot merge" style={`--lane:${row.lane}; --lane-color:${row.color}`}></span>
              {:else}
                {@const avatar = avatarFor(row.commit.email, settings.showAvatars, failedAvatars)}
                <span class="commit-dot" style={`--lane:${row.lane}; --lane-color:${row.color}`}>
                  {authorInitials(row.commit.author)}
                  {#if avatar}<img src={avatar} alt="" loading="lazy" on:error={() => avatarFailed(avatar)} />{/if}
                </span>
              {/if}
            </span>
            <span class="commit-main">
              <strong>
                <span use:hoverExpand={{ text: row.commit.subject }}>{row.commit.subject}</span>
                {#if row.commit.bodySummary}
                  <span class="commit-summary">
                    <em use:hoverExpand={{ text: row.commit.bodySummary }}>{row.commit.bodySummary}</em>
                  </span>
                {/if}
              </strong>
              {#if rowIndex > 0 && visibleGraphRows[rowIndex - 1].dateBucket !== row.dateBucket}
                <span class="date-marker">{row.dateBucket}</span>
              {/if}
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
              commit: <strong>{selectedCommit.hash.slice(0, 6)}</strong>
            </button>
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
                <span class="author-badge">
                  {authorInitials(commitDetail.author)}
                  {#if detailAvatar}<img src={detailAvatar} alt="" on:error={() => detailAvatar && avatarFailed(detailAvatar)} />{/if}
                </span>
                <div class="commit-author-main">
                  <strong>{commitDetail.author}</strong>
                  <small>authored {commitDetail.date}</small>
                </div>
                {#if commitDetail.parents.length}
                  <div class="commit-parent">
                    <span>{commitDetail.parents.length === 1 ? "parent" : "parents"}:</span>
                    <strong>{commitDetail.parents.map((parent) => parent.slice(0, 6)).join(", ")}</strong>
                  </div>
                {/if}
              </div>

              {#if coAuthors.length}
                <div class="commit-coauthors">
                  <span>Co-authors:</span>
                  {#each coAuthors as person (person.email)}
                    {@const url = avatarFor(person.email, settings.showAvatars, failedAvatars)}
                    <span class="author-badge coauthor-badge" title={`${person.name} <${person.email}>`}>
                      {authorInitials(person.name)}
                      {#if url}<img src={url} alt="" on:error={() => avatarFailed(url)} />{/if}
                    </span>
                  {/each}
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
                <label class="view-all-files">
                  <input type="checkbox" bind:checked={viewAllFiles} />
                  View all files
                </label>
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
                        class:unchanged={!row.item.status}
                        style={`--depth:${row.depth}`}
                        title={row.item.status ? `${statusLabel(row.item.status)}: ${row.item.path}` : row.item.path}
                        disabled={!row.item.status}
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
                    {@const parts = splitPath(change.path)}
                    <button
                      class="commit-file-row"
                      class:active={commitFilePath === change.path && diffContext === "commit"}
                      class:unchanged={!change.status}
                      title={change.status ? `${statusLabel(change.status)}: ${change.path}` : change.path}
                      disabled={!change.status}
                      on:click={() => openCommitFile(change)}
                    >
                      <span class={commitStatusGlyph(change.status).cls}>{commitStatusGlyph(change.status).glyph}</span>
                      <strong>{#if parts.dir}<em class="path-dir">{parts.dir}</em>{/if}{parts.name}</strong>
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
          <label class="checkbox">
            <input type="checkbox" bind:checked={settings.showAvatars} />
            Show author avatars (loaded from GitHub and Gravatar)
          </label>
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
