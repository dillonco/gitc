import { describe, expect, it } from "vitest";
import { buildGraphRows, groupRefs, isGithubUrl, relativeBucket } from "./graph";
import type { CommitNode } from "./types";

describe("isGithubUrl", () => {
  it("recognises GitHub remotes in every URL form git accepts", () => {
    expect(isGithubUrl("git@github.com:octo-org/example.git")).toBe(true);
    expect(isGithubUrl("https://github.com/octo-org/example.git")).toBe(true);
    expect(isGithubUrl("https://x-access-token:abc@github.com/octo-org/example")).toBe(true);
    expect(isGithubUrl("ssh://git@ssh.github.com:443/octo-org/example.git")).toBe(true);
  });

  it("rejects other hosts, including look-alikes", () => {
    expect(isGithubUrl("git@gitlab.com:group/repo.git")).toBe(false);
    expect(isGithubUrl("https://github.com.evil.dev/repo.git")).toBe(false);
    expect(isGithubUrl("https://notgithub.com/repo.git")).toBe(false);
    expect(isGithubUrl("/srv/repos/local.git")).toBe(false);
    expect(isGithubUrl(undefined)).toBe(false);
  });
});

function node(hash: string, parents: string[], refs: string[] = []): CommitNode {
  return {
    hash,
    shortHash: hash,
    parents,
    refs,
    author: "Octo Cat",
    email: "octocat@example.com",
    timestamp: 0,
    relativeDate: "",
    subject: hash,
    bodySummary: "",
  };
}

// The shape of a real history (`git log --all --date-order`, renamed) where
// five branches fork from one merge commit. It should lay out in five columns
// that all close at that merge and are reused below it.
const forked = [
  node("main2", ["main1"], ["origin/main", "origin/HEAD"]),
  node("main1", ["mergeB"]),
  node("mergeB", ["mergeA", "b2"]),
  node("b2", ["b1"], ["feature-b"]),
  node("b1", ["b0"]),
  node("c3", ["c2"], ["origin/feature-c", "feature-c"]),
  node("c2", ["c1"]),
  node("c1", ["mergeA"]),
  node("d1", ["mergeA"], ["origin/fix-d", "fix-d"]),
  node("b0", ["mergeA"]),
  node("e6", ["e5"], ["HEAD -> feature-e", "origin/feature-e"]),
  node("e5", ["e4"]),
  node("e4", ["e3"]),
  node("e3", ["e2"]),
  node("e2", ["e1"]),
  node("e1", ["mergeA"]),
  node("mergeA", ["base", "f4"], ["main", "feature-a"]),
  node("f4", ["f3"], ["origin/feature-f", "feature-f"]),
  node("f3", ["f2"]),
  node("f2", ["f1"]),
  node("f1", ["base"]),
  node("base", ["root"], ["feature-g"]),
  node("h1", ["root"], ["origin/fix/h"]),
  node("i1", ["i0"], ["feature-i"]),
  node("root", ["older"]),
];

describe("buildGraphRows", () => {
  const rows = buildGraphRows(forked, { remotes: ["origin"] });
  const row = (hash: string) => rows.find((entry) => entry.commit.hash === hash)!;

  it("places every commit in its expected column", () => {
    expect(rows.map((entry) => entry.lane)).toEqual([0, 0, 0, 1, 1, 2, 2, 2, 3, 1, 4, 4, 4, 4, 4, 4, 0, 1, 1, 1, 1, 0, 1, 2, 0]);
  });

  it("closes every lane waiting on a commit at that commit", () => {
    expect(row("mergeA").joins.map((join) => join.from)).toEqual([1, 2, 3, 4]);
    expect(row("base").joins.map((join) => join.from)).toEqual([1]);
    expect(row("root").joins.map((join) => join.from)).toEqual([1]);
    // A closing lane turns into the node instead of also drawing a rail.
    expect(row("mergeA").lanes.map((lane) => lane.index)).toEqual([0]);
  });

  it("stops rails at the merge they fork from, so the column narrows again", () => {
    const below = rows.slice(rows.indexOf(row("mergeA")) + 1);
    expect(Math.max(...below.flatMap((entry) => entry.lanes.map((lane) => lane.index)))).toBe(2);
  });

  it("draws merge edges into the second parent's lane", () => {
    expect(row("mergeB").edges).toEqual([{ from: 0, to: 1, color: "#0669f7" }]);
    expect(row("mergeA").edges).toEqual([{ from: 0, to: 1, color: "#0669f7" }]);
  });

  it("finds HEAD from the refs, not by comparing abbreviated hashes", () => {
    expect(rows.filter((entry) => entry.head).map((entry) => entry.commit.hash)).toEqual(["e6"]);
  });

  it("caps the rail of a root commit", () => {
    const [root] = buildGraphRows([node("a", [])]);
    expect(root.lanes).toEqual([{ index: 0, color: "#15a0bf", capStart: true, capEnd: true }]);
  });
});

describe("groupRefs", () => {
  it("folds a branch and its remote copy into one pill", () => {
    expect(groupRefs(["HEAD -> feature-e", "origin/feature-e"], ["origin"])).toEqual([
      { name: "feature-e", head: true, local: true, remotes: ["origin"], tag: false },
    ]);
  });

  it("names a remote-only branch without its remote and drops origin/HEAD", () => {
    expect(groupRefs(["origin/main", "origin/HEAD"], ["origin"])).toEqual([
      { name: "main", head: false, local: false, remotes: ["origin"], tag: false },
    ]);
  });

  it("puts HEAD first, then branches by name, then tags", () => {
    const names = groupRefs(["tag: v1.0", "main", "fix/alpha", "HEAD -> zed"], ["origin"]).map((ref) => ref.name);
    expect(names).toEqual(["zed", "fix/alpha", "main", "v1.0"]);
  });

  it("keeps a slash-named local branch local", () => {
    expect(groupRefs(["fix/lifecycle"], ["origin"])[0]).toMatchObject({ local: true, remotes: [] });
  });
});

describe("relativeBucket", () => {
  const minutes = 60;
  const hours = 60 * minutes;

  it("floors to the hour, then to the day", () => {
    expect(relativeBucket(57 * minutes)).toBe("less than an hour ago");
    expect(relativeBucket(83 * minutes)).toBe("1 hour ago");
    expect(relativeBucket(2.8 * hours)).toBe("2 hours ago");
    expect(relativeBucket(21.4 * hours)).toBe("21 hours ago");
    expect(relativeBucket(47 * hours)).toBe("yesterday");
    expect(relativeBucket(3 * 24 * hours)).toBe("3 days ago");
    expect(relativeBucket(10 * 24 * hours)).toBe("last week");
    expect(relativeBucket(70 * 24 * hours)).toBe("2 months ago");
  });
});
