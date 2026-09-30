// Pure helpers for CloneDialog. Extracted so they can be unit tested without
// mounting the component (see gh.test.ts).
import type { GhRepo } from "./types";

/**
 * Derive a clone-into path from a repository URL and the user's configured
 * clone directory, e.g. `suggestClonePath("https://github.com/o/r.git", "/dev")`
 * -> `/dev/r`. Handles both HTTPS URLs and the scp-like SSH form
 * (`git@github.com:o/r.git`), strips a trailing `.git`, and tolerates a
 * trailing slash on either input.
 */
export function suggestClonePath(url: string, clonePath: string): string {
  const base = clonePath.replace(/\/+$/, "");
  return `${base}/${repoNameFromUrl(url)}`;
}

function repoNameFromUrl(url: string): string {
  const trimmed = url.trim().replace(/\/+$/, "");
  if (!trimmed) return "repo";
  // Split on both `/` and `:` so the scp-like SSH form
  // (`git@github.com:owner/repo.git`) yields the same last segment as an
  // HTTPS URL (`https://github.com/owner/repo.git`).
  const segments = trimmed.split(/[/:]/).filter(Boolean);
  const last = segments.pop() ?? "repo";
  const name = last.replace(/\.git$/i, "").trim();
  return name || "repo";
}

const OWNER_REPO = /^([A-Za-z0-9-]+)\/([A-Za-z0-9._-]+?)(?:\.git)?\/?$/;

/**
 * `owner/repo` for anything that names a GitHub repository: the shorthand
 * itself, `github.com/owner/repo`, an HTTPS or SSH URL, or a browser URL
 * with extra path (`/tree/main/src`). Null for anything else.
 */
export function githubRepoRef(input: string): string | null {
  const text = input.trim();
  if (!text) return null;
  const shorthand = text.match(OWNER_REPO);
  if (shorthand && !text.includes(":")) return `${shorthand[1]}/${shorthand[2]}`;
  const match = text.match(
    /^(?:https?:\/\/|ssh:\/\/git@|git@)?(?:www\.)?github\.com[/:]([A-Za-z0-9-]+)\/([A-Za-z0-9._-]+?)(?:\.git)?(?:[/?#].*)?$/i,
  );
  return match ? `${match[1]}/${match[2]}` : null;
}

/** Clone URL for `owner/repo` in the protocol `gh` is configured for. */
export function githubCloneUrl(ref: string, protocol: string | null | undefined): string {
  return protocol === "ssh" ? `git@github.com:${ref}.git` : `https://github.com/${ref}.git`;
}

/**
 * What the URL tab will actually hand to `git clone`: a full URL or local
 * path passes through; GitHub shorthand (`owner/repo`, `github.com/o/r`)
 * becomes a real clone URL.
 */
export function resolveCloneUrl(input: string, protocol: string | null | undefined): string {
  const text = input.trim();
  if (!text) return "";
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text) || /^[^/\s]+@[^/\s]+:/.test(text) || text.startsWith("/") || text.startsWith("~")) {
    return text;
  }
  const ref = githubRepoRef(text);
  return ref ? githubCloneUrl(ref, protocol) : text;
}

/**
 * Canonical `host/owner/repo` form of a remote URL, so an SSH origin and
 * an HTTPS URL for the same repository compare equal.
 */
