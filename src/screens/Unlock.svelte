<script lang="ts">
  import BrandMark from "../components/BrandMark.svelte";
  import Icon from "../components/Icon.svelte";
  import Input from "../components/Input.svelte";
  import Button from "../components/Button.svelte";

  interface Props {
    hint: string;
    onunlock: (password: string) => Promise<boolean>;
  }
  let { hint, onunlock }: Props = $props();

  let pw = $state("");
  let err = $state(false);
  let showHint = $state(false);
  let busy = $state(false);

  async function submit() {
    if (busy) return;
    busy = true;
    const ok = await onunlock(pw);
    busy = false;
    if (!ok) {
      err = true;
      pw = "";
    }
  }
</script>

<div class="lock-screen">
  <div class="lock-card">
    <div class="lock-badge"><Icon name="lock" size={26} /></div>
    <BrandMark size={28} />
    <h2>Welcome back</h2>
    <p>Enter your master password to unlock the vault.</p>

    <Input
      bind:value={pw}
      type="password"
      placeholder="Master password"
      mono
      autofocus
      oninput={() => (err = false)}
      onkeydownEnter={submit}
    />
    {#if err}
      <div class="form-err"><Icon name="close" size={14} /> Incorrect master password.</div>
    {/if}

    <Button
      variant="primary"
      full
      icon="unlock"
      loading={busy}
      onclick={submit}
      style="margin-top:6px">{busy ? "Unlocking…" : "Unlock"}</Button
    >

    {#if hint}
      <div class="lock-foot">
        {#if showHint}
          <span class="hint-text">Hint: <em>{hint}</em></span>
        {:else}
          <button class="link-btn" onclick={() => (showHint = true)}>Show hint</button>
        {/if}
      </div>
    {/if}
  </div>
</div>
