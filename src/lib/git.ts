import type {
  BranchCleanupReport,
  CommitDetail,
  CommitGraph,
  ConflictFile,
  FileDiff,
  GhRepo,
  GhStatus,
  GitAction,
  GitResult,
  RebasePlan,
  RebaseStep,
  RefCompare,
  RepositoryState,
} from "./types";

// When the UI runs in a plain browser (`npm run dev:ui`) there is no Tauri
// backend, so route every call to the in-memory demo repository instead.
// This keeps the full UI testable for design work without a Rust build.
const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (hasTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  }
  const { demoInvoke } = await import("./demo");
  return demoInvoke<T>(command, args ?? {});
}

export function getRepositoryState(): Promise<RepositoryState> {
  return call("get_repository_state");
}

export function getCommitGraph(limit = 250): Promise<CommitGraph> {
  return call("get_commit_graph", { limit });
}

export function getCommitDetail(hash: string): Promise<CommitDetail> {
  return call("get_commit_detail", { hash });
}

export function getCommitFileDiff(hash: string, path: string): Promise<FileDiff> {
  return call("get_commit_file_diff", { hash, path });
}

export function runGitAction(action: GitAction): Promise<GitResult> {
  return call("run_git_action", { action });
}

export function setRepositoryPath(path: string): Promise<RepositoryState> {
  return call("set_repository_path", { path });
}

export function openTerminal(): Promise<GitResult> {
  return call("open_terminal");
}

export function pickRepositoryFolder(): Promise<string | null> {
  return call<string | null>("pick_repository_folder");
}

export function createRepository(path: string): Promise<RepositoryState> {
  return call("create_repository", { path });
}

export function cloneRepository(url: string, path: string): Promise<RepositoryState> {
  return call("clone_repository", { url, path });
}

export function applyHunk(patch: string, mode: "stage" | "unstage" | "discard"): Promise<GitResult> {
  return call("apply_hunk", { patch, mode });
}

export function getConflictFile(path: string): Promise<ConflictFile> {
  return call("get_conflict_file", { path });
}

export function getFileDiff(path: string, staged: boolean): Promise<FileDiff> {
  return call("get_file_diff", { path, staged });
}

export function getFileContent(path: string, staged: boolean): Promise<string> {
  return call("get_file_content", { path, staged });
}

export function getFileBlame(path: string): Promise<string> {
  return call("get_file_blame", { path });
}

export function getFileHistory(path: string): Promise<string> {
  return call("get_file_history", { path });
}

export function saveConflictResolution(path: string, content: string): Promise<GitResult> {
  return call("save_conflict_resolution", { path, content });
}

export function getBranchCleanup(base: string | null, staleDays: number | null): Promise<BranchCleanupReport> {
  return call("get_branch_cleanup", { base, staleDays });
}

export function getRefCompare(base: string | null, head: string, threeDot: boolean): Promise<RefCompare> {
  return call("get_ref_compare", { base, head, threeDot });
}

export function getRefFileDiff(
  base: string | null,
  head: string,
  path: string,
  threeDot: boolean,
): Promise<FileDiff> {
  return call("get_ref_file_diff", { base, head, path, threeDot });
}

export function ghStatus(): Promise<GhStatus> {
  return call("gh_status");
}

export function ghRepoList(owner: string | null, limit: number | null): Promise<GhRepo[]> {
  return call("gh_repo_list", { owner, limit });
}

export function getRebasePlan(base: string | null): Promise<RebasePlan> {
  return call("get_rebase_plan", { base });
}

export function runInteractiveRebase(base: string, steps: RebaseStep[]): Promise<GitResult> {
  return call("run_interactive_rebase", { base, steps });
}