export function normalizeRemote(url: string | null | undefined): string {
  let text = (url ?? "").trim().toLowerCase();
  text = text.replace(/^[a-z][a-z0-9+.-]*:\/\//, "");
  text = text.replace(/^[^@/]+@/, "");
  text = text.replace(/^([^/:]+):(?!\d+\/)/, "$1/");
  text = text.replace(/^([^/:]+):\d+\//, "$1/");
  return text.replace(/\/+$/, "").replace(/\.git$/, "");
}

export function sameRemote(a: string | null | undefined, b: string | null | undefined): boolean {
  const left = normalizeRemote(a);
  return Boolean(left) && left === normalizeRemote(b);
}

/** Filter the repo list; a pasted GitHub URL matches its repository. */
export function filterRepos(repos: GhRepo[], query: string): GhRepo[] {
  const text = query.trim().toLowerCase();
  if (!text) return repos;
  const ref = githubRepoRef(text);
  if (ref && (text.includes("github.com") || text.includes("/"))) {
    const exact = repos.filter((repo) => repo.nameWithOwner.toLowerCase() === ref.toLowerCase());
    if (exact.length) return exact;
  }
  return repos.filter(
    (repo) =>
      repo.nameWithOwner.toLowerCase().includes(text) ||
      (repo.description ?? "").toLowerCase().includes(text) ||
      (repo.language ?? "").toLowerCase() === text,
  );
}

export type OwnerGroup = { owner: string; repos: GhRepo[] };

/**
 * Group repositories by owner: the signed-in account first, then the other
 * owners alphabetically. Order within a group is kept (most recently pushed).
 */
export function groupByOwner(repos: GhRepo[], self: string | null | undefined): OwnerGroup[] {
  const groups = new Map<string, GhRepo[]>();
  for (const repo of repos) {
    const list = groups.get(repo.owner);
    if (list) list.push(repo);
    else groups.set(repo.owner, [repo]);
  }
  const selfKey = (self ?? "").toLowerCase();
  return [...groups.entries()]
    .map(([owner, list]) => ({ owner, repos: list }))
    .sort((a, b) => {
      const aSelf = a.owner.toLowerCase() === selfKey;
      const bSelf = b.owner.toLowerCase() === selfKey;
      if (aSelf !== bSelf) return aSelf ? -1 : 1;
      return a.owner.localeCompare(b.owner, undefined, { sensitivity: "base" });
    });
}

// Listing repositories through `gh` takes a second or two, so the last list
// is kept (in memory and across launches): the picker fills instantly and
// refreshes in the background.
const REPO_CACHE_KEY = "gitc.clone.repos.v2";
let repoListCache: GhRepo[] | null = null;

export function cachedRepoList(): GhRepo[] | null {
  if (repoListCache) return repoListCache;
  try {
    const raw = localStorage.getItem(REPO_CACHE_KEY);
    const parsed = raw ? JSON.parse(raw) : null;
    repoListCache = Array.isArray(parsed) ? (parsed as GhRepo[]) : null;
  } catch {
    repoListCache = null;
  }
  return repoListCache;
}

export function cacheRepoList(repos: GhRepo[]) {
  repoListCache = repos;
  try {
    localStorage.setItem(REPO_CACHE_KEY, JSON.stringify(repos));
  } catch {
    // Storage full or unavailable: the in-memory copy still helps.
  }
}

export function clearRepoListCache() {
  repoListCache = null;
  try {
    localStorage.removeItem(REPO_CACHE_KEY);
  } catch {
    // Nothing to clear.
  }
}

const LAST_PARENT_KEY = "gitc.clone.lastParent";

/** The folder the last clone went into, if the user picked one. */
export function lastCloneParent(): string | null {
  try {
    return localStorage.getItem(LAST_PARENT_KEY);
  } catch {
    return null;
  }
}

export function rememberCloneParent(parent: string) {
  try {
    localStorage.setItem(LAST_PARENT_KEY, parent);
  } catch {
    // Not worth surfacing.
  }
}

/** Split `/a/b/repo` into `/a/b` and `repo`. */
export function splitPath(path: string): { parent: string; name: string } {
  const trimmed = path.trim().replace(/\/+$/, "");
  const index = trimmed.lastIndexOf("/");
  if (index < 0) return { parent: "", name: trimmed };
  return { parent: trimmed.slice(0, index) || "/", name: trimmed.slice(index + 1) };
}

export function joinPath(parent: string, name: string): string {
  const base = parent.trim().replace(/\/+$/, "");
  const leaf = name.trim().replace(/^\/+/, "");
  if (!leaf) return "";
  return base ? `${base}/${leaf}` : leaf;
}

// GitHub's linguist colours for the languages people actually clone.
const LANGUAGE_COLORS: Record<string, string> = {
  C: "#555555",
  "C#": "#178600",
  "C++": "#f34b7d",
  CSS: "#663399",
  Dart: "#00b4ab",
  Dockerfile: "#384d54",
  Elixir: "#6e4a7e",
  Go: "#00add8",
  HCL: "#844fba",
  HTML: "#e34c26",
  Java: "#b07219",
  JavaScript: "#f1e05a",
  "Jupyter Notebook": "#da5b0b",
  Kotlin: "#a97bff",
  Lua: "#000080",
  Makefile: "#427819",
  MDX: "#fcb32c",
  Nix: "#7e7eff",
  PHP: "#4f5d95",
  Python: "#3572a5",
  Ruby: "#701516",
  Rust: "#dea584",
  SCSS: "#c6538c",
  Shell: "#89e051",
  Solidity: "#aa6746",
  Svelte: "#ff3e00",
  Swift: "#f05138",
  TypeScript: "#3178c6",
  Vue: "#41b883",
  Zig: "#ec915c",
};

export function languageColor(language: string | null | undefined): string {
  return (language && LANGUAGE_COLORS[language]) || "#8b93a0";
}

/** Split `text` around the first case-insensitive occurrence of `query`. */
export function highlightParts(text: string, query: string): { text: string; match: boolean }[] {
  const needle = query.trim().toLowerCase();
  const index = needle ? text.toLowerCase().indexOf(needle) : -1;
  if (index < 0) return [{ text, match: false }];
  return [
    { text: text.slice(0, index), match: false },
    { text: text.slice(index, index + needle.length), match: true },
    { text: text.slice(index + needle.length), match: false },
  ].filter((part) => part.text);
}

/** GitHub avatar for a user or organization login. */
export function ownerAvatarUrl(login: string, size = 40): string {
  return `https://avatars.githubusercontent.com/${encodeURIComponent(login)}?s=${size}`;
}
