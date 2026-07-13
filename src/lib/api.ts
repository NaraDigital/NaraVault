import { invoke } from "@tauri-apps/api/core";
import type { Item, ItemMeta, VaultStatus } from "./types";

export interface S3Config {
  endpoint: string;
  access_key_id: string;
  secret_access_key: string;
  bucket: string;
  region: string;
}

/**
 * Typed wrappers around the Rust command surface. This is the *only* module that
 * talks to the backend; everything sensitive (keys, ciphertext) stays in Rust.
 */
export const api = {
  vaultStatus: () => invoke<VaultStatus>("vault_status"),

  createVault: (password: string, hint: string) =>
    invoke<void>("create_vault", { password, hint }),

  unlock: (password: string) => invoke<void>("unlock", { password }),

  lock: () => invoke<void>("lock"),

  verifyMaster: (password: string) =>
    invoke<void>("verify_master", { password }),

  changeMaster: (current: string, newPassword: string) =>
    invoke<void>("change_master", { current, newPassword }),

  listItems: () => invoke<Item[]>("list_items"),

  listItemMeta: () => invoke<ItemMeta[]>("list_item_meta"),

  launcherOpenItem: (id: string, password: string) =>
    invoke<void>("launcher_open_item", { id, password }),

  autofillConsentReply: (id: number, approved: boolean) =>
    invoke<void>("autofill_consent_reply", { id, approved }),

  saveItem: (item: Item) => invoke<void>("save_item", { item }),

  deleteItem: (id: string) => invoke<void>("delete_item", { id }),

  deleteItems: (ids: string[]) => invoke<void>("delete_items", { ids }),

  resetVault: () => invoke<void>("reset_vault"),

  ping: () => invoke<string>("ping"),

  getAutofillPrompt: () => invoke<boolean>("get_autofill_prompt"),
  setAutofillPrompt: (enabled: boolean) => invoke<void>("set_autofill_prompt", { enabled }),

  saveS3Config: (config: S3Config) => invoke<void>("save_s3_config", { config }),
  loadS3Config: () => invoke<S3Config | null>("load_s3_config"),
  testS3Connection: () => invoke<string>("test_s3_connection"),
  exportToS3: (filename: string, password: string) => invoke<number>("export_to_s3", { filename, password }),
  listS3Backups: () => invoke<string[]>("list_s3_backups"),
  importFromS3: (filename: string, password: string) => invoke<number>("import_from_s3", { filename, password }),
};
