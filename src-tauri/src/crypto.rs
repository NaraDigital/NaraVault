//! Cryptographic core for NaraVault.
//!
//! Threat model: a local, offline vault protected by a single master password.
//! We use envelope encryption:
//!
//!   master password --Argon2id--> KEK (key-encryption key)
//!   KEK --AES-256-GCM--> wraps a random DEK (data-encryption key)
//!   DEK --AES-256-GCM--> encrypts every vault item
//!
//! Only the DEK lives in memory while unlocked; it is zeroized on lock/close.
//! Changing the master password only re-wraps the DEK, never re-encrypts data.
//!
//! Every ciphertext carries a fresh random 96-bit nonce, stored alongside it.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};

pub const KEY_LEN: usize = 32; // 256-bit
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12; // 96-bit GCM nonce

/// Argon2id parameters. Persisted in the vault so they can be tuned/migrated later.
/// Defaults follow OWASP guidance for password hashing (>= 19 MiB), tuned up for a
/// high-value secret store.
#[derive(Clone, Copy, Debug)]
pub struct KdfParams {
    pub mem_kib: u32,
    pub time: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        KdfParams {
            mem_kib: 64 * 1024, // 64 MiB
            time: 3,
            parallelism: 1,
        }
    }
}

/// Fill a fixed-size buffer with cryptographically secure random bytes.
fn random_bytes<const N: usize>() -> AppResult<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::getrandom(&mut buf).map_err(|_| AppError::Crypto)?;
    Ok(buf)
}

pub fn random_salt() -> AppResult<[u8; SALT_LEN]> {
    random_bytes::<SALT_LEN>()
}

/// Derive the 256-bit KEK from the master password + salt using Argon2id.
pub fn derive_kek(
    password: &str,
    salt: &[u8],
    params: KdfParams,
) -> AppResult<Zeroizing<[u8; KEY_LEN]>> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let p = Params::new(
        params.mem_kib,
        params.time,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|_| AppError::Crypto)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, p);

    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(password.as_bytes(), salt, out.as_mut())
        .map_err(|_| AppError::Crypto)?;
    Ok(out)
}

/// Generate a fresh random Data-Encryption-Key.
pub fn generate_dek() -> AppResult<Zeroizing<[u8; KEY_LEN]>> {
    Ok(Zeroizing::new(random_bytes::<KEY_LEN>()?))
}

/// A nonce + ciphertext pair. The nonce is public; security relies on the key.
#[derive(Clone)]
pub struct Sealed {
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

/// Encrypt `plaintext` with `key` (AES-256-GCM, fresh random nonce).
pub fn seal(key: &[u8; KEY_LEN], plaintext: &[u8]) -> AppResult<Sealed> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce_bytes = random_bytes::<NONCE_LEN>()?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| AppError::Crypto)?;
    Ok(Sealed {
        nonce: nonce_bytes,
        ciphertext,
    })
}

/// Decrypt a `Sealed` blob with `key`. A failure (wrong key or tampering) is
/// surfaced as a generic crypto error — never distinguishable from each other.
pub fn open(key: &[u8; KEY_LEN], nonce: &[u8], ciphertext: &[u8]) -> AppResult<Vec<u8>> {
    if nonce.len() != NONCE_LEN {
        return Err(AppError::Crypto);
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::Crypto)
}

/// Wrap (encrypt) the DEK with the KEK.
pub fn wrap_dek(kek: &[u8; KEY_LEN], dek: &[u8; KEY_LEN]) -> AppResult<Sealed> {
    seal(kek, dek.as_slice())
}

/// Unwrap (decrypt) the DEK with the KEK. AEAD tag failure => wrong password.
pub fn unwrap_dek(
    kek: &[u8; KEY_LEN],
    nonce: &[u8],
    wrapped: &[u8],
) -> AppResult<Zeroizing<[u8; KEY_LEN]>> {
    let bytes = open(kek, nonce, wrapped).map_err(|_| AppError::InvalidPassword)?;
    if bytes.len() != KEY_LEN {
        return Err(AppError::Crypto);
    }
    let mut dek = Zeroizing::new([0u8; KEY_LEN]);
    dek.copy_from_slice(&bytes);
    Ok(dek)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_envelope() {
        let params = KdfParams {
            mem_kib: 8 * 1024,
            time: 1,
            parallelism: 1,
        };
        let salt = random_salt().unwrap();
        let kek = derive_kek("correct horse battery staple", &salt, params).unwrap();
        let dek = generate_dek().unwrap();

        let wrapped = wrap_dek(&kek, &dek).unwrap();
        let dek2 = unwrap_dek(&kek, &wrapped.nonce, &wrapped.ciphertext).unwrap();
        assert_eq!(dek.as_slice(), dek2.as_slice());

        // wrong password must fail
        let bad = derive_kek("wrong password", &salt, params).unwrap();
        assert!(unwrap_dek(&bad, &wrapped.nonce, &wrapped.ciphertext).is_err());
    }

    #[test]
    fn roundtrip_item() {
        let dek = generate_dek().unwrap();
        let msg = b"{\"secret\":\"hunter2\"}";
        let sealed = seal(&dek, msg).unwrap();
        let out = open(&dek, &sealed.nonce, &sealed.ciphertext).unwrap();
        assert_eq!(out, msg);

        // tampering flips the tag
        let mut tampered = sealed.ciphertext.clone();
        tampered[0] ^= 0xff;
        assert!(open(&dek, &sealed.nonce, &tampered).is_err());
    }
}
