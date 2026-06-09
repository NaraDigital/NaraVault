<script lang="ts">
  import Onboarding from "./screens/Onboarding.svelte";
  import Unlock from "./screens/Unlock.svelte";
  import Vault from "./screens/Vault.svelte";
  import ToastHost from "./components/ToastHost.svelte";
  import ConfirmHost from "./components/ConfirmHost.svelte";
  import { vault } from "./lib/vault.svelte";
  import { tweaks } from "./lib/tweaks.svelte";
  import { confirm } from "./lib/confirm.svelte";
  import { toasts } from "./lib/toast.svelte";
  import { launcherBridge } from "./lib/launcher.svelte";
  import { api } from "./lib/api";
  import { listen } from "@tauri-apps/api/event";

  interface ConsentPayload {
    id: number;
    origin: string;
    name: string;
    kind?: "login" | "card";
  }

  vault.init().catch((e: unknown) => {
    const msg =
      typeof e === "string"
        ? e
        : (e as { message?: string })?.message ??
          (e != null ? JSON.stringify(e) : "unknown");
    toasts.push(msg ? `Vault error: ${msg}` : "Failed to open the vault", "lock");
  });

  async function syncFromBackend() {
    await vault.refreshStatus();
    if (vault.phase === "unlocked") await vault.reloadItems();
  }

  // The quick launcher can unlock the vault and request an item even while this
  // window still shows the lock screen. These listeners (always mounted) keep
  // the main UI in sync: refresh phase/items, and stash the requested item id so
  // Vault.svelte can open it once it mounts.
  $effect(() => {
    const subs = [
      listen<string>("naravault://open-item", async (e) => {
        launcherBridge.pendingOpenId = e.payload;
        await syncFromBackend();
      }),
      listen("naravault://vault-changed", () => void syncFromBackend()),
      // Browser-autofill consent (M-A): the bridge asks before releasing any
      // secret to a site the user hasn't approved this session. We answer with an
      // explicit Allow/Deny so a local process that only stole the bridge token
      // still can't silently dump the vault.
      listen<ConsentPayload>("naravault://autofill-consent", async (e) => {
        const { id, origin, name, kind } = e.payload;
        const what = kind === "card" ? "card" : "login";
        const ok = await confirm.ask({
          title: "Allow browser autofill?",
          message: `Fill the ${what} “${name}” into ${origin}? Only allow this if you just triggered autofill there.`,
          confirmLabel: "Allow autofill",
        });
        await api.autofillConsentReply(id, ok).catch(() => {});
      }),
      // Browser-save consent: the extension wants to WRITE (add or update) an
      // item. A write is more sensitive than autofill, so it is never cached —
      // every save asks here before anything is persisted to the vault.
      listen<ConsentPayload>("naravault://autofill-save-consent", async (e) => {
        const { id, origin, name } = e.payload;
        const where = origin ? ` for ${origin}` : "";
        const ok = await confirm.ask({
          title: "Save to vault from browser?",
          message: `Save “${name}”${where} to your vault? Only allow this if you just chose to save it from the NaraVault extension.`,
          confirmLabel: "Save",
        });
        await api.autofillConsentReply(id, ok).catch(() => {});
      }),
    ];
    return () => {
      for (const s of subs) void s.then((un) => un());
    };
  });

  // Keep the document chrome in sync with appearance prefs.
  $effect(() => {
    tweaks.t; // track
    tweaks.apply();
  });

  // Auto-lock: wipe the in-memory DEK after a period of inactivity, so an
  // unattended unlocked vault doesn't stay decrypted indefinitely. The delay is
  // user-configurable in Settings; `null` disables auto-lock entirely ("Never"),
  // keeping the vault open until manual lock / Quit.
  $effect(() => {
    if (vault.phase !== "unlocked") return;
    const idleMs = tweaks.t.autoLockMs; // reactive: re-arms when the setting changes
    if (idleMs == null) return; // "Never" — no idle timer at all
    let timer: ReturnType<typeof setTimeout>;
    const reset = () => {
      clearTimeout(timer);
      timer = setTimeout(() => void vault.lock().catch(() => {}), idleMs);
    };
    const events = ["mousemove", "mousedown", "keydown", "scroll", "touchstart"] as const;
    for (const e of events) window.addEventListener(e, reset, { passive: true });
    reset();
    return () => {
      clearTimeout(timer);
      for (const e of events) window.removeEventListener(e, reset);
    };
  });

  async function create(password: string, hint: string) {
    await vault.createVault(password, hint);
  }


</script>

{#if vault.loading}
  <div class="boot">
    <div class="boot-pulse"></div>
  </div>
{:else if vault.phase === "onboarding"}
  <Onboarding oncreate={create} />
{:else if vault.phase === "locked"}
  <Unlock hint={vault.hint} onunlock={(pw) => vault.unlock(pw)} />
{:else}
  <Vault />
{/if}

<ToastHost />
<ConfirmHost />

<style>
  .boot {
    height: 100vh;
    display: grid;
    place-items: center;
    background: var(--bg);
  }
  .boot-pulse {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.6;
    animation: boot-pulse 1.1s ease-in-out infinite;
  }
  @keyframes boot-pulse {
    0%,
    100% {
      transform: scale(0.7);
      opacity: 0.25;
    }
    50% {
      transform: scale(1);
      opacity: 0.7;
    }
  }
</style>
