<script lang="ts">
  import IconButton from "./IconButton.svelte";
  import { copyToClipboard, copySecret, CLIPBOARD_CLEAR_MS } from "../lib/util";
  import { toasts } from "../lib/toast.svelte";

  interface Props {
    text: string;
    label?: string;
    size?: number;
    /** Secrets auto-clear from the clipboard after a short delay. */
    secret?: boolean;
  }
  let { text, label = "", size = 16, secret = false }: Props = $props();

  let done = $state(false);

  async function copy() {
    try {
      if (secret) await copySecret(text);
      else await copyToClipboard(text);
    } catch {
      /* ignore */
    }
    done = true;
    const base = label ? `${label} copied` : "Copied";
    toasts.push(secret ? `${base} · clears in ${CLIPBOARD_CLEAR_MS / 1000}s` : base, "copy");
    setTimeout(() => (done = false), 1200);
  }
</script>

<IconButton name={done ? "check" : "copy"} {size} onclick={copy} title="Copy" active={done} />
