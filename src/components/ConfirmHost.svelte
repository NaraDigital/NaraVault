<script lang="ts">
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import { confirm } from "../lib/confirm.svelte";
</script>

<Modal open={confirm.open} title={confirm.title} width={400} onclose={() => confirm.resolve(false)}>
  <div class="gate" style="text-align:left;gap:18px">
    <p style="margin:0;color:var(--text-dim);font-size:14px;line-height:1.55">{confirm.message}</p>
    <div class="gate-actions">
      <Button variant="ghost" onclick={() => confirm.resolve(false)} full>Cancel</Button>
      <Button
        variant={confirm.danger ? "danger" : "primary"}
        onclick={() => confirm.resolve(true)}
        full>{confirm.confirmLabel}</Button
      >
    </div>
  </div>
</Modal>
