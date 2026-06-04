<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    variant?: "primary" | "ghost" | "danger";
    size?: "md" | "sm";
    icon?: string;
    full?: boolean;
    disabled?: boolean;
    loading?: boolean;
    type?: "button" | "submit";
    style?: string;
    onclick?: () => void;
    children?: Snippet;
  }
  let {
    variant = "primary",
    size = "md",
    icon,
    full = false,
    disabled = false,
    loading = false,
    type = "button",
    style = "",
    onclick,
    children,
  }: Props = $props();
</script>

<button
  {type}
  disabled={disabled || loading}
  {style}
  class="btn btn-{variant} btn-{size}{full ? ' btn-full' : ''}{loading ? ' is-loading' : ''}"
  {onclick}
>
  {#if loading}
    <span class="spinner" aria-hidden="true"></span>
  {:else if icon}
    <Icon name={icon} size={size === "sm" ? 14 : 16} />
  {/if}
  {#if children}<span>{@render children()}</span>{/if}
</button>
