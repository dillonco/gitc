import type {
  CommitDetail,
  CommitFileChange,
  CommitNode,
  ConflictFile,
  FileDiff,
  FileStatus,
  GitAction,
  GitResult,
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

interface DemoState {
  root: string;
  currentBranch: string;
  merging: boolean;
  rebasing: boolean;
  files: FileStatus[];
  stashes: StashEntry[];
  tags: string[];
  branches: { name: string; current: boolean; upstream?: string | null }[];
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
    { name: "feature/commit-details", current: true, upstream: "origin/feature/commit-details" },
    { name: "feature/hunk-staging", current: false, upstream: null },
    { name: "feature/graph", current: false, upstream: null },
    { name: "release/0.2", current: false, upstream: "origin/release/0.2" },
    { name: "main", current: false, upstream: "origin/main" },
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
        { name: action.branch ?? "new-branch", current: true, upstream: null },
      ];
      demo.currentBranch = action.branch ?? "new-branch";
      return ok();
    }
    case "deleteBranch":
    case "deleteBranchForce": {
      const branch = demo.branches.find((entry) => entry.name === action.branch);
      if (!branch) return fail(`branch '${action.branch}' not found`);
      if (branch.current) return fail(`cannot delete the checked out branch '${action.branch}'`);
      demo.branches = demo.branches.filter((entry) => entry.name !== action.branch);
      return ok();
    }
    case "checkoutRemote": {
      const local = (action.target ?? "").split("/").slice(1).join("/") || "tracked";
      demo.branches = [
        ...demo.branches.map((entry) => ({ ...entry, current: false })),
        { name: local, current: true, upstream: action.target },
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
        demo.branches = [...demo.branches, { name: branch, current: false, upstream: null }];
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
      throw new Error("demo backend: get_branch_cleanup not implemented yet");
    // ---- F2: ref compare ----
    case "get_ref_compare":
      throw new Error("demo backend: get_ref_compare not implemented yet");
    case "get_ref_file_diff":
      throw new Error("demo backend: get_ref_file_diff not implemented yet");
    // ---- F3: gh clone ----
    case "gh_status":
      throw new Error("demo backend: gh_status not implemented yet");
    case "gh_repo_list":
      throw new Error("demo backend: gh_repo_list not implemented yet");
    // ---- F4: rebase ----
    case "get_rebase_plan":
      throw new Error("demo backend: get_rebase_plan not implemented yet");
    case "run_interactive_rebase":
      throw new Error("demo backend: run_interactive_rebase not implemented yet");
    default:
      throw new Error(`demo backend: unknown command '${command}'`);
  }
}
