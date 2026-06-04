# NaraVault

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-Windows-0078d4.svg)

Offline-first, zero-knowledge password manager built with Tauri 2 and Rust. All encryption happens locally — your master password and keys never leave your device.

<!-- screenshot -->

## Features

- **4 item types** — Login (with TOTP), Credit/Debit Card, Crypto Seed Phrase, Secure Note
- **Zero-knowledge encryption** — AES-256-GCM per item, key derivation via Argon2id
- **Browser autofill** — Chrome/Firefox extension with a local native messaging bridge
- **Cloud backup** — Export encrypted `.nvb` backup files to S3, Cloudflare R2, or any S3-compatible storage
- **Quick launcher** — `Alt+N` global shortcut for instant access
- **System tray** — App stays running in the background; close hides to tray
- **Auto-lock** — Vault locks automatically after 5 minutes of idle
- **Import from Bitwarden** — Supports Bitwarden JSON export (Login, Card, Secure Note)
- **Password generator**, bulk delete, favorites, search & category filter, dark/light theme

## Security Model

NaraVault uses an envelope encryption scheme: your master password derives a Key Encryption Key (KEK) via Argon2id (64 MiB, 3 iterations), which wraps a per-vault Data Encryption Key (DEK). Each vault item is independently encrypted with AES-256-GCM. Changing your master password only re-wraps the DEK — no bulk re-encryption needed.

- All cryptographic operations run in the Rust backend; the frontend never handles keys
- The DEK exists in memory only while the vault is unlocked, and is zeroized on lock or quit
- Memory pages holding the DEK are locked from being swapped to disk (best-effort, via `VirtualLock`/`mlock`)
- Backup files (`.nvb`) are encrypted with a separate file password — S3/R2 stores only ciphertext
- The browser autofill bridge binds to `127.0.0.1` only and uses a random per-session token

## Installation

### Download

Pre-built Windows installers (MSI and NSIS) are available on the [Releases](../../releases) page.

### Build from Source

**Prerequisites**

