import type {
  BranchAudit,
  BranchCleanupReport,
  CommitDetail,
  CommitFileChange,
  CommitNode,
  ConflictFile,
  FileDiff,
  FileStatus,
  GhRepo,
  GhStatus,
  GitAction,
  GitResult,
  RefCompare,
  RepositoryState,
  StashEntry,
  Worktree,
} from "./types";

// In-memory demo repository used when the UI runs without a Tauri backend.
// Mutating actions update this state so the whole workflow stays interactive.

interface DemoCommit extends CommitNode {
  email: string;
  date: string;
  body: string;
  files: CommitFileChange[];
}

const authors = [
  { name: "Christine Ware", email: "christine@wareness.com" },
  { name: "Dillon Cordova", email: "dillon@wareness.com" },
  { name: "Priya Natarajan", email: "priya@wareness.com" },
];

function commit(
  hash: string,
  parents: string[],
  refs: string[],
  authorIndex: number,
  relativeDate: string,
  date: string,
  subject: string,
  body: string,
  files: CommitFileChange[],
): DemoCommit {
  const author = authors[authorIndex % authors.length];
  return {
    hash,
    shortHash: hash.slice(0, 8),
    parents,
    refs,
    author: author.name,
    email: author.email,
    relativeDate,
    date,
    subject,
    body,
    bodySummary: body.split("\n")[0] ?? "",
    files,
  };
}

const h = (n: number) => n.toString(16).padStart(2, "0").repeat(20);

const demoCommits: DemoCommit[] = [
  commit(h(0x11), [h(0x12)], ["HEAD -> feature/commit-details", "origin/feature/commit-details"], 0, "2 hours ago", "2026-07-12 09:41", "feat: commit detail panel with file diffs", "Show metadata, changed files, and per-file diffs when a commit is selected in the graph.", [
    { status: "M", path: "src/App.svelte" },
    { status: "A", path: "src/lib/CommitDetail.svelte" },
    { status: "M", path: "src/styles.css" },
  ]),
  commit(h(0x12), [h(0x14)], [], 0, "5 hours ago", "2026-07-12 06:18", "feat: stash management in left panel", "Apply, pop, and drop stashes without leaving the graph.", [
    { status: "M", path: "src/App.svelte" },
    { status: "M", path: "src-tauri/src/lib.rs" },
  ]),
  commit(h(0x13), [h(0x14)], ["main", "origin/main"], 1, "yesterday", "2026-07-11 17:02", "fix: keep graph lanes stable across merges", "Lane assignment now reuses freed columns so long histories stay compact.", [
    { status: "M", path: "src/App.svelte" },
  ]),
  commit(h(0x14), [h(0x15), h(0x16)], [], 1, "2 days ago", "2026-07-10 11:24", "Merge branch 'feature/hunk-staging'", "", [
    { status: "M", path: "src/App.svelte" },
    { status: "M", path: "src-tauri/src/lib.rs" },
  ]),
  commit(h(0x15), [h(0x17)], [], 1, "2 days ago", "2026-07-10 09:12", "chore: tighten porcelain v2 parsing", "Handles partially staged files by splitting them into both sections.", [
    { status: "M", path: "src-tauri/src/lib.rs" },
  ]),
  commit(h(0x16), [h(0x17)], ["feature/hunk-staging"], 2, "3 days ago", "2026-07-09 15:45", "feat: stage, unstage, and discard hunks", "Selected hunk is extracted into a minimal patch and applied with git apply.", [
    { status: "M", path: "src/App.svelte" },
    { status: "M", path: "src/lib/git.ts" },
    { status: "M", path: "src-tauri/src/lib.rs" },
  ]),
  commit(h(0x17), [h(0x18)], ["tag:v0.2.0"], 0, "4 days ago", "2026-07-08 10:31", "feat: merge editor with explicit save", "Base, ours, theirs, and an editable resolution pane.", [
    { status: "A", path: "src/lib/ReadonlyPane.svelte" },
    { status: "M", path: "src/App.svelte" },
  ]),
  commit(h(0x18), [h(0x19), h(0x1a)], [], 2, "5 days ago", "2026-07-07 13:58", "Merge branch 'feature/graph'", "", [
    { status: "M", path: "src/App.svelte" },
  ]),
  commit(h(0x19), [h(0x1b)], [], 0, "6 days ago", "2026-07-06 16:20", "feat: conflict detection and resolution flow", "Conflicted files open a four-pane merge editor.", [
    { status: "M", path: "src-tauri/src/lib.rs" },
    { status: "M", path: "src/lib/types.ts" },
  ]),
  commit(h(0x1a), [h(0x1b)], ["feature/graph"], 1, "6 days ago", "2026-07-06 09:03", "feat: visual commit graph with lanes and merges", "Topological order with colored rails, merge edges, and author initials.", [
    { status: "M", path: "src/App.svelte" },
    { status: "M", path: "src/styles.css" },
  ]),
  commit(h(0x1b), [h(0x1c)], [], 2, "last week", "2026-07-04 14:11", "feat: branch checkout, create, and upstream display", "", [
    { status: "M", path: "src/App.svelte" },
    { status: "M", path: "src-tauri/src/lib.rs" },
  ]),
  commit(h(0x1c), [h(0x1d)], ["tag:v0.1.0"], 0, "2 weeks ago", "2026-06-28 10:44", "feat: status groups with stage and discard actions", "", [
    { status: "A", path: "src/lib/FileGroup.svelte" },
    { status: "M", path: "src/App.svelte" },
  ]),
  commit(h(0x1d), [h(0x1e)], [], 1, "2 weeks ago", "2026-06-27 09:30", "Basic Version", "", [
    { status: "A", path: "src/App.svelte" },
    { status: "A", path: "src-tauri/src/lib.rs" },
    { status: "A", path: "package.json" },
  ]),
  commit(h(0x1e), [], [], 1, "3 weeks ago", "2026-06-21 08:15", "Initial commit", "", [
    { status: "A", path: "README.md" },
    { status: "A", path: ".gitignore" },
  ]),
];

