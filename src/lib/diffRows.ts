export type DiffRow = { kind: string; oldNo: string; newNo: string; text: string };

export function parseDiffRows(diff: string): DiffRow[] {
  let oldLine = 0;
  let newLine = 0;
  const skip = [
    "diff --git",
    "index ",
    "--- ",
    "+++ ",
    "new file mode",
    "deleted file mode",
    "old mode",
    "new mode",
    "similarity index",
    "rename from",
    "rename to",
    "copy from",
    "copy to",
  ];
  return diff
    .split("\n")
    .filter((line) => !skip.some((prefix) => line.startsWith(prefix)))
    .map((line) => {
      const hunk = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      if (hunk) {
        oldLine = Number(hunk[1]);
        newLine = Number(hunk[2]);
        return { kind: "hunk", oldNo: "", newNo: "", text: line };
      }
      if (line.startsWith("\\")) return { kind: "meta", oldNo: "", newNo: "", text: line };
      if (line.startsWith("+")) return { kind: "add", oldNo: "", newNo: String(newLine++), text: line };
      if (line.startsWith("-")) return { kind: "del", oldNo: String(oldLine++), newNo: "", text: line };
      return { kind: "ctx", oldNo: oldLine ? String(oldLine++) : "", newNo: newLine ? String(newLine++) : "", text: line };
    });
}
