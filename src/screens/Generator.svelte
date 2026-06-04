<script lang="ts">
  import IconButton from "../components/IconButton.svelte";
  import Button from "../components/Button.svelte";
  import StrengthMeter from "../components/StrengthMeter.svelte";
  import { generatePassword, copySecret } from "../lib/util";
  import { toasts } from "../lib/toast.svelte";

  interface Props {
    onuse?: (password: string) => void;
  }
  let { onuse }: Props = $props();

  let length = $state(20);
  let upper = $state(true);
  let lower = $state(true);
  let digits = $state(true);
  let symbols = $state(true);
  let pw = $state("");

  function regen() {
    pw = generatePassword({ length, upper, lower, digits, symbols });
  }

  // Regenerate whenever any option changes.
  $effect(() => {
    void length;
    void upper;
    void lower;
    void digits;
    void symbols;
    regen();
  });

  async function copy() {
    try {
      await copySecret(pw);
    } catch {
      /* ignore */
    }
    toasts.push("Password copied", "copy");
  }

  const toggles = [
    { key: "upper", label: "Uppercase  A-Z" },
    { key: "lower", label: "Lowercase  a-z" },
    { key: "digits", label: "Digits  0-9" },
    { key: "symbols", label: "Symbols  !@#$" },
  ] as const;

  function get(key: string): boolean {
    return key === "upper" ? upper : key === "lower" ? lower : key === "digits" ? digits : symbols;
  }
  function set(key: string, v: boolean) {
    if (key === "upper") upper = v;
    else if (key === "lower") lower = v;
    else if (key === "digits") digits = v;
    else symbols = v;
  }
</script>

<div class="gen">
  <div class="gen-out">
    <span class="gen-pw mono">{pw}</span>
    <div class="gen-out-tools">
      <IconButton name="refresh" size={18} onclick={regen} title="Regenerate" />
      <IconButton name="copy" size={18} onclick={copy} title="Copy" />
    </div>
  </div>
  <StrengthMeter password={pw} />

  <div class="gen-controls">
    <div class="gen-row">
      <span class="gen-label">Length</span>
      <span class="gen-val mono">{length}</span>
    </div>
    <input type="range" min="8" max="48" bind:value={length} class="gen-slider" />

    {#each toggles as t (t.key)}
      <button
        class="toggle-row{get(t.key) ? ' is-on' : ''}"
        onclick={() => set(t.key, !get(t.key))}
      >
        <span class="mono">{t.label}</span>
        <span class="switch"><span class="knob"></span></span>
      </button>
    {/each}
  </div>

  {#if onuse}
    <Button variant="primary" full icon="check" onclick={() => onuse(pw)}>Use this password</Button>
  {/if}
</div>
