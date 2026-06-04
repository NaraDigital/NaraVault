<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    label?: string;
    hint?: string;
    children: Snippet;
  }
  let { label = "", hint = "", children }: Props = $props();
</script>

<label class="field">
  {#if label}<span class="field-label">{label}</span>{/if}
  <div>{@render children()}</div>
  {#if hint}<span class="field-hint">{hint}</span>{/if}
</label>
