<script lang="ts">
  import IconButton from "../../components/IconButton.svelte";
  import CopyButton from "../../components/CopyButton.svelte";
  import { totpCode, totpRemaining } from "../../lib/util";

  interface Props {
    secret: string;
    unlocked: boolean;
    onUnlock: () => void;
  }
  let { secret, unlocked, onUnlock }: Props = $props();

  let now = $state(Date.now());
  let code = $state<string | null>(null);

  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });

  // Recompute the code whenever the time step or secret changes.
  $effect(() => {
    const s = secret;
    const t = now;
    let cancelled = false;
    totpCode(s, t).then((c) => {
      if (!cancelled) code = c;
    });
    return () => {
      cancelled = true;
    };
  });

  let remain = $derived(totpRemaining(now));
  let pct = $derived((remain / 30) * 100);
  let valid = $derived(code !== null);
  let display = $derived(
    !unlocked ? "••• •••" : valid ? `${code!.slice(0, 3)} ${code!.slice(3)}` : "invalid",
  );
</script>

<div class="srow totp-row">
  <div class="srow-head">
    <span class="srow-label">One-time code (TOTP)</span>
    <div class="srow-tools">
      {#if !unlocked}
        <IconButton name="eye" size={16} onclick={onUnlock} title="Reveal" />
      {:else if valid}
        <CopyButton text={code!} label="Code" secret />
      {/if}
    </div>
  </div>
  <div class="totp-body">
    <div class="totp-code mono{!unlocked ? ' is-masked' : ''}">{display}</div>
    {#if unlocked && valid}
      <div class="totp-timer" title="{remain}s remaining">
        <svg viewBox="0 0 36 36" width="34" height="34">
          <circle cx="18" cy="18" r="15" class="totp-track" />
          <circle
            cx="18"
            cy="18"
            r="15"
            class="totp-prog"
            style="stroke-dasharray:{2 * Math.PI * 15};stroke-dashoffset:{2 *
              Math.PI *
              15 *
              (1 - pct / 100)}"
          />
        </svg>
        <span class="totp-num mono">{remain}</span>
      </div>
    {/if}
  </div>
</div>
