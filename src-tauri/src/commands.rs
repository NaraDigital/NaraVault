//! Tauri command surface — the only bridge between the Svelte UI and the vault.
//! Commands that touch secrets require the vault to be unlocked (DEK in memory).

use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroizing;

use crate::crypto::{
    self, derive_kek, generate_dek, random_salt, unwrap_dek, wrap_dek, KdfParams,
};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::model::{Item, ItemMeta, VaultPhase, VaultStatus};
use crate::s3::{self, S3Config};
use crate::state::AppState;

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Current high-level vault phase + (when locked) the user's hint.
#[tauri::command]
pub fn vault_status(state: State<AppState>) -> AppResult<VaultStatus> {
    let conn = state.conn.lock().expect("conn poisoned");
    if !db::vault_exists(&conn)? {
        return Ok(VaultStatus {
            phase: VaultPhase::Onboarding,
            hint: String::new(),
        });
    }
    let phase = if state.is_unlocked() {
        VaultPhase::Unlocked
    } else {
        VaultPhase::Locked
    };
    Ok(VaultStatus {
        phase,
        hint: db::read_hint(&conn)?,
    })
}

/// First-run: create the vault, derive keys, seed sample items, and unlock.
#[tauri::command]
pub fn create_vault(state: State<AppState>, password: String, hint: String) -> AppResult<()> {
    // Take ownership of the IPC-deserialized password so its heap buffer is
    // wiped when this command returns, instead of lingering in freed memory.
    let password = Zeroizing::new(password);
    if password.len() < 8 {
        return Err(AppError::Data("password too short".into()));
    }
    let conn = state.conn.lock().expect("conn poisoned");
    if db::vault_exists(&conn)? {
        return Err(AppError::AlreadyExists);
    }

    let kdf = KdfParams::default();
    let salt = random_salt()?;
    let kek = derive_kek(&password, &salt, kdf)?;
    let dek = generate_dek()?;
    let wrapped = wrap_dek(&kek, &dek)?;

    db::write_meta(
        &conn,
        &salt,
        kdf,
        &wrapped.ciphertext,
        &wrapped.nonce,
        &hint,
        now_ms(),
    )?;

    // A freshly created vault starts empty — no sample/seed data in production.
    drop(conn);
    state.set_dek(dek);
    Ok(())
}

/// Unlock with the master password. Returns `InvalidPassword` on failure.
#[tauri::command]
pub fn unlock(state: State<AppState>, password: String) -> AppResult<()> {
    let password = Zeroizing::new(password);
    let conn = state.conn.lock().expect("conn poisoned");
    let meta = db::read_meta(&conn)?.ok_or(AppError::NoVault)?;
    drop(conn);

    let kek = derive_kek(&password, &meta.kdf_salt, meta.kdf)?;
    let dek = unwrap_dek(&kek, &meta.wrapped_dek_nonce, &meta.wrapped_dek)?;
    state.set_dek(dek);
    Ok(())
}

/// Lock the vault: zeroize the in-memory key.
#[tauri::command]
pub fn lock(state: State<AppState>) {
    state.clear_dek();
}

/// Re-verify the master password (for the per-item reveal gate). Does not change
/// lock state; succeeds only when the password unwraps the stored DEK.
#[tauri::command]
pub fn verify_master(state: State<AppState>, password: String) -> AppResult<()> {
    let password = Zeroizing::new(password);
    let conn = state.conn.lock().expect("conn poisoned");
    let meta = db::read_meta(&conn)?.ok_or(AppError::NoVault)?;
    drop(conn);

    let kek = derive_kek(&password, &meta.kdf_salt, meta.kdf)?;
    let _ = unwrap_dek(&kek, &meta.wrapped_dek_nonce, &meta.wrapped_dek)?;
    Ok(())
}

