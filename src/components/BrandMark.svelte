<script lang="ts">
  import Icon from "./Icon.svelte";

  interface Props {
    size?: number;
  }
  let { size = 30 }: Props = $props();
</script>

<div class="brand">
  <div class="brand-mark" style="width:{size}px;height:{size}px">
    <Icon name="shield" size={Math.round(size * 0.62)} />
  </div>
  <span class="brand-name">Nara<span class="brand-vault">Vault</span></span>
</div>
