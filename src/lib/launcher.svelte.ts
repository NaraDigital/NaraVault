/**
 * Cross-window bridge for the quick launcher. The launcher window (a separate
 * webview) can't touch the main window's component state directly, so it emits
 * Tauri events that App.svelte forwards into this store. Vault.svelte consumes
 * `pendingOpenId` once it is mounted with items loaded — this decouples the
 * "open item" request from main-window mount timing (it may still be unlocking).
 */
class LauncherBridge {
  pendingOpenId = $state<string | null>(null);
}

export const launcherBridge = new LauncherBridge();