const file = (path: string, index: string, worktree: string, group: FileStatus["group"]): FileStatus => ({
  path,
  index,
  worktree,
  group,
});

const newDemoBranch = (name: string, current: boolean, upstream: string | null) => ({
  name,
  current,
  upstream,
  upstreamGone: false,
  ahead: 0,
  behind: 0,
  lastCommitUnix: Math.floor(Date.now() / 1000),
  lastCommitRelative: "just now",
});

interface DemoState {
  root: string;
  currentBranch: string;
  merging: boolean;
  rebasing: boolean;
  files: FileStatus[];
  stashes: StashEntry[];
  tags: string[];
  branches: {
    name: string;
    current: boolean;
    upstream?: string | null;
    upstreamGone: boolean;
    ahead: number;
    behind: number;
    lastCommitUnix: number;
    lastCommitRelative: string;
  }[];
  worktrees: Worktree[];
}

const demo: DemoState = {
  root: "/Users/christine/dev/gitc",
  currentBranch: "feature/commit-details",
  merging: false,
  rebasing: false,
  files: [
    file("src/App.svelte", ".", "M", "unstaged"),
    file("src/styles.css", "M", ".", "staged"),
    file("src/lib/CommitDetail.svelte", "A", ".", "staged"),
    file("src/lib/demo-notes.md", "?", "?", "untracked"),
    file("docs/roadmap.md", ".", "M", "unstaged"),
  ],
  stashes: [
    { name: "stash@{0}", message: "WIP on feature/commit-details: experiment with lane colors" },
    { name: "stash@{1}", message: "On main: half-finished settings modal" },
  ],
  tags: ["v0.2.0", "v0.1.0"],
  branches: [
    {
      name: "feature/commit-details",
      current: true,
      upstream: "origin/feature/commit-details",
      upstreamGone: false,
      ahead: 1,
      behind: 0,
      lastCommitUnix: 1783863660,
      lastCommitRelative: "2 hours ago",
    },
    {
      name: "feature/hunk-staging",
      current: false,
      upstream: null,
      upstreamGone: false,
      ahead: 0,
      behind: 0,
      lastCommitUnix: 1783626300,
      lastCommitRelative: "3 days ago",
    },
    {
      name: "feature/graph",
      current: false,
      upstream: null,
      upstreamGone: false,
      ahead: 0,
      behind: 0,
      lastCommitUnix: 1783342980,
      lastCommitRelative: "6 days ago",
    },
    {
      name: "release/0.2",
      current: false,
      upstream: "origin/release/0.2",
      upstreamGone: false,
      ahead: 2,
      behind: 0,
      lastCommitUnix: 1783803720,
      lastCommitRelative: "yesterday",
    },
    {
      name: "hotfix/old-login",
      current: false,
      upstream: "origin/hotfix/old-login",
      upstreamGone: true,
      ahead: 0,
      behind: 0,
      lastCommitUnix: 1781964000,
      lastCommitRelative: "3 weeks ago",
    },
    {
      name: "experiment/lanes",
      current: false,
      upstream: null,
      upstreamGone: false,
      ahead: 0,
      behind: 0,
      lastCommitUnix: 1777636800,
      lastCommitRelative: "2 months ago",
    },
    {
      name: "main",
      current: false,
      upstream: "origin/main",
      upstreamGone: false,
      ahead: 0,
      behind: 0,
      lastCommitUnix: 1783803720,
      lastCommitRelative: "yesterday",
    },
  ],
  worktrees: [
    {
      path: "/Users/christine/dev/gitc",
      head: "d41f22a1",
      branch: "feature/commit-details",
      detached: false,
      bare: false,
      current: true,
      main: true,
      locked: false,
      lockReason: null,
      prunable: false,
    },
    {
      path: "/Users/christine/dev/gitc-release",
      head: "9c31e07b",
      branch: "release/0.2",
      detached: false,
      bare: false,
      current: false,
      main: false,
      locked: false,
      lockReason: null,
      prunable: false,
    },
    {
      path: "/Volumes/scratch/gitc-bisect",
      head: "77aa41c0",
      branch: null,
      detached: true,
      bare: false,
      current: false,
      main: false,
      locked: false,
      lockReason: null,
      prunable: true,
    },
  ],
};

