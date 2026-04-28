<script lang="ts">
  export let title: string;
  export let content: string;
</script>

<div class="merge-pane">
  <h2>{title}</h2>
  <pre>{content || "No content available"}</pre>
</div>
