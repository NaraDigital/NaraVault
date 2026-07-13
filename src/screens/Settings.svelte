<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../components/Button.svelte";
  import { tweaks } from "../lib/tweaks.svelte";
  import { parseBitwardenExport } from "../lib/import-bitwarden";
  import { vault } from "../lib/vault.svelte";
  import { toasts } from "../lib/toast.svelte";
  import { api, type S3Config } from "../lib/api";

  interface Props {
    count: number;
    onlock: () => Promise<void> | void;
    onreset: () => Promise<void> | void;
    onchangeMaster: () => void;
  }
  let { count, onlock, onreset, onchangeMaster }: Props = $props();

  // Idle auto-lock choices. `value` is a string for the <select>; "never" maps to
  // null (no auto-lock), everything else is a millisecond delay.
  const AUTO_LOCK_OPTIONS: { value: string; label: string }[] = [
    { value: String(5 * 60 * 1000), label: "5 minutes" },
    { value: String(10 * 60 * 1000), label: "10 minutes" },
    { value: String(20 * 60 * 1000), label: "20 minutes" },
    { value: String(30 * 60 * 1000), label: "30 minutes" },
    { value: String(60 * 60 * 1000), label: "1 hour" },
    { value: "never", label: "Never" },
  ];

  let busy = $state<"" | "lock" | "reset" | "import" | "s3-save" | "s3-test" | "s3-export" | "s3-import">("");

  async function run(kind: "lock" | "reset", fn: () => Promise<void> | void) {
    if (busy) return;
    busy = kind;
    try {
      await fn();
    } finally {
      busy = "";
    }
  }

  let fileInput: HTMLInputElement | undefined = $state();

  function triggerImport() {
    if (busy) return;
    fileInput?.click();
  }

  async function handleFileChange(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    // Reset input so the same file can be re-selected later
    input.value = "";
    if (!file) return;

    busy = "import";
    try {
      const text = await file.text();
      let json: unknown;
      try {
        json = JSON.parse(text);
      } catch {
        toasts.push("Import failed: file is not valid JSON.", "close");
        return;
      }

      const { items, skipped } = parseBitwardenExport(json);

      for (const item of items) {
        await vault.save(item);
      }

      const skippedNote = skipped > 0 ? ` (${skipped} skipped)` : "";
      toasts.push(`Imported ${items.length} items${skippedNote}.`, "check");
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Unknown error.";
      toasts.push(`Import failed: ${msg}`, "close");
    } finally {
      busy = "";
    }
  }

  // S3 / Cloud backup state
  let s3 = $state<S3Config>({
    endpoint: "",
    access_key_id: "",
    secret_access_key: "",
    bucket: "",
    region: "",
  });
  let showSecret = $state(false);
  // Revealing the S3 secret requires re-entering the master password, so a
  // bystander at an already-unlocked vault can't read stored cloud credentials.
  let showPrompt = $state(false);
  let promptPw = $state("");
  let promptErr = $state("");
  let promptBusy = $state(false);

  function toggleSecret() {
    if (showSecret) {
      // Hiding never needs a password.
      showSecret = false;
      return;
    }
    // Revealing → ask for the master password first.
    promptPw = "";
    promptErr = "";
    showPrompt = true;
  }

  async function confirmReveal() {
    if (promptBusy) return;
    if (!promptPw) {
      promptErr = "Enter your master password.";
      return;
    }
    promptBusy = true;
    promptErr = "";
    try {
      await api.verifyMaster(promptPw);
      showSecret = true;
      showPrompt = false;
      promptPw = "";
    } catch {
      promptErr = "Wrong password";
    } finally {
      promptBusy = false;
    }
  }

  function cancelReveal() {
    showPrompt = false;
    promptPw = "";
    promptErr = "";
  }

  // Browser-autofill consent prompt. true = the desktop app asks for approval on
  // every login fill (secure default); false = fill silently (origin match +
  // unlocked still enforced by the bridge). Cards always prompt regardless.
  let autofillPrompt = $state(true);
  let autofillBusy = $state(false);

  onMount(async () => {
    try {
      const saved = await api.loadS3Config();
      if (saved) s3 = saved;
    } catch {
      // Not configured yet — leave defaults
    }
    try {
      autofillPrompt = await api.getAutofillPrompt();
    } catch {
      // Locked or unset — keep secure default
    }
  });

  async function setAutofillPrompt(enabled: boolean) {
    if (autofillBusy || enabled === autofillPrompt) return;
    autofillBusy = true;
    const prev = autofillPrompt;
    autofillPrompt = enabled;
    try {
      await api.setAutofillPrompt(enabled);
    } catch (err) {
      autofillPrompt = prev;
      toasts.push(`Couldn't update setting: ${errMsg(err)}`, "close");
    } finally {
      autofillBusy = false;
    }
  }

  /** Extract a readable message from any thrown value (Tauri errors are plain objects). */
  function errMsg(err: unknown): string {
    if (typeof err === "string") return err;
    if (err instanceof Error) return err.message;
    // Tauri IPC errors: { code: string; message: string }
    const e = err as { message?: string };
    return e?.message ?? JSON.stringify(err);
  }

  async function saveS3Config() {
    if (busy) return;
    busy = "s3-save";
    try {
      await api.saveS3Config(s3);
      toasts.push("Cloud backup settings saved.", "check");
    } catch (err) {
      toasts.push(`Save failed: ${errMsg(err)}`, "close");
    } finally {
      busy = "";
    }
  }

  async function testS3Connection() {
    if (busy) return;
    busy = "s3-test";
    try {
      await api.testS3Connection();
      toasts.push("Connection successful.", "check");
    } catch (err) {
      toasts.push(`Connection failed: ${errMsg(err)}`, "close");
    } finally {
      busy = "";
    }
  }

  // Export modal state
  let exportModal = $state(false);
  let exportFilename = $state("");
  let exportPassword = $state("");

  function openExportModal() {
    if (busy) return;
    exportFilename = "";
    exportPassword = "";
    exportModal = true;
  }

  function closeExportModal() {
    exportModal = false;
    exportFilename = "";
    exportPassword = "";
  }

  async function submitExport() {
    if (!exportFilename.trim()) {
      toasts.push("Please enter a filename.", "close");
      return;
    }
    if (!exportPassword) {
      toasts.push("Please enter a password for this backup file.", "close");
      return;
    }
    const filename = exportFilename.trim();
    const password = exportPassword;
    closeExportModal();
    busy = "s3-export";
    try {
      const count = await api.exportToS3(filename, password);
      toasts.push(`Export successful — ${count} items saved to ${filename}.nvb.`, "check");
    } catch (err) {
      toasts.push(`Export failed: ${errMsg(err)}`, "close");
    } finally {
      busy = "";
    }
  }

  // Import modal state
  let importModal = $state(false);
  let importFiles = $state<string[]>([]);
  let importLoading = $state(false);
  let importSelected = $state("");
  let importPassword = $state("");

  async function openImportModal() {
    if (busy) return;
    importPassword = "";
    importFiles = [];
    importSelected = "";
    importModal = true;
    importLoading = true;
    try {
      const files = await api.listS3Backups();
      importFiles = files;
      importSelected = files[0] ?? "";
    } catch (err) {
      toasts.push(`Failed to list backups: ${errMsg(err)}`, "close");
      importModal = false;
    } finally {
      importLoading = false;
    }
  }

  function closeImportModal() {
    importModal = false;
    importFiles = [];
    importSelected = "";
    importPassword = "";
    importLoading = false;
  }

  async function submitImport() {
    if (!importSelected) {
      toasts.push("Please select a backup file.", "close");
      return;
    }
    if (!importPassword) {
      toasts.push("Please enter the password for this backup file.", "close");
      return;
    }
    const filename = importSelected;
    const password = importPassword;
    closeImportModal();
    busy = "s3-import";
    try {
      const count = await api.importFromS3(filename, password);
      await vault.reloadItems();
      toasts.push(`Imported ${count} items from ${filename}.nvb.`, "check");
    } catch (err) {
      toasts.push(`Import failed: ${errMsg(err)}`, "close");
    } finally {
      busy = "";
    }
  }
