<script lang="ts">
  import type { Snippet } from "svelte";
  import IconButton from "./IconButton.svelte";

  interface Props {
    open: boolean;
    title?: string;
    width?: number;
    onclose?: () => void;
    children: Snippet;
  }
  let { open, title = "", width = 440, onclose, children }: Props = $props();

  $effect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onclose?.();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  });
</script>

{#if open}
  <div
    class="modal-scrim"
    role="presentation"
    onmousedown={() => onclose?.()}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      style:width="{width}px"
      onmousedown={(e) => e.stopPropagation()}
    >
      {#if title}
        <div class="modal-head">
          <h3>{title}</h3>
          <IconButton name="close" size={18} onclick={() => onclose?.()} title="Close" />
        </div>
      {/if}
      {@render children()}
    </div>
  </div>
{/if}
