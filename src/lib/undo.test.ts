import { describe, expect, it } from "vitest";
import type { FileStatus, GitResult } from "./types";
import { blockedReason, undoEntryFor, undoItem, type RepoSnapshot } from "./undo";

const dirtyFile: FileStatus = { path: "a.txt", index: ".", worktree: "M", group: "unstaged" };
const at = (head: string, currentBranch: string | null = "main", files: FileStatus[] = []): RepoSnapshot => ({
  head,
  currentBranch,
  files,
});
const ok = (stdout = ""): GitResult => ({ ok: true, stdout, stderr: "", code: 0, refresh: true });

describe("undoEntryFor", () => {
  it("undoes a commit with a soft reset and redoes it by moving back", () => {
    expect(undoEntryFor({ kind: "commit", message: "x" }, at("aaa"), at("bbb"), ok())).toEqual({
      label: "commit",
      undo: [{ kind: "reset", target: "aaa", mode: "soft" }],
      redo: [{ kind: "reset", target: "bbb", mode: "soft" }],
      needsCleanTree: false,
    });
  });

  it("records nothing for a failed action or a first commit", () => {
    expect(undoEntryFor({ kind: "commit" }, at("aaa"), at("bbb"), { ...ok(), ok: false })).toBeNull();
    expect(undoEntryFor({ kind: "commit" }, at("unborn"), at("bbb"), ok())).toBeNull();
  });

  it("undoes a checkout by returning to the previous branch, or commit when detached", () => {
    expect(undoEntryFor({ kind: "checkoutBranch", branch: "dev" }, at("aaa"), at("bbb", "dev"), ok())?.undo).toEqual([
      { kind: "checkoutBranch", branch: "main" },
    ]);
    const detached = undoEntryFor({ kind: "checkoutBranch", branch: "dev" }, at("aaa", null), at("bbb", "dev"), ok());
    expect(detached?.undo).toEqual([{ kind: "checkoutCommit", target: "aaa" }]);
    expect(detached?.redo).toEqual([{ kind: "checkoutBranch", branch: "dev" }]);
  });

  it("undoes creating a branch by leaving it and deleting it", () => {
    const entry = undoEntryFor({ kind: "createBranch", branch: "feat" }, at("aaa"), at("aaa", "feat"), ok());
    expect(entry?.undo).toEqual([
      { kind: "checkoutBranch", branch: "main" },
      { kind: "deleteBranchForce", branch: "feat" },
    ]);
    expect(entry?.redo).toEqual([{ kind: "createBranch", branch: "feat", target: "aaa" }]);
  });

  it("restores a deleted branch or tag at the commit git reported", () => {
    const branch = undoEntryFor({ kind: "deleteBranch", branch: "old" }, at("aaa"), at("aaa"), ok("Deleted branch old (was 1a2b3c4).\n"));
    expect(branch?.undo).toEqual([{ kind: "branchAt", branch: "old", target: "1a2b3c4" }]);
    const tag = undoEntryFor({ kind: "deleteTag", branch: "v1" }, at("aaa"), at("aaa"), ok("Deleted tag 'v1' (was 9f8e7d6)\n"));
    expect(tag?.undo).toEqual([{ kind: "createTag", branch: "v1", target: "9f8e7d6" }]);
    expect(undoEntryFor({ kind: "deleteBranch", branch: "old" }, at("aaa"), at("aaa"), ok(""))).toBeNull();
  });

  it("undoes a merge or pull only from a clean tree, with a hard reset", () => {
    expect(undoEntryFor({ kind: "pullRebase" }, at("aaa"), at("bbb"), ok())).toMatchObject({
      label: "pull",
      undo: [{ kind: "reset", target: "aaa", mode: "hard" }],
      needsCleanTree: true,
    });
    expect(undoEntryFor({ kind: "merge", target: "dev" }, at("aaa", "main", [dirtyFile]), at("bbb"), ok())).toBeNull();
    expect(undoEntryFor({ kind: "pull" }, at("aaa"), at("aaa"), ok())).toBeNull();
  });

  it("will not record a hard reset that threw away changes", () => {
    expect(undoEntryFor({ kind: "reset", target: "x", mode: "hard" }, at("aaa", "main", [dirtyFile]), at("bbb"), ok())).toBeNull();
    expect(undoEntryFor({ kind: "reset", target: "x", mode: "mixed" }, at("aaa", "main", [dirtyFile]), at("bbb"), ok())?.undo).toEqual([
      { kind: "reset", target: "aaa", mode: "mixed" },
    ]);
  });

  it("ignores actions with no inverse", () => {
    expect(undoEntryFor({ kind: "stage", path: "a.txt" }, at("aaa"), at("aaa"), ok())).toBeNull();
    expect(undoEntryFor({ kind: "push" }, at("aaa"), at("aaa"), ok())).toBeNull();
  });
});

describe("blockedReason", () => {
  const merge = undoEntryFor({ kind: "merge", target: "dev" }, at("aaa"), at("bbb"), ok())!;
  const item = undoItem(merge, at("bbb"));

  it("allows the entry while the repository is where the action left it", () => {
    expect(blockedReason(item, at("bbb"))).toBeNull();
  });

  it("refuses once HEAD or the branch has moved", () => {
    expect(blockedReason(item, at("ccc"))).toBe("moved");
    expect(blockedReason(item, at("bbb", "dev"))).toBe("moved");
  });

  it("refuses a hard-reset undo over uncommitted work", () => {
    expect(blockedReason(item, at("bbb", "main", [dirtyFile]))).toBe("dirty");
  });
});
