<script lang="ts">
  import Icon from "./Icon.svelte";

  interface Props {
    name: string;
    title?: string;
    size?: number;
    active?: boolean;
    danger?: boolean;
    disabled?: boolean;
    loading?: boolean;
    onclick?: () => void;
  }
  let {
    name,
    title = "",
    size = 18,
    active = false,
    danger = false,
    disabled = false,
    loading = false,
    onclick,
  }: Props = $props();
</script>

<button
  type="button"
  {title}
  aria-label={title}
  disabled={disabled || loading}
  class="icon-btn{active ? ' is-active' : ''}{danger ? ' is-danger' : ''}{loading
    ? ' is-loading'
    : ''}"
  {onclick}
>
  {#if loading}
    <span class="spinner spinner-sm" aria-hidden="true"></span>
  {:else}
    <Icon {name} {size} />
  {/if}
</button>
