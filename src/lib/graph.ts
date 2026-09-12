import type { CommitNode } from "./types";

// The graph palette, handed out by column so a lane keeps
// its colour for as long as it runs.
export const graphColors = [
  "#15a0bf",
  "#0669f7",
  "#8e00c2",
  "#c517b6",
  "#d90171",
  "#cd0101",
  "#f25d2e",
  "#f2ca33",
  "#7bd938",
  "#2ece9d",
];

export function laneColor(index: number) {
  return graphColors[index % graphColors.length];
}

export type GraphLane = {
  index: number;
  color: string;
  capStart: boolean;
  capEnd: boolean;
};

export type GraphEdge = {
  from: number;
  to: number;
  color: string;
};

// One pill in the branch column. A local branch and its remote-tracking
// copies that sit on the same commit share a pill, named without the remote
// prefix.
export type RefGroup = {
  name: string;
  head: boolean;
  local: boolean;
  remotes: string[];
  tag: boolean;
};

export type GraphRow = {
  commit: CommitNode;
  lane: number;
  lanes: GraphLane[];
  // Merge edges: out of the node, across, and down into a parent's lane.
  edges: GraphEdge[];
  // Lanes that end at this commit: down their own column, then across into the node.
  joins: GraphEdge[];
  color: string;
  head: boolean;
  refs: RefGroup[];
  labels: string[];
  dateBucket: string;
};

export type GraphOptions = {
  hasWip?: boolean;
  remotes?: string[];
  now?: number;
};

function claimLane(lanes: string[], hash: string) {
  let lane = lanes.findIndex((value) => value === "");
  if (lane === -1) lane = lanes.length;
  lanes[lane] = hash;
  return lane;
}

export function buildGraphRows(nodes: CommitNode[], options: GraphOptions = {}): GraphRow[] {
  const { hasWip = false, remotes = [], now = Date.now() / 1000 } = options;
  const lanes: string[] = [];
  const rows: GraphRow[] = [];

  for (const commit of nodes) {
    let lane = lanes.indexOf(commit.hash);
    const laneIsNew = lane === -1;
    if (laneIsNew) lane = claimLane(lanes, commit.hash);

    // Branches that forked from this commit each hold a lane waiting for it.
    // The node takes the leftmost, and every other one ends here by turning
    // into the node; left open, they ran on past their parent to the bottom
    // of the graph and were never reused.
    const joins: GraphEdge[] = [];
    lanes.forEach((hash, index) => {
      if (index !== lane && hash === commit.hash) joins.push({ from: index, to: lane, color: laneColor(index) });
    });

    const firstParent = commit.parents[0] ?? "";
    const visibleLanes = lanes.flatMap((hash, index) =>
      hash && (index === lane || hash !== commit.hash)
        ? [
            {
              index,
              color: laneColor(index),
              // The topmost lane-0 commit is left uncapped only when a WIP row sits
              // above it to connect to. On a clean tree nothing is up there, so
              // capping it stops a rail stub hanging off the top of the graph.
              capStart: index === lane && laneIsNew && !(rows.length === 0 && lane === 0 && hasWip),
              capEnd: index === lane && !firstParent,
            },
          ]
        : [],
    );

    for (const join of joins) lanes[join.from] = "";
    lanes[lane] = firstParent;

    const edges: GraphEdge[] = [];
    for (const parent of commit.parents.slice(1)) {
      if (parent === firstParent) continue;
      let parentLane = lanes.indexOf(parent);
      if (parentLane === -1) parentLane = claimLane(lanes, parent);
      edges.push({ from: lane, to: parentLane, color: laneColor(parentLane) });
    }

    // Trailing free lanes would otherwise keep the column at its widest.
    while (lanes.length && !lanes[lanes.length - 1]) lanes.pop();

    rows.push({
      commit,
      lane,
      lanes: visibleLanes,
      edges,
      joins,
      color: laneColor(lane),
      head: commit.refs.some((ref) => ref === "HEAD" || ref.startsWith("HEAD -> ")),
      refs: groupRefs(commit.refs, remotes),
      labels: commit.refs
        .map((ref) => ref.replace(/^HEAD -> /, "").replace(/^tag:\s*/, "tag:"))
        .filter((ref) => ref && !ref.endsWith("/HEAD")),
      dateBucket: relativeBucket(now - commit.timestamp),
    });
  }

  return rows;
}

export function groupRefs(refs: string[], remotes: string[]): RefGroup[] {
  const groups = new Map<string, RefGroup>();
  const group = (name: string, tag = false) => {
    const key = `${tag ? "tag" : "branch"}:${name}`;
    let entry = groups.get(key);
    if (!entry) {
      entry = { name, head: false, local: false, remotes: [], tag };
      groups.set(key, entry);
    }
    return entry;
  };

  for (const raw of refs) {
    const ref = raw.trim();
    if (!ref || ref.endsWith("/HEAD")) continue;
    if (ref === "HEAD") {
      group("HEAD").head = true;
      continue;
    }
    if (ref.startsWith("HEAD -> ")) {
      const entry = group(ref.slice("HEAD -> ".length));
      entry.head = true;
      entry.local = true;
      continue;
    }
    const tag = ref.match(/^tag:\s*(.+)$/);
    if (tag) {
      group(tag[1], true);
      continue;
    }
    const remote = remotes.find((name) => ref.startsWith(`${name}/`));
    if (remote) {
      group(ref.slice(remote.length + 1)).remotes.push(remote);
      continue;
    }
    group(ref).local = true;
  }

  return [...groups.values()].sort(
    (a, b) => Number(b.head) - Number(a.head) || Number(a.tag) - Number(b.tag) || a.name.localeCompare(b.name),
  );
}

// https, ssh (scp-style or ssh://, including ssh.github.com:443) and token URLs.
const githubUrl = /^(?:(?:https?|ssh|git):\/\/)?(?:[^@/]+@)?(?:www\.|ssh\.)?github\.com(?::\d+)?[:/]+([^/]+)\//i;

export function isGithubUrl(url: string | null | undefined) {
  return !!url && /^(?:(?:https?|ssh|git):\/\/)?(?:[^@/]+@)?(?:www\.|ssh\.)?github\.com[:/]/i.test(url.trim());
}

// The account (user or org) a GitHub remote belongs to, so a pill can carry
// its avatar in place of a generic GitHub mark. Null for anything that is
// not a GitHub URL with an owner segment.
export function githubOwner(url: string | null | undefined): string | null {
  const match = url?.trim().match(githubUrl);
  return match && match[1] ? match[1] : null;
}

export function githubOwnerAvatar(url: string | null | undefined, size = 32): string | null {
  const owner = githubOwner(url);
  return owner ? `https://avatars.githubusercontent.com/${encodeURIComponent(owner)}?s=${size}` : null;
}

// Coarse, floored buckets for the date dividers. git's own relative dates
// are minute-precise under 90 minutes, which put a divider on nearly every
// row of a busy afternoon.
export function relativeBucket(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  if (hours < 1) return "less than an hour ago";
  if (hours < 24) return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
  const days = Math.floor(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 30) {
    const weeks = Math.floor(days / 7);
    return weeks === 1 ? "last week" : `${weeks} weeks ago`;
  }
  if (days < 365) {
    const months = Math.floor(days / 30);
    return months === 1 ? "last month" : `${months} months ago`;
  }
  const years = Math.floor(days / 365);
  return years === 1 ? "last year" : `${years} years ago`;
}