const demoDiffs: Record<string, string> = {
  "src/App.svelte": `diff --git a/src/App.svelte b/src/App.svelte
index 72fec28..a1b2c3d 100644
--- a/src/App.svelte
+++ b/src/App.svelte
@@ -50,9 +50,12 @@
   let state: RepositoryState | null = null;
   let commits: CommitNode[] = [];
-  let selectedCommit: CommitNode | null = null;
+  let selectedCommit: CommitNode | null = null;
+  let commitDetail: CommitDetail | null = null;
+  let commitFileDiff: FileDiff | null = null;
   let selectedFile: FileStatus | null = null;
   let selectedDiff: FileDiff | null = null;
@@ -128,6 +131,18 @@
   async function refresh() {
     busy = true;
     error = "";
+    try {
+      const detail = await getCommitDetail(hash);
+      commitDetail = detail;
+    } catch (err) {
+      error = String(err);
+    }
     try {
       const [nextState, graph] = await Promise.all([
         getRepositoryState(),
`,
  "docs/roadmap.md": `diff --git a/docs/roadmap.md b/docs/roadmap.md
index 9e8ec13..2f1a4b7 100644
--- a/docs/roadmap.md
+++ b/docs/roadmap.md
@@ -1,7 +1,9 @@
 # Roadmap

-- [ ] commit detail panel
+- [x] commit detail panel
+- [x] stash management
 - [ ] interactive rebase
-- [ ] submodule support
+- [ ] submodule support (blocked on design)
+- [ ] partial clone support
`,
  "src/styles.css": `diff --git a/src/styles.css b/src/styles.css
index 43da9ef..b7fa724 100644
--- a/src/styles.css
+++ b/src/styles.css
@@ -1160,6 +1160,22 @@
 .commit-button {
   width: 100%;
   min-height: 48px;
+}
+
+.commit-detail {
+  display: grid;
+  grid-template-rows: max-content max-content minmax(0, 1fr);
+  min-height: 0;
+  overflow: hidden;
 }
`,
  "src/lib/CommitDetail.svelte": `diff --git a/src/lib/CommitDetail.svelte b/src/lib/CommitDetail.svelte
new file mode 100644
--- /dev/null
+++ b/src/lib/CommitDetail.svelte
@@ -0,0 +1,12 @@
+<script lang="ts">
+  import type { CommitDetail } from "./types";
+
+  export let detail: CommitDetail;
+</script>
+
+<div class="commit-detail">
+  <h2>{detail.subject}</h2>
+  <p>{detail.author} · {detail.date}</p>
+</div>
`,
};

const untrackedContent = `# Demo notes

Scratch notes for the demo repository.
- The browser build uses an in-memory backend.
- Every mutation updates this fake state.
`;

// ---- F3: gh clone ----
// The demo backend always looks like a signed-in `gh` with a small seeded
// account so the CloneDialog's GitHub tab is exercisable in `npm run dev:ui`.
const demoGhLogin = "christine";

