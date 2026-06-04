<script lang="ts">
  import IconButton from "./IconButton.svelte";
  import CopyButton from "./CopyButton.svelte";

  interface Props {
    label: string;
    value: string;
    revealed?: boolean;
    onToggle?: () => void;
    mono?: boolean;
    masked?: string;
    copyable?: boolean;
    mask?: string;
    /** When true, copying this value auto-clears the clipboard. */
    secret?: boolean;
  }
  let {
    label,
    value,
    revealed = false,
    onToggle,
    mono = true,
    masked,
    copyable = true,
    mask = "•••••••••••••",
    secret = false,
  }: Props = $props();
</script>

<div class="srow">
  <div class="srow-head">
    <span class="srow-label">{label}</span>
    <div class="srow-tools">
      {#if onToggle}
        <IconButton
          name={revealed ? "eyeOff" : "eye"}
          size={16}
          onclick={onToggle}
          title={revealed ? "Hide" : "Reveal"}
        />
      {/if}
      {#if copyable}<CopyButton text={value} {label} {secret} />{/if}
    </div>
  </div>
  <div class="srow-value{mono ? ' mono' : ''}{!revealed ? ' is-masked' : ''}">
    {revealed ? value : (masked ?? mask)}
  </div>
</div>
