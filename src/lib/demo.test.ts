import { describe, expect, it, vi } from "vitest";
import type { GitAction, GitResult, RepositoryState } from "./types";

// Each test gets a fresh copy of the module so mutations to its in-memory
// state (demo.files, demo.branches, ...) never leak between tests.
async function loadDemo() {
  vi.resetModules();
  const { demoInvoke } = await import("./demo");
  return demoInvoke;
}

type Invoke = Awaited<ReturnType<typeof loadDemo>>;

function state(demoInvoke: Invoke) {
  return demoInvoke<RepositoryState>("get_repository_state", {});
}

function act(demoInvoke: Invoke, action: GitAction) {
  return demoInvoke<GitResult>("run_git_action", { action });
}

describe("demo backend", () => {
  it("seeds a repository state with the expected shape", async () => {
    const demoInvoke = await loadDemo();
    const result = await state(demoInvoke);
    expect(result.root).toBe("/Users/christine/dev/gitc");
    expect(result.currentBranch).toBe("feature/commit-details");
    expect(result.files).toHaveLength(5);
    expect(result.branches.find((b) => b.current)?.name).toBe("feature/commit-details");
  });

  it("stages an unstaged file", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "stage", path: "src/App.svelte" });
    expect(result.ok).toBe(true);
    const files = (await state(demoInvoke)).files;
    expect(files.filter((f) => f.path === "src/App.svelte")).toHaveLength(1);
    expect(files.find((f) => f.path === "src/App.svelte")?.group).toBe("staged");
  });

  it("fails to stage a file that doesn't exist", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "stage", path: "nope.txt" });
    expect(result.ok).toBe(false);
    expect(result.stderr).toContain("nope.txt");
  });

  it("unstages a staged file back to the working tree", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "unstage", path: "src/styles.css" });
    expect(result.ok).toBe(true);
    const files = (await state(demoInvoke)).files;
    expect(files.find((f) => f.path === "src/styles.css")?.group).toBe("unstaged");
  });

  it("commit clears only staged files", async () => {
    const demoInvoke = await loadDemo();
    const before = await state(demoInvoke);
    const stagedBefore = before.files.filter((f) => f.group === "staged").length;
    expect(stagedBefore).toBeGreaterThan(0);

    const result = await act(demoInvoke, { kind: "commit", message: "test" });
    expect(result.ok).toBe(true);
    const files = (await state(demoInvoke)).files;
    expect(files.some((f) => f.group === "staged")).toBe(false);
    expect(files).toHaveLength(before.files.length - stagedBefore);
  });

  it("rejects checking out an unknown branch", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "checkoutBranch", branch: "does-not-exist" });
    expect(result.ok).toBe(false);
  });

  it("checks out an existing branch and updates current", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "checkoutBranch", branch: "main" });
    expect(result.ok).toBe(true);
    const after = await state(demoInvoke);
    expect(after.currentBranch).toBe("main");
    expect(after.branches.filter((b) => b.current)).toHaveLength(1);
    expect(after.branches.find((b) => b.current)?.name).toBe("main");
  });

  it("rejects creating a branch that already exists", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "createBranch", branch: "main" });
    expect(result.ok).toBe(false);
  });

  it("creates and switches to a new branch", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "createBranch", branch: "feature/new-thing" });
    expect(result.ok).toBe(true);
    const after = await state(demoInvoke);
    expect(after.currentBranch).toBe("feature/new-thing");
    expect(after.branches.some((b) => b.name === "feature/new-thing" && b.current)).toBe(true);
  });

  it("refuses to remove the main worktree", async () => {
    const demoInvoke = await loadDemo();
    const before = await state(demoInvoke);
    const main = before.worktrees.find((w) => w.main)!;
    const result = await act(demoInvoke, { kind: "worktreeRemove", path: main.path });
    expect(result.ok).toBe(false);
    expect(result.stderr).toContain("main working tree");
  });

  it("refuses to remove the currently open worktree", async () => {
    const demoInvoke = await loadDemo();
    const before = await state(demoInvoke);
    const release = before.worktrees.find((w) => w.branch === "release/0.2")!;
    // Switch into the non-main worktree so it becomes the "currently open" one.
    await demoInvoke<RepositoryState>("set_repository_path", { path: release.path });

    const result = await act(demoInvoke, { kind: "worktreeRemove", path: release.path });
    expect(result.ok).toBe(false);
    expect(result.stderr).toContain("currently open");
  });

  it("adds then removes a linked worktree", async () => {
    const demoInvoke = await loadDemo();
    const add = await act(demoInvoke, {
      kind: "worktreeAdd",
      mode: "new",
      branch: "feature/scratch",
      path: "/tmp/gitc-scratch",
    });
    expect(add.ok).toBe(true);
    expect((await state(demoInvoke)).worktrees.some((w) => w.path === "/tmp/gitc-scratch")).toBe(true);

    const remove = await act(demoInvoke, { kind: "worktreeRemove", path: "/tmp/gitc-scratch" });
    expect(remove.ok).toBe(true);
    expect((await state(demoInvoke)).worktrees.some((w) => w.path === "/tmp/gitc-scratch")).toBe(false);
  });

  it("prunes only the stale worktree", async () => {
    const demoInvoke = await loadDemo();
    const before = await state(demoInvoke);
    const prunableCount = before.worktrees.filter((w) => w.prunable).length;
    expect(prunableCount).toBeGreaterThan(0);

    const result = await act(demoInvoke, { kind: "worktreePrune" });
    expect(result.ok).toBe(true);
    const after = await state(demoInvoke);
    expect(after.worktrees).toHaveLength(before.worktrees.length - prunableCount);
    expect(after.worktrees.every((w) => !w.prunable)).toBe(true);
  });

  it("switching repository path into a worktree checks out its branch", async () => {
    const demoInvoke = await loadDemo();
    const before = await state(demoInvoke);
    const release = before.worktrees.find((w) => w.branch === "release/0.2")!;

    const after = await demoInvoke<RepositoryState>("set_repository_path", { path: release.path });
    expect(after.currentBranch).toBe("release/0.2");
    expect(after.branches.find((b) => b.current)?.name).toBe("release/0.2");
  });

  it("open_terminal is unavailable in the browser demo", async () => {
    const demoInvoke = await loadDemo();
    const result = await demoInvoke<GitResult>("open_terminal", {});
    expect(result.ok).toBe(false);
  });

  it("rejects an unknown command", async () => {
    const demoInvoke = await loadDemo();
    await expect(demoInvoke("not_a_real_command", {})).rejects.toThrow();
  });

  it("rejects an unknown commit hash", async () => {
    const demoInvoke = await loadDemo();
    await expect(demoInvoke("get_commit_detail", { hash: "deadbeef" })).rejects.toThrow();
  });
});