const demoGhRepos: GhRepo[] = [
  {
    name: "gitc",
    nameWithOwner: "christine/gitc",
    owner: "christine",
    description: "A desktop git client built for the agent era",
    isPrivate: false,
    isFork: false,
    isArchived: false,
    pushedAt: "2026-09-05T19:24:10Z",
    url: "https://github.com/christine/gitc",
    sshUrl: "git@github.com:christine/gitc.git",
    language: "Rust",
    defaultBranch: "main",
  },
  {
    name: "data-layer",
    nameWithOwner: "christine/data-layer",
    owner: "christine",
    description: "Internal data platform services",
    isPrivate: true,
    isFork: false,
    isArchived: false,
    pushedAt: "2026-09-04T08:12:00Z",
    url: "https://github.com/christine/data-layer",
    sshUrl: "git@github.com:christine/data-layer.git",
    language: "TypeScript",
    defaultBranch: "main",
  },
  {
    name: "waas",
    nameWithOwner: "christine/waas",
    owner: "christine",
    description: null,
    isPrivate: true,
    isFork: false,
    isArchived: false,
    pushedAt: "2026-08-20T14:05:00Z",
    url: "https://github.com/christine/waas",
    sshUrl: "git@github.com:christine/waas.git",
    language: "Go",
    defaultBranch: "main",
  },
  {
    name: "dotfiles",
    nameWithOwner: "christine/dotfiles",
    owner: "christine",
    description: "Personal shell and editor configuration",
    isPrivate: false,
    isFork: true,
    isArchived: false,
    pushedAt: "2026-06-11T10:41:00Z",
    url: "https://github.com/christine/dotfiles",
    sshUrl: "git@github.com:christine/dotfiles.git",
    language: "Shell",
    defaultBranch: "main",
  },
  {
    name: "old-render-engine",
    nameWithOwner: "christine/old-render-engine",
    owner: "christine",
    description: "Archived prototype renderer",
    isPrivate: false,
    isFork: false,
    isArchived: true,
    pushedAt: "2025-01-04T09:00:00Z",
    url: "https://github.com/christine/old-render-engine",
    sshUrl: "git@github.com:christine/old-render-engine.git",
    language: "C++",
    defaultBranch: "master",
  },
  {
    name: "gitc-plugins",
    nameWithOwner: "osfmanagement/gitc-plugins",
    owner: "osfmanagement",
    description: "Community plugins for gitc",
    isPrivate: false,
    isFork: false,
    isArchived: false,
    pushedAt: "2026-09-01T17:30:00Z",
    url: "https://github.com/osfmanagement/gitc-plugins",
    sshUrl: "git@github.com:osfmanagement/gitc-plugins.git",
    language: "TypeScript",
    defaultBranch: "main",
  },
];

function ok(stdout = ""): GitResult {
  return { ok: true, stdout, stderr: "", code: 0, refresh: true };
}

function fail(stderr: string): GitResult {
  // The real backend refreshes after failed git commands too (state may have changed).
  return { ok: false, stdout: "", stderr, code: 1, refresh: true };
}

function repositoryState(): RepositoryState {
  return {
    root: demo.root,
    currentBranch: demo.currentBranch,
    head: demoCommits[0].shortHash,
    merging: demo.merging,
    rebasing: demo.rebasing,
    files: demo.files.map((entry) => ({ ...entry })),
    branches: demo.branches.map((entry) => ({ ...entry })),
    remotes: ["origin"],
    remoteBranches: ["origin/main", "origin/feature/commit-details", "origin/release/0.2"],
    tags: [...demo.tags],
    worktrees: demo.worktrees.map((entry) => ({ ...entry, current: entry.path === demo.root })),
    stashes: demo.stashes.map((entry) => ({ ...entry })),
    userName: "Christine Ware",
  };
}

// ---- F1: branch & worktree cleanup ----

function demoBranchHead(name: string): string {
  const onGraph = demoCommits.find((entry) => entry.refs.some((ref) => ref === name || ref === `HEAD -> ${name}`));
  if (onGraph) return onGraph.hash;
  const fallback: Record<string, string> = {
    "release/0.2": h(0x30),
    "hotfix/old-login": h(0x31),
    "experiment/lanes": h(0x32),
  };
  return fallback[name] ?? h(0x2f);
}

type CleanupOverride = Pick<BranchAudit, "classification"> &
  Partial<Pick<BranchAudit, "merged" | "squashMerged" | "stale" | "aheadOfBase" | "behindBase">>;

// Hard-coded per PLAN.md: feature/graph is merged, feature/hunk-staging is
// squash-merged, release/0.2 is active, hotfix/old-login's upstream is gone,
// and experiment/lanes is stale. Real classification math lives in the Rust
// backend (src-tauri/src/cleanup.rs); this just gives the demo UI something
// representative to render.
const cleanupOverrides: Record<string, CleanupOverride> = {
  "feature/commit-details": { classification: "current" },
  main: { classification: "base" },
  "feature/graph": { classification: "merged", merged: true, aheadOfBase: 0, behindBase: 4 },
  "feature/hunk-staging": { classification: "squashMerged", squashMerged: true, aheadOfBase: 2, behindBase: 5 },
  "release/0.2": { classification: "active", aheadOfBase: 3, behindBase: 1 },
  "hotfix/old-login": { classification: "gone", aheadOfBase: 1, behindBase: 6 },
  "experiment/lanes": { classification: "stale", stale: true, aheadOfBase: 2, behindBase: 9 },
};

