<script lang="ts">
  import Icon from "./Icon.svelte";
  import { toasts } from "../lib/toast.svelte";
</script>

<div class="toast-wrap">
  {#each toasts.items as t (t.id)}
    <div class="toast">
      <Icon name={t.icon || "check"} size={15} />
      <span>{t.msg}</span>
    </div>
  {/each}
</div>
