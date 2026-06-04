<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen, emit } from "@tauri-apps/api/event";
  import Icon from "../components/Icon.svelte";
  import ItemGlyph from "../components/ItemGlyph.svelte";
  import { TYPE_LABEL } from "../lib/categories";
  import { api } from "../lib/api";
  import { isAppError, type ItemMeta } from "../lib/types";

  type Stage = "loading" | "locked" | "onboarding" | "list" | "auth";

  let stage = $state<Stage>("loading");
  let metas = $state<ItemMeta[]>([]);
  let query = $state("");
  let activeIndex = $state(0);
  let selected = $state<ItemMeta | null>(null);
  let pw = $state("");
  let err = $state(false);
  let busy = $state(false);

  // Separate state for the "unlock the whole vault" form shown when locked.
  let unlockPw = $state("");
  let unlockErr = $state(false);

  let searchEl = $state<HTMLInputElement | null>(null);
  let pwEl = $state<HTMLInputElement | null>(null);
  let unlockEl = $state<HTMLInputElement | null>(null);

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = q
      ? metas.filter((m) => (m.name + " " + m.sub).toLowerCase().includes(q))
      : metas;
    return [...list].sort(
      (a, b) => (b.fav ? 1 : 0) - (a.fav ? 1 : 0) || a.name.localeCompare(b.name),
    );
  });

  // Keep the highlight in range as the filtered list shrinks/grows.
  $effect(() => {
    if (activeIndex > filtered.length - 1) activeIndex = Math.max(0, filtered.length - 1);
  });

  async function load() {
    try {
      const s = await api.vaultStatus();
      if (s.phase === "onboarding") {
        stage = "onboarding";
        return;
      }
      if (s.phase === "locked") {
        stage = "locked";
        queueMicrotask(() => unlockEl?.focus());
        return;
      }
      metas = await api.listItemMeta();
      stage = "list";
      queueMicrotask(() => searchEl?.focus());
    } catch {
      stage = "locked";
      queueMicrotask(() => unlockEl?.focus());
    }
  }

  function resetView() {
    query = "";
    activeIndex = 0;
    selected = null;
    pw = "";
    err = false;
    busy = false;
    unlockPw = "";
    unlockErr = false;
    stage = "loading";
    void load();
  }

  // Unlock the whole vault straight from the launcher, without opening main.
  async function submitUnlock() {
    if (busy) return;
    busy = true;
    try {
      await api.unlock(unlockPw);
      unlockPw = "";
      unlockErr = false;
      metas = await api.listItemMeta();
      stage = "list";
      // Tell the main window to leave its lock screen and reload items.
      await emit("naravault://vault-changed");
      queueMicrotask(() => searchEl?.focus());
    } catch (e) {
      if (isAppError(e) && e.code === "INVALID_PASSWORD") {
        unlockErr = true;
        unlockPw = "";
      } else {
        throw e;
      }
    } finally {
      busy = false;
    }
  }

  // Initial load + refresh every time the window is shown via Alt+N.
  $effect(() => {
    void load();
    const un = listen("naravault://launcher-shown", () => resetView());
    return () => {
      void un.then((f) => f());
    };
  });

  async function hideLauncher() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* not in Tauri */
    }
  }

  function choose(item: ItemMeta) {
    selected = item;
    pw = "";
    err = false;
    stage = "auth";
    queueMicrotask(() => pwEl?.focus());
  }

  async function submitAuth() {
    if (busy || !selected) return;
    busy = true;
    try {
      // Single fail-closed call: Rust verifies the master password and only then
      // focuses main + emits open-item. Wrong password → InvalidPassword, no reveal.
      await api.launcherOpenItem(selected.id, pw);
      query = "";
      selected = null;
      pw = "";
      stage = "list";
    } catch (e) {
      if (isAppError(e) && e.code === "INVALID_PASSWORD") {
        err = true;
        pw = "";
      } else {
        throw e;
      }
    } finally {
      busy = false;
    }
  }

  function backToList() {
    selected = null;
    pw = "";
    err = false;
    stage = "list";
    queueMicrotask(() => searchEl?.focus());
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (stage === "auth") backToList();
      else void hideLauncher();
      return;
    }
    if (stage === "list") {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        activeIndex = Math.min(activeIndex + 1, filtered.length - 1);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        activeIndex = Math.max(activeIndex - 1, 0);
      } else if (e.key === "Enter") {
        e.preventDefault();
        const it = filtered[activeIndex];
        if (it) choose(it);
      }
    } else if (stage === "auth" && e.key === "Enter") {
      e.preventDefault();
      void submitAuth();
    } else if (stage === "locked" && e.key === "Enter") {
      e.preventDefault();
      void submitUnlock();
    }
  }
