import { api } from "./api";
import { isAppError, type Item, type VaultPhase } from "./types";

/**
 * Central vault state. Mirrors the backend phase (onboarding/locked/unlocked) and
 * holds the decrypted items while unlocked. The actual secrets only ever live here
 * in memory; on lock we clear them and ask Rust to zeroize the key.
 */
class VaultStore {
  phase = $state<VaultPhase>("onboarding");
  hint = $state("");
  items = $state<Item[]>([]);
  loading = $state(true);

  async init() {
    // On production builds the Tauri backend (setup()) may still be registering
    // AppState when the WebView fires its very first IPC call — because WebView2
    // runs in a separate process and can start executing JS before setup()
    // finishes calling app.manage(). Retry briefly instead of surfacing an error.
    const MAX_ATTEMPTS = 20;
    const RETRY_MS = 100;
    try {
      for (let attempt = 0; attempt <= MAX_ATTEMPTS; attempt++) {
        try {
          await this.refreshStatus();
          if (this.phase === "unlocked") await this.reloadItems();
          return; // success — finally still runs
        } catch (e: unknown) {
          const msg =
            typeof e === "string"
              ? e
              : (e as { message?: string })?.message ?? "";
          // "state not managed" → setup() hasn't finished yet; wait and retry.
          if (msg.includes("state not managed") && attempt < MAX_ATTEMPTS) {
            await new Promise<void>((r) => setTimeout(r, RETRY_MS));
            continue;
          }
          throw e; // non-transient error — propagate to caller
        }
      }
    } finally {
      // Always drop the boot splash, even if the backend call threw — otherwise
      // a transient error would leave the app stuck on the loading screen.
      this.loading = false;
    }
  }

  async refreshStatus() {
    const s = await api.vaultStatus();
    this.phase = s.phase;
    this.hint = s.hint;
  }

  async reloadItems() {
    this.items = await api.listItems();
  }

  async createVault(password: string, hint: string) {
    await api.createVault(password, hint);
    await this.refreshStatus();
    await this.reloadItems();
  }

  /** Returns true on success, false on wrong password. */
  async unlock(password: string): Promise<boolean> {
    try {
      await api.unlock(password);
      await this.refreshStatus();
      await this.reloadItems();
      return true;
    } catch (e) {
      if (isAppError(e) && e.code === "INVALID_PASSWORD") return false;
      throw e;
    }
  }

  /** Returns true on success, false on wrong password (used by the reveal gate). */
  async verifyMaster(password: string): Promise<boolean> {
    try {
      await api.verifyMaster(password);
      return true;
    } catch (e) {
      if (isAppError(e) && e.code === "INVALID_PASSWORD") return false;
      throw e;
    }
  }

  async lock() {
    await api.lock();
    this.items = [];
    await this.refreshStatus();
  }

  /** Returns true on success, false if the current password is wrong. */
  async changeMaster(current: string, newPassword: string): Promise<boolean> {
    try {
      await api.changeMaster(current, newPassword);
      return true;
    } catch (e) {
      if (isAppError(e) && e.code === "INVALID_PASSWORD") return false;
      throw e;
    }
  }

  async save(item: Item) {
    await api.saveItem(item);
    await this.reloadItems();
  }

  async remove(id: string) {
    await api.deleteItem(id);
    await this.reloadItems();
  }

  async removeMany(ids: string[]) {
    await api.deleteItems(ids);
    await this.reloadItems();
  }

  async reset() {
    await api.resetVault();
    this.items = [];
    await this.refreshStatus();
  }
}

export const vault = new VaultStore();
