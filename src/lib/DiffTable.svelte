<script lang="ts">
  type DiffRow = { kind: string; oldNo: string; newNo: string; text: string };
  type SplitRow =
    | { kind: "full"; row: DiffRow }
    | { kind: "pair"; left: DiffRow | null; right: DiffRow | null };

  export let rows: DiffRow[];
  export let split = false;
  export let selectedHunkRow: number | null = null;

  $: splitRows = split ? buildSplitRows(rows) : [];

  function buildSplitRows(source: DiffRow[]): SplitRow[] {
    const out: SplitRow[] = [];
    let index = 0;
    while (index < source.length) {
      const row = source[index];
      if (row.kind === "hunk" || row.kind === "meta") {
        out.push({ kind: "full", row });
        index += 1;
        continue;
      }
      if (row.kind === "ctx") {
        out.push({ kind: "pair", left: row, right: row });
        index += 1;
        continue;
      }
      const dels: DiffRow[] = [];
      const adds: DiffRow[] = [];
      while (index < source.length && source[index].kind === "del") dels.push(source[index++]);
      while (index < source.length && source[index].kind === "add") adds.push(source[index++]);
      const pairs = Math.max(dels.length, adds.length);
      for (let pair = 0; pair < pairs; pair += 1) {
        out.push({ kind: "pair", left: dels[pair] ?? null, right: adds[pair] ?? null });
      }
    }
    return out;
  }
</script>

{#if split}
  <div class="diff-table split-diff">
    {#each splitRows as item}
      {#if item.kind === "full"}
        <div class="diff-line hunk-row split-full"><code>{item.row.text}</code></div>
      {:else}
        <div class="split-line">
          <span class="line-no">{item.left?.oldNo ?? ""}</span>
          <code class:del-cell={item.left?.kind === "del"} class:blank-cell={!item.left}>{item.left?.text ?? ""}</code>
          <span class="line-no">{item.right?.newNo ?? ""}</span>
          <code class:add-cell={item.right?.kind === "add"} class:blank-cell={!item.right}>{item.right?.text ?? ""}</code>
        </div>
      {/if}
    {/each}
  </div>
{:else}
  <div class="diff-table">
    {#each rows as row, index}
      <div
        class="diff-line"
        class:hunk-row={row.kind === "hunk"}
        class:selected-hunk={selectedHunkRow === index}
        class:add-row={row.kind === "add"}
        class:del-row={row.kind === "del"}
        class:meta-row={row.kind === "meta"}
      >
        <span class="line-no">{row.oldNo}</span>
        <span class="line-no">{row.newNo}</span>
        <code>{row.text}</code>
      </div>
    {/each}
  </div>
{/if}
