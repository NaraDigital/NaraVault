//! RFC 6238 TOTP code generation, used by the autofill bridge so the running app
//! (sole DEK holder) can hand the browser extension a *live* 6-digit code instead
//! of the raw shared secret. Mirrors the frontend `totpCode` implementation.

use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

const STEP_SECS: u64 = 30;
const DIGITS: u32 = 6;

/// Decode an RFC 4648 base32 secret (padding/spaces/hyphens tolerated).
/// Returns `None` if the string contains a non-base32 character.
fn base32_decode(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits: u32 = 0;
    let mut value: u32 = 0;
    let mut out = Vec::new();
    for ch in input.chars() {
        if ch == '=' || ch == ' ' || ch == '-' || ch == '\t' {
            continue;
        }
        let up = ch.to_ascii_uppercase() as u8;
        let idx = ALPHABET.iter().position(|&c| c == up)? as u32;
        value = (value << 5) | idx;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((value >> bits) & 0xff) as u8);
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Compute the current 6-digit TOTP for a base32 `secret`. Returns `None` when the
/// secret is not valid base32 (so callers can simply omit the code).
pub fn code(secret: &str, now_secs: u64) -> Option<String> {
    let key = base32_decode(secret)?;
    let counter = now_secs / STEP_SECS;

    let mut mac = HmacSha1::new_from_slice(&key).ok()?;
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();

    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let bin = ((u32::from(digest[offset]) & 0x7f) << 24)
        | ((u32::from(digest[offset + 1]) & 0xff) << 16)
        | ((u32::from(digest[offset + 2]) & 0xff) << 8)
        | (u32::from(digest[offset + 3]) & 0xff);

    let modulo = 10u32.pow(DIGITS);
    Some(format!("{:0width$}", bin % modulo, width = DIGITS as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 test vector (SHA-1, secret "12345678901234567890" -> base32).
    #[test]
    fn rfc6238_vector() {
        // ASCII "12345678901234567890" in base32.
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        // At T = 59s, counter = 1 -> code 94287082.
        assert_eq!(code(secret, 59).unwrap(), "287082");
        // At T = 1111111109s -> code 07081804.
        assert_eq!(code(secret, 1111111109).unwrap(), "081804");
    }

    #[test]
    fn invalid_secret() {
        assert!(code("not base32 !!!", 0).is_none());
        assert!(code("", 0).is_none());
    }
}
