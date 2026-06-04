//! Loopback autofill bridge for the browser extension.
//!
//! Security model (non-negotiable):
//!   * Binds 127.0.0.1 ONLY — never a routable interface, never "cloud".
//!   * The main app is the SOLE holder of the DEK and the SOLE decryptor. This
//!     server reads the live DEK from `AppState`; if the vault is locked every
//!     secret endpoint returns 423 and leaks nothing.
//!   * A random per-session token (written to a 0600 handshake file that only the
//!     OS user can read) authenticates every request, so a hostile local web page
//!     that blindly POSTs to the port is rejected with 401.
//!   * `/fill` re-checks that the requesting origin matches the stored item URL,
//!     so site A can never pull site B's credentials.
//!
//! The native-messaging host (`naravault-host`) is the only intended client: it
//! relays the browser extension's requests here over a raw localhost socket.

use std::path::PathBuf;

use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::crypto;
use crate::db;
use crate::model::Item;
use crate::state::AppState;
use crate::totp;

/// Preferred fixed port (documented in the host manifest / setup guide). If it is
/// already taken we fall back to an OS-assigned ephemeral port and advertise the
/// real one through the handshake file.
const PREFERRED_PORT: u16 = 27432;

fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Generate the per-session bearer token from the OS CSPRNG. Returns `None` if
/// the CSPRNG fails — the caller MUST then refuse to start the bridge rather than
/// fall back to anything predictable (M-C: fail closed, never a guessable token).
fn random_token() -> Option<String> {
    let mut buf = [0u8; 24];
    getrandom::getrandom(&mut buf).ok()?;
    Some(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Reduce a stored URL or an incoming origin to a bare lowercase host, dropping
/// scheme, path, port and a leading `www.`.
fn host_of(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();
    if let Some(idx) = s.find("://") {
        s = s[idx + 3..].to_string();
    }
    // strip path / query / port
    s = s
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_string();
    s.strip_prefix("www.").map(str::to_string).unwrap_or(s)
}

/// Registrable domain (eTLD+1) per the embedded Mozilla Public Suffix List, e.g.
/// `accounts.google.com` -> `google.com`, `bank.co.uk` -> `bank.co.uk`.
fn registrable(host: &str) -> Option<String> {
    psl::domain_str(host).map(str::to_string)
}

/// Does an incoming `origin` correspond to a stored item `url`?
///
/// PSL-aware matching (H-A fix): a page matches a stored login when their hosts
/// are equal OR they share the same registrable domain as defined by the public
/// suffix list. Because the PSL knows `co.uk`/`com.au`/… are public suffixes,
/// `mybank.co.uk` resolves to `mybank.co.uk` (not `co.uk`), so it can never match
/// an unrelated `evil.co.uk`. `accounts.google.com` still matches `google.com`.
fn origin_matches(stored_url: &str, origin: &str) -> bool {
    let stored = host_of(stored_url);
    let req = host_of(origin);
    if stored.is_empty() || req.is_empty() {
        return false;
    }
    if req == stored {
        return true;
    }
    match (registrable(&stored), registrable(&req)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn str_field<'a>(item: &'a Item, key: &str) -> &'a str {
    item.data.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Decrypt every login item (requires the vault unlocked). Returns `None` when
/// locked so callers can answer 423 without distinguishing "empty" from "locked".
fn load_logins(state: &AppState) -> Option<Vec<Item>> {
    if !state.is_unlocked() {
        return None;
    }
    let rows = {
        let conn = state.conn.lock().ok()?;
        db::read_all_items(&conn).ok()?
    };
    state
        .with_dek(|dek| {
            let mut out = Vec::new();
            for row in rows {
                let plaintext = crypto::open(dek, &row.nonce, &row.ciphertext)?;
                let item: Item = serde_json::from_slice(&plaintext)?;
                if item.item_type == "login" {
                    out.push(item);
                }
            }
            Ok(out)
        })
        .ok()
}

fn json_response(status: u16, body: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let data = body.to_string().into_bytes();
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header");
    Response::from_data(data)
        .with_status_code(StatusCode(status))
        .with_header(header)
}

/// Start the bridge: bind loopback, persist the handshake file, and serve on a
/// dedicated thread. Errors are swallowed (logged) — autofill is best-effort and
/// must never take down the main app.
pub fn start(app: tauri::AppHandle, handshake_path: PathBuf) {
    // Fail closed: with no secure token there is no auth, so don't serve at all.
    let token = match random_token() {
        Some(t) => t,
        None => {
            eprintln!("[bridge] CSPRNG unavailable; refusing to start autofill bridge");
            return;
        }
    };

    let server = match Server::http(("127.0.0.1", PREFERRED_PORT)) {
        Ok(s) => s,
        Err(_) => match Server::http(("127.0.0.1", 0)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[bridge] failed to bind loopback: {e}");
                return;
            }
        },
    };

    let port = match server.server_addr() {
        tiny_http::ListenAddr::IP(addr) => addr.port(),
        #[cfg(unix)]
        tiny_http::ListenAddr::Unix(_) => PREFERRED_PORT,
    };

    // Advertise the live port + token to the native host. Written atomically-ish
    // (truncate + write); perms are tightened to owner-only where supported.
    let handshake = json!({ "port": port, "token": token, "pid": std::process::id() });
    if let Err(e) = write_handshake(&handshake_path, &handshake.to_string()) {
        eprintln!("[bridge] failed to write handshake file: {e}");
    }

    std::thread::Builder::new()
        .name("naravault-bridge".into())
        .spawn(move || serve(app, server, token))
        .ok();
}

fn write_handshake(path: &PathBuf, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn serve(app: tauri::AppHandle, server: Server, token: String) {
    use tauri::Manager;
    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or("").to_string();

        // Auth gate: constant-time compare of the session token to avoid a timing
        // oracle that would let a caller recover the token byte-by-byte (M-B).
        let provided = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("X-NaraVault-Token"))
            .map(|h| h.value.as_str().to_string())
            .unwrap_or_default();
        if !bool::from(provided.as_bytes().ct_eq(token.as_bytes())) {
            let _ = request.respond(json_response(401, json!({ "error": "unauthorized" })));
            continue;
        }

        let mut body = String::new();
        if matches!(method, Method::Post) {
            let _ = request.as_reader().read_to_string(&mut body);
        }
        let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);

        let state = app.state::<AppState>();
        let resp = route(&app, &path, &parsed, &state);
        let _ = request.respond(resp);
    }
}

