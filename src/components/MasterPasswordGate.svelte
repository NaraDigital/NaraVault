<script lang="ts">
  import Modal from "./Modal.svelte";
  import Icon from "./Icon.svelte";
  import Input from "./Input.svelte";
  import Button from "./Button.svelte";

  type Action = "reveal" | "edit" | "delete";
  interface Props {
    open: boolean;
    itemName?: string;
    action?: Action;
    onclose: () => void;
    onverify: (password: string) => Promise<boolean>;
  }
  let { open, itemName = "", action = "reveal", onclose, onverify }: Props = $props();

  const VERB: Record<Action, string> = { reveal: "reveal", edit: "edit", delete: "delete" };
  const CTA: Record<Action, string> = { reveal: "Reveal", edit: "Unlock edit", delete: "Delete" };
  const ICON: Record<Action, string> = { reveal: "unlock", edit: "edit", delete: "trash" };

  let pw = $state("");
  let err = $state(false);
  let busy = $state(false);

  // Clear the field whenever the gate opens OR closes, so the master password
  // never lingers in component memory after a reveal.
  $effect(() => {
    open;
    pw = "";
    err = false;
  });

  async function submit() {
    if (busy) return;
    busy = true;
    try {
      const ok = await onverify(pw);
      if (!ok) {
        err = true;
        pw = "";
      }
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} {onclose} width={400}>
  <div class="gate">
    <div class="gate-icon"><Icon name="lock" size={22} /></div>
    <h3>Confirm master password</h3>
    <p>
      Re-enter your master password to {VERB[action]}
      {#if itemName}<strong>{itemName}</strong>{:else}this item{/if}.
    </p>
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
      <div class="gate-err"><Icon name="close" size={13} /> Incorrect password. Try again.</div>
    {/if}
    <div class="gate-actions">
      <Button variant="ghost" onclick={onclose} full disabled={busy}>Cancel</Button>
      <Button
        variant={action === "delete" ? "danger" : "primary"}
        onclick={submit}
        icon={ICON[action]}
        full
        loading={busy}>{busy ? "Verifying…" : CTA[action]}</Button
      >
    </div>
  </div>
</Modal>
