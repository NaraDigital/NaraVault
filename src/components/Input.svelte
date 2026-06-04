<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    value: string;
    type?: "text" | "password";
    placeholder?: string;
    mono?: boolean;
    autofocus?: boolean;
    onkeydownEnter?: () => void;
    oninput?: (v: string) => void;
    right?: Snippet;
  }
  let {
    value = $bindable(""),
    type = "text",
    placeholder = "",
    mono = false,
    autofocus = false,
    onkeydownEnter,
    oninput,
    right,
  }: Props = $props();

  function handleKey(e: KeyboardEvent) {
    if (e.key === "Enter" && onkeydownEnter) onkeydownEnter();
  }
  function handleInput(e: Event) {
    value = (e.target as HTMLInputElement).value;
    oninput?.(value);
  }
</script>

<div class="input-wrap">
  <!-- svelte-ignore a11y_autofocus -->
  <input
    class="input{mono ? ' input-mono' : ''}"
    {type}
    {value}
    {placeholder}
    {autofocus}
    onkeydown={handleKey}
    oninput={handleInput}
  />
  {#if right}<div class="input-right">{@render right()}</div>{/if}
</div>
