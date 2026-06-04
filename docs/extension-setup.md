# NaraVault Browser Extension — Panduan Setup & Penggunaan

Dokumen ini menjelaskan cara memasang dan menggunakan ekstensi browser NaraVault untuk autofill username, password, dan kode TOTP secara otomatis di halaman login website manapun.

---

## Cara kerjanya

```
Ekstensi Browser  ──native messaging (stdio)──►  naravault-host  ──127.0.0.1:27432──►  Aplikasi NaraVault
 (deteksi form)                                   (relay tipis)                          (pemegang kunci,
                                                                                          dekripsi data)
```

- **Aplikasi NaraVault** adalah satu-satunya yang menyimpan kunci enkripsi (DEK) dan mendekripsi data vault. Kalau aplikasi ditutup atau vault dikunci, autofill tidak bisa bekerja — ekstensi tidak punya akses ke kunci apapun.
- **naravault-host** hanya bertugas sebagai relay. Tidak menyimpan data apapun.
- Semua komunikasi terjadi lewat `127.0.0.1` (loopback) — tidak ada data yang keluar ke internet.
- Setiap sesi menggunakan token acak sehingga program lain di komputer yang sama tidak bisa mencuri data dari bridge.

---

## Prasyarat

Sebelum mulai setup, pastikan:

- [x] **Rust toolchain** sudah terinstall (`rustup`, `cargo`)
- [x] **NaraVault** sudah di-build dan bisa dijalankan
- [x] Browser yang dipakai: **Chrome**, **Edge**, **Brave**, atau **Firefox**
- [x] Saat autofill digunakan nanti, **aplikasi NaraVault harus dalam kondisi terbuka dan vault sudah dibuka** (unlock dengan master password)

---

## Setup (lakukan sekali saja)

### Langkah 1 — Build aplikasi dan host binary

Jalankan perintah ini dari root folder project:

```bash
cd src-tauri
cargo build --release
```

Perintah ini menghasilkan dua binary di `src-tauri/target/release/`:
- `naravault` (atau `naravault.exe`) — aplikasi desktop utama
- `naravault-host` (atau `naravault-host.exe`) — relay untuk native messaging

> **Catatan:** Kalau kamu hanya ingin build host-nya saja (bukan aplikasi penuh), jalankan:
> ```bash
> cargo build --release --bin naravault-host
> ```

---

### Langkah 2 — Load ekstensi di browser

#### Chrome / Edge / Brave

1. Buka halaman ekstensi di browser:
   - Chrome: `chrome://extensions`
   - Edge: `edge://extensions`
   - Brave: `brave://extensions`
2. Aktifkan **Developer mode** (toggle di pojok kanan atas).
3. Klik **Load unpacked**.
4. Pilih folder `extension/` dari root project NaraVault.
5. Setelah berhasil dimuat, catat **Extension ID** yang tampil di kartu ekstensi — formatnya berupa string panjang seperti `abcdefghijklmnopabcdefghijklmnop`. ID ini dibutuhkan di langkah berikutnya.

#### Firefox

1. Buka `about:debugging` di address bar.
2. Klik **This Firefox** di sidebar kiri.
3. Klik **Load Temporary Add-on**.
4. Pilih file `extension/manifest.json`.
5. Catat **Add-on ID** yang muncul (contoh: `naravault@naravault.dev`).

> **Perhatian untuk Firefox:** Ekstensi yang dimuat via `about:debugging` bersifat sementara dan akan hilang saat browser ditutup. Untuk penggunaan permanen, ekstensi harus dipublikasikan atau di-sign.

---

### Langkah 3 — Daftarkan native host ke browser

Browser perlu tahu di mana binary `naravault-host` berada dan ekstensi mana yang boleh berkomunikasi dengannya. Jalankan installer sesuai sistem operasi kamu menggunakan Extension ID dari Langkah 2.

#### Windows (PowerShell)

```powershell
cd extension\native-host
.\install-windows.ps1 -ExtensionId <extension-id-kamu> -Browser chrome
```

Ganti nilai `-Browser` sesuai browser yang kamu pakai: `chrome`, `edge`, atau `brave`.

**Apa yang dilakukan script ini:**
- Mencari binary `naravault-host.exe` di folder `src-tauri/target/release/` (otomatis, tidak perlu input manual)
- Membuat file manifest `com.naravault.host.json` di `%APPDATA%\NaraVault\`
- Mendaftarkan path manifest tersebut ke **registry Windows** di `HKCU\Software\...\NativeMessagingHosts\com.naravault.host`

> **Tidak perlu hak admin.** Script ini bekerja di scope user (`HKCU`), bukan system (`HKLM`).

Contoh output sukses:
```
Installed NaraVault host for chrome.
  host binary : C:\Users\kamu\...\naravault-host.exe
  manifest    : C:\Users\kamu\AppData\Roaming\NaraVault\com.naravault.host.json
  extension   : abcdefghijklmnopabcdefghijklmnop
