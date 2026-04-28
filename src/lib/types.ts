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

export interface RepositoryState {
  root: string;
  currentBranch?: string | null;
  head: string;
  merging: boolean;
  rebasing: boolean;
  files: FileStatus[];
  branches: Branch[];
  remotes: string[];
  worktrees: string[];
  stashes: StashEntry[];
}

export interface CommitNode {
  hash: string;
  shortHash: string;
  parents: string[];
  refs: string[];
  author: string;
  relativeDate: string;
  subject: string;
}

export interface CommitGraph {
  commits: CommitNode[];
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
