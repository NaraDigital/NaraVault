//! S3 / Cloudflare R2 cloud backup helpers.
//!
//! The S3Config is stored encrypted in the `settings` table (key = "s3_config").
//! Backup blobs are AES-256-GCM encrypted with the vault DEK and uploaded as a
//! single object ("naravault-backup.enc") to the configured bucket.
//!
//! Blob wire format:
//!   [4 bytes]  magic  "NVB1"
//!   [12 bytes] nonce  (AES-256-GCM nonce)
//!   [N bytes]  ciphertext of JSON: {"version":1,"items":[...Item...]}

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;

use crate::error::{AppError, AppResult};

pub const S3_CONFIG_KEY: &str = "s3_config";
pub const BACKUP_OBJECT_KEY: &str = "naravault-backup.enc";
/// Legacy format: encrypted with vault DEK (not portable across vault resets).
pub const BLOB_MAGIC_V1: &[u8; 4] = b"NVB1";
/// Current format: encrypted with Argon2id(master_password, backup_salt).
/// Portable: can be restored after vault reset as long as the password is the same.
pub const BLOB_MAGIC_V2: &[u8; 4] = b"NVB2";

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
    pub region: String,
}

/// Build an authenticated S3 client from config. If `endpoint` is non-empty
/// (e.g. a Cloudflare R2 URL) it is used as the custom endpoint; otherwise the
/// default AWS endpoint resolver is used.
pub async fn build_s3_client(cfg: &S3Config) -> Client {
    let creds = Credentials::new(
        &cfg.access_key_id,
        &cfg.secret_access_key,
        None,
        None,
        "naravault",
    );

    let region = Region::new(cfg.region.clone());

    let mut config_loader = aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(creds)
        .region(region);

    if !cfg.endpoint.is_empty() {
        config_loader = config_loader.endpoint_url(cfg.endpoint.clone());
    }

    let aws_cfg = config_loader.load().await;
    Client::new(&aws_cfg)
}

/// Build a v2 backup blob (password-encrypted, portable across vault resets).
/// Layout: magic(4) + backup_salt(16) + nonce(12) + ciphertext(N)
pub fn build_blob_v2(backup_salt: &[u8; 16], nonce: &[u8; 12], ciphertext: &[u8]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(4 + 16 + 12 + ciphertext.len());
    blob.extend_from_slice(BLOB_MAGIC_V2);
    blob.extend_from_slice(backup_salt);
    blob.extend_from_slice(nonce);
    blob.extend_from_slice(ciphertext);
    blob
}

pub enum ParsedBlob<'a> {
    /// v2: (backup_salt, nonce, ciphertext) — decrypt with password-derived key.
    V2 { backup_salt: [u8; 16], nonce: [u8; 12], ciphertext: &'a [u8] },
    /// v1 (legacy): (nonce, ciphertext) — decrypt with vault DEK.
    V1 { nonce: [u8; 12], ciphertext: &'a [u8] },
}

/// Parse a backup blob, detecting v1 or v2 format.
pub fn parse_blob(blob: &[u8]) -> AppResult<ParsedBlob<'_>> {
    if blob.len() < 4 {
        return Err(AppError::Data("backup file is too short or corrupt".into()));
    }
    match &blob[..4] {
        b if b == BLOB_MAGIC_V2 => {
            // NVB2: magic(4) + salt(16) + nonce(12) + ciphertext
            if blob.len() < 4 + 16 + 12 {
                return Err(AppError::Data("backup file is too short or corrupt".into()));
            }
            let mut backup_salt = [0u8; 16];
            backup_salt.copy_from_slice(&blob[4..20]);
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&blob[20..32]);
            Ok(ParsedBlob::V2 { backup_salt, nonce, ciphertext: &blob[32..] })
        }
        b if b == BLOB_MAGIC_V1 => {
            // NVB1 (legacy): magic(4) + nonce(12) + ciphertext
            if blob.len() < 4 + 12 {
                return Err(AppError::Data("backup file is too short or corrupt".into()));
            }
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&blob[4..16]);
            Ok(ParsedBlob::V1 { nonce, ciphertext: &blob[16..] })
        }
        _ => Err(AppError::Data(
            "backup file has invalid magic — not a NaraVault backup".into(),
        )),
    }
}

/// Convert an SDK error to a short, human-readable string.
fn sdk_err_msg(e: impl std::fmt::Display) -> String {
    let raw = e.to_string();
    // The AWS SDK wraps errors in a verbose chain. Extract the innermost message:
    // "... caused by: ..." → take the last segment after the final "caused by:".
    if let Some(idx) = raw.rfind("caused by:") {
        raw[idx + "caused by:".len()..].trim().to_string()
    } else {
        // Trim common SDK boilerplate prefix if present.
        raw.trim_start_matches("dispatch failure:").trim().to_string()
    }
}

/// Upload bytes to S3. Returns a user-friendly error string on failure.
pub async fn upload(client: &Client, bucket: &str, key: &str, data: Vec<u8>) -> AppResult<()> {
    let body = ByteStream::from(data);
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("S3 upload failed: {}", sdk_err_msg(e))))?;
    Ok(())
}

/// Download bytes from S3. Returns a user-friendly error string on failure.
pub async fn download(client: &Client, bucket: &str, key: &str) -> AppResult<Vec<u8>> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("S3 download failed: {}", sdk_err_msg(e))))?;

    let bytes = resp
        .body
        .collect()
        .await
        .map_err(|e| AppError::Internal(format!("failed to read S3 response body: {e}")))?
        .into_bytes();

    Ok(bytes.to_vec())
}

/// Check whether the bucket is accessible. Returns a user-friendly error on failure.
pub async fn head_bucket(client: &Client, bucket: &str) -> AppResult<()> {
    client
        .head_bucket()
        .bucket(bucket)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("connection test failed: {}", sdk_err_msg(e))))?;
    Ok(())
}

/// List all `.nvb` objects in the bucket. Returns base names without the `.nvb` extension.
pub async fn list_nvb_objects(client: &Client, bucket: &str) -> AppResult<Vec<String>> {
    let resp = client
        .list_objects_v2()
        .bucket(bucket)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("S3 list failed: {}", sdk_err_msg(e))))?;

    let mut names: Vec<String> = resp
        .contents()
        .iter()
        .filter_map(|obj| obj.key())
        .filter(|key| key.ends_with(".nvb"))
        .map(|key| key[..key.len() - 4].to_string())
        .collect();

    names.sort();
    Ok(names)
}
