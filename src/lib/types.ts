export type FileGroup = "staged" | "unstaged" | "untracked" | "conflicted";

export interface FileStatus {
  path: string;
  index: string;
  worktree: string;
  group: FileGroup;
}

export interface Branch {
  name: string;
  current: boolean;
  upstream?: string | null;
  // F1 fields are optional for now so Stage 0 compiles without touching demo
  // seeds; F1 makes them required and updates the seeds.
  upstreamGone?: boolean;
  ahead?: number;
  behind?: number;
  lastCommitUnix?: number;
  lastCommitRelative?: string;
}

export interface StashEntry {
  name: string;
  message: string;
}

export interface Worktree {
  path: string;
  head: string;
  branch?: string | null;
  detached: boolean;
  bare: boolean;
  current: boolean;
  main: boolean;
  locked: boolean;
  lockReason?: string | null;
  prunable: boolean;
}

export interface RepositoryState {
  root: string;
  currentBranch?: string | null;
  head: string;
  merging: boolean;
  rebasing: boolean;
  files: FileStatus[];
  branches: Branch[];
  remotes: string[];
  remoteBranches: string[];
  tags: string[];
  worktrees: Worktree[];
  stashes: StashEntry[];
  userName?: string | null;
}

export interface CommitNode {
  hash: string;
  shortHash: string;
  parents: string[];
  refs: string[];
  author: string;
  relativeDate: string;
  subject: string;
  bodySummary: string;
}

export interface CommitGraph {
  commits: CommitNode[];
}

export interface CommitFileChange {
  status: string;
  path: string;
}

export interface CommitDetail {
  hash: string;
  shortHash: string;
  parents: string[];
  refs: string[];
  author: string;
  email: string;
  date: string;
  relativeDate: string;
  subject: string;
  body: string;
  files: CommitFileChange[];
}

export interface GitAction {
  kind: string;
  path?: string | null;
  message?: string | null;
  branch?: string | null;
  target?: string | null;
  remote?: string | null;
  mode?: string | null;
}

export interface GitResult {
  ok: boolean;
  stdout: string;
  stderr: string;
  code: number;
  refresh: boolean;
}

export interface ConflictFile {
  path: string;
  base?: string | null;
  ours?: string | null;
  theirs?: string | null;
  working: string;
}

export interface FileDiff {
  path: string;
  staged: boolean;
  diff: string;
  binary: boolean;
}


// ---- F1: branch & worktree cleanup ----

export interface BranchAudit {
  name: string;
  current: boolean;
  isBase: boolean;
  head: string;
  shortHead: string;
  upstream?: string | null;
  upstreamGone: boolean;
  ahead: number;
  behind: number;
  aheadOfBase: number;
  behindBase: number;
  merged: boolean;
  squashMerged: boolean;
  stale: boolean;
  lastCommitUnix: number;
  lastCommitRelative: string;
  worktreePath?: string | null;
  classification: string; // "current" | "base" | "merged" | "squashMerged" | "gone" | "stale" | "active"
}

export interface BranchCleanupReport {
  base: string;
  staleDays: number;
  branches: BranchAudit[];
}



// ---- F2: ref compare ----

export interface RefCompare {
  base: string;
  head: string;
  mergeBase?: string | null;
  threeDot: boolean;
  ahead: number;
  behind: number;
  files: CommitFileChange[];
  commits: CommitNode[];
  commitsTruncated: boolean;
}



// ---- F3: gh clone ----

export interface GhStatus {
  installed: boolean;
  authenticated: boolean;
  login?: string | null;
  host: string;
  protocol: string; // "https" | "ssh"
  message?: string | null;
}

export interface GhRepo {
  name: string;
  nameWithOwner: string;
  owner: string;
  description?: string | null;
  isPrivate: boolean;
  isFork: boolean;
  isArchived: boolean;
  pushedAt?: string | null;
  url: string;
  sshUrl: string;
  language?: string | null;
  defaultBranch?: string | null;
}



// ---- F4: rebase ----

export interface RebaseStep {
  action: string;
  hash: string;
  message?: string | null;
}

export interface RebasePlan {
  base: string;
  mergeBase?: string | null;
  commits: CommitNode[];
  clean: boolean;
  inProgress: boolean;
  currentBranch?: string | null;
  upstream?: string | null;
}