/// Change the master password by re-wrapping the existing DEK under a new KEK.
/// Item ciphertext is never touched.
#[tauri::command]
pub fn change_master(
    state: State<AppState>,
    current: String,
    new_password: String,
) -> AppResult<()> {
    let current = Zeroizing::new(current);
    let new_password = Zeroizing::new(new_password);
    if new_password.len() < 8 {
        return Err(AppError::Data("password too short".into()));
    }
    let conn = state.conn.lock().expect("conn poisoned");
    let meta = db::read_meta(&conn)?.ok_or(AppError::NoVault)?;

    // Verify the current password by unwrapping the DEK.
    let old_kek = derive_kek(&current, &meta.kdf_salt, meta.kdf)?;
    let dek = unwrap_dek(&old_kek, &meta.wrapped_dek_nonce, &meta.wrapped_dek)?;

    // Re-wrap the same DEK under a fresh salt + KEK.
    let kdf = KdfParams::default();
    let new_salt = random_salt()?;
    let new_kek = derive_kek(&new_password, &new_salt, kdf)?;
    let rewrapped = wrap_dek(&new_kek, &dek)?;

    db::update_meta_rekey(&conn, &new_salt, kdf, &rewrapped.ciphertext, &rewrapped.nonce)?;
    drop(conn);

    // Keep the session unlocked with the (unchanged) DEK.
    state.set_dek(dek);
    Ok(())
}

/// Decrypt and return every vault item. Requires an unlocked vault.
#[tauri::command]
pub fn list_items(state: State<AppState>) -> AppResult<Vec<Item>> {
    let conn = state.conn.lock().expect("conn poisoned");
    let rows = db::read_all_items(&conn)?;
    drop(conn);

    state.with_dek(|dek| {
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let plaintext = crypto::open(dek, &row.nonce, &row.ciphertext)?;
            let item: Item = serde_json::from_slice(&plaintext)?;
            items.push(item);
        }
        Ok(items)
    })
}

/// Decrypt items but return only non-secret metadata (id/type/name/sub/fav) —
/// never the `data` payload. The quick launcher uses this so secrets never load
/// into the second webview. Requires an unlocked vault.
#[tauri::command]
pub fn list_item_meta(state: State<AppState>) -> AppResult<Vec<ItemMeta>> {
    let conn = state.conn.lock().expect("conn poisoned");
    let rows = db::read_all_items(&conn)?;
    drop(conn);

    state.with_dek(|dek| {
        let mut metas = Vec::with_capacity(rows.len());
        for row in rows {
            let plaintext = crypto::open(dek, &row.nonce, &row.ciphertext)?;
            let item: Item = serde_json::from_slice(&plaintext)?;
            metas.push(ItemMeta {
                id: item.id,
                item_type: item.item_type,
                name: item.name,
                sub: item.sub,
                fav: item.fav,
            });
        }
        Ok(metas)
    })
}

/// Re-authenticate, then bring the main window forward and ask it to open +
/// reveal `id`. The master-password check happens HERE (server-side) and is the
/// only trigger for main's open-item handler — so a reveal is impossible without
/// a valid password, even if a caller invokes this directly. Fail-closed:
/// returns `InvalidPassword` (and emits nothing) on a wrong password.
#[tauri::command]
pub fn launcher_open_item(
    state: State<AppState>,
    app: AppHandle,
    id: String,
    password: String,
) -> AppResult<()> {
    let password = Zeroizing::new(password);
    let conn = state.conn.lock().expect("conn poisoned");
    let meta = db::read_meta(&conn)?.ok_or(AppError::NoVault)?;
    drop(conn);
    let kek = derive_kek(&password, &meta.kdf_salt, meta.kdf)?;
    let _ = unwrap_dek(&kek, &meta.wrapped_dek_nonce, &meta.wrapped_dek)?;

    if let Some(launcher) = app.get_webview_window("launcher") {
        let _ = launcher.hide();
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
        main.emit("naravault://open-item", id)
            .map_err(|e| AppError::Data(e.to_string()))?;
    }
    Ok(())
}

/// The main window's answer to an autofill consent prompt (M-A). Delivers the
/// user's Allow/Deny decision back to the bridge thread that is waiting on it.
#[tauri::command]
pub fn autofill_consent_reply(state: State<AppState>, id: u64, approved: bool) {
    state.resolve_consent(id, approved);
}

/// Create or update an item (encrypting it under the DEK).
#[tauri::command]
pub fn save_item(state: State<AppState>, item: Item) -> AppResult<()> {
    let plaintext = serde_json::to_vec(&item)?;
    let sealed = state.with_dek(|dek| crypto::seal(dek, &plaintext))?;
    let conn = state.conn.lock().expect("conn poisoned");
    db::upsert_item(&conn, &item.id, &sealed.nonce, &sealed.ciphertext, now_ms())?;
    Ok(())
}