function branchCleanupReport(base: string | null, staleDays: number | null): BranchCleanupReport {
  const resolvedBase = base?.trim() || "main";
  const rawStaleDays = staleDays == null || Number.isNaN(staleDays) ? 30 : Math.round(staleDays);
  const resolvedStaleDays = Math.min(3650, Math.max(1, rawStaleDays));

  const branches: BranchAudit[] = demo.branches.map((branch) => {
    const override = cleanupOverrides[branch.name] ?? { classification: "active" };
    const worktree = demo.worktrees.find((entry) => !entry.main && entry.branch === branch.name);
    const head = demoBranchHead(branch.name);
    return {
      name: branch.name,
      current: branch.current,
      isBase: branch.name === resolvedBase,
      head,
      shortHead: head.slice(0, 7),
      upstream: branch.upstream ?? null,
      upstreamGone: branch.upstreamGone,
      ahead: branch.ahead,
      behind: branch.behind,
      aheadOfBase: override.aheadOfBase ?? 0,
      behindBase: override.behindBase ?? 0,
      merged: override.merged ?? false,
      squashMerged: override.squashMerged ?? false,
      stale: override.stale ?? false,
      lastCommitUnix: branch.lastCommitUnix,
      lastCommitRelative: branch.lastCommitRelative,
      worktreePath: worktree?.path ?? null,
      classification: override.classification,
    };
  });

  return { base: resolvedBase, staleDays: resolvedStaleDays, branches };
}

function removeFile(path: string, group?: FileStatus["group"]) {
  demo.files = demo.files.filter((entry) => entry.path !== path || (group ? entry.group !== group : false));
}

