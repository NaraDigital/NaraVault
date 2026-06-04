import { writeText, readText, clear } from "@tauri-apps/plugin-clipboard-manager";
import { open } from "@tauri-apps/plugin-shell";

/* ===================== PASSWORD STRENGTH ====================== */
export interface Strength {
  score: number; // 0..4 tier
  label: string;
  pct: number;
}

export function passwordStrength(pw: string): Strength {
  if (!pw) return { score: 0, label: "Empty", pct: 0 };
  let score = 0;
  if (pw.length >= 8) score++;
  if (pw.length >= 12) score++;
  if (pw.length >= 16) score++;
  if (/[A-Z]/.test(pw) && /[a-z]/.test(pw)) score++;
  if (/\d/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw)) score++;
  const labels = ["Very weak", "Weak", "Fair", "Good", "Strong", "Excellent", "Excellent"];
  const idx = Math.min(score, 6);
  const tier = score <= 1 ? 0 : score <= 2 ? 1 : score <= 3 ? 2 : score <= 4 ? 3 : 4;
  return { score: tier, label: labels[idx], pct: Math.min(100, Math.round((score / 6) * 100)) };
}

/* ===================== PASSWORD GENERATOR ===================== */
const GEN_SETS = {
  lower: "abcdefghijkmnpqrstuvwxyz",
  upper: "ABCDEFGHJKLMNPQRSTUVWXYZ",
  digits: "23456789",
  symbols: "!@#$%^&*()-_=+[]{};:,.?",
};

export interface GenOpts {
  length?: number;
  upper?: boolean;
  lower?: boolean;
  digits?: boolean;
  symbols?: boolean;
}

export function generatePassword(opts: GenOpts = {}): string {
  const { length = 20, upper = true, lower = true, digits = true, symbols = true } = opts;
  let pool = "";
  if (lower) pool += GEN_SETS.lower;
  if (upper) pool += GEN_SETS.upper;
  if (digits) pool += GEN_SETS.digits;
  if (symbols) pool += GEN_SETS.symbols;
  if (!pool) pool = GEN_SETS.lower;
  const arr = new Uint32Array(length);
  crypto.getRandomValues(arr);
  let out = "";
  for (let i = 0; i < length; i++) out += pool[arr[i] % pool.length];
  return out;
}

/* ====================== TOTP (RFC 6238) ====================== */
// Real time-based one-time codes: base32 secret -> HMAC-SHA1 -> 6 digits / 30s.
const B32_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

function base32Decode(input: string): Uint8Array | null {
  const clean = input.replace(/[=\s-]/g, "").toUpperCase();
  if (!clean) return null;
  let bits = 0;
  let value = 0;
  const out: number[] = [];
  for (const ch of clean) {
    const idx = B32_ALPHABET.indexOf(ch);
    if (idx === -1) return null;
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out.push((value >>> bits) & 0xff);
    }
  }
  return new Uint8Array(out);
}

/** Returns a 6-digit TOTP code, or null if the secret is not valid base32. */
export async function totpCode(secret: string, nowMs = Date.now()): Promise<string | null> {
  const key = base32Decode(secret);
  if (!key || key.length === 0) return null;

  const counter = Math.floor(nowMs / 1000 / 30);
  const msg = new Uint8Array(8);
  // 64-bit big-endian counter (JS bitwise is 32-bit, so split the halves)
  const high = Math.floor(counter / 0x100000000);
  const low = counter >>> 0;
  new DataView(msg.buffer).setUint32(0, high);
  new DataView(msg.buffer).setUint32(4, low);

  try {
    const cryptoKey = await crypto.subtle.importKey(
      "raw",
      key as BufferSource,
      { name: "HMAC", hash: "SHA-1" },
      false,
      ["sign"],
    );
    const sig = new Uint8Array(await crypto.subtle.sign("HMAC", cryptoKey, msg as BufferSource));
    const offset = sig[sig.length - 1] & 0x0f;
    const bin =
      ((sig[offset] & 0x7f) << 24) |
      ((sig[offset + 1] & 0xff) << 16) |
      ((sig[offset + 2] & 0xff) << 8) |
      (sig[offset + 3] & 0xff);
    return (bin % 1_000_000).toString().padStart(6, "0");
  } catch {
    return null;
  }
}

export function totpRemaining(nowMs = Date.now()): number {
  return 30 - (Math.floor(nowMs / 1000) % 30);
}

/* ===================== SEEDPHRASE SAMPLE ===================== */
const WORDLIST =
  "abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across action actor actress actual adapt add addict address adjust admit adult advance advice aerobic affair afford afraid again age agent agree ahead aim air airport aisle alarm album alcohol alert alien all alley allow almost alone alpha already also alter always amateur amazing among amount amused analyst anchor ancient anger angle angry animal ankle announce annual another answer antenna antique anxiety any apart apology appear apple approve april arch arctic area arena argue arm armed armor army around arrange arrest arrive arrow art artefact artist artwork ask aspect assault asset assist assume asthma athlete atom attack attend attitude attract auction audit august aunt author auto autumn average avocado avoid awake aware away awesome awful awkward axis".split(
    " ",
  );

export function randomSeed(count: number): string[] {
  const arr = new Uint32Array(count);
  crypto.getRandomValues(arr);
  return [...arr].map((n) => WORDLIST[n % WORDLIST.length]);
}

/* ===================== CLIPBOARD ===================== */

/** How long a copied secret is allowed to live on the clipboard, in ms. */
export const CLIPBOARD_CLEAR_MS = 20_000;

let clearTimer: ReturnType<typeof setTimeout> | null = null;
let lastCopied: string | null = null;

/** Plain copy with no auto-clear (for non-secret values like usernames/URLs). */
export async function copyToClipboard(text: string): Promise<void> {
  await writeText(text);
}

/**
 * Copy a secret and schedule the clipboard to be wiped after `delayMs`.
 * The wipe only fires if the clipboard still holds exactly what we wrote, so we
 * never clobber something the user copied afterwards. A new secret-copy cancels
 * the previous pending wipe.
 */
export async function copySecret(text: string, delayMs = CLIPBOARD_CLEAR_MS): Promise<void> {
  await writeText(text);
  lastCopied = text;
  if (clearTimer) clearTimeout(clearTimer);
  clearTimer = setTimeout(async () => {
    clearTimer = null;
    try {
      const current = await readText();
      if (current === lastCopied) await clear();
    } catch {
      /* clipboard unreadable (e.g. focus lost) — leave it */
    }
    lastCopied = null;
  }, delayMs);
}

/* ===================== EXTERNAL LINKS ===================== */
/**
 * Open a website in the OS default browser — never inside the Tauri webview,
 * so remote content can't run in the privileged app origin. Accepts a bare host
 * ("example.com") or full URL, and only ever opens `https://` (other schemes,
 * incl. javascript:/file:/http:, are rejected).
 */
export async function openExternal(raw: string): Promise<boolean> {
  const trimmed = raw.trim();
  if (!trimmed) return false;
  const candidate = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  let url: URL;
  try {
    url = new URL(candidate);
  } catch {
    return false;
  }
  if (url.protocol !== "https:") return false;
  await open(url.href);
  return true;
}