</script>

<svelte:window on:keydown={onKeydown} />

<div class="launcher" role="dialog" aria-label="Quick launch">
  {#if stage === "auth" && selected}
    <div class="launcher-auth">
      <button class="launcher-back" onclick={backToList} title="Back">
        <Icon name="back" size={18} />
      </button>
      <div class="launcher-auth-head">
        <ItemGlyph
          type={selected.type}
          size={40}
          letter={selected.type === "login" ? selected.name[0] : null}
        />
        <div>
          <div class="launcher-auth-name">{selected.name}</div>
          <div class="launcher-auth-sub mono">{TYPE_LABEL[selected.type]} · {selected.sub}</div>
        </div>
      </div>
      <div class="launcher-pw">
        <Icon name="lock" size={16} />
        <input
          bind:this={pwEl}
          class="launcher-pw-input mono"
          type="password"
          placeholder="Master password to open"
          bind:value={pw}
          oninput={() => (err = false)}
          disabled={busy}
        />
        <button class="launcher-go" onclick={submitAuth} disabled={busy} title="Open">
          {#if busy}
            <span class="spinner spinner-sm"></span>
          {:else}
            <Icon name="unlock" size={16} />
          {/if}
        </button>
      </div>
      {#if err}
        <div class="launcher-err"><Icon name="close" size={13} /> Incorrect password. Try again.</div>
      {/if}
    </div>
  {:else if stage === "locked"}
    <div class="launcher-auth">
      <div class="launcher-auth-head">
        <div class="launcher-lock-badge"><Icon name="lock" size={20} /></div>
        <div>
          <div class="launcher-auth-name">Unlock NaraVault</div>
          <div class="launcher-auth-sub mono">Enter your master password</div>
        </div>
      </div>
      <div class="launcher-pw">
        <Icon name="lock" size={16} />
        <input
          bind:this={unlockEl}
          class="launcher-pw-input mono"
          type="password"
          placeholder="Master password"
          bind:value={unlockPw}
          oninput={() => (unlockErr = false)}
          disabled={busy}
        />
        <button class="launcher-go" onclick={submitUnlock} disabled={busy} title="Unlock">
          {#if busy}
            <span class="spinner spinner-sm"></span>
          {:else}
            <Icon name="unlock" size={16} />
          {/if}
        </button>
      </div>
      {#if unlockErr}
        <div class="launcher-err"><Icon name="close" size={13} /> Incorrect password. Try again.</div>
      {/if}
    </div>
  {:else if stage === "onboarding"}
    <div class="launcher-msg" style="height:100vh">
      <Icon name="shield" size={22} />
      <span>Vault not set up yet. Open NaraVault to create it first.</span>
    </div>
  {:else}
    <div class="launcher-search">
      <Icon name="search" size={18} />
      <input
        bind:this={searchEl}
        class="launcher-search-input"
        placeholder="Search your vault…"
        bind:value={query}
        disabled={stage !== "list"}
      />
      <kbd class="launcher-kbd mono">Esc</kbd>
    </div>

    <div class="launcher-body">
      {#if stage === "loading"}
        <div class="launcher-msg"><span class="spinner"></span></div>
      {:else if filtered.length === 0}
        <div class="launcher-msg">
          <Icon name="search" size={22} />
          <span>{query ? `Nothing matches “${query}”.` : "Your vault is empty."}</span>
        </div>
      {:else}
        <div class="launcher-list">
          {#each filtered as m, i (m.id)}
            <button
              class="launcher-row{i === activeIndex ? ' is-active' : ''}"
              onclick={() => choose(m)}
              onmousemove={() => (activeIndex = i)}
            >
              <ItemGlyph type={m.type} size={34} letter={m.type === "login" ? m.name[0] : null} />
              <div class="launcher-row-text">
                <span class="launcher-row-name">{m.name}</span>
                <span class="launcher-row-sub mono">{TYPE_LABEL[m.type]} · {m.sub}</span>
              </div>
              {#if m.fav}<Icon name="star" size={13} style="color:var(--ink);fill:var(--ink)" />{/if}
              <Icon name="back" size={15} style="transform:rotate(180deg);opacity:.4" />
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>