function runAction(action: GitAction): GitResult {
  switch (action.kind) {
    case "stage": {
      if (action.path === ".") {
        demo.files = demo.files
          .filter((entry) => entry.group === "staged" || entry.group === "unstaged" || entry.group === "untracked")
          .map((entry) => (entry.group === "staged" ? entry : file(entry.path, "M", ".", "staged")));
        return ok();
      }
      const target = demo.files.find((entry) => entry.path === action.path && entry.group !== "staged");
      if (!target) return fail(`pathspec '${action.path}' did not match any files`);
      removeFile(target.path);
      demo.files = [...demo.files, file(target.path, target.group === "untracked" ? "A" : "M", ".", "staged")];
      return ok();
    }
    case "unstage": {
      if (action.path === ".") {
        demo.files = demo.files.map((entry) =>
          entry.group === "staged" ? file(entry.path, ".", "M", "unstaged") : entry,
        );
        return ok();
      }
      const target = demo.files.find((entry) => entry.path === action.path && entry.group === "staged");
      if (!target) return fail(`'${action.path}' is not staged`);
      removeFile(target.path, "staged");
      demo.files = [...demo.files, file(target.path, ".", "M", "unstaged")];
      return ok();
    }
    case "discard":
    case "cleanUntracked":
      removeFile(action.path ?? "");
      return ok();
    case "markResolved": {
      const target = demo.files.find((entry) => entry.path === action.path && entry.group === "conflicted");
      if (target) {
        removeFile(target.path);
        demo.files = [...demo.files, file(target.path, "M", ".", "staged")];
      }
      if (!demo.files.some((entry) => entry.group === "conflicted")) demo.merging = false;
      return ok();
    }
    case "commit":
    case "commitAmend":
      demo.files = demo.files.filter((entry) => entry.group !== "staged");
      return ok();
    case "checkoutBranch": {
      const branch = demo.branches.find((entry) => entry.name === action.branch);
      if (!branch) return fail(`branch '${action.branch}' not found`);
      demo.branches = demo.branches.map((entry) => ({ ...entry, current: entry.name === action.branch }));
      demo.currentBranch = branch.name;
      return ok();
    }
    case "createBranch": {
      if (demo.branches.some((entry) => entry.name === action.branch)) {
        return fail(`a branch named '${action.branch}' already exists`);
      }
      demo.branches = [
        ...demo.branches.map((entry) => ({ ...entry, current: false })),
        newDemoBranch(action.branch ?? "new-branch", true, null),
      ];
      demo.currentBranch = action.branch ?? "new-branch";
      return ok();
    }
    case "deleteBranch":
    case "deleteBranchForce": {
      const branch = demo.branches.find((entry) => entry.name === action.branch);
      if (!branch) return fail(`branch '${action.branch}' not found`);
      if (branch.current) return fail(`cannot delete the checked out branch '${action.branch}'`);
      // Mirror real git: a plain `-d` refuses on anything that isn't an
      // ancestor of main; `-D` (deleteBranchForce) always succeeds.
      const merged = cleanupOverrides[branch.name]?.merged ?? false;
      if (action.kind === "deleteBranch" && !merged) {
        return fail(
          `error: The branch '${branch.name}' is not fully merged.\nIf you are sure you want to delete it, run 'git branch -D ${branch.name}'.`,
        );
      }
      demo.branches = demo.branches.filter((entry) => entry.name !== action.branch);
      return ok();
    }
    case "checkoutRemote": {
      const local = (action.target ?? "").split("/").slice(1).join("/") || "tracked";
      demo.branches = [
        ...demo.branches.map((entry) => ({ ...entry, current: false })),
        newDemoBranch(local, true, action.target ?? null),
      ];
      demo.currentBranch = local;
      return ok();
    }
    case "checkoutCommit":
      demo.currentBranch = "";
      demo.branches = demo.branches.map((entry) => ({ ...entry, current: false }));
      return ok(`HEAD is now at ${(action.target ?? "").slice(0, 8)}`);
    case "createTag":
      if (demo.tags.includes(action.branch ?? "")) return fail(`tag '${action.branch}' already exists`);
      demo.tags = [action.branch ?? "tag", ...demo.tags];
      return ok();
    case "deleteTag":
      demo.tags = demo.tags.filter((tag) => tag !== action.branch);
      return ok();
    case "stashCreate":
      demo.stashes = [
        { name: "stash@{0}", message: `On ${demo.currentBranch || "HEAD"}: ${action.message ?? "gitc stash"}` },
        ...demo.stashes.map((entry, index) => ({ ...entry, name: `stash@{${index + 1}}` })),
      ];
      demo.files = demo.files.filter((entry) => entry.group === "conflicted");
      return ok();
    case "stashApply":
      demo.files = [...demo.files, file("src/lib/settings.ts", ".", "M", "unstaged")];
      return ok();
    case "stashPop":
      demo.files = [...demo.files, file("src/lib/settings.ts", ".", "M", "unstaged")];
      demo.stashes = demo.stashes
        .filter((entry) => entry.name !== action.target)
        .map((entry, index) => ({ ...entry, name: `stash@{${index}}` }));
      return ok();
    case "stashDrop":
      demo.stashes = demo.stashes
        .filter((entry) => entry.name !== action.target)
        .map((entry, index) => ({ ...entry, name: `stash@{${index}}` }));
      return ok();
    case "merge":
      demo.merging = true;
      demo.files = [...demo.files, file("src/lib/git.ts", "U", "U", "conflicted")];
      return fail(`CONFLICT (content): merge conflict in src/lib/git.ts\nAutomatic merge failed; fix conflicts and then commit the result.`);
    case "mergeContinue":
      if (demo.files.some((entry) => entry.group === "conflicted")) {
        return fail("cannot continue: unresolved conflicts remain");
      }
      demo.merging = false;
      return ok();
    case "mergeAbort":
      demo.merging = false;
      demo.files = demo.files.filter((entry) => entry.group !== "conflicted");
      return ok();
    case "rebaseContinue":
      demo.rebasing = false;
      return ok();
    case "rebaseAbort":
      demo.rebasing = false;
      demo.files = demo.files.filter((entry) => entry.group !== "conflicted");
      return ok();
    case "worktreeAdd": {
      const path = action.path?.trim();
      if (!path) return fail("worktree path is required");
      if (demo.worktrees.some((entry) => entry.path === path)) {
        return fail(`fatal: '${path}' already exists`);
      }
      const mode = action.mode ?? "checkout";
      const branch = action.branch?.trim() ?? "";
      if (mode !== "detach" && !branch) return fail("branch is required");
      if (mode === "new") {
        if (demo.branches.some((entry) => entry.name === branch)) {
          return fail(`fatal: a branch named '${branch}' already exists`);
        }
        demo.branches = [...demo.branches, newDemoBranch(branch, false, null)];
      } else if (mode === "checkout") {
        if (!demo.branches.some((entry) => entry.name === branch)) {
          return fail(`fatal: invalid reference: ${branch}`);
        }
        if (demo.worktrees.some((entry) => entry.branch === branch)) {
          return fail(`fatal: '${branch}' is already used by worktree`);
        }
      }
      demo.worktrees = [
        ...demo.worktrees,
        {
          path,
          head: demoCommits[0].shortHash,
          branch: mode === "detach" ? null : branch,
          detached: mode === "detach",
          bare: false,
          current: false,
          main: false,
          locked: false,
          lockReason: null,
          prunable: false,
        },
      ];
      return ok(`Preparing worktree (${mode === "new" ? `new branch '${branch}'` : branch || action.target})`);
    }
    case "worktreeRemove":
    case "worktreeRemoveForce": {
      const path = action.path?.trim();
      const entry = demo.worktrees.find((item) => item.path === path);
      if (!entry) return fail(`fatal: '${path}' is not a working tree`);
      if (entry.main) return fail(`fatal: '${path}' is a main working tree`);
      if (entry.path === demo.root) return fail("cannot remove the worktree that is currently open");
      if (entry.locked && action.kind === "worktreeRemove") {
        return fail(`fatal: cannot remove a locked working tree, lock reason: ${entry.lockReason ?? "unknown"}`);
      }
      demo.worktrees = demo.worktrees.filter((item) => item.path !== path);
      return ok();
    }
    case "worktreePrune": {
      const pruned = demo.worktrees.filter((entry) => entry.prunable);
      demo.worktrees = demo.worktrees.filter((entry) => !entry.prunable);
      return ok(pruned.map((entry) => `Removing worktrees/${entry.path.split("/").at(-1)}: gitdir file points to non-existent location`).join("\n"));
    }
    case "fetch":
    case "fetchAll":
    case "pull":
    case "push":
    case "forcePush":
    case "rebase":
    case "cherryPick":
    case "revert":
    case "reset":
      return ok();
    // ---- F1: branch & worktree cleanup ----
    // ---- F2: ref compare ----
    // ---- F3: gh clone ----
    // ---- F4: rebase ----
    default:
      return fail(`demo backend: unknown action '${action.kind}'`);
  }
}