#[tauri::command]
pub fn delete_item(state: State<AppState>, id: String) -> AppResult<()> {
    if !state.is_unlocked() {
        return Err(AppError::Locked);
    }
    let conn = state.conn.lock().expect("conn poisoned");
    db::delete_item(&conn, &id)?;
    Ok(())
}

/// Permanently wipe the entire vault and lock the session.
#[tauri::command]
pub fn reset_vault(state: State<AppState>) -> AppResult<()> {
    let conn = state.conn.lock().expect("conn poisoned");
    db::reset(&conn)?;
    drop(conn);
    state.clear_dek();
    Ok(())
}

/// Diagnostic command: no AppState required, just returns "pong". Used to test
/// whether IPC itself is working in production builds.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

// ---------------------------------------------------------------------------
// S3 / Cloud-backup commands
// ---------------------------------------------------------------------------

/// Encrypt and persist the S3 config in the `settings` table.
#[tauri::command]
pub async fn save_s3_config(state: State<'_, AppState>, config: S3Config) -> AppResult<()> {
    let json = serde_json::to_vec(&config)?;
    let sealed = state.with_dek(|dek| crate::crypto::seal(dek, &json))?;
    let conn = state.conn.lock().expect("conn poisoned");
    db::save_setting(&conn, s3::S3_CONFIG_KEY, &sealed.nonce, &sealed.ciphertext)?;
    Ok(())
}

/// Decrypt and return the stored S3 config, or `null` if none is saved yet.
#[tauri::command]
pub async fn load_s3_config(state: State<'_, AppState>) -> AppResult<Option<S3Config>> {
    let row = {
        let conn = state.conn.lock().expect("conn poisoned");
        db::load_setting(&conn, s3::S3_CONFIG_KEY)?
    };
    let Some((nonce, ciphertext)) = row else {
        return Ok(None);
    };
    let plaintext = state.with_dek(|dek| crate::crypto::open(dek, &nonce, &ciphertext))?;
    let cfg: S3Config = serde_json::from_slice(&plaintext)?;
    Ok(Some(cfg))
}

/// Test bucket connectivity. Returns `"ok"` on success or an error.
#[tauri::command]
pub async fn test_s3_connection(state: State<'_, AppState>) -> AppResult<String> {
    let cfg = load_s3_config(state)
        .await?
        .ok_or_else(|| AppError::Data("S3 is not configured yet".into()))?;
    let client = s3::build_s3_client(&cfg).await;
    s3::head_bucket(&client, &cfg.bucket).await?;
    Ok("ok".into())
}

/// Export all vault items to S3 as a named `.nvb` file.
/// The payload is encrypted with a key derived from `password` (Argon2id + random salt).
/// `password` is the backup file password — not the master password.
/// Returns the number of items exported.
#[tauri::command]
pub async fn export_to_s3(
    state: State<'_, AppState>,
    filename: String,
    password: String,
) -> AppResult<usize> {
    let password = Zeroizing::new(password);

    // 1. Sanitize filename.
    let filename = filename.trim().to_string();
    let filename = filename.strip_suffix(".nvb").unwrap_or(&filename).to_string();
    if filename.is_empty() {
        return Err(AppError::Data("filename cannot be empty".into()));
    }
    if filename.len() > 100 {
        return Err(AppError::Data("filename is too long (max 100 characters)".into()));
    }
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err(AppError::Data("filename contains invalid characters".into()));
    }

    // 2. Load S3 config.
    let cfg = load_s3_config(state.clone())
        .await?
        .ok_or_else(|| AppError::Data("S3 is not configured yet".into()))?;

    // 3. Decrypt all items with vault DEK.
    let rows = {
        let conn = state.conn.lock().expect("conn poisoned");
        db::read_all_items(&conn)?
    };
    let items: Vec<Item> = state.with_dek(|dek| {
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let plaintext = crate::crypto::open(dek, &row.nonce, &row.ciphertext)?;
            let item: Item = serde_json::from_slice(&plaintext)?;
            out.push(item);
        }
        Ok(out)
    })?;
    let count = items.len();

    // 4. Serialize payload.
    #[derive(serde::Serialize)]
    struct BackupPayload<'a> {
        version: u32,
        items: &'a [Item],
    }
    let payload = serde_json::to_vec(&BackupPayload { version: 2, items: &items })?;

    // 5. Derive export key from the file password (not the master password).
    let export_salt = crypto::random_salt()?;
    let export_key = crypto::derive_kek(&password, &export_salt, KdfParams::default())?;
    let sealed = crypto::seal(&export_key, &payload)?;

    // 6. Build v2 blob and upload as "{filename}.nvb".
    let blob = s3::build_blob_v2(&export_salt, &sealed.nonce, &sealed.ciphertext);
    let client = s3::build_s3_client(&cfg).await;
    let object_key = format!("{filename}.nvb");
    s3::upload(&client, &cfg.bucket, &object_key, blob).await?;

    Ok(count)
}

