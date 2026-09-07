// Pure helpers for CloneDialog's URL tab. Extracted so they can be unit
// tested without mounting the component (see gh.test.ts).

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
