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

use std::io::Read;
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

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Fresh NaraVault-format primary key ("i_" + 8 random hex chars), matching the
/// frontend's `newId()`. The id is the only plaintext field stored, carries no
/// secret, and is generated app-side so the extension can never overwrite an
/// existing item by id (a /create always inserts a brand-new row).
fn random_id() -> String {
    let mut buf = [0u8; 4];
    if getrandom::getrandom(&mut buf).is_err() {
        return format!("i_{:08x}", now_secs() as u32);
    }
    format!(
        "i_{}",
        buf.iter().map(|b| format!("{:02x}", b)).collect::<String>()
    )
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

/// Decrypt every vault item (requires the vault unlocked). Returns `None` when
/// locked so callers can answer 423 without distinguishing "empty" from "locked".
fn load_all_items(state: &AppState) -> Option<Vec<Item>> {
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
                out.push(item);
            }
            Ok(out)
        })
        .ok()
}

/// Whether the desktop app should pop a per-fill consent prompt for autofill.
/// Default `true` (the secure baseline). When the user opts out in Settings we
/// store an encrypted `"0"`, letting autofill proceed silently while the vault
/// is unlocked. This now covers BOTH logins (origin still must match) and cards
/// (user explicitly opted in to loosen high-value fills too).
fn autofill_prompt_enabled(state: &AppState) -> bool {
    let row = {
        let conn = match state.conn.lock() {
            Ok(c) => c,
            Err(_) => return true,
        };
        match db::load_setting(&conn, crate::commands::AUTOFILL_PROMPT_KEY) {
            Ok(Some(r)) => r,
            _ => return true, // unset → secure default (prompt ON)
        }
    };
    state
        .with_dek(|dek| {
            let plain = crypto::open(dek, &row.0, &row.1)?;
            Ok(plain != b"0")
        })
        .unwrap_or(true)
}

/// Just the login items, for the origin-matched autofill list.
fn load_logins(state: &AppState) -> Option<Vec<Item>> {
    Some(
        load_all_items(state)?
            .into_iter()
            .filter(|it| it.item_type == "login")
            .collect(),
    )
}

/// Last 4 digits of a card number (digits only). Shown for display/disambiguation;
/// the full PAN never leaves the app except via a consent-gated /fill.
fn last4(num: &str) -> String {
    let digits: String = num.chars().filter(char::is_ascii_digit).collect();
    let n = digits.len();
    if n >= 4 {
        digits[n - 4..].to_string()
    } else {
        digits
    }
}

