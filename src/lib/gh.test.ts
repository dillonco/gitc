import { describe, expect, it, vi } from "vitest";
import {
  filterRepos,
  githubCloneUrl,
  githubRepoRef,
  groupByOwner,
  highlightParts,
  joinPath,
  languageColor,
  normalizeRemote,
  resolveCloneUrl,
  sameRemote,
  splitPath,
  suggestClonePath,
} from "./cloneUtils";
import type { CloneProgress, CloneTarget, GhRepo, GhStatus, RemoteBranches } from "./types";

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

function ghRepoList(demoInvoke: Invoke) {
  return demoInvoke<GhRepo[]>("gh_repo_list", {});
}

describe("cloneUtils.suggestClonePath", () => {
  it("joins the clone directory and the repo name from an https url", () => {
    expect(suggestClonePath("https://github.com/dillonco/kgit.git", "/Users/dillon/dev")).toBe(
      "/Users/dillon/dev/kgit",
    );
  });

  it("strips a trailing .git regardless of case", () => {
    expect(suggestClonePath("https://github.com/dillonco/kgit.GIT", "/dev")).toBe("/dev/kgit");
  });

  it("handles the scp-like ssh form", () => {
    expect(suggestClonePath("git@github.com:dillonco/kgit.git", "/dev")).toBe("/dev/kgit");
  });

  it("handles a url with no .git suffix", () => {
    expect(suggestClonePath("https://github.com/dillonco/kgit", "/dev")).toBe("/dev/kgit");
  });

  it("tolerates a trailing slash on the clone directory", () => {
    expect(suggestClonePath("https://github.com/dillonco/kgit.git", "/dev/")).toBe("/dev/kgit");
  });

  it("tolerates a trailing slash on the url", () => {
    expect(suggestClonePath("https://github.com/dillonco/kgit/", "/dev")).toBe("/dev/kgit");
  });

  it("falls back to 'repo' for an empty url", () => {
    expect(suggestClonePath("", "/dev")).toBe("/dev/repo");
  });
});

describe("cloneUtils.githubRepoRef", () => {
  it.each([
    ["dillonco/kgit", "dillonco/kgit"],
    ["github.com/dillonco/kgit", "dillonco/kgit"],
    ["https://github.com/dillonco/kgit.git", "dillonco/kgit"],
    ["https://github.com/dillonco/kgit/tree/main/src", "dillonco/kgit"],
    ["git@github.com:dillonco/kgit.git", "dillonco/kgit"],
    ["ssh://git@github.com/dillonco/kgit", "dillonco/kgit"],
    ["https://github.com/dillonco/my.repo", "dillonco/my.repo"],
  ])("reads %s", (input, expected) => {
    expect(githubRepoRef(input)).toBe(expected);
  });

  it.each(["kgit", "https://gitlab.com/a/b", "/Users/me/repo", "a/b/c", ""])("rejects %s", (input) => {
    expect(githubRepoRef(input)).toBeNull();
  });
});

describe("cloneUtils.resolveCloneUrl", () => {
  it("expands shorthand using the gh protocol", () => {
    expect(resolveCloneUrl("o/r", "https")).toBe("https://github.com/o/r.git");
    expect(resolveCloneUrl("github.com/o/r", "ssh")).toBe("git@github.com:o/r.git");
    expect(githubCloneUrl("o/r", undefined)).toBe("https://github.com/o/r.git");
  });

  it("passes real URLs and local paths through untouched", () => {
    for (const url of [
      "https://gitlab.com/a/b.git",
      "https://github.com/o/r",
      "git@github.com:o/r.git",
      "/srv/git/r.git",
      "~/src/r",
      "file:///srv/r",
    ]) {
      expect(resolveCloneUrl(url, "ssh")).toBe(url);
    }
  });
});

describe("cloneUtils.normalizeRemote", () => {
  it("treats ssh and https forms of one repository as the same remote", () => {
    expect(normalizeRemote("git@github.com:Owner/Repo.git")).toBe("github.com/owner/repo");
    expect(sameRemote("git@github.com:o/r.git", "https://github.com/o/r")).toBe(true);
    expect(sameRemote("ssh://git@github.com:22/o/r.git", "https://user@github.com/o/r/")).toBe(true);
    expect(sameRemote("https://github.com/o/r", "https://github.com/o/other")).toBe(false);
    expect(sameRemote("", "")).toBe(false);
  });
});

describe("cloneUtils display helpers", () => {
  it("highlights the first case-insensitive match", () => {
    expect(highlightParts("data-Layer", "lay")).toEqual([
      { text: "data-", match: false },
      { text: "Lay", match: true },
      { text: "er", match: false },
    ]);
    expect(highlightParts("kgit", "")).toEqual([{ text: "kgit", match: false }]);
    expect(highlightParts("kgit", "zz")).toEqual([{ text: "kgit", match: false }]);
  });

  it("colours known languages and greys unknown ones", () => {
    expect(languageColor("Rust")).toBe("#dea584");
    expect(languageColor("Brainfuck")).toBe("#8b93a0");
    expect(languageColor(null)).toBe("#8b93a0");
  });
});

