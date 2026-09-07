import { describe, expect, it, vi } from "vitest";
import type { FileDiff, RefCompare } from "./types";

// Each test gets a fresh copy of the module so mutations to its in-memory
// state never leak between tests (mirrors demo.test.ts's pattern).
async function loadDemo() {
  vi.resetModules();
  const { demoInvoke } = await import("./demo");
  return demoInvoke;
}

type Invoke = Awaited<ReturnType<typeof loadDemo>>;

function compare(demoInvoke: Invoke, base: string | null, head: string, threeDot: boolean) {
  return demoInvoke<RefCompare>("get_ref_compare", { base, head, threeDot });
}

function fileDiff(demoInvoke: Invoke, base: string | null, head: string, path: string, threeDot: boolean) {
  return demoInvoke<FileDiff>("get_ref_file_diff", { base, head, path, threeDot });
}

describe("demo ref compare", () => {
  it("resolves the default base and returns the commits and files ahead of it", async () => {
    const demoInvoke = await loadDemo();
    const result = await compare(demoInvoke, null, "feature/commit-details", true);

    expect(result.base).toBe("main");
    expect(result.head).toBe("feature/commit-details");
    expect(result.ahead).toBe(2);
    expect(result.commits.map((c) => c.subject)).toEqual([
      "feat: commit detail panel with file diffs",
      "feat: stash management in left panel",
    ]);
    expect(result.files.some((f) => f.status === "A" && f.path === "src/lib/CommitDetail.svelte")).toBe(true);
    expect(result.commitsTruncated).toBe(false);
  });

  it("reports up to date once a branch is fully merged into the base", async () => {
    const demoInvoke = await loadDemo();
    const result = await compare(demoInvoke, "main", "feature/graph", true);

    expect(result.ahead).toBe(0);
    expect(result.commits).toHaveLength(0);
    expect(result.files).toHaveLength(0);
  });

  it("direct (two-dot) diffs also surface the base's own unique changes", async () => {
    const demoInvoke = await loadDemo();
    const threeDot = await compare(demoInvoke, "main", "feature/hunk-staging", true);
    const twoDot = await compare(demoInvoke, "main", "feature/hunk-staging", false);

    // Three-dot only shows what the branch itself contributed; since it is
    // fully merged into main, that is nothing.
    expect(threeDot.files).toHaveLength(0);
    // Two-dot is a direct tree diff, so main's own commits since the branch
    // point show up too.
    expect(twoDot.files.length).toBeGreaterThan(0);
    expect(twoDot.files.map((f) => f.path)).toEqual(expect.arrayContaining(["src/App.svelte", "src-tauri/src/lib.rs"]));
  });

  it("rejects an unknown ref instead of returning a blank result", async () => {
    const demoInvoke = await loadDemo();
    await expect(compare(demoInvoke, "no-such-branch", "main", true)).rejects.toThrow(/unknown ref/i);
  });
});

describe("demo ref file diff", () => {
  it("returns the diff for a known path", async () => {
    const demoInvoke = await loadDemo();
    const result = await fileDiff(demoInvoke, "main", "feature/commit-details", "src/lib/CommitDetail.svelte", true);
    expect(result.binary).toBe(false);
    expect(result.diff).toContain("CommitDetail");
  });

  it("falls back to a generic diff for a path with no authored fixture", async () => {
    const demoInvoke = await loadDemo();
    const result = await fileDiff(demoInvoke, "main", "feature/commit-details", "src-tauri/src/lib.rs", true);
    expect(result.path).toBe("src-tauri/src/lib.rs");
    expect(result.diff.length).toBeGreaterThan(0);
  });

  it("rejects an unknown ref", async () => {
    const demoInvoke = await loadDemo();
    await expect(fileDiff(demoInvoke, "no-such-branch", "main", "src/App.svelte", true)).rejects.toThrow(/unknown ref/i);
  });
});
