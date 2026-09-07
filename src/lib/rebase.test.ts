import { describe, expect, it, vi } from "vitest";
import type { GitResult, RebasePlan } from "./types";

// Each test gets a fresh copy of the module so mutations to its in-memory
// state (demo.files, demo.branches, demoCommits, ...) never leak between
// tests. Mirrors demo.test.ts's own loadDemo helper (that file is frozen —
// each stream keeps its own copy per PLAN.md section 6).
async function loadDemo() {
  vi.resetModules();
  const { demoInvoke } = await import("./demo");
  return demoInvoke;
}

type Invoke = Awaited<ReturnType<typeof loadDemo>>;

function plan(demoInvoke: Invoke, base: string | null = null) {
  return demoInvoke<RebasePlan>("get_rebase_plan", { base });
}

describe("demo rebase plan", () => {
  it("lists the two commits between main and HEAD, oldest first", async () => {
    const demoInvoke = await loadDemo();
    const result = await plan(demoInvoke);
    expect(result.base).toBe("main");
    expect(result.commits.map((c) => c.subject)).toEqual([
      "feat: stash management in left panel",
      "feat: commit detail panel with file diffs",
    ]);
  });

  it("defaults to main when no base is given", async () => {
    const demoInvoke = await loadDemo();
    const result = await plan(demoInvoke, null);
    expect(result.base).toBe("main");
  });

  it("reports the seeded working tree as dirty", async () => {
    // The demo seeds several staged/unstaged/untracked files so the
    // preflight "commit or stash your changes" banner has something to show
    // out of the box.
    const demoInvoke = await loadDemo();
    const result = await plan(demoInvoke);
    expect(result.clean).toBe(false);
  });

  it("is clean once every change is committed or stashed", async () => {
    const demoInvoke = await loadDemo();
    const stash = await demoInvoke<GitResult>("run_git_action", {
      action: { kind: "stashCreate", message: "before rebase" },
    });
    expect(stash.ok).toBe(true);
    const result = await plan(demoInvoke);
    expect(result.clean).toBe(true);
    expect(result.inProgress).toBe(false);
  });

  it("throws for an unknown ref", async () => {
    const demoInvoke = await loadDemo();
    await expect(plan(demoInvoke, "no-such-branch")).rejects.toThrow();
  });

  it("reports current branch and upstream for the divergence notice", async () => {
    const demoInvoke = await loadDemo();
    const result = await plan(demoInvoke);
    expect(result.currentBranch).toBe("feature/commit-details");
    expect(result.upstream).toBe("origin/feature/commit-details");
  });
});