describe("cloneUtils paths", () => {
  it("splits and joins parent and folder name", () => {
    expect(splitPath("/Users/me/dev/kgit/")).toEqual({ parent: "/Users/me/dev", name: "kgit" });
    expect(splitPath("/kgit")).toEqual({ parent: "/", name: "kgit" });
    expect(joinPath("~/dev/", "kgit")).toBe("~/dev/kgit");
    expect(joinPath("/dev", "")).toBe("");
  });
});

describe("cloneUtils.filterRepos", () => {
  const repo = (nameWithOwner: string, extra: Partial<GhRepo> = {}): GhRepo => ({
    name: nameWithOwner.split("/")[1],
    nameWithOwner,
    owner: nameWithOwner.split("/")[0],
    isPrivate: false,
    isFork: false,
    isArchived: false,
    url: `https://github.com/${nameWithOwner}`,
    sshUrl: `git@github.com:${nameWithOwner}.git`,
    ...extra,
  });
  const repos = [repo("a/kgit"), repo("a/kgit-plugins"), repo("b/tools", { description: "Git helpers", language: "Go" })];

  it("matches across owner/name, description and exact language", () => {
    expect(filterRepos(repos, "a/kg").map((r) => r.name)).toEqual(["kgit", "kgit-plugins"]);
    expect(filterRepos(repos, "helpers").map((r) => r.name)).toEqual(["tools"]);
    expect(filterRepos(repos, "go").map((r) => r.name)).toEqual(["tools"]);
  });

  it("narrows a pasted URL to its exact repository", () => {
    expect(filterRepos(repos, "https://github.com/a/kgit").map((r) => r.nameWithOwner)).toEqual(["a/kgit"]);
  });

  it("groups by owner with the signed-in account first", () => {
    const groups = groupByOwner([repo("zed/x"), repo("b/tools"), repo("me/one"), repo("b/more")], "me");
    expect(groups.map((group) => group.owner)).toEqual(["me", "b", "zed"]);
    expect(groups[1].repos.map((r) => r.name)).toEqual(["tools", "more"]);
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
    expect(repos.some((repo) => repo.name === "kgit")).toBe(true);
  });

  it("includes organization repositories alongside the user's own", async () => {
    const demoInvoke = await loadDemo();
    const owners = new Set((await ghRepoList(demoInvoke)).map((repo) => repo.owner));
    expect(owners.has("christine")).toBe(true);
    expect(owners.has("octo-org")).toBe(true);
  });

  it("looks up a fork's parent", async () => {
    const demoInvoke = await loadDemo();
    expect(await demoInvoke("gh_repo_parent", { nameWithOwner: "christine/dotfiles" })).toBe("shell-guild/dotfiles");
    expect(await demoInvoke("gh_repo_parent", { nameWithOwner: "christine/kgit" })).toBeNull();
  });
});

describe("demo backend: clone", () => {
  it("lists remote branches with the default first", async () => {
    const demoInvoke = await loadDemo();
    const result = await demoInvoke<RemoteBranches>("list_remote_branches", { url: "https://github.com/northwind/infra" });
    expect(result.defaultBranch).toBe("trunk");
    expect(result.branches[0]).toBe("trunk");
  });

  it("reports an existing checkout and a free path", async () => {
    const demoInvoke = await loadDemo();
    const taken = await demoInvoke<CloneTarget>("inspect_clone_target", { path: "/dev/kgit" });
    expect(taken.isRepo).toBe(true);
    const free = await demoInvoke<CloneTarget>("inspect_clone_target", { path: "/dev/brand-new" });
    expect(free.exists).toBe(false);
    const found = await demoInvoke<string[]>("existing_checkouts", { paths: ["/dev/kgit", "/dev/waas"] });
    expect(found).toEqual(["/dev/kgit"]);
  });

  it("streams progress and can be cancelled", async () => {
    vi.useFakeTimers();
    try {
      vi.resetModules();
      const { demoClone, demoInvoke } = await import("./demo");
      const updates: CloneProgress[] = [];
      const pending = demoClone({ url: "o/r", path: "/dev/r" }, (update) => updates.push(update));
      const settled = pending.catch((err: Error) => err.message);
      await vi.advanceTimersByTimeAsync(500);
      expect(updates.length).toBeGreaterThan(2);
      expect(await demoInvoke<boolean>("cancel_clone", {})).toBe(true);
      expect(await settled).toBe("Clone cancelled.");
      expect(await demoInvoke<boolean>("cancel_clone", {})).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });
});