/// List all `.nvb` backup files in the configured S3 bucket.
/// Returns base names (without `.nvb` extension), sorted alphabetically.
#[tauri::command]
pub async fn list_s3_backups(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    let cfg = load_s3_config(state)
        .await?
        .ok_or_else(|| AppError::Data("S3 is not configured yet".into()))?;
    let client = s3::build_s3_client(&cfg).await;
    s3::list_nvb_objects(&client, &cfg.bucket).await
}

/// Download a named `.nvb` backup from S3, decrypt with the file password,
/// re-encrypt each item with the current vault DEK, and upsert into the vault.
/// Returns the number of items imported.
#[tauri::command]
pub async fn import_from_s3(
    state: State<'_, AppState>,
    filename: String,
    password: String,
) -> AppResult<usize> {
    let password = Zeroizing::new(password);

    // 1. Load S3 config.
    let cfg = load_s3_config(state.clone())
        .await?
        .ok_or_else(|| AppError::Data("S3 is not configured yet".into()))?;

    // 2. Download "{filename}.nvb".
    let client = s3::build_s3_client(&cfg).await;
    let object_key = format!("{filename}.nvb");
    let blob = s3::download(&client, &cfg.bucket, &object_key).await?;

    // 3. Parse blob — only V2 is accepted; V1 is too old.
    let (backup_salt, nonce, ciphertext_owned) = match s3::parse_blob(&blob)? {
        s3::ParsedBlob::V2 { backup_salt, nonce, ciphertext } => {
            (backup_salt, nonce, ciphertext.to_vec())
        }
        s3::ParsedBlob::V1 { .. } => {
            return Err(AppError::Data(
                "this backup is in the old format — please create a new backup first".into(),
            ));
        }
    };

    // 4. Derive key from the file password and decrypt.
    let backup_key = crypto::derive_kek(&password, &backup_salt, KdfParams::default())?;
    let plaintext = crypto::open(&backup_key, &nonce, &ciphertext_owned)
        .map_err(|_| AppError::Data("wrong password for this backup file".into()))?;

    // 5. Parse JSON payload.
    #[derive(serde::Deserialize)]
    struct BackupPayload {
        #[allow(dead_code)]
        version: u32,
        items: Vec<Item>,
    }
    let payload: BackupPayload = serde_json::from_slice(&plaintext)?;
    let count = payload.items.len();

    // 6. Re-encrypt each item with the current vault DEK and upsert.
    {
        let conn = state.conn.lock().expect("conn poisoned");
        for item in payload.items {
            let json = serde_json::to_vec(&item)?;
            let sealed = state.with_dek(|dek| crate::crypto::seal(dek, &json))?;
            db::upsert_item(&conn, &item.id, &sealed.nonce, &sealed.ciphertext, now_ms())?;
        }
    }

    Ok(count)
}

/// Delete multiple items in a single transaction. Requires unlocked vault.
#[tauri::command]
pub fn delete_items(state: State<AppState>, ids: Vec<String>) -> AppResult<()> {
    // Guard: require vault to be unlocked (DEK present) so a locked process
    // can't be tricked into deleting items via a stale IPC call.
    state.with_dek(|_| Ok(()))?;
    let conn = state.conn.lock().expect("conn poisoned");
    conn.execute_batch("BEGIN IMMEDIATE")?;
    for id in &ids {
        db::delete_item(&conn, id)?;
    }
    conn.execute_batch("COMMIT")?;
    Ok(())
}
