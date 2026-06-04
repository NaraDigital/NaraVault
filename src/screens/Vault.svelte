<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import Button from "../components/Button.svelte";
  import BrandMark from "../components/BrandMark.svelte";
  import ItemGlyph from "../components/ItemGlyph.svelte";
  import MasterPasswordGate from "../components/MasterPasswordGate.svelte";
  import DetailView from "./DetailView.svelte";
  import ItemForm from "./ItemForm.svelte";
  import Generator from "./Generator.svelte";
  import Settings from "./Settings.svelte";
  import ChangeMaster from "./ChangeMaster.svelte";
  import { CATEGORIES } from "../lib/categories";
  import { vault } from "../lib/vault.svelte";
  import { confirm } from "../lib/confirm.svelte";
  import { launcherBridge } from "../lib/launcher.svelte";
  import type { Item } from "../lib/types";

  type View = "detail" | "form" | "generator" | "settings";

  let category = $state<string>("all");
  let query = $state("");
  let selectedId = $state<string | null>(null);
  let view = $state<View>("detail");
  let editing = $state<Item | null>(null);
  let selectionMode = $state(false);
  let selectedIds = $state(new Set<string>());
  let revealed = $state<Record<string, boolean>>({});
  let gate = $state<string | null>(null);
  // Why the gate is open: reveal a field, unlock the edit form, or authorise a delete.
  let gateIntent = $state<"reveal" | "edit" | "delete">("reveal");
  let changePw = $state(false);

  let items = $derived(vault.items);

  let counts = $derived({
    all: items.length,
    login: items.filter((i) => i.type === "login").length,
    card: items.filter((i) => i.type === "card").length,
    seed: items.filter((i) => i.type === "seed").length,
    note: items.filter((i) => i.type === "note").length,
  } as Record<string, number>);

  let filtered = $derived.by(() => {
    let list = items;
    if (category !== "all") list = list.filter((i) => i.type === category);
    if (query.trim()) {
      const q = query.toLowerCase();
      list = list.filter((i) =>
        (i.name + " " + i.sub + " " + (i.data.username ?? "") + " " + (i.data.url ?? ""))
          .toString()
          .toLowerCase()
          .includes(q),
      );
    }
    return [...list].sort(
      (a, b) => (b.fav ? 1 : 0) - (a.fav ? 1 : 0) || a.name.localeCompare(b.name),
    );
  });

  let selected = $derived(items.find((i) => i.id === selectedId) ?? null);
  let gateName = $derived(gate ? (items.find((i) => i.id === gate)?.name ?? "") : "");
  let activeCategoryLabel = $derived(CATEGORIES.find((c) => c.id === category)?.label ?? "");
  let allFilteredSelected = $derived(
    filtered.length > 0 && filtered.every((i) => selectedIds.has(i.id))
  );

  function openNew() {
    editing = null;
    view = "form";
  }
  function startEdit(item: Item) {
    editing = item;
    view = "form";
  }
  function openEdit(item: Item) {
    // Logins are not protected behind the gate, so edit them directly.
    // Everything else requires the master password before entering edit mode.
    if (item.type === "login") {
      startEdit(item);
    } else {
      gateIntent = "edit";
      gate = item.id;
    }
  }
  function requestReveal(id: string) {
    gateIntent = "reveal";
    gate = id;
  }
  function requestDelete(id: string) {
    gateIntent = "delete";
    gate = id;
  }
  function selectItem(id: string) {
    selectedId = id;
    view = "detail";
  }

  function enterSelectionMode() {
    selectionMode = true;
    selectedIds = new Set();
  }

  function exitSelectionMode() {
    selectionMode = false;
    selectedIds = new Set();
  }

  function toggleSelect(id: string) {
    const next = new Set(selectedIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedIds = next;
  }

  function toggleSelectAll() {
    if (allFilteredSelected) {
      selectedIds = new Set();
    } else {
      selectedIds = new Set(filtered.map((i) => i.id));
    }
  }

  async function deleteSelected() {
    const ids = [...selectedIds];
    if (ids.length === 0) return;
    const ok = await confirm.ask({
      title: `Delete ${ids.length} item${ids.length > 1 ? "s" : ""}?`,
      message: `This will permanently delete ${ids.length} selected item${ids.length > 1 ? "s" : ""}. This cannot be undone.`,
      confirmLabel: "Delete",
      danger: true,
    });
    if (!ok) return;
    // Clear selection state before removing to avoid stale refs
    const toDelete = ids;
    exitSelectionMode();
    if (selectedId && toDelete.includes(selectedId)) selectedId = null;
    await vault.removeMany(toDelete);
  }

  async function saveItem(item: Item) {
    await vault.save(item);
    selectedId = item.id;
    view = "detail";
    editing = null;
  }

  async function deleteItem(id: string) {
    if (await confirm.ask({ title: "Delete item", message: "Delete this item permanently?", confirmLabel: "Delete", danger: true })) {
      await vault.remove(id);
      selectedId = null;
    }
  }

  async function toggleFav(item: Item) {
    await vault.save({ ...item, fav: !item.fav });
  }

  async function lock() {
    revealed = {};
    selectedId = null;
    view = "detail";
    await vault.lock();
  }

  async function resetVault() {
    if (
      await confirm.ask({
        title: "Reset vault",
        message: "Permanently delete the entire vault? This cannot be undone.",
        confirmLabel: "Delete everything",
        danger: true,
      })
    ) {
      revealed = {};
      await vault.reset();
    }
  }

  // Quick launcher (Alt+N) hands off here via the bridge store. It already
  // re-authenticated server-side, so we open + reveal the chosen item directly.
  // Reacts to both the pending id and the items list, so it works whether Vault
  // was already mounted or only mounts after the launcher unlocks the vault.
  $effect(() => {
    const id = launcherBridge.pendingOpenId;
    if (!id) return;
    if (!items.some((i) => i.id === id)) return;
    selectedId = id;
    view = "detail";
    revealed = { ...revealed, [id]: true };
    launcherBridge.pendingOpenId = null;
  });

  async function verifyReveal(pw: string): Promise<boolean> {
    const ok = await vault.verifyMaster(pw);
    if (ok && gate) {
      const target = items.find((i) => i.id === gate) ?? null;
      const id = gate;
      gate = null;
      if (gateIntent === "edit" && target) {
        startEdit(target);
      } else if (gateIntent === "delete") {
        await deleteItem(id);
      } else {
        revealed = { ...revealed, [id]: true };
      }
    }
    return ok;
  }
</script>

<div class="app">
  <!-- sidebar -->
  <aside class="sidebar">
    <div class="sb-brand"><BrandMark size={28} /></div>
    <button class="new-btn" onclick={openNew}>
      <Icon name="plus" size={16} /><span>New item</span>
    </button>
    <nav class="sb-nav">
      {#each CATEGORIES as c (c.id)}
        <button
          class="sb-item{category === c.id && view !== 'generator' && view !== 'settings'
            ? ' is-active'
            : ''}"
          onclick={() => {
            category = c.id;
            view = "detail";
          }}
        >
          <Icon name={c.icon} size={17} /><span>{c.label}</span>
          <span class="sb-count mono">{counts[c.id]}</span>
        </button>
      {/each}
    </nav>
    <div class="sb-spacer"></div>
    <nav class="sb-nav">
      <button class="sb-item{view === 'generator' ? ' is-active' : ''}" onclick={() => (view = "generator")}>
        <Icon name="key" size={17} /><span>Generator</span>
      </button>
      <button class="sb-item{view === 'settings' ? ' is-active' : ''}" onclick={() => (view = "settings")}>
        <Icon name="settings" size={17} /><span>Settings</span>
      </button>
    </nav>
    <button class="lock-btn" onclick={lock}>
      <Icon name="lock" size={15} /><span>Lock vault</span>
    </button>
  </aside>

  <!-- list column -->
  {#if view === "detail" || view === "form"}
    <section class="list-col">
      <div class="list-head">
        <div class="search">
          <Icon name="search" size={16} />
          <input class="search-input" placeholder="Search vault…" bind:value={query} />
          {#if query}
            <button class="search-clear" onclick={() => (query = "")}>
              <Icon name="close" size={14} />
            </button>
          {/if}
        </div>
        <IconButton name="plus" size={20} onclick={openNew} title="New item" />
        {#if selectionMode}
          <button class="sel-cancel-btn" onclick={exitSelectionMode}>Cancel</button>
        {:else}
          <IconButton name="check" size={20} onclick={enterSelectionMode} title="Select items" />
        {/if}
      </div>
      <div class="list-meta mono">{activeCategoryLabel} · {filtered.length}</div>
      <div class="list-scroll">
        {#if filtered.length === 0}
          <div class="empty">
            <div class="empty-icon"><Icon name={query ? "search" : "all"} size={26} /></div>
            {#if query}
              <h3>No matches</h3>
              <p>Nothing matches “{query}”.</p>
            {:else}
              <h3>Nothing here yet</h3>
              <p>This category is empty.</p>
              <Button variant="ghost" icon="plus" onclick={openNew}>Add item</Button>
            {/if}
          </div>
        {:else}
          {#each filtered as i (i.id)}
            <button
              class="list-item{!selectionMode && selectedId === i.id && view === 'detail' ? ' is-active' : ''}{selectionMode && selectedIds.has(i.id) ? ' is-selected' : ''}"
              onclick={() => selectionMode ? toggleSelect(i.id) : selectItem(i.id)}
            >
              {#if selectionMode}
                <div class="sel-checkbox{selectedIds.has(i.id) ? ' sel-checked' : ''}">
                  {#if selectedIds.has(i.id)}<Icon name="check" size={12} />{/if}
                </div>
              {:else}
                <ItemGlyph type={i.type} size={36} letter={i.type === "login" ? i.name[0] : null} />
              {/if}
              <div class="li-text">
                <span class="li-name">{i.name}</span>
                <span class="li-sub mono">{i.sub}</span>
              </div>
              {#if !selectionMode && i.fav}
                <Icon name="star" size={14} style="color:var(--ink);fill:var(--ink)" />
              {/if}
            </button>
          {/each}
        {/if}
      </div>
      {#if selectionMode}
        <div class="sel-bar">
          <button class="sel-all-btn" onclick={toggleSelectAll}>
            {allFilteredSelected ? "Deselect all" : "Select all"}
          </button>
          <span class="sel-count mono">{selectedIds.size} selected</span>
          <button
            class="sel-delete-btn"
            disabled={selectedIds.size === 0}
            onclick={deleteSelected}
          >
            <Icon name="trash" size={14} />
            Delete
          </button>
        </div>
      {/if}
    </section>
  {/if}

  <!-- main pane -->
  <main class="main-col">
    {#if view === "form"}
      <ItemForm
        initial={editing}
        onsave={saveItem}
        oncancel={() => {
          view = "detail";
          editing = null;
        }}
      />
    {:else if view === "generator"}
      <div class="pane-pad">
        <div class="pane-title">
          <h2>Password generator</h2>
          <span class="detail-sub mono">Strong, random, ready to copy</span>
        </div>
        <div class="gen-wrap"><Generator /></div>
      </div>
    {:else if view === "settings"}
      <div class="pane-pad">
        <Settings
          count={items.length}
          onlock={lock}
          onreset={resetVault}
          onchangeMaster={() => (changePw = true)}
        />
      </div>
    {:else if selected}
      <DetailView
        item={selected}
        unlocked={selected.type === "login" ? true : !!revealed[selected.id]}
        onUnlock={() => requestReveal(selected!.id)}
        onEdit={() => openEdit(selected!)}
        onDelete={() => requestDelete(selected!.id)}
        onToggleFav={() => toggleFav(selected!)}
      />
    {:else}
      <div class="no-sel">
        <div class="no-sel-badge"><Icon name="shield" size={30} /></div>
        <h2>{items.length} secrets, one key</h2>
        <p>Select an item to view its details, or create something new.</p>
        <Button variant="primary" icon="plus" onclick={openNew}>New item</Button>
      </div>
    {/if}
  </main>

  <MasterPasswordGate
    open={!!gate}
    itemName={gateName}
    action={gateIntent}
    onclose={() => (gate = null)}
    onverify={verifyReveal}
  />
  <ChangeMaster open={changePw} onclose={() => (changePw = false)} />
</div>
