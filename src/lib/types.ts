export type ItemType = "login" | "card" | "seed" | "note";

export type ItemData = Record<string, unknown>;

export interface Item {
  id: string;
  type: ItemType;
  name: string;
  sub: string;
  fav: boolean;
  data: ItemData;
}

/** Non-secret item projection used by the quick launcher (no `data` payload). */
export interface ItemMeta {
  id: string;
  type: ItemType;
  name: string;
  sub: string;
  fav: boolean;
}

export type VaultPhase = "onboarding" | "locked" | "unlocked";

export interface VaultStatus {
  phase: VaultPhase;
  hint: string;
}

/** Error shape thrown by Tauri commands (see Rust `AppError`). */
export interface AppErrorShape {
  code: string;
  message: string;
}

export function isAppError(e: unknown): e is AppErrorShape {
  return typeof e === "object" && e !== null && "code" in e;
}