fn data_str<'a>(data: &'a Value, key: &str) -> &'a str {
    data.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Default display name when the user leaves the name blank (mirrors the desktop
/// `defaultName()` in ItemForm.svelte).
fn default_name(item_type: &str, data: &Value) -> String {
    match item_type {
        "login" => {
            let url = data_str(data, "url");
            if !url.is_empty() {
                return url.to_string();
            }
            let user = data_str(data, "username");
            if !user.is_empty() {
                user.to_string()
            } else {
                "Login".to_string()
            }
        }
        "card" => {
            let brand = data_str(data, "brand");
            let base = if brand.is_empty() {
                "Card".to_string()
            } else {
                brand.to_string()
            };
            let l4 = last4(data_str(data, "number"));
            if l4.is_empty() {
                base
            } else {
                format!("{base} •• {l4}")
            }
        }
        _ => "Item".to_string(),
    }
}

/// Subtitle (`sub`) for an item, mirroring the desktop `subFor()`.
fn sub_for(item_type: &str, data: &Value) -> String {
    match item_type {
        "login" => {
            let user = data_str(data, "username");
            if !user.is_empty() {
                user.to_string()
            } else {
                data_str(data, "url").to_string()
            }
        }
        "card" => data_str(data, "holder").to_string(),
        _ => String::new(),
    }
}

/// Encrypt `item` under the live DEK and upsert it. Returns false on any failure
/// (locked / lock poisoned / db error) so the caller can answer 5xx/423.
fn persist_item(state: &AppState, item: &Item) -> bool {
    let plaintext = match serde_json::to_vec(item) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let sealed = match state.with_dek(|dek| crypto::seal(dek, &plaintext)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    match state.conn.lock() {
        Ok(conn) => {
            db::upsert_item(&conn, &item.id, &sealed.nonce, &sealed.ciphertext, now_ms()).is_ok()
        }
        Err(_) => false,
    }
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
            // Cap the body before buffering: every endpoint's payload is tiny
            // (a few small fields), so a 32 KiB ceiling bounds memory even though
            // the socket is already loopback + token-gated.
            const MAX_BODY: u64 = 32 * 1024;
            let _ = request
                .as_reader()
                .take(MAX_BODY)
                .read_to_string(&mut body);
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
    kind: &str,
) -> bool {
    use tauri::{Emitter, Manager};

    let (id, rx) = state.register_consent();
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
        let _ = main.emit(
            "naravault://autofill-consent",
            json!({ "id": id, "origin": origin_host, "name": item_name, "kind": kind }),
        );
    } else {
        // No window to ask -> fail closed.
        return false;
    }
    // Bounded wait: an ignored or dismissed prompt is treated as a denial.
    matches!(rx.recv_timeout(std::time::Duration::from_secs(30)), Ok(true))
}

/// Ask the user (in the main window) to approve SAVING a new login that the
/// extension wants to write for `origin_host`. Unlike autofill consent this is
/// never cached: every write prompts, so a local process holding the token can
/// not silently pollute the vault. Returns true only on an explicit approval.
fn request_save_consent(
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
            "naravault://autofill-save-consent",
            json!({ "id": id, "origin": origin_host, "name": item_name }),
        );
    } else {
        return false;
    }
    // Give the user a bit longer for a write decision; a dismissed prompt denies.
    matches!(rx.recv_timeout(std::time::Duration::from_secs(60)), Ok(true))
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

        // Non-secret login list, split by the page origin:
        //   * `items`  — logins whose stored URL matches the page (autofillable here)
        //   * `others` — every other login in the vault (shown for visibility /
        //                editing; cross-origin fill stays blocked by /fill).
        // id + display name + username only — NEVER a password or TOTP.
        "/match" => {
            let origin = body.get("origin").and_then(Value::as_str).unwrap_or("");
            match load_logins(state) {
                None => json_response(423, json!({ "error": "locked" })),
                Some(items) => {
                    let project = |it: &Item| {
                        json!({
                            "id": it.id,
                            "name": it.name,
                            "username": str_field(it, "username"),
                            "hasTotp": !str_field(it, "totp").is_empty(),
                        })
                    };
                    let mut matched = Vec::new();
                    let mut others = Vec::new();
                    for it in &items {
                        if !origin.is_empty() && origin_matches(str_field(it, "url"), origin) {
                            matched.push(project(it));
                        } else {
                            others.push(project(it));
                        }
                    }
                    json_response(200, json!({ "items": matched, "others": others }))
                }
            }
        }

        // Non-secret list of ALL cards (no origin binding — cards aren't tied to a
        // site). id + display name + brand + last4 + holder + expiry only. NEVER
        // the full PAN or CVV.
        "/cards" => match load_all_items(state) {
            None => json_response(423, json!({ "error": "locked" })),
            Some(items) => {
                let cards: Vec<Value> = items
                    .iter()
                    .filter(|it| it.item_type == "card")
                    .map(|it| {
                        json!({
                            "id": it.id,
                            "name": it.name,
                            "brand": str_field(it, "brand"),
                            "last4": last4(str_field(it, "number")),
                            "holder": str_field(it, "holder"),
                            "expiry": str_field(it, "expiry"),
                        })
                    })
                    .collect();
                json_response(200, json!({ "items": cards }))
            }
        },

        // Non-secret projection of ONE item, used to pre-fill the edit form. Never
        // returns a password / TOTP / PAN / CVV — only `hasX` flags so the form can
        // show a "leave blank to keep" placeholder for secret fields.
        "/item" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            match load_all_items(state) {
                None => json_response(423, json!({ "error": "locked" })),
                Some(items) => match items.iter().find(|it| it.id == id) {
                    None => json_response(404, json!({ "error": "not_found" })),
                    Some(it) => match it.item_type.as_str() {
                        "login" => json_response(
                            200,
                            json!({
                                "type": "login",
                                "name": it.name,
                                "url": str_field(it, "url"),
                                "username": str_field(it, "username"),
                                "hasPassword": !str_field(it, "password").is_empty(),
                                "hasTotp": !str_field(it, "totp").is_empty(),
                            }),
                        ),
                        "card" => json_response(
                            200,
                            json!({
                                "type": "card",
                                "name": it.name,
                                "holder": str_field(it, "holder"),
                                "expiry": str_field(it, "expiry"),
                                "brand": str_field(it, "brand"),
                                "last4": last4(str_field(it, "number")),
                                "hasNumber": !str_field(it, "number").is_empty(),
                                "hasCvv": !str_field(it, "cvv").is_empty(),
                            }),
                        ),
                        _ => json_response(404, json!({ "error": "unsupported_type" })),
                    },
                },
            }
        }

        // The actual secret hand-off. Requires unlocked + consent.
        //   * login: origin must match the stored URL (one site can't pull
        //     another's login); consent is cached per-origin for the session.
        //   * card: no origin binding, so consent is ALWAYS prompted (uncached) —
        //     a PAN/CVV is high value and shouldn't fill silently after one Allow.
        "/fill" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let origin = body.get("origin").and_then(Value::as_str).unwrap_or("");
            let items = match load_all_items(state) {
                None => return json_response(423, json!({ "error": "locked" })),
                Some(v) => v,
            };
            let it = match items.iter().find(|it| it.id == id) {
                None => return json_response(404, json!({ "error": "not_found" })),
                Some(it) => it,
            };
            match it.item_type.as_str() {
                "login" => {
                    if !origin_matches(str_field(it, "url"), origin) {
                        return json_response(403, json!({ "error": "origin_mismatch" }));
                    }
                    let host = host_of(origin);
                    let key = registrable(&host).unwrap_or(host.clone());
                    // Per-fill consent is the secure default but can be disabled in
                    // Settings. When disabled we still require the origin to match
                    // the stored URL and the vault to be unlocked — we just skip the
                    // interactive prompt.
                    if autofill_prompt_enabled(state) && !state.is_origin_approved(&key) {
                        if !request_consent(app, state, &host, &it.name, "login") {
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
                            "type": "login",
                            "username": str_field(it, "username"),
                            "password": str_field(it, "password"),
                            "totp": totp_code,
                        }),
                    )
                }
                "card" => {
                    // Cards honor the same Settings toggle as logins. When the
                    // prompt is enabled (default) we ask uncached every time;
                    // when the user has opted out, card fills proceed silently
                    // while unlocked.
                    if autofill_prompt_enabled(state) {
                        let host = host_of(origin);
                        if !request_consent(app, state, &host, &it.name, "card") {
                            return json_response(403, json!({ "error": "consent_denied" }));
                        }
                    }
                    json_response(
                        200,
                        json!({
                            "type": "card",
                            "number": str_field(it, "number"),
                            "holder": str_field(it, "holder"),
                            "expiry": str_field(it, "expiry"),
                            "cvv": str_field(it, "cvv"),
                            "brand": str_field(it, "brand"),
                        }),
                    )
                }
                _ => json_response(404, json!({ "error": "not_found" })),
            }
        }

        // Write path — INSERT a new login or card. Guards:
        //   * 423 if locked.
        //   * ALWAYS prompts the user in the app (uncached) before writing.
        //   * Server generates a fresh id, so this can only INSERT, never clobber.
        //   * No secret is read back to the extension.
        "/create" => {
            if !state.is_unlocked() {
                return json_response(423, json!({ "error": "locked" }));
            }
            let origin = body.get("origin").and_then(Value::as_str).unwrap_or("");
            let item_type = body
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("login")
                .to_string();
            let g = |k: &str| {
                body.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            let name_in = g("name").trim().to_string();
            if name_in.len() > 200 {
                return json_response(400, json!({ "error": "too_large" }));
            }

            let data = match item_type.as_str() {
                "login" => {
                    let password = g("password");
                    if password.is_empty() {
                        return json_response(400, json!({ "error": "invalid_input" }));
                    }
                    let username = g("username");
                    let mut url = g("url").trim().to_string();
                    if url.is_empty() {
                        url = origin.trim().to_string();
                    }
                    let totp = g("totp").trim().to_string();
                    if username.len() > 400
                        || password.len() > 2000
                        || url.len() > 2000
                        || totp.len() > 512
                    {
                        return json_response(400, json!({ "error": "too_large" }));
                    }
                    json!({ "username": username, "password": password, "url": url, "totp": totp, "notes": "" })
                }
                "card" => {
                    let number = g("number");
                    if number.is_empty() {
                        return json_response(400, json!({ "error": "invalid_input" }));
                    }
                    let holder = g("holder");
                    let expiry = g("expiry").trim().to_string();
                    let cvv = g("cvv").trim().to_string();
                    let brand = g("brand").trim().to_string();
                    if number.len() > 40
                        || holder.len() > 200
                        || expiry.len() > 16
                        || cvv.len() > 8
                        || brand.len() > 40
                    {
                        return json_response(400, json!({ "error": "too_large" }));
                    }
                    json!({ "holder": holder, "number": number, "expiry": expiry, "cvv": cvv, "brand": brand, "notes": "" })
                }
                _ => return json_response(400, json!({ "error": "unsupported_type" })),
            };

            let name = if name_in.is_empty() {
                default_name(&item_type, &data)
            } else {
                name_in
            };
            let host = host_of(origin);
            if !request_save_consent(app, state, &host, &name) {
                return json_response(403, json!({ "error": "consent_denied" }));
            }
            let item = Item {
                id: random_id(),
                item_type: item_type.clone(),
                name,
                sub: sub_for(&item_type, &data),
                fav: false,
                data,
            };
            if !persist_item(state, &item) {
                return json_response(500, json!({ "error": "save_failed" }));
            }
            {
                use tauri::Emitter;
                let _ = app.emit("naravault://vault-changed", ());
            }
            json_response(200, json!({ "ok": true, "id": item.id }))
        }

        // Write path — UPDATE an existing login or card by id. The existing item is
        // decrypted server-side and merged: non-secret fields are overwritten from
        // the request, but secret fields (password/totp, number/cvv) are kept
        // UNLESS the request supplies a non-empty replacement. The extension never
        // sees the old secret, and a blank secret field means "leave unchanged".
        "/update" => {
            if !state.is_unlocked() {
                return json_response(423, json!({ "error": "locked" }));
            }
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            if id.is_empty() {
                return json_response(400, json!({ "error": "invalid_input" }));
            }
            let origin = body.get("origin").and_then(Value::as_str).unwrap_or("");
            let g = |k: &str| {
                body.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };

            let mut items = match load_all_items(state) {
                None => return json_response(423, json!({ "error": "locked" })),
                Some(v) => v,
            };
            let idx = match items.iter().position(|it| it.id == id) {
                None => return json_response(404, json!({ "error": "not_found" })),
                Some(i) => i,
            };
            let mut item = items.swap_remove(idx);
            let mut data = item
                .data
                .as_object()
                .cloned()
                .unwrap_or_default();
            let set_secret_if_present = |data: &mut serde_json::Map<String, Value>, key: &str, v: String| {
                if !v.is_empty() {
                    data.insert(key.to_string(), Value::String(v));
                }
            };

            match item.item_type.as_str() {
                "login" => {
                    let url = g("url");
                    let username = g("username");
                    if url.len() > 2000 || username.len() > 400 {
                        return json_response(400, json!({ "error": "too_large" }));
                    }
                    data.insert("url".into(), Value::String(url));
                    data.insert("username".into(), Value::String(username));
                    let pw = g("password");
                    let totp = g("totp").trim().to_string();
                    if pw.len() > 2000 || totp.len() > 512 {
                        return json_response(400, json!({ "error": "too_large" }));
                    }
                    set_secret_if_present(&mut data, "password", pw);
                    set_secret_if_present(&mut data, "totp", totp);
                }
                "card" => {
                    let holder = g("holder");
                    let expiry = g("expiry").trim().to_string();
                    let brand = g("brand").trim().to_string();
                    if holder.len() > 200 || expiry.len() > 16 || brand.len() > 40 {
                        return json_response(400, json!({ "error": "too_large" }));
                    }
                    data.insert("holder".into(), Value::String(holder));
                    data.insert("expiry".into(), Value::String(expiry));
                    data.insert("brand".into(), Value::String(brand));
                    let number = g("number");
                    let cvv = g("cvv").trim().to_string();
                    if number.len() > 40 || cvv.len() > 8 {
                        return json_response(400, json!({ "error": "too_large" }));
                    }
                    set_secret_if_present(&mut data, "number", number);
                    set_secret_if_present(&mut data, "cvv", cvv);
                }
                _ => return json_response(400, json!({ "error": "unsupported_type" })),
            }

            let new_data = Value::Object(data);
            let item_type = item.item_type.clone();
            let name_in = g("name").trim().to_string();
            if name_in.len() > 200 {
                return json_response(400, json!({ "error": "too_large" }));
            }
            let name = if name_in.is_empty() {
                default_name(&item_type, &new_data)
            } else {
                name_in
            };
            let host = host_of(origin);
            if !request_save_consent(app, state, &host, &name) {
                return json_response(403, json!({ "error": "consent_denied" }));
            }
            item.name = name;
            item.sub = sub_for(&item_type, &new_data);
            item.data = new_data;
            if !persist_item(state, &item) {
                return json_response(500, json!({ "error": "save_failed" }));
            }
            {
                use tauri::Emitter;
                let _ = app.emit("naravault://vault-changed", ());
            }
            json_response(200, json!({ "ok": true, "id": item.id }))
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
