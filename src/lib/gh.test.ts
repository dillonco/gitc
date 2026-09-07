import { describe, expect, it, vi } from "vitest";
import { suggestClonePath } from "./cloneUtils";
import type { GhRepo, GhStatus } from "./types";

// Each test gets a fresh copy of the demo module for the same reason
// demo.test.ts does: `demo.ts` holds mutable module-scope state.
async function loadDemo() {
  vi.resetModules();
  const { demoInvoke } = await import("./demo");
  return demoInvoke;
}

type Invoke = Awaited<ReturnType<typeof loadDemo>>;

function ghStatus(demoInvoke: Invoke) {
  return demoInvoke<GhStatus>("gh_status", {});
}

function ghRepoList(demoInvoke: Invoke, owner: string | null = null, limit: number | null = null) {
  return demoInvoke<GhRepo[]>("gh_repo_list", { owner, limit });
}

describe("cloneUtils.suggestClonePath", () => {
  it("joins the clone directory and the repo name from an https url", () => {
    expect(suggestClonePath("https://github.com/dillonco/gitc.git", "/Users/dillon/dev")).toBe(
      "/Users/dillon/dev/gitc",
    );
  });

  it("strips a trailing .git regardless of case", () => {
    expect(suggestClonePath("https://github.com/dillonco/gitc.GIT", "/dev")).toBe("/dev/gitc");
  });

  it("handles the scp-like ssh form", () => {
    expect(suggestClonePath("git@github.com:dillonco/gitc.git", "/dev")).toBe("/dev/gitc");
  });

  it("handles a url with no .git suffix", () => {
    expect(suggestClonePath("https://github.com/dillonco/gitc", "/dev")).toBe("/dev/gitc");
  });

  it("tolerates a trailing slash on the clone directory", () => {
    expect(suggestClonePath("https://github.com/dillonco/gitc.git", "/dev/")).toBe("/dev/gitc");
  });

  it("tolerates a trailing slash on the url", () => {
    expect(suggestClonePath("https://github.com/dillonco/gitc/", "/dev")).toBe("/dev/gitc");
  });

  it("falls back to 'repo' for an empty url", () => {
    expect(suggestClonePath("", "/dev")).toBe("/dev/repo");
  });
});

describe("demo backend: gh_status / gh_repo_list", () => {
  it("reports installed and authenticated with a seeded login", async () => {
    const demoInvoke = await loadDemo();
    const status = await ghStatus(demoInvoke);
    expect(status.installed).toBe(true);
    expect(status.authenticated).toBe(true);
    expect(status.login).toBe("christine");
    expect(status.message).toBeFalsy();
  });

  it("lists the seeded repositories", async () => {
    const demoInvoke = await loadDemo();
    const repos = await ghRepoList(demoInvoke);
    expect(repos.length).toBeGreaterThanOrEqual(6);
    expect(repos.some((repo) => repo.name === "gitc")).toBe(true);
  });

  it("filters the list by owner", async () => {
    const demoInvoke = await loadDemo();
    const all = await ghRepoList(demoInvoke);
    const subset = await ghRepoList(demoInvoke, "osfmanagement");
    expect(subset.length).toBeGreaterThan(0);
    expect(subset.length).toBeLessThan(all.length);
    expect(subset.every((repo) => repo.owner === "osfmanagement")).toBe(true);
  });

  it("respects the limit argument", async () => {
    const demoInvoke = await loadDemo();
    const limited = await ghRepoList(demoInvoke, null, 2);
    expect(limited).toHaveLength(2);
  });
});