// ---- F2: ref compare (demo helpers) ----
// `demoCommits` is a small, hand-built DAG (not a strict single-parent chain —
// several commits are merges), so ref-to-ref compare walks it with real
// ancestor sets rather than assuming a linear history.

function resolveDemoRef(ref: string): DemoCommit {
  const trimmed = ref.trim();
  const byHash = demoCommits.find((entry) => entry.hash === trimmed || entry.shortHash === trimmed);
  if (byHash) return byHash;
  const byLabel = demoCommits.find((entry) =>
    entry.refs.some((label) => label === trimmed || label === `HEAD -> ${trimmed}` || label === `tag:${trimmed}`),
  );
  if (byLabel) return byLabel;
  throw new Error(`unknown ref '${ref}'`);
}

function demoAncestors(hash: string): Set<string> {
  const seen = new Set<string>();
  const stack = [hash];
  while (stack.length) {
    const current = stack.pop() as string;
    if (seen.has(current)) continue;
    seen.add(current);
    const commit = demoCommits.find((entry) => entry.hash === current);
    if (commit) stack.push(...commit.parents);
  }
  return seen;
}

function demoMergeBase(aHash: string, bHash: string): string | null {
  const ancestorsA = demoAncestors(aHash);
  const ancestorsB = demoAncestors(bHash);
  // `demoCommits` is already newest-first, so the first hash common to both
  // ancestor sets is the most recent common ancestor.
  const common = demoCommits.find((entry) => ancestorsA.has(entry.hash) && ancestorsB.has(entry.hash));
  return common?.hash ?? null;
}

function asCommitNode({ email, date, body, files, ...node }: DemoCommit): CommitNode {
  return { ...node };
}

function buildRefCompare(baseInput: string | null, headInput: string, threeDot: boolean): RefCompare {
  const baseRef = baseInput?.trim() || "main";
  const base = resolveDemoRef(baseRef);
  const head = resolveDemoRef(headInput);

  const baseAncestors = demoAncestors(base.hash);
  const headAncestors = demoAncestors(head.hash);
  const aheadCommits = demoCommits.filter((entry) => headAncestors.has(entry.hash) && !baseAncestors.has(entry.hash));
  const behindCommits = demoCommits.filter((entry) => baseAncestors.has(entry.hash) && !headAncestors.has(entry.hash));

  // Three-dot (since merge base) only shows what head itself contributed;
  // two-dot (direct) also surfaces base's own unique changes, since a direct
  // diff includes whatever base did that head never picked up.
  const fileSource = threeDot ? aheadCommits : [...aheadCommits, ...behindCommits];
  const files = new Map<string, CommitFileChange>();
  for (const commit of fileSource) {
    for (const change of commit.files) {
      if (!files.has(change.path)) files.set(change.path, change);
    }
  }

  return {
    base: baseRef,
    head: headInput,
    mergeBase: demoMergeBase(base.hash, head.hash),
    threeDot,
    ahead: aheadCommits.length,
    behind: behindCommits.length,
    files: [...files.values()],
    commits: aheadCommits.map(asCommitNode),
    commitsTruncated: false,
  };
}

