<script lang="ts">
  import { passwordStrength } from "../lib/util";

  interface Props {
    password: string;
  }
  let { password }: Props = $props();

  const COLORS = ["var(--danger)", "var(--warn)", "var(--warn)", "var(--ink)", "var(--ink)"];
  let s = $derived(passwordStrength(password));
</script>

<div class="strength">
  <div class="strength-bars">
    {#each [0, 1, 2, 3, 4] as i (i)}
      <span
        class="strength-bar"
        style:background={i <= s.score && password ? COLORS[s.score] : "var(--track)"}
      ></span>
    {/each}
  </div>
  <span class="strength-label" style:color={password ? COLORS[s.score] : "var(--text-dim)"}>
    {s.label}
  </span>
</div>