- [Rust toolchain](https://rustup.rs/) (stable)
- [Bun](https://bun.sh/) (or Node.js + npm)
- [Tauri v2 prerequisites](https://tauri.app/start/prerequisites/) for your platform (Windows: WebView2 runtime)

**Steps**

```bash
git clone https://github.com/your-username/naravault.git
cd naravault

bun install
bun run tauri build
```

The installer will be output to `src-tauri/target/release/bundle/`.

For development with hot-reload:

```bash
bun run tauri dev
```

> macOS and Linux are supported by Tauri but have not been tested or documented yet.

## Usage

### First Run

Launch the app and create a vault with a strong master password. You will not be able to recover your vault if you forget it — there is no server-side reset.

<<<<<<< HEAD
### Browser Autofill Extension

NaraVault ships with a browser extension for Chrome, Edge, Brave, and Firefox. It auto-fills saved logins (including live TOTP codes) directly from your local vault — no cloud relay involved.

```
Browser extension  ──native messaging──►  naravault-host  ──127.0.0.1──►  NaraVault app
   (detects forms)                          (thin relay)                  (holds the key)
```

The app is the sole decryptor. If the vault is **locked** or the app is **closed**, autofill is unavailable.

#### Step 1 — Build the host binary

```bash
cd src-tauri
cargo build --release
```

This produces `naravault-host` (or `naravault-host.exe`) in `src-tauri/target/release/`.

> If you installed from a pre-built release, the host binary is bundled and this step is already done.

#### Step 2 — Load the extension (unpacked)

1. Open **`chrome://extensions`** (or `edge://extensions`, `brave://extensions`).
2. Enable **Developer mode** (toggle, top-right).
3. Click **Load unpacked** → select the `extension/` folder from this repo.
4. Copy the **Extension ID** shown on the card (e.g. `abcdefghijklmnopabcdefghijklmnop`).

> **Firefox**: open `about:debugging` → *This Firefox* → **Load Temporary Add-on** → pick `extension/manifest.json`. Copy the Add-on ID shown.

#### Step 3 — Register the native messaging host

**Windows (PowerShell — no admin required):**

```powershell
cd extension\native-host
.\install-windows.ps1 -ExtensionId <your-extension-id> -Browser chrome
# -Browser options: chrome | edge | brave
```

**macOS / Linux:**

```bash
cd extension/native-host
chmod +x install-unix.sh
./install-unix.sh <your-extension-id> chrome
# last arg: chrome | chromium | edge | brave | firefox
```

The script writes the host manifest and registers it in the browser's native messaging directory (registry on Windows, a JSON file in the user config directory on macOS/Linux).

#### Step 4 — Restart the browser

Close the browser fully and reopen it, then reload the extension once from the extensions page. You're done.

#### Daily usage

1. Open NaraVault and **unlock** your vault. Keep the app running in the tray.
2. Navigate to a login page (e.g. `github.com/login`).
3. Click the **NaraVault key icon** inside the password field, or click the toolbar icon to see logins matching the current site.
4. **First time on a site:** NaraVault shows an *"Allow browser autofill?"* consent prompt in the app window — click **Allow**. This prevents other local processes from silently requesting credentials. The consent lasts until you lock or quit.
5. Username and password are filled. If the login has TOTP, the current code is filled into the OTP field automatically (or copied to clipboard if no OTP field is detected).

> For a login to match a site, the item's **URL** field must contain the site's domain (e.g. `github.com`).

#### Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| "NaraVault not running" | App is closed | Start NaraVault |
| "Vault is locked" | App is locked | Unlock with your master password |
| "No saved logins" | Item URL doesn't match the site | Add the site's domain to the login item's URL field |
| Nothing happens at all | Host not registered / wrong extension ID | Re-run the installer with the correct ID, restart browser |
| Fill does nothing after prompt | Consent was denied or timed out (30 s) | Trigger autofill again and click Allow in the app |

> The bridge binds to `127.0.0.1:27432` by default. If that port is taken the app falls back to a random free port — the host discovers it automatically from the handshake file at `%APPDATA%\dev.naravault.app\bridge.json` (Windows) or `~/.local/share/dev.naravault.app/bridge.json` (Linux/macOS). No action needed.

### Quick Launcher (`Alt+N`)

Press **`Alt+N`** from anywhere on your desktop to instantly open the NaraVault quick launcher — a Spotlight-style popup that lets you search and open any vault item without switching to the main window.

- **Search** by name or subtitle as you type.
- **Click an item** to jump straight to it in the main window (vault must be unlocked).
- **Dismiss** by pressing `Escape` or clicking away — the launcher hides itself on focus loss.
- The shortcut works even when NaraVault is minimized to the system tray.

> **Conflict**: If another app already holds `Alt+N`, NaraVault will silently skip shortcut registration (non-fatal). In that case, open the main window from the system tray instead.
=======
### Browser Autofill

Install the browser extension and register the native messaging host so the browser can communicate with the desktop app. See [`docs/extension-setup.md`](docs/extension-setup.md) for setup instructions *(English translation in progress)*.

The vault must be unlocked for autofill to work. Autofill requests are authorized via a per-session token over a local loopback connection.
>>>>>>> 7e3bdb51bcc3c9d9aa30f1727953dd2a41b768b6

### Cloud Backup (S3 / Cloudflare R2)

Go to **Settings → Cloud Backup** and fill in your storage credentials:

| Field | AWS S3 | Cloudflare R2 |
|---|---|---|
| Endpoint URL | *(leave blank)* | `https://<account-id>.r2.cloudflarestorage.com` |
| Region | e.g. `us-east-1` | `auto` |
| Access Key ID | IAM key | R2 API token ID |
| Secret Key | IAM secret | R2 API token secret |
| Bucket | your bucket name | your bucket name |

Backups are exported as `.nvb` files encrypted with a custom file password (independent of your master password). You can maintain multiple backup files with different passwords.

### Import from Bitwarden

Go to **Settings → Import**, select your Bitwarden JSON export file. Supported types: Login, Card, Secure Note.

## Project Structure

```
src/               # Svelte 5 frontend
src-tauri/src/
  commands.rs      # Tauri command surface (IPC handlers)
  crypto.rs        # AES-256-GCM + Argon2id
  db.rs            # SQLite persistence
  s3.rs            # S3/R2 backup helpers
  bridge.rs        # Browser autofill loopback server
  state.rs         # In-memory DEK management
extension/         # Browser extension source
docs/              # Documentation
```

## Tech Stack

| Layer | Technology |
|---|---|
| Desktop framework | [Tauri 2](https://tauri.app/) |
| Backend / crypto | Rust |
| Frontend | Svelte 5 + TypeScript |
| Local storage | SQLite (encrypted at item level) |
| Build tooling | Bun + Vite |

## Contributing

Contributions are welcome. Please open an issue first to discuss significant changes before submitting a pull request.

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/your-feature`)
3. Commit your changes
4. Open a pull request

Bug reports and feature requests can be filed via the [issue tracker](../../issues).

## License

[MIT](LICENSE)
