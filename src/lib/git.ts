import { invoke } from "@tauri-apps/api/core";
import type { CommitGraph, ConflictFile, FileDiff, GitAction, GitResult, RepositoryState } from "./types";

export function getRepositoryState(): Promise<RepositoryState> {
  return invoke("get_repository_state");
}

export function getCommitGraph(limit = 250): Promise<CommitGraph> {
  return invoke("get_commit_graph", { limit });
}

export function runGitAction(action: GitAction): Promise<GitResult> {
  return invoke("run_git_action", { action });
}

export function setRepositoryPath(path: string): Promise<RepositoryState> {
  return invoke("set_repository_path", { path });
}

export function openTerminal(): Promise<GitResult> {
  return invoke("open_terminal");
}

export function createRepository(path: string): Promise<RepositoryState> {
  return invoke("create_repository", { path });
}

export function cloneRepository(url: string, path: string): Promise<RepositoryState> {
  return invoke("clone_repository", { url, path });
}

export function applyHunk(patch: string, mode: "stage" | "unstage" | "discard"): Promise<GitResult> {
  return invoke("apply_hunk", { patch, mode });
}

export function getConflictFile(path: string): Promise<ConflictFile> {
  return invoke("get_conflict_file", { path });
}

export function getFileDiff(path: string, staged: boolean): Promise<FileDiff> {
  return invoke("get_file_diff", { path, staged });
}

export function getFileContent(path: string, staged: boolean): Promise<string> {
  return invoke("get_file_content", { path, staged });
}

export function getFileBlame(path: string): Promise<string> {
  return invoke("get_file_blame", { path });
}

export function getFileHistory(path: string): Promise<string> {
  return invoke("get_file_history", { path });
}

export function saveConflictResolution(path: string, content: string): Promise<GitResult> {
  return invoke("save_conflict_resolution", { path, content });
}
