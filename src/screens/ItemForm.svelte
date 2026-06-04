<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import Button from "../components/Button.svelte";
  import Field from "../components/Field.svelte";
  import Input from "../components/Input.svelte";
  import Textarea from "../components/Textarea.svelte";
  import StrengthMeter from "../components/StrengthMeter.svelte";
  import { TYPE_LABEL } from "../lib/categories";
  import { generatePassword, randomSeed } from "../lib/util";
  import type { Item, ItemType, ItemData } from "../lib/types";

  interface Props {
    initial: Item | null;
    onsave: (item: Item) => Promise<void> | void;
    oncancel: () => void;
  }
  let { initial, onsave, oncancel }: Props = $props();

  let busy = $state(false);

  const SEED_COUNTS = [12, 18, 20, 24];

  function blankData(t: ItemType): ItemData {
    if (t === "login") return { username: "", password: "", url: "", totp: "", notes: "" };
    if (t === "card")
      return { holder: "", number: "", expiry: "", cvv: "", brand: "", notes: "" };
    if (t === "seed") return { wallet: "", words: Array(12).fill(""), notes: "" };
    return { content: "", notes: "" };
  }

  function uid(): string {
    return "i_" + crypto.getRandomValues(new Uint32Array(2))
      .reduce((a, n) => a + n.toString(36), "")
      .slice(0, 8);
  }

  // The form is seeded once from `initial`; later prop changes don't re-seed
  // (the parent always remounts with a fresh key), so reading it here is intended.
  // svelte-ignore state_referenced_locally
  let type = $state<ItemType>(initial ? initial.type : "login");
  // svelte-ignore state_referenced_locally
  let name = $state(initial ? initial.name : "");
  // svelte-ignore state_referenced_locally
  let d = $state<Record<string, any>>(initial ? { ...initial.data } : blankData("login"));
  let showPw = $state(false);

  const s = (v: unknown): string => (v == null ? "" : String(v));

  // "0328" -> "03/28"; keeps partial input usable while typing.
  function formatExpiry(raw: string): string {
    const digits = raw.replace(/\D/g, "").slice(0, 4);
    if (digits.length >= 3) return digits.slice(0, 2) + "/" + digits.slice(2);
    return digits;
  }

  // Detect card brand from leading digits (IIN/BIN ranges). Returns null
  // when no rule matches so a manual brand override is never clobbered.
  function detectBrand(num: string): string | null {
    const n = num.replace(/\D/g, "");
    if (n.length < 2) return null;
    if (n[0] === "4") return "VISA";
    if (n[0] === "3" && (n[1] === "4" || n[1] === "7")) return "AMEX";
    const p2 = Number(n.slice(0, 2));
    if (p2 >= 51 && p2 <= 55) return "MASTERCARD";
    if (n.length >= 4) {
      const p4 = Number(n.slice(0, 4));
      if (p4 >= 2221 && p4 <= 2720) return "MASTERCARD";
      if (p4 === 6011) return "DISCOVER";
    }
    if (p2 === 65) return "DISCOVER";
    return null;
  }

  // Group digits 4-4-4-4 for readability (max 16). Spaces are stripped
  // again before brand detection / storage compare.
  function formatCardNumber(raw: string): string {
    return raw
      .replace(/\D/g, "")
      .slice(0, 16)
      .replace(/(.{4})/g, "$1 ")
      .trim();
  }

  function setCardNumber(v: string) {
    const formatted = formatCardNumber(v);
    const brand = detectBrand(formatted);
    d = { ...d, number: formatted, ...(brand ? { brand } : {}) };
  }

  function changeType(t: ItemType) {
    type = t;
    if (!initial) d = blankData(t);
  }

  function setSeedCount(n: number) {
    const cur: string[] = Array.isArray(d.words) ? d.words : [];
    const next = cur.slice(0, n);
    while (next.length < n) next.push("");
    d = { ...d, words: next };
  }

  function defaultName(): string {
    if (type === "login") return s(d.url) || s(d.username) || "Login";
    if (type === "card")
      return (s(d.brand) || "Card") + (d.number ? " •• " + s(d.number).replace(/\s/g, "").slice(-4) : "");
    if (type === "seed") return (s(d.wallet) || "Wallet") + " phrase";
    return "Secure note";
  }

  function subFor(): string {
    if (type === "login") return s(d.username) || s(d.url);
    if (type === "card") return s(d.holder);
    if (type === "seed") return `${(d.words?.length ?? 0)} words`;
    return "Secure note";
  }

  async function submit() {
    if (busy) return;
    busy = true;
    try {
      await onsave({
        id: initial ? initial.id : uid(),
        type,
        name: name || defaultName(),
        sub: subFor(),
        fav: initial ? initial.fav : false,
        data: d,
      });
    } finally {
      busy = false;
    }
  }

  let seedWords = $derived(Array.isArray(d.words) ? (d.words as string[]) : []);
