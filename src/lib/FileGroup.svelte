<script lang="ts">
  import type { FileStatus } from "./types";

  export let title: string;
  export let files: FileStatus[];
  export let open: (file: FileStatus) => void;
  export let action: (file: FileStatus, kind: string, label: string) => void;
  export let selectedPath: string | undefined = undefined;

  function actionsFor(file: FileStatus) {
    if (file.group === "staged") return [{ kind: "unstage", label: "Unstage" }];
    if (file.group === "conflicted") return [{ kind: "markResolved", label: "Mark Resolved" }];
    return [
      { kind: "stage", label: "Stage" },
      { kind: "discard", label: "Discard" },
    ];
  }
</script>

<div class="file-group">
  <h2>{title} <span>{files.length}</span></h2>
  <div class="files">
    {#each files as file}
      <div class="file-row" class:active={selectedPath === file.path}>
        <button class="file-name" on:click={() => open(file)} title={file.path}>
          <span>{file.index}{file.worktree}</span>
          <strong>{file.path}</strong>
        </button>
        <div class="file-actions">
          {#each actionsFor(file) as item}
            <button class:item-danger={item.kind === "discard"} on:click={() => action(file, item.kind, item.label)}>
              {item.label}
            </button>
          {/each}
        </div>
      </div>
    {:else}
      <p class="empty">No files</p>
    {/each}
  </div>
</div>
