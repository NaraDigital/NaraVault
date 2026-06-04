import type { ItemType } from "./types";

export interface Category {
  id: "all" | ItemType;
  label: string;
  icon: string;
}

export const CATEGORIES: Category[] = [
  { id: "all", label: "All items", icon: "all" },
  { id: "login", label: "Logins", icon: "login" },
  { id: "card", label: "Cards", icon: "card" },
  { id: "seed", label: "Seedphrases", icon: "seed" },
  { id: "note", label: "Notes", icon: "note" },
];

export const TYPE_LABEL: Record<ItemType, string> = {
  login: "Login",
  card: "Card",
  seed: "Seedphrase",
  note: "Secure note",
};

export const TYPE_TINT: Record<ItemType, string> = {
  login: "var(--t-login)",
  card: "var(--t-card)",
  seed: "var(--t-seed)",
  note: "var(--t-note)",
};
