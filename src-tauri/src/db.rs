//! SQLite persistence. Everything sensitive is stored as ciphertext; this layer
//! never sees plaintext secrets except the values it is explicitly handed to write.

use rusqlite::{params, Connection, OptionalExtension};

use crate::crypto::KdfParams;
use crate::error::{AppError, AppResult};

/// Persisted vault metadata needed to derive keys and unwrap the DEK.
pub struct VaultMeta {
    pub kdf_salt: Vec<u8>,
    pub kdf: KdfParams,
    pub wrapped_dek: Vec<u8>,
    pub wrapped_dek_nonce: Vec<u8>,
    pub hint: String,
}

/// One encrypted item row as stored on disk.
pub struct ItemRow {
    pub id: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

pub fn open(path: &std::path::Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS vault_meta (
            id                INTEGER PRIMARY KEY CHECK (id = 1),
            kdf_salt          BLOB    NOT NULL,
            kdf_mem           INTEGER NOT NULL,
            kdf_time          INTEGER NOT NULL,
            kdf_par           INTEGER NOT NULL,
            wrapped_dek       BLOB    NOT NULL,
            wrapped_dek_nonce BLOB    NOT NULL,
            hint              TEXT    NOT NULL DEFAULT '',
            created_at        INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS items (
            id         TEXT PRIMARY KEY,
            nonce      BLOB    NOT NULL,
            ciphertext BLOB    NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key        TEXT PRIMARY KEY,
            nonce      BLOB NOT NULL,
            ciphertext BLOB NOT NULL
        );
        "#,
    )?;
    Ok(())
}

pub fn vault_exists(conn: &Connection) -> AppResult<bool> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM vault_meta", [], |r| r.get(0))?;
    Ok(n > 0)
}

pub fn read_meta(conn: &Connection) -> AppResult<Option<VaultMeta>> {
    let meta = conn
        .query_row(
            "SELECT kdf_salt, kdf_mem, kdf_time, kdf_par, wrapped_dek, wrapped_dek_nonce, hint
             FROM vault_meta WHERE id = 1",
            [],
            |r| {
                Ok(VaultMeta {
                    kdf_salt: r.get(0)?,
                    kdf: KdfParams {
                        mem_kib: r.get::<_, i64>(1)? as u32,
                        time: r.get::<_, i64>(2)? as u32,
                        parallelism: r.get::<_, i64>(3)? as u32,
                    },
                    wrapped_dek: r.get(4)?,
                    wrapped_dek_nonce: r.get(5)?,
                    hint: r.get(6)?,
                })
            },
        )
        .optional()?;
    Ok(meta)
}

pub fn read_hint(conn: &Connection) -> AppResult<String> {
    let hint = conn
        .query_row("SELECT hint FROM vault_meta WHERE id = 1", [], |r| r.get(0))
        .optional()?
        .unwrap_or_default();
    Ok(hint)
}

#[allow(clippy::too_many_arguments)]
pub fn write_meta(
    conn: &Connection,
    salt: &[u8],
    kdf: KdfParams,
    wrapped_dek: &[u8],
    wrapped_dek_nonce: &[u8],
    hint: &str,
    created_at: i64,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO vault_meta
            (id, kdf_salt, kdf_mem, kdf_time, kdf_par, wrapped_dek, wrapped_dek_nonce, hint, created_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            salt,
            kdf.mem_kib as i64,
            kdf.time as i64,
            kdf.parallelism as i64,
            wrapped_dek,
            wrapped_dek_nonce,
            hint,
            created_at
        ],
    )?;
    Ok(())
}

/// Re-key the vault: replace salt/params and the (re-wrapped) DEK. Item ciphertext
/// is untouched because the DEK itself never changes.
pub fn update_meta_rekey(
    conn: &Connection,
    salt: &[u8],
    kdf: KdfParams,
    wrapped_dek: &[u8],
    wrapped_dek_nonce: &[u8],
) -> AppResult<()> {
    let rows = conn.execute(
        "UPDATE vault_meta
            SET kdf_salt = ?1, kdf_mem = ?2, kdf_time = ?3, kdf_par = ?4,
                wrapped_dek = ?5, wrapped_dek_nonce = ?6
          WHERE id = 1",
        params![
            salt,
            kdf.mem_kib as i64,
            kdf.time as i64,
            kdf.parallelism as i64,
            wrapped_dek,
            wrapped_dek_nonce
        ],
    )?;
    // Guard against a silent no-op: re-wrapping the DEK must hit the meta row,
    // otherwise the new master password would appear to work but never persist.
    if rows != 1 {
        return Err(AppError::NoVault);
    }
    Ok(())
}

pub fn read_all_items(conn: &Connection) -> AppResult<Vec<ItemRow>> {
    let mut stmt =
        conn.prepare("SELECT id, nonce, ciphertext FROM items ORDER BY updated_at DESC")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ItemRow {
                id: r.get(0)?,
                nonce: r.get(1)?,
                ciphertext: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn upsert_item(
    conn: &Connection,
    id: &str,
    nonce: &[u8],
    ciphertext: &[u8],
    updated_at: i64,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO items (id, nonce, ciphertext, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE
            SET nonce = excluded.nonce,
                ciphertext = excluded.ciphertext,
                updated_at = excluded.updated_at",
        params![id, nonce, ciphertext, updated_at],
    )?;
    Ok(())
}

pub fn delete_item(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    Ok(())
}

/// Persist an encrypted key-value setting.
pub fn save_setting(conn: &Connection, key: &str, nonce: &[u8], ciphertext: &[u8]) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, nonce, ciphertext)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE
            SET nonce = excluded.nonce,
                ciphertext = excluded.ciphertext",
        params![key, nonce, ciphertext],
    )?;
    Ok(())
}

/// Load an encrypted setting by key. Returns `None` if not found.
/// Returns `Some((nonce, ciphertext))` if found.
pub fn load_setting(conn: &Connection, key: &str) -> AppResult<Option<(Vec<u8>, Vec<u8>)>> {
    let result = conn
        .query_row(
            "SELECT nonce, ciphertext FROM settings WHERE key = ?1",
            params![key],
            |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
        )
        .optional()?;
    Ok(result)
}

/// Wipe the entire vault (items + metadata). Used by "Reset vault".
pub fn reset(conn: &Connection) -> AppResult<()> {
    // Single transaction: never leave the vault half-wiped (items gone but meta
    // left, or vice-versa) if the process dies mid-reset.
    conn.execute_batch("BEGIN IMMEDIATE; DELETE FROM items; DELETE FROM vault_meta; COMMIT;")?;
    Ok(())
}
