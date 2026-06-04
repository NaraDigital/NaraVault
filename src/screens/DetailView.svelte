<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import CopyButton from "../components/CopyButton.svelte";
  import ItemGlyph from "../components/ItemGlyph.svelte";
  import SecretRow from "../components/SecretRow.svelte";
  import TotpField from "./detail/TotpField.svelte";
  import { TYPE_LABEL } from "../lib/categories";
  import { openExternal } from "../lib/util";
  import type { Item } from "../lib/types";

  interface Props {
    item: Item;
    unlocked: boolean;
    onUnlock: () => void;
    onEdit: () => void;
    onDelete: () => Promise<void> | void;
    onToggleFav: () => void;
  }
  let { item, unlocked, onUnlock, onEdit, onDelete, onToggleFav }: Props = $props();

  let deleting = $state(false);
  async function handleDelete() {
    if (deleting) return;
    deleting = true;
    try {
      await onDelete();
    } finally {
      deleting = false;
    }
  }

  // Per-field reveal toggles; reset when the selected item changes.
  let showPw = $state(false);
  let showNum = $state(false);
  let showCvv = $state(false);
  $effect(() => {
    void item.id;
    showPw = false;
    showNum = false;
    showCvv = false;
  });

  const s = (v: unknown): string => (v == null ? "" : String(v));
  let d = $derived(item.data as Record<string, unknown>);
  let words = $derived(Array.isArray(d.words) ? (d.words as string[]) : []);
  let maskedNum = $derived("•••• •••• •••• " + s(d.number).slice(-4));
</script>

<div class="detail">
  <div class="detail-head">
    <ItemGlyph type={item.type} size={52} letter={item.type === "login" ? item.name[0] : null} />
    <div class="detail-title">
      <h2>{item.name}</h2>
      <span class="detail-sub mono">{TYPE_LABEL[item.type]} · {item.sub}</span>
    </div>
    <div class="detail-actions">
      <IconButton name="star" size={18} active={item.fav} onclick={onToggleFav} title="Favourite" />
      <IconButton name="edit" size={18} onclick={onEdit} title="Edit" disabled={deleting} />
      <IconButton name="trash" size={18} danger onclick={handleDelete} title="Delete" loading={deleting} />
    </div>
  </div>

  {#if !unlocked && item.type !== "login"}
    <div class="locked-banner">
      <Icon name="shield" size={15} />
      <span>Protected. Master password required to view contents.</span>
    </div>
  {/if}

  <div class="detail-body">
    {#if item.type === "login"}
      <SecretRow label="Username / email" value={s(d.username)} revealed copyable mono={false} />
      <SecretRow
        label="Password"
        value={s(d.password)}
        revealed={unlocked && showPw}
        onToggle={() => (unlocked ? (showPw = !showPw) : onUnlock())}
        copyable={unlocked}
        secret
      />
      {#if s(d.totp)}
        <TotpField secret={s(d.totp)} {unlocked} {onUnlock} />
      {/if}
      {#if s(d.url)}
        <div class="srow">
          <div class="srow-head">
            <span class="srow-label">Website</span>
            <div class="srow-tools"><CopyButton text={s(d.url)} label="URL" /></div>
          </div>
          <button type="button" class="srow-value link" style="padding:0;text-align:left" onclick={() => openExternal(s(d.url))}>
            {s(d.url)} <Icon name="external" size={13} />
          </button>
        </div>
      {/if}
      {#if s(d.notes)}
        <div class="srow">
          <div class="srow-head"><span class="srow-label">Notes</span></div>
          <pre class="note-body mono dim">{s(d.notes)}</pre>
        </div>
      {/if}
    {:else if item.type === "card"}
      <div class="cc-preview" data-brand={s(d.brand)}>
        <div class="cc-top">
          <span class="cc-brand mono">{s(d.brand)}</span><Icon name="card" size={20} />
        </div>
        <div class="cc-number mono">{unlocked && showNum ? s(d.number) : maskedNum}</div>
        <div class="cc-bottom">
          <div>
            <span class="cc-cap">Holder</span><div class="cc-val mono">{s(d.holder)}</div>
          </div>
          <div>
            <span class="cc-cap">Expires</span><div class="cc-val mono">{s(d.expiry)}</div>
          </div>
        </div>
      </div>
      <SecretRow
        label="Card number"
        value={s(d.number)}
        revealed={unlocked && showNum}
        onToggle={() => (unlocked ? (showNum = !showNum) : onUnlock())}
        copyable={unlocked}
        masked={maskedNum}
        secret
      />
      <div class="srow-grid">
        <SecretRow label="Expiry" value={s(d.expiry)} revealed copyable />
        <SecretRow
          label="CVV"
          value={s(d.cvv)}
          revealed={unlocked && showCvv}
          onToggle={() => (unlocked ? (showCvv = !showCvv) : onUnlock())}
          copyable={unlocked}
          mask="•••"
          secret
        />
      </div>
      <SecretRow label="Cardholder" value={s(d.holder)} revealed copyable mono={false} />
      {#if s(d.notes)}
        <div class="srow">
          <div class="srow-head"><span class="srow-label">Notes</span></div>
          <pre class="note-body mono dim">{s(d.notes)}</pre>
        </div>
      {/if}
    {:else if item.type === "seed"}
      <div class="seed-meta">
        <span class="chip mono">{words.length} words</span>
        {#if s(d.wallet)}<span class="chip">{s(d.wallet)}</span>{/if}
        <div class="seed-tools">
          {#if unlocked}<CopyButton text={words.join(" ")} label="Phrase" secret />{/if}
        </div>
      </div>
      <div class="seed-grid{!unlocked ? ' is-locked' : ''}">
        {#each words as w, i (i)}
          <div class="seed-word">
            <span class="seed-idx mono">{String(i + 1).padStart(2, "0")}</span>
            <span class="seed-text mono">{unlocked ? w : "••••••"}</span>
          </div>
        {/each}
      </div>
      {#if !unlocked}
        <button class="seed-reveal" onclick={onUnlock}>
          <Icon name="lock" size={15} /> Enter master password to reveal recovery phrase
        </button>
      {/if}
      {#if s(d.notes)}
        <div class="srow">
          <div class="srow-head"><span class="srow-label">Notes</span></div>
          <pre class="note-body mono dim">{s(d.notes)}</pre>
        </div>
      {/if}
    {:else if item.type === "note"}
      <div class="srow">
        <div class="srow-head">
          <span class="srow-label">Note contents</span>
          <div class="srow-tools">
            {#if !unlocked}
              <IconButton name="eye" size={16} onclick={onUnlock} title="Reveal" />
            {:else}
              <CopyButton text={s(d.content)} label="Note" secret />
            {/if}
          </div>
        </div>
        {#if unlocked}
          <pre class="note-body mono">{s(d.content)}</pre>
        {:else}
          <button class="note-locked" onclick={onUnlock}>
            <Icon name="lock" size={16} />
            <span>Enter master password to read this note</span>
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>