Restart the browser, then reload the extension.
```

#### macOS / Linux

```bash
cd extension/native-host
./install-unix.sh <extension-id-kamu> chrome
```

Ganti argumen terakhir sesuai browser: `chrome`, `chromium`, `edge`, `brave`, atau `firefox`.

**Apa yang dilakukan script ini:**
- Mencari binary `naravault-host` di `src-tauri/target/release/`
- Membuat file manifest `com.naravault.host.json` di direktori konfigurasi browser yang sesuai:

| Browser | Lokasi manifest (macOS) | Lokasi manifest (Linux) |
|---|---|---|
| Chrome | `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` | `~/.config/Google/Chrome/NativeMessagingHosts/` |
| Chromium | `~/Library/Application Support/Chromium/NativeMessagingHosts/` | `~/.config/chromium/NativeMessagingHosts/` |
| Edge | `~/Library/Application Support/Microsoft Edge/NativeMessagingHosts/` | `~/.config/Microsoft Edge/NativeMessagingHosts/` |
| Brave | `~/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/` | `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` |
| Firefox | `~/Library/Application Support/Mozilla/NativeMessagingHosts/` | `~/.mozilla/native-messaging-hosts/` |

- Mengatur permission file manifest ke `600` (hanya bisa dibaca oleh user yang bersangkutan)

Contoh output sukses:
```
Installed NaraVault host for chrome
  host binary : /home/kamu/NaraVault/src-tauri/target/release/naravault-host
  manifest    : /home/kamu/.config/Google/Chrome/NativeMessagingHosts/com.naravault.host.json
  extension   : abcdefghijklmnopabcdefghijklmnop
Restart the browser, then reload the extension.
```

---

### Langkah 4 — Restart browser

Tutup browser sepenuhnya (termasuk semua jendela), lalu buka kembali. Setelah itu, reload ekstensi NaraVault di halaman ekstensi browser.

Setup selesai. Tidak perlu diulang kecuali kamu reinstall browser atau pindah ke komputer baru.

---

## Cara pakai sehari-hari

1. **Buka aplikasi NaraVault** dan unlock vault dengan master password kamu. Aplikasi boleh diminimize, tidak perlu selalu di depan layar.
2. Buka browser dan navigasi ke website yang credentials-nya sudah tersimpan di NaraVault — misalnya `https://github.com/login`.
3. Ada dua cara mengisi form:

   **Cara A — Ikon di dalam field:**
   Klik ikon kunci kecil NaraVault yang muncul di dalam field password, lalu pilih akun yang ingin digunakan.

   **Cara B — Toolbar browser:**
   Klik ikon NaraVault di toolbar browser. Daftar akun yang URL-nya cocok dengan website saat ini akan muncul. Klik salah satu untuk mengisi.

4. Username dan password langsung terisi di form.
5. Jika login tersebut punya **TOTP (kode 2FA)**:
   - Kalau halaman punya field untuk one-time code → kode TOTP saat ini langsung diisi otomatis.
   - Kalau tidak ada field tersebut → kode TOTP disalin ke clipboard, tinggal paste manual.

> **Autofill tidak pernah berjalan otomatis tanpa klik.** Selalu butuh aksi eksplisit dari kamu.

---

## Troubleshooting

| Gejala | Kemungkinan penyebab | Solusi |
|---|---|---|
| Muncul pesan "NaraVault not running" | Aplikasi NaraVault belum dibuka | Buka aplikasi NaraVault |
| Muncul pesan "Vault is locked" | Vault belum dibuka dengan master password | Unlock vault di aplikasi NaraVault |
| Muncul pesan "No saved logins" | Tidak ada item yang URL-nya cocok dengan website saat ini | Buka item di NaraVault, tambahkan URL website ke field URL |
| Ikon NaraVault tidak muncul sama sekali | Host belum terdaftar atau Extension ID salah | Jalankan ulang installer (Langkah 3) dengan ID yang benar, lalu restart browser |
| Autofill tidak mengisi apapun setelah klik | Website menggunakan form non-standar | Isi manual; laporkan ke developer untuk investigasi lebih lanjut |

### Informasi tambahan

- Bridge berjalan di port `27432` secara default. Kalau port tersebut sedang dipakai program lain, aplikasi akan otomatis memilih port lain yang bebas. Host menemukan port tersebut lewat file handshake yang ditulis aplikasi:
  - **Windows:** `%APPDATA%\dev.naravault.app\bridge.json`
  - **Linux:** `~/.local/share/dev.naravault.app/bridge.json`
  - **macOS:** `~/Library/Application Support/dev.naravault.app/bridge.json`
- Agar ikon NaraVault muncul di suatu website, field **URL** pada item di vault harus mengandung domain website tersebut (contoh: `github.com`).
- Menghapus atau mengubah Extension ID (misalnya setelah reinstall ekstensi) memerlukan registrasi ulang native host di Langkah 3.