</script>

<div class="settings">
  <div class="settings-head">
    <h2>Settings</h2>
    <span class="detail-sub mono">{count} items protected</span>
  </div>

  <div class="set-group">
    <span class="set-group-label mono">APPEARANCE</span>
    <div class="set-row">
      <div><strong>Theme</strong><span class="set-desc">Switch between dark and light.</span></div>
      <div class="seg">
        <button
          class="seg-opt{tweaks.t.theme === 'dark' ? ' is-active' : ''}"
          onclick={() => tweaks.set("theme", "dark")}>Dark</button
        >
        <button
          class="seg-opt{tweaks.t.theme === 'light' ? ' is-active' : ''}"
          onclick={() => tweaks.set("theme", "light")}>Light</button
        >
      </div>
    </div>
  </div>

  <div class="set-group">
    <span class="set-group-label mono">DATA</span>
    <div class="set-row">
      <div>
        <strong>Import from Bitwarden</strong><span class="set-desc"
          >Load items from an unencrypted Bitwarden JSON export.</span
        >
      </div>
      <Button
        variant="ghost"
        icon="refresh"
        loading={busy === "import"}
        disabled={busy !== ""}
        onclick={triggerImport}>Import</Button
      >
    </div>
  </div>

  <div class="set-group">
    <span class="set-group-label mono">CLOUD BACKUP</span>
    <div class="set-row set-row--col">
      <div>
        <strong>S3 / Cloudflare R2</strong>
        <span class="set-desc">Encrypted backup to your own cloud storage. Items are encrypted with your vault key before upload.</span>
      </div>
      <div class="s3-form">
        <label class="s3-label" for="s3-endpoint">Endpoint URL <span class="s3-hint">(leave blank for AWS S3)</span></label>
        <input
          id="s3-endpoint"
          class="s3-input"
          type="url"
          placeholder="https://&lt;account&gt;.r2.cloudflarestorage.com"
          bind:value={s3.endpoint}
          disabled={busy !== ""}
        />

        <label class="s3-label" for="s3-access-key">Access Key ID</label>
        <input
          id="s3-access-key"
          class="s3-input"
          type="text"
          placeholder="Access key ID"
          bind:value={s3.access_key_id}
          disabled={busy !== ""}
        />

        <label class="s3-label" for="s3-secret">Secret Access Key</label>
        <div class="s3-secret-wrap">
          <input
            id="s3-secret"
            class="s3-input s3-input--secret"
            type={showSecret ? "text" : "password"}
            placeholder="Secret access key"
            bind:value={s3.secret_access_key}
            disabled={busy !== ""}
          />
          <button
            class="s3-reveal"
            type="button"
            onclick={toggleSecret}
            disabled={busy !== ""}
            aria-label={showSecret ? "Hide secret" : "Show secret"}
          >{showSecret ? "Hide" : "Show"}</button>
        </div>

        {#if showPrompt}
          <div class="reveal-prompt">
            <input
              class="s3-input"
              type="password"
              placeholder="Master password to reveal"
              bind:value={promptPw}
              disabled={promptBusy}
              onkeydown={(e) => { if (e.key === "Enter") confirmReveal(); if (e.key === "Escape") cancelReveal(); }}
            />
            <div class="reveal-actions">
              <Button variant="ghost" onclick={cancelReveal} disabled={promptBusy}>Cancel</Button>
              <Button variant="primary" onclick={confirmReveal} loading={promptBusy} disabled={promptBusy}>Confirm</Button>
            </div>
            {#if promptErr}
              <span class="reveal-err">{promptErr}</span>
            {/if}
          </div>
        {/if}

        <label class="s3-label" for="s3-bucket">Bucket</label>
        <input
          id="s3-bucket"
          class="s3-input"
          type="text"
          placeholder="my-naravault-backup"
          bind:value={s3.bucket}
          disabled={busy !== ""}
        />

        <label class="s3-label" for="s3-region">Region</label>
        <input
          id="s3-region"
          class="s3-input"
          type="text"
          placeholder="auto (R2) or ap-southeast-1 (AWS)"
          bind:value={s3.region}
          disabled={busy !== ""}
        />

        <div class="s3-actions">
          <Button
            variant="ghost"
            icon="check"
            loading={busy === "s3-save"}
            disabled={busy !== ""}
            onclick={saveS3Config}>Save</Button
          >
          <Button
            variant="ghost"
            icon="refresh"
            loading={busy === "s3-test"}
            disabled={busy !== ""}
            onclick={testS3Connection}>Test Connection</Button
          >
          <Button
            variant="ghost"
            icon="refresh"
            loading={busy === "s3-export"}
            disabled={busy !== ""}
            onclick={openExportModal}>Export to S3</Button
          >
          <Button
            variant="ghost"
            icon="refresh"
            loading={busy === "s3-import"}
            disabled={busy !== ""}
            onclick={openImportModal}>Import from S3</Button
          >
        </div>
      </div>
    </div>
  </div>

  {#if exportModal}
    <div class="s3-pw-overlay" role="dialog" aria-modal="true">
      <div class="s3-pw-card">
        <p class="s3-pw-title">Export to S3</p>
        <p class="s3-pw-desc">Choose a filename and set a password to encrypt this backup file. The password is specific to this file — not your master password.</p>
        <label class="s3-label" for="export-filename">Filename</label>
        <input
          id="export-filename"
          class="s3-pw-input"
          type="text"
          placeholder="backup-juni-2025"
          bind:value={exportFilename}
          onkeydown={(e) => { if (e.key === "Enter") submitExport(); if (e.key === "Escape") closeExportModal(); }}
        />
        <label class="s3-label" for="export-password">File password</label>
        <input
          id="export-password"
          class="s3-pw-input"
          type="password"
          placeholder="Password for this backup file"
          bind:value={exportPassword}
          onkeydown={(e) => { if (e.key === "Enter") submitExport(); if (e.key === "Escape") closeExportModal(); }}
        />
        <div class="s3-pw-btns">
          <Button variant="ghost" onclick={closeExportModal}>Cancel</Button>
          <Button variant="primary" onclick={submitExport} disabled={!exportFilename.trim() || !exportPassword}>Export</Button>
        </div>
      </div>
    </div>
  {/if}

  {#if importModal}
    <div class="s3-pw-overlay" role="dialog" aria-modal="true">
      <div class="s3-pw-card">
        <p class="s3-pw-title">Import from S3</p>
        {#if importLoading}
          <p class="s3-pw-desc">Loading backup files...</p>
        {:else if importFiles.length === 0}
          <p class="s3-pw-desc">No backup files found in this bucket.</p>
        {:else}
          <p class="s3-pw-desc">Select a backup file and enter its password to restore items.</p>
          <label class="s3-label" for="import-select">Backup file</label>
          <select
            id="import-select"
            class="s3-select"
            bind:value={importSelected}
          >
            {#each importFiles as f}
              <option value={f}>{f}.nvb</option>
            {/each}
          </select>
          <label class="s3-label" for="import-password">File password</label>
          <input
            id="import-password"
            class="s3-pw-input"
            type="password"
            placeholder="Password for this backup file"
            bind:value={importPassword}
            onkeydown={(e) => { if (e.key === "Enter") submitImport(); if (e.key === "Escape") closeImportModal(); }}
          />
        {/if}
        <div class="s3-pw-btns">
          <Button variant="ghost" onclick={closeImportModal}>Cancel</Button>
          <Button
            variant="primary"
            onclick={submitImport}
            disabled={importLoading || importFiles.length === 0 || !importSelected || !importPassword}
          >Import</Button>
        </div>
      </div>
    </div>
  {/if}

  <div class="set-group">
    <span class="set-group-label mono">SECURITY</span>
    <div class="set-row">
      <div>
        <strong>Auto-lock</strong><span class="set-desc"
          >Lock the vault after inactivity. Choose <em>Never</em> to keep it open until you lock or quit.</span
        >
      </div>
      <select
        class="set-select"
        value={tweaks.t.autoLockMs === null ? "never" : String(tweaks.t.autoLockMs)}
        onchange={(e) => {
          const v = (e.currentTarget as HTMLSelectElement).value;
          tweaks.set("autoLockMs", v === "never" ? null : Number(v));
        }}
      >
        {#each AUTO_LOCK_OPTIONS as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>
    <div class="set-row">
      <div>
        <strong>Browser autofill approval</strong><span class="set-desc"
          >Ask for confirmation in this app each time the extension fills a login or
          card. Turn off to fill silently — for logins the origin still has to match,
          and the vault must be unlocked either way.</span
        >
      </div>
      <div class="seg">
        <button
          class="seg-opt{autofillPrompt ? ' is-active' : ''}"
          disabled={autofillBusy}
          onclick={() => setAutofillPrompt(true)}>Ask</button
        >
        <button
          class="seg-opt{!autofillPrompt ? ' is-active' : ''}"
          disabled={autofillBusy}
          onclick={() => setAutofillPrompt(false)}>Off</button
        >
      </div>
    </div>
    {#if !autofillPrompt}
      <div class="set-warn" role="note">
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><path d="M12 9v4"/><path d="M12 17h.01"/></svg>
        <span>Logins and cards can be filled without confirmation while the vault is unlocked. Anything running on this machine that reaches the autofill bridge can read your card details or a matching login silently. Lock the vault when you step away.</span>
      </div>
    {/if}
    <div class="set-row">
      <div>
        <strong>Lock vault</strong><span class="set-desc">Require master password to reopen.</span>
      </div>
      <Button
        variant="ghost"
        icon="lock"
        loading={busy === "lock"}
        disabled={busy !== ""}
        onclick={() => run("lock", onlock)}>Lock now</Button
      >
    </div>
    <div class="set-row">
      <div>
        <strong>Change master password</strong><span class="set-desc"
          >Update the key that protects everything.</span
        >
      </div>
      <Button variant="ghost" icon="key" disabled={busy !== ""} onclick={onchangeMaster}>Change</Button>
    </div>
  </div>

  <div class="set-group danger-group">
    <span class="set-group-label mono">DANGER ZONE</span>
    <div class="set-row">
      <div>
        <strong>Reset vault</strong><span class="set-desc"
          >Permanently delete all items and the master password.</span
        >
      </div>
      <Button
        variant="danger"
        icon="trash"
        loading={busy === "reset"}
        disabled={busy !== ""}
        onclick={() => run("reset", onreset)}>Reset</Button
      >
    </div>
  </div>

  <!-- Hidden file picker — triggered programmatically by the Import button -->
  <input
    bind:this={fileInput}
    type="file"
    accept=".json,application/json"
    style="display:none"
    onchange={handleFileChange}
  />
</div>

<style>
  .set-row--col {
    flex-direction: column;
    align-items: stretch;
    gap: 12px;
  }

  .s3-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .s3-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    margin-top: 6px;
  }

  .s3-hint {
    font-weight: 400;
    color: var(--text-faint);
  }

  .s3-input {
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    outline: none;
    box-sizing: border-box;
  }

  .s3-input:focus {
    border-color: var(--accent);
  }

  .s3-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .s3-secret-wrap {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .s3-input--secret {
    flex: 1;
    width: auto;
  }

  .s3-reveal {
    padding: 8px 12px;
    font-size: 12px;
    font-weight: 600;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-dim);
    cursor: pointer;
    white-space: nowrap;
  }

  .s3-reveal:hover {
    color: var(--text);
  }

  .s3-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 10px;
  }

  /* Password prompt overlay for backup / restore */
  .s3-pw-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: grid;
    place-items: center;
    z-index: 200;
  }
  .s3-pw-card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 24px;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .s3-pw-title {
    font-weight: 700;
    font-size: 15px;
    margin: 0;
  }
  .s3-pw-desc {
    font-size: 13px;
    color: var(--text-dim);
    margin: 0;
  }
  .s3-pw-input {
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    outline: none;
    box-sizing: border-box;
    font-family: var(--font-mono);
  }
  .s3-pw-input:focus { border-color: var(--accent); }
  .s3-select {
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    outline: none;
    box-sizing: border-box;
    cursor: pointer;
  }
  .s3-select:focus {
    border-color: var(--accent);
  }
  .set-select {
    padding: 8px 10px;
    font-size: 13px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    outline: none;
    cursor: pointer;
    min-width: 130px;
  }
  .set-select:focus {
    border-color: var(--accent);
  }
  .set-warn {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-top: 10px;
    padding: 10px 12px;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-dim);
    background: color-mix(in srgb, var(--danger, #e5484d) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--danger, #e5484d) 35%, transparent);
    border-radius: var(--radius);
  }
  .set-warn svg {
    flex-shrink: 0;
    margin-top: 1px;
    color: var(--danger, #e5484d);
  }
  .reveal-prompt {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
    padding: 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .reveal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .reveal-err {
    font-size: 12px;
    color: var(--danger, #e5484d);
    font-weight: 600;
  }
  .s3-pw-btns {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
