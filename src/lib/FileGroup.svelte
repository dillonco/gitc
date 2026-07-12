<script lang="ts">
  import type { FileStatus } from "./types";

  export let title: string;
  export let files: FileStatus[];
  export let open: (file: FileStatus) => void;
  export let action: (file: FileStatus, kind: string, label: string) => void;
  export let selectedPath: string | undefined = undefined;
  export let tree = false;
  export let hideWhenEmpty = false;

  type TreeRow = { key: string; depth: number; kind: "dir" | "file"; name: string; file?: FileStatus };
  type TreeDir = { dirs: Map<string, TreeDir>; files: FileStatus[] };

  let collapsed = new Set<string>();

  $: treeRows = tree ? buildTreeRows(files, collapsed) : [];

  function actionsFor(file: FileStatus) {
    if (file.group === "staged") return [{ kind: "unstage", label: "Unstage" }];
    if (file.group === "conflicted") return [{ kind: "markResolved", label: "Mark Resolved" }];
    if (file.group === "untracked") {
      return [
        { kind: "stage", label: "Stage" },
        { kind: "cleanUntracked", label: "Delete" },
      ];
    }
    return [
      { kind: "stage", label: "Stage" },
      { kind: "discard", label: "Discard" },
    ];
  }

  function buildTreeRows(source: FileStatus[], hidden: Set<string>): TreeRow[] {
    const root: TreeDir = { dirs: new Map(), files: [] };
    for (const entry of source) {
      const parts = entry.path.split("/");
      let node = root;
      for (const part of parts.slice(0, -1)) {
        if (!node.dirs.has(part)) node.dirs.set(part, { dirs: new Map(), files: [] });
        node = node.dirs.get(part)!;
      }
      node.files.push(entry);
    }

    const rows: TreeRow[] = [];
    const walk = (node: TreeDir, prefix: string, depth: number) => {
      for (const [name, child] of [...node.dirs.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
        const key = prefix ? `${prefix}/${name}` : name;
        rows.push({ key, depth, kind: "dir", name });
        if (!hidden.has(key)) walk(child, key, depth + 1);
      }
      for (const entry of [...node.files].sort((a, b) => a.path.localeCompare(b.path))) {
        rows.push({
          key: `${entry.path}:${entry.group}`,
          depth,
          kind: "file",
          name: entry.path.split("/").at(-1) ?? entry.path,
          file: entry,
        });
      }
    };
    walk(root, "", 0);
    return rows;
  }

  function toggleDir(key: string) {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    collapsed = next;
  }
</script>

<div class="file-group" class:hidden={hideWhenEmpty && files.length === 0}>
  <h2>{title} <span>{files.length}</span></h2>
  <div class="files">
    {#if tree}
      {#each treeRows as row (row.key)}
        {#if row.kind === "dir"}
          <button class="tree-dir" style={`--depth:${row.depth}`} on:click={() => toggleDir(row.key)}>
            <span>{collapsed.has(row.key) ? "›" : "⌄"}</span>
            <strong>{row.name}</strong>
          </button>
        {:else if row.file}
          <div class="file-row tree-row" class:active={selectedPath === row.file.path} style={`--depth:${row.depth}`}>
            <button class="file-name tree-file" on:click={() => row.file && open(row.file)} title={row.file.path}>
              <span>{row.file.index}{row.file.worktree}</span>
              <strong>{row.name}</strong>
            </button>
            <div class="file-actions">
              {#each actionsFor(row.file) as item}
                <button
                  class:item-danger={item.kind === "discard" || item.kind === "cleanUntracked"}
                  on:click={() => row.file && action(row.file, item.kind, item.label)}
                >
                  {item.label}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      {:else}
        <p class="empty">No files</p>
      {/each}
    {:else}
      {#each files as file}
        <div class="file-row" class:active={selectedPath === file.path}>
          <button class="file-name" on:click={() => open(file)} title={file.path}>
            <span>{file.index}{file.worktree}</span>
            <strong>{file.path}</strong>
          </button>
          <div class="file-actions">
            {#each actionsFor(file) as item}
              <button
                class:item-danger={item.kind === "discard" || item.kind === "cleanUntracked"}
                on:click={() => action(file, item.kind, item.label)}
              >
                {item.label}
              </button>
            {/each}
          </div>
        </div>
      {:else}
        <p class="empty">No files</p>
      {/each}
    {/if}
  </div>
</div>
