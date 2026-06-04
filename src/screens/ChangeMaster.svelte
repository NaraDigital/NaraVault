<script lang="ts">
  import Modal from "../components/Modal.svelte";
  import Icon from "../components/Icon.svelte";
  import Field from "../components/Field.svelte";
  import Input from "../components/Input.svelte";
  import Button from "../components/Button.svelte";
  import StrengthMeter from "../components/StrengthMeter.svelte";
  import { vault } from "../lib/vault.svelte";
  import { toasts } from "../lib/toast.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
  }
  let { open, onclose }: Props = $props();

  let cur = $state("");
  let nw = $state("");
  let cf = $state("");
  let err = $state("");
  let busy = $state(false);

  $effect(() => {
    if (open) {
      cur = "";
      nw = "";
      cf = "";
      err = "";
    }
  });

  async function submit() {
    if (busy) return;
    if (nw.length < 8) {
      err = "New password must be at least 8 characters.";
      return;
    }
    if (nw !== cf) {
      err = "New passwords do not match.";
      return;
    }
    busy = true;
    try {
      const ok = await vault.changeMaster(cur, nw);
      if (!ok) {
        err = "Current password is incorrect.";
        return;
      }
      toasts.push("Master password changed", "check");
      onclose();
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} {onclose} title="Change master password" width={420}>
  <div class="gate" style="padding-top:6px">
    <Field label="Current password">
      <Input bind:value={cur} type="password" mono autofocus oninput={() => (err = "")} />
    </Field>
    <Field label="New password">
      <Input bind:value={nw} type="password" mono oninput={() => (err = "")} />
    </Field>
    <StrengthMeter password={nw} />
    <Field label="Confirm new password">
      <Input
        bind:value={cf}
        type="password"
        mono
        oninput={() => (err = "")}
        onkeydownEnter={submit}
      />
    </Field>
    {#if err}
      <div class="gate-err"><Icon name="close" size={13} /> {err}</div>
    {/if}
    <div class="gate-actions">
      <Button variant="ghost" onclick={onclose} full disabled={busy}>Cancel</Button>
      <Button variant="primary" onclick={submit} icon="key" full loading={busy}
        >{busy ? "Updating…" : "Update"}</Button
      >
    </div>
  </div>
</Modal>
