# NaraVault Browser Autofill — Setup & Usage

Autofill your saved logins **and live TOTP codes** into any website, straight
from your local NaraVault vault. Nothing is uploaded — the browser extension
talks to the desktop app over a `127.0.0.1` (loopback) channel only, and the app
stays the **only** thing that can decrypt your vault.

## How it works (the short version)

```
Browser extension  ──native messaging (stdio)──►  naravault-host  ──127.0.0.1──►  NaraVault app
   (detects forms)                                  (thin relay)                  (holds the key,
                                                                                   decrypts on demand)
```

- The **app** is the sole holder of the encryption key (DEK) and the sole
  decryptor. If the vault is **locked** or the app is **closed**, autofill is
  simply unavailable — the extension/host can never unlock it.
- The **host** is a tiny relay. It holds no secrets and just forwards requests.
- Every request carries a random per-session token, so other local programs or
  hostile web pages can't pull data from the bridge.
- A site only ever receives a credential whose saved URL matches that site.

---

## One-time setup

### 1. Build the desktop app + host

```bash
cd src-tauri
cargo build --release
```

This produces both the app and the `naravault-host` binary in
`src-tauri/target/release/`.

### 2. Load the extension (unpacked)

1. Open `chrome://extensions` (or `edge://extensions`, `brave://extensions`).
2. Turn on **Developer mode**.
3. Click **Load unpacked** and select the `extension/` folder.
4. Copy the **extension ID** shown on the card (a long string like
   `abcdefghijklmnopabcdefghijklmnop`).

> Firefox: open `about:debugging` → This Firefox → **Load Temporary Add-on**,
> pick `extension/manifest.json`. Use your add-on id in step 3 below.

### 3. Register the native host

The browser needs to know where `naravault-host` lives and which extension may
talk to it. Run the installer for your OS with the extension ID from step 2.

**Windows (PowerShell):**

```powershell
cd extension\native-host
.\install-windows.ps1 -ExtensionId <your-extension-id> -Browser chrome
# -Browser can be chrome | edge | brave
```

**macOS / Linux:**

```bash
cd extension/native-host
./install-unix.sh <your-extension-id> chrome
# last arg: chrome | chromium | edge | brave | firefox
```

### 4. Restart the browser

Close it fully and reopen, then reload the extension once. Done.

---

## Daily usage

1. Open the **NaraVault** desktop app and **unlock** it with your master
   password. (Keep it running — minimized is fine.)
2. Browse to a site you have a login for, e.g. `https://github.com/login`.
3. Fill it one of two ways:
   - **In-page:** click the small NaraVault key icon that appears inside the
     password field, then pick the account.
   - **Toolbar:** click the NaraVault icon in the browser toolbar to see the
     accounts that match the current site, and click one.
4. **First time on a site:** the NaraVault app shows an *"Allow browser
   autofill?"* prompt. Click **Allow** — the site is then trusted for the rest
   of the unlocked session (you won't be asked again until you lock or restart
   the app). This consent step is what stops other local programs from quietly
   pulling your logins. If you didn't just trigger autofill, click Deny.
5. The username and password are filled. If the login has TOTP:
   - if the page has a one-time-code field, the current code is filled too;
   - otherwise the code is copied to your clipboard so you can paste it.

---

## Troubleshooting

| What you see | Why | Fix |
|---|---|---|
| "NaraVault not running" | App is closed | Start the app |
| "Vault is locked" | App is locked | Unlock it with your master password |
| "No saved logins" | No item's URL matches this site | Add the site's URL to the login item |
| Nothing happens at all | Host not registered / wrong extension ID | Re-run the installer in step 3 with the correct ID, restart the browser |
| "This site doesn't match the saved login" | Page origin ≠ item URL | Check the login item's URL field |
| Fill does nothing after a prompt | You (or a timeout) denied consent | Trigger autofill again and click Allow within 30s |

### Notes

- The fixed bridge port is `27432`. If it's already in use, the app falls back
  to a random free port automatically and the host discovers it from the
  handshake file (`%APPDATA%\dev.naravault.app\bridge.json` on Windows,
  `~/.local/share/dev.naravault.app/bridge.json` on Linux,
  `~/Library/Application Support/dev.naravault.app/bridge.json` on macOS) — no
  action needed.
- Autofill only fires on an explicit click. The extension never fills silently.
- For the in-page icon to match a site, the login item's **URL** field must
  contain that site's domain (e.g. `github.com`).