/// Ask the user (in the main window) to approve autofill for `origin`/`item_name`.
/// Blocks the bridge thread until they answer or a timeout/relock cancels it.
/// Returns true only on an explicit approval.
fn request_consent(
    app: &tauri::AppHandle,
    state: &AppState,
    origin_host: &str,
    item_name: &str,
) -> bool {
    use tauri::{Emitter, Manager};

    let (id, rx) = state.register_consent();
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
        let _ = main.emit(
            "naravault://autofill-consent",
            json!({ "id": id, "origin": origin_host, "name": item_name }),
        );
    } else {
        // No window to ask -> fail closed.
        return false;
    }
    // Bounded wait: an ignored or dismissed prompt is treated as a denial.
    matches!(rx.recv_timeout(std::time::Duration::from_secs(30)), Ok(true))
}

fn route(
    app: &tauri::AppHandle,
    path: &str,
    body: &Value,
    state: &AppState,
) -> Response<std::io::Cursor<Vec<u8>>> {
    match path {
        // Liveness + lock state. Lets the extension show "locked" vs "ready"
        // without revealing anything about the contents.
        "/status" => {
            let has_vault = state
                .conn
                .lock()
                .ok()
                .and_then(|c| db::vault_exists(&c).ok())
                .unwrap_or(false);
            json_response(
                200,
                json!({
                    "app": "naravault",
                    "hasVault": has_vault,
                    "locked": !state.is_unlocked(),
                }),
            )
        }

        // Non-secret list of accounts that match the page origin: id + display
        // name + username only. NEVER a password or TOTP.
        "/match" => {
            let origin = body.get("origin").and_then(Value::as_str).unwrap_or("");
            match load_logins(state) {
                None => json_response(423, json!({ "error": "locked" })),
                Some(items) => {
                    let matches: Vec<Value> = items
                        .iter()
                        .filter(|it| origin_matches(str_field(it, "url"), origin))
                        .map(|it| {
                            json!({
                                "id": it.id,
                                "name": it.name,
                                "username": str_field(it, "username"),
                                "hasTotp": !str_field(it, "totp").is_empty(),
                            })
                        })
                        .collect();
                    json_response(200, json!({ "items": matches }))
                }
            }
        }

        // The actual secret hand-off. Requires unlocked + a verified origin match
        // for the requested id, so one site can't exfiltrate another's login.
        "/fill" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let origin = body.get("origin").and_then(Value::as_str).unwrap_or("");
            match load_logins(state) {
                None => json_response(423, json!({ "error": "locked" })),
                Some(items) => match items.iter().find(|it| it.id == id) {
                    None => json_response(404, json!({ "error": "not_found" })),
                    Some(it) => {
                        if !origin_matches(str_field(it, "url"), origin) {
                            return json_response(403, json!({ "error": "origin_mismatch" }));
                        }
                        // M-A: app-side consent before any secret leaves the app.
                        // First use per origin (this unlocked session) prompts the
                        // user; a local process that merely stole the token still
                        // can't dump secrets without the user clicking Allow.
                        let host = host_of(origin);
                        let key = registrable(&host).unwrap_or(host.clone());
                        if !state.is_origin_approved(&key) {
                            if !request_consent(app, state, &host, &it.name) {
                                return json_response(403, json!({ "error": "consent_denied" }));
                            }
                            state.approve_origin(key);
                        }
                        let totp_secret = str_field(it, "totp");
                        let totp_code = if totp_secret.is_empty() {
                            String::new()
                        } else {
                            totp::code(totp_secret, now_secs()).unwrap_or_default()
                        };
                        json_response(
                            200,
                            json!({
                                "username": str_field(it, "username"),
                                "password": str_field(it, "password"),
                                "totp": totp_code,
                            }),
                        )
                    }
                },
            }
        }

        _ => json_response(404, json!({ "error": "not_found" })),
    }
}

#[cfg(test)]
mod tests {
    use super::origin_matches;

    #[test]
    fn exact_and_subdomain_match() {
        assert!(origin_matches("github.com", "https://github.com/login"));
        assert!(origin_matches("github.com", "https://www.github.com/login"));
        assert!(origin_matches("google.com", "https://accounts.google.com/signin"));
        assert!(origin_matches("https://accounts.google.com", "https://mail.google.com"));
    }

    #[test]
    fn h_a_public_suffix_isolation() {
        // The original bug: any two hosts sharing a multi-label public suffix
        // collapsed to the same 2-label "registrable" and matched. With the PSL,
        // each resolves to its own eTLD+1, so these must NOT match.
        assert!(!origin_matches("mybank.co.uk", "https://evil.co.uk/login"));
        assert!(!origin_matches("shop.com.au", "https://phish.com.au"));
        assert!(!origin_matches("github.com", "https://github.evil.com"));
        assert!(!origin_matches("github.com", "https://notgithub.com"));
    }

    #[test]
    fn empty_never_matches() {
        assert!(!origin_matches("", "https://github.com"));
        assert!(!origin_matches("github.com", ""));
    }
}