</script>

<div class="form">
  <div class="form-head">
    <IconButton name="back" size={20} onclick={oncancel} title="Back" />
    <h2>{initial ? "Edit item" : "New item"}</h2>
    <div style="flex:1"></div>
    <Button variant="ghost" onclick={oncancel} disabled={busy}>Cancel</Button>
    <Button variant="primary" icon="check" loading={busy} onclick={submit}
      >{busy ? "Saving…" : "Save"}</Button
    >
  </div>

  {#if !initial}
    <div class="type-picker">
      {#each ["login", "card", "seed", "note"] as t (t)}
        <button
          class="type-opt{type === t ? ' is-active' : ''}"
          onclick={() => changeType(t as ItemType)}
        >
          <Icon name={t} size={20} />
          <span>{TYPE_LABEL[t as ItemType]}</span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="form-body">
    <Field label="Name">
      <Input bind:value={name} placeholder={defaultName() || "Item name"} autofocus />
    </Field>

    {#if type === "login"}
      <Field label="Username / email">
        <Input value={s(d.username)} oninput={(v) => (d = { ...d, username: v })} placeholder="you@example.com" />
      </Field>
      <Field label="Password">
        <Input
          value={s(d.password)}
          oninput={(v) => (d = { ...d, password: v })}
          type={showPw ? "text" : "password"}
          mono
        >
          {#snippet right()}
            <IconButton
              name={showPw ? "eyeOff" : "eye"}
              size={15}
              onclick={() => (showPw = !showPw)}
              title="Show"
            />
            <IconButton
              name="refresh"
              size={15}
              title="Generate"
              onclick={() => (d = { ...d, password: generatePassword({ length: 20 }) })}
            />
          {/snippet}
        </Input>
      </Field>
      <StrengthMeter password={s(d.password)} />
      <Field label="Website">
        <Input value={s(d.url)} oninput={(v) => (d = { ...d, url: v })} placeholder="example.com" mono />
      </Field>
      <Field label="Authenticator secret (TOTP)" hint="Optional — base32 secret for one-time codes">
        <Input value={s(d.totp)} oninput={(v) => (d = { ...d, totp: v })} placeholder="JBSWY3DPEHPK3PXP" mono />
      </Field>
    {:else if type === "card"}
      <Field label="Cardholder name">
        <Input value={s(d.holder)} oninput={(v) => (d = { ...d, holder: v.toUpperCase() })} placeholder="NAME ON CARD" mono />
      </Field>
      <Field label="Card number">
        <Input value={s(d.number)} oninput={(v) => setCardNumber(v)} placeholder="0000 0000 0000 0000" mono />
      </Field>
      <div class="srow-grid">
        <Field label="Expiry">
          <Input value={s(d.expiry)} oninput={(v) => (d = { ...d, expiry: formatExpiry(v) })} placeholder="MM/YY" mono />
        </Field>
        <Field label="CVV">
          <Input value={s(d.cvv)} oninput={(v) => (d = { ...d, cvv: v })} placeholder="123" mono />
        </Field>
      </div>
      <Field label="Brand" hint="Detected automatically from the card number">
        <div class="brand-detect mono">
          {#if s(d.brand)}
            <span class="brand-chip">{s(d.brand)}</span>
          {:else}
            <span class="dim">Type the card number to detect…</span>
          {/if}
        </div>
      </Field>
    {:else if type === "seed"}
      <Field label="Wallet">
        <Input value={s(d.wallet)} oninput={(v) => (d = { ...d, wallet: v })} placeholder="MetaMask, Ledger…" />
      </Field>
      <Field label="Phrase length">
        <div class="seg">
          {#each SEED_COUNTS as n (n)}
            <button
              class="seg-opt mono{seedWords.length === n ? ' is-active' : ''}"
              onclick={() => setSeedCount(n)}>{n}</button
            >
          {/each}
        </div>
      </Field>
      <div class="seed-input-grid">
        {#each seedWords as w, i (i)}
          <div class="seed-input">
            <span class="seed-idx mono">{String(i + 1).padStart(2, "0")}</span>
            <input
              class="input input-mono seed-input-f"
              value={w}
              oninput={(e) => {
                const nx = [...seedWords];
                nx[i] = (e.target as HTMLInputElement).value.trim();
                d = { ...d, words: nx };
              }}
            />
          </div>
        {/each}
      </div>
      {#if seedWords.length > 0}
        <Button
          variant="ghost"
          size="sm"
          icon="refresh"
          onclick={() => (d = { ...d, words: randomSeed(seedWords.length) })}
          >Fill with sample words</Button
        >
      {/if}
    {:else if type === "note"}
      <Field label="Note">
        <Textarea
          value={s(d.content)}
          oninput={(v) => (d = { ...d, content: v })}
          rows={8}
          mono
          placeholder="Type anything you want to keep secret…"
        />
      </Field>
    {/if}
  </div>
</div>
