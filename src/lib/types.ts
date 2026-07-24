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
