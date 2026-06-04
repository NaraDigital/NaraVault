import type { Item, ItemType } from "./types";

// ─── Bitwarden JSON shape (unencrypted export) ───────────────────────────────

interface BwUri {
  uri: string | null;
  match: unknown;
}

interface BwLogin {
  username?: string | null;
  password?: string | null;
  totp?: string | null;
  uris?: BwUri[] | null;
}

interface BwCard {
  cardholderName?: string | null;
  brand?: string | null;
  number?: string | null;
  expMonth?: string | null;
  expYear?: string | null;
  code?: string | null;
}

interface BwSecureNote {
  type?: number;
}

interface BwItem {
  id?: string;
  type?: number;
  name?: string | null;
  notes?: string | null;
  favorite?: boolean | null;
  folderId?: string | null;
  login?: BwLogin | null;
  card?: BwCard | null;
  secureNote?: BwSecureNote | null;
}

interface BwExport {
  encrypted?: boolean;
  items?: BwItem[];
}

// ─── Bitwarden item type constants ───────────────────────────────────────────

const BW_TYPE_LOGIN = 1;
const BW_TYPE_NOTE = 2;
const BW_TYPE_CARD = 3;
const BW_TYPE_IDENTITY = 4; // not supported in NaraVault — skip

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Generate a NaraVault-format id: "i_" + 8 random hex chars. */
function newId(): string {
  const bytes = new Uint8Array(4);
  crypto.getRandomValues(bytes);
  return "i_" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Extract the Base32 secret from an otpauth:// URI.
 * Returns the raw secret if already a plain Base32 string.
 * Returns null if the input is falsy.
 */
function extractTotpSecret(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const trimmed = raw.trim();
  if (trimmed.toLowerCase().startsWith("otpauth://")) {
    try {
      const url = new URL(trimmed);
      const secret = url.searchParams.get("secret");
      return secret ? secret.toUpperCase() : null;
    } catch {
      return null;
    }
  }
  // Already a plain Base32 secret
  return trimmed.toUpperCase();
}

/** Normalise a card brand to uppercase (Bitwarden uses title-case). */
function normaliseBrand(raw: string | null | undefined): string {
  if (!raw) return "";
  return raw.trim().toUpperCase();
}

/**
 * Combine expiry month + year → "MM/YY".
 * Month is zero-padded to 2 digits; year is truncated to last 2 digits.
 */
function buildExpiry(
  expMonth: string | null | undefined,
  expYear: string | null | undefined,
): string {
  const month = (expMonth ?? "").trim().padStart(2, "0");
  const year = (expYear ?? "").trim().slice(-2);
  if (!month || !year) return "";
  return `${month}/${year}`;
}

/** Derive the `sub` subtitle for a NaraVault item. */
function buildSub(type: ItemType, data: Record<string, unknown>): string {
  switch (type) {
    case "login":
      return (data.username as string) || (data.url as string) || "";
    case "note":
      return "Secure note";
    case "card":
      return (data.holder as string) || "";
    default:
      return "";
  }
}

// ─── Per-type converters ──────────────────────────────────────────────────────

function convertLogin(bw: BwItem): Item {
  const login = bw.login ?? {};
  const totp = extractTotpSecret(login.totp);
  const url = login.uris?.[0]?.uri ?? null;

  const data: Record<string, unknown> = {
    username: login.username ?? "",
    password: login.password ?? "",
    url: url ?? "",
    totp: totp ?? "",
    notes: bw.notes ?? "",
  };

  return {
    id: newId(),
    type: "login",
    name: bw.name ?? "Untitled",
    sub: buildSub("login", data),
    fav: bw.favorite ?? false,
    data,
  };
}

function convertNote(bw: BwItem): Item {
  const data: Record<string, unknown> = {
    content: bw.notes ?? "",
    notes: "",
  };

  return {
    id: newId(),
    type: "note",
    name: bw.name ?? "Untitled",
    sub: buildSub("note", data),
    fav: bw.favorite ?? false,
    data,
  };
}

function convertCard(bw: BwItem): Item {
  const card = bw.card ?? {};

  const data: Record<string, unknown> = {
    holder: card.cardholderName ?? "",
    number: (card.number ?? "").replace(/\s+/g, ""),
    expiry: buildExpiry(card.expMonth, card.expYear),
    cvv: card.code ?? "",
    brand: normaliseBrand(card.brand),
    notes: bw.notes ?? "",
  };

  return {
    id: newId(),
    type: "card",
    name: bw.name ?? "Untitled",
    sub: buildSub("card", data),
    fav: bw.favorite ?? false,
    data,
  };
}

// ─── Public API ───────────────────────────────────────────────────────────────

export interface ParseResult {
  items: Item[];
  skipped: number;
}

/**
 * Parse a Bitwarden unencrypted JSON export and return NaraVault `Item[]`.
 *
 * Throws with a descriptive message if the top-level shape is unrecognised.
 * Identity items (type 4) and any unknown types are silently skipped —
 * the caller can inspect `skipped` to surface this to the user.
 */
export function parseBitwardenExport(json: unknown): ParseResult {
  if (typeof json !== "object" || json === null) {
    throw new Error("Invalid Bitwarden export: expected a JSON object.");
  }

  const root = json as Record<string, unknown>;

  if (!("items" in root) || !Array.isArray(root.items)) {
    throw new Error(
      'Invalid Bitwarden export: missing "items" array. ' +
        "Make sure you exported an unencrypted Bitwarden JSON vault.",
    );
  }

  if ((root as BwExport).encrypted === true) {
    throw new Error(
      "This Bitwarden export is encrypted. " +
        "Please export again without encryption (Account → Export Vault → File Format: .json, no password).",
    );
  }

  const items: Item[] = [];
  let skipped = 0;

  for (const raw of root.items as unknown[]) {
    if (typeof raw !== "object" || raw === null) {
      skipped++;
      continue;
    }

    const bw = raw as BwItem;

    switch (bw.type) {
      case BW_TYPE_LOGIN:
        items.push(convertLogin(bw));
        break;
      case BW_TYPE_NOTE:
        items.push(convertNote(bw));
        break;
      case BW_TYPE_CARD:
        items.push(convertCard(bw));
        break;
      case BW_TYPE_IDENTITY:
        // Identity type is not supported in NaraVault
        skipped++;
        break;
      default:
        skipped++;
        break;
    }
  }

  return { items, skipped };
}
