<script lang="ts">
  import Icon from "./Icon.svelte";
  import { TYPE_TINT } from "../lib/categories";
  import type { ItemType } from "../lib/types";

  interface Props {
    type: ItemType;
    size?: number;
    letter?: string | null;
  }
  let { type, size = 38, letter = null }: Props = $props();
</script>

<div class="item-glyph" style="width:{size}px;height:{size}px;--tint:{TYPE_TINT[type]}">
  {#if letter}
    <span class="glyph-letter">{letter}</span>
  {:else}
    <Icon name={type} size={Math.round(size * 0.46)} />
  {/if}
</div>