export async function demoInvoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "get_repository_state":
      return repositoryState() as T;
    case "get_commit_graph":
      return { commits: demoCommits.map(({ email, date, body, files, ...node }) => ({ ...node })) } as T;
    case "get_commit_detail": {
      const found = demoCommits.find((entry) => entry.hash === args.hash);
      if (!found) throw new Error(`unknown commit ${String(args.hash)}`);
      const detail: CommitDetail = {
        hash: found.hash,
        shortHash: found.shortHash,
        parents: found.parents,
        refs: found.refs,
        author: found.author,
        email: found.email,
        date: found.date,
        relativeDate: found.relativeDate,
        subject: found.subject,
        body: found.body,
        files: found.files,
      };
      return detail as T;
    }
    case "get_commit_file_diff": {
      const diff = demoDiffs[String(args.path)] ?? demoDiffs["docs/roadmap.md"];
      return { path: args.path, staged: false, diff, binary: false } as FileDiff as T;
    }
    case "get_file_diff": {
      const path = String(args.path);
      const entry = demo.files.find((item) => item.path === path);
      if (entry?.group === "untracked") {
        const body = untrackedContent
          .split("\n")
          .map((line) => `+${line}`)
          .join("\n");
        return {
          path,
          staged: false,
          diff: `diff --git a/${path} b/${path}\nnew file mode 100644\n--- /dev/null\n+++ b/${path}\n@@ -0,0 +1,6 @@\n${body}`,
          binary: false,
        } as FileDiff as T;
      }
      return { path, staged: Boolean(args.staged), diff: demoDiffs[path] ?? "", binary: false } as FileDiff as T;
    }
    case "get_file_content":
      return `// ${String(args.path)}\n// Demo file content shown in File View mode.\nexport const demo = true;\n` as T;
    case "get_file_blame":
      return demoCommits
        .slice(0, 6)
        .map((entry, index) => `${entry.shortHash} (${entry.author.padEnd(18)} ${entry.date}  ${index + 1}) demo line ${index + 1}`)
        .join("\n") as T;
    case "get_file_history":
      return demoCommits
        .slice(0, 8)
        .map((entry) => `${entry.shortHash}  ${entry.relativeDate.padEnd(12)}  ${entry.author.padEnd(18)}  ${entry.subject}`)
        .join("\n") as T;
    case "get_conflict_file": {
      const path = String(args.path);
      const conflictFile: ConflictFile = {
        path,
        base: `export function call() {\n  return invoke(command);\n}\n`,
        ours: `export function call() {\n  return invoke(command, args);\n}\n`,
        theirs: `export async function call() {\n  return invoke(command, payload);\n}\n`,
        working: `export function call() {\n<<<<<<< HEAD\n  return invoke(command, args);\n=======\n  return invoke(command, payload);\n>>>>>>> feature/hunk-staging\n}\n`,
      };
      return conflictFile as T;
    }
    case "save_conflict_resolution":
      return ok() as T;
    case "apply_hunk":
      return ok() as T;
    case "run_git_action":
      return runAction(args.action as GitAction) as T;
    case "set_repository_path": {
      const path = String(args.path);
      demo.root = path;
      // Switching into another worktree also switches the checked-out branch.
      const worktree = demo.worktrees.find((entry) => entry.path === path);
      if (worktree?.branch) {
        demo.currentBranch = worktree.branch;
        demo.branches = demo.branches.map((entry) => ({ ...entry, current: entry.name === worktree.branch }));
      }
      return repositoryState() as T;
    }
    case "create_repository":
    case "clone_repository":
      demo.root = String(args.path);
      return repositoryState() as T;
    case "open_terminal":
      return fail("terminal is unavailable in browser demo mode") as T;
    case "pick_repository_folder":
      return "/Users/christine/dev/demo-picked" as T;
    // ---- F1: branch & worktree cleanup ----
    case "get_branch_cleanup":
      return branchCleanupReport(
        (args.base as string | null | undefined) ?? null,
        (args.staleDays as number | null | undefined) ?? null,
      ) as T;
    // ---- F2: ref compare ----
    case "get_ref_compare":
      return buildRefCompare((args.base as string | null) ?? null, String(args.head), Boolean(args.threeDot)) as T;
    case "get_ref_file_diff": {
      const baseRef = (args.base as string | null)?.trim() || "main";
      resolveDemoRef(baseRef);
      resolveDemoRef(String(args.head));
      const path = String(args.path);
      const diff = demoDiffs[path] ?? demoDiffs["docs/roadmap.md"];
      return { path, staged: false, diff, binary: false } as FileDiff as T;
    }
    // ---- F3: gh clone ----
    case "gh_status":
      return {
        installed: true,
        authenticated: true,
        login: demoGhLogin,
        host: "github.com",
        protocol: "https",
        message: null,
      } as GhStatus as T;
    case "gh_repo_list": {
      const owner = typeof args.owner === "string" && args.owner.trim() ? args.owner.trim().toLowerCase() : null;
      const limit = typeof args.limit === "number" && args.limit > 0 ? args.limit : 100;
      const repos = owner ? demoGhRepos.filter((repo) => repo.owner.toLowerCase() === owner) : demoGhRepos;
      return repos.slice(0, limit) as T;
    }
    // ---- F4: rebase ----
    case "get_rebase_plan":
      throw new Error("demo backend: get_rebase_plan not implemented yet");
    case "run_interactive_rebase":
      throw new Error("demo backend: run_interactive_rebase not implemented yet");
    default:
      throw new Error(`demo backend: unknown command '${command}'`);
  }
}
