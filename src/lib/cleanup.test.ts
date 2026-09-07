import { describe, expect, it, vi } from "vitest";
import type { BranchCleanupReport, GitAction, GitResult } from "./types";

// Each test gets a fresh copy of the module so mutations to its in-memory
// state (demo.branches, ...) never leak between tests. Mirrors demo.test.ts.
async function loadDemo() {
  vi.resetModules();
  const { demoInvoke } = await import("./demo");
  return demoInvoke;
}

type Invoke = Awaited<ReturnType<typeof loadDemo>>;

function cleanupReport(demoInvoke: Invoke, base: string | null = null, staleDays: number | null = null) {
  return demoInvoke<BranchCleanupReport>("get_branch_cleanup", { base, staleDays });
}

function act(demoInvoke: Invoke, action: GitAction) {
  return demoInvoke<GitResult>("run_git_action", { action });
}

describe("demo branch cleanup", () => {
  it("returns a report shaped like the backend's BranchCleanupReport", async () => {
    const demoInvoke = await loadDemo();
    const report = await cleanupReport(demoInvoke);

    expect(report.base).toBe("main");
    expect(report.staleDays).toBe(30);
    expect(report.branches.length).toBeGreaterThan(0);
    for (const audit of report.branches) {
      expect(typeof audit.name).toBe("string");
      expect(typeof audit.classification).toBe("string");
      expect(typeof audit.head).toBe("string");
      expect(typeof audit.shortHead).toBe("string");
    }
  });

  it("clamps and defaults staleDays like the backend", async () => {
    const demoInvoke = await loadDemo();
    expect((await cleanupReport(demoInvoke, null, null)).staleDays).toBe(30);
    expect((await cleanupReport(demoInvoke, null, 0)).staleDays).toBe(1);
    expect((await cleanupReport(demoInvoke, null, 999_999)).staleDays).toBe(3650);
  });

  it("classifies the seeded branches as documented in PLAN.md", async () => {
    const demoInvoke = await loadDemo();
    const report = await cleanupReport(demoInvoke);
    const byName = Object.fromEntries(report.branches.map((b) => [b.name, b]));

    expect(byName["feature/commit-details"].classification).toBe("current");
    expect(byName.main.classification).toBe("base");
    expect(byName["feature/graph"].classification).toBe("merged");
    expect(byName["feature/hunk-staging"].classification).toBe("squashMerged");
    expect(byName["feature/hunk-staging"].merged).toBe(false);
    expect(byName["release/0.2"].classification).toBe("active");
    expect(byName["hotfix/old-login"].classification).toBe("gone");
    expect(byName["hotfix/old-login"].upstreamGone).toBe(true);
    expect(byName["experiment/lanes"].classification).toBe("stale");
    expect(byName["experiment/lanes"].stale).toBe(true);
  });

  it("the default-deletable set is exactly merged, squash-merged, and gone branches", async () => {
    const demoInvoke = await loadDemo();
    const report = await cleanupReport(demoInvoke);
    const deletable = report.branches
      .filter((b) => b.classification === "merged" || b.classification === "squashMerged" || b.classification === "gone")
      .map((b) => b.name)
      .sort();

    expect(deletable).toEqual(["feature/graph", "feature/hunk-staging", "hotfix/old-login"].sort());
  });

  it("deleteBranch on the current branch fails", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "deleteBranch", branch: "feature/commit-details" });
    expect(result.ok).toBe(false);
    expect(result.stderr).toContain("feature/commit-details");
  });

  it("deleteBranch on an unmerged branch fails with a 'not fully merged' style error", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "deleteBranch", branch: "release/0.2" });
    expect(result.ok).toBe(false);
    expect(result.stderr.toLowerCase()).toContain("not fully merged");
  });

  it("deleteBranch on a merged branch succeeds", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "deleteBranch", branch: "feature/graph" });
    expect(result.ok).toBe(true);
  });

  it("force delete of a squash-merged branch succeeds", async () => {
    const demoInvoke = await loadDemo();
    const result = await act(demoInvoke, { kind: "deleteBranchForce", branch: "feature/hunk-staging" });
    expect(result.ok).toBe(true);

    const report = await cleanupReport(demoInvoke);
    expect(report.branches.some((b) => b.name === "feature/hunk-staging")).toBe(false);
  });
});
