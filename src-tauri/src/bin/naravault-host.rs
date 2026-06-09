//! NaraVault native-messaging host.
//!
//! A deliberately *thin* relay. The browser launches this binary and speaks the
//! Chrome/Firefox native-messaging framing over stdio (4-byte little-endian
//! length prefix + UTF-8 JSON). For each message we forward the request to the
//! running NaraVault app's loopback bridge (127.0.0.1, port + token discovered
//! from the handshake file) and stream the reply back to the extension.
//!
//! This process NEVER holds the DEK, NEVER decrypts, and CANNOT unlock the vault.
//! If the app is closed or locked the bridge answers accordingly and autofill is
//! simply unavailable. Uses std + serde_json only — no Tauri, no network crates.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

const APP_IDENTIFIER: &str = "dev.naravault.vault";
const HANDSHAKE_FILE: &str = "bridge.json";

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    loop {
        let msg = match read_message(&mut reader) {
            Ok(Some(v)) => v,
            Ok(None) => break, // browser closed the port
            Err(_) => break,
        };

        let reply = handle(&msg);

        if write_message(&mut writer, &reply).is_err() {
            break;
        }
    }
}

/// Read one native-messaging frame. `Ok(None)` signals a clean EOF.
fn read_message<R: Read>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    // Guard against absurd sizes (Chrome caps host-bound messages well below this).
    if len > 64 * 1024 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    let value = serde_json::from_slice(&buf)
        .unwrap_or_else(|_| json!({ "type": "invalid" }));
    Ok(Some(value))
}

fn write_message<W: Write>(writer: &mut W, value: &Value) -> io::Result<()> {
    let bytes = value.to_string().into_bytes();
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

/// Map an extension request to a bridge call and wrap the result. Transport-level
/// problems become a structured `{ ok:false, error:... }` rather than a crash.
fn handle(msg: &Value) -> Value {
    let kind = msg.get("type").and_then(Value::as_str).unwrap_or("");

    let (port, token) = match read_handshake() {
        Some(h) => h,
        None => return json!({ "ok": false, "error": "app_not_running" }),
    };

    // Endpoints that may park on a user-consent prompt in the app need a generous
    // read timeout; pure metadata calls fail fast so a hung app doesn't stall the
    // popup. (timeout in seconds)
    let (path, body, timeout): (&str, Value, u64) = match kind {
        "status" => ("/status", json!({}), 5),
        "match" => (
            "/match",
            json!({ "origin": msg.get("origin").and_then(Value::as_str).unwrap_or("") }),
            5,
        ),
        "fill" => (
            "/fill",
            json!({
                "id": msg.get("id").and_then(Value::as_str).unwrap_or(""),
                "origin": msg.get("origin").and_then(Value::as_str).unwrap_or(""),
            }),
            35,
        ),
        "cards" => ("/cards", json!({}), 5),
        "item" => (
            "/item",
            json!({ "id": msg.get("id").and_then(Value::as_str).unwrap_or("") }),
            5,
        ),
        "create" => (
            "/create",
            json!({
                "type": msg.get("itemType").and_then(Value::as_str).unwrap_or("login"),
                "origin": msg.get("origin").and_then(Value::as_str).unwrap_or(""),
                "name": msg.get("name").and_then(Value::as_str).unwrap_or(""),
                "username": msg.get("username").and_then(Value::as_str).unwrap_or(""),
                "password": msg.get("password").and_then(Value::as_str).unwrap_or(""),
                "url": msg.get("url").and_then(Value::as_str).unwrap_or(""),
                "totp": msg.get("totp").and_then(Value::as_str).unwrap_or(""),
                "holder": msg.get("holder").and_then(Value::as_str).unwrap_or(""),
                "number": msg.get("number").and_then(Value::as_str).unwrap_or(""),
                "expiry": msg.get("expiry").and_then(Value::as_str).unwrap_or(""),
                "cvv": msg.get("cvv").and_then(Value::as_str).unwrap_or(""),
                "brand": msg.get("brand").and_then(Value::as_str).unwrap_or(""),
            }),
            65,
        ),
        "update" => (
            "/update",
            json!({
                "id": msg.get("id").and_then(Value::as_str).unwrap_or(""),
                "origin": msg.get("origin").and_then(Value::as_str).unwrap_or(""),
                "name": msg.get("name").and_then(Value::as_str).unwrap_or(""),
                "username": msg.get("username").and_then(Value::as_str).unwrap_or(""),
                "password": msg.get("password").and_then(Value::as_str).unwrap_or(""),
                "url": msg.get("url").and_then(Value::as_str).unwrap_or(""),
                "totp": msg.get("totp").and_then(Value::as_str).unwrap_or(""),
                "holder": msg.get("holder").and_then(Value::as_str).unwrap_or(""),
                "number": msg.get("number").and_then(Value::as_str).unwrap_or(""),
                "expiry": msg.get("expiry").and_then(Value::as_str).unwrap_or(""),
                "cvv": msg.get("cvv").and_then(Value::as_str).unwrap_or(""),
                "brand": msg.get("brand").and_then(Value::as_str).unwrap_or(""),
            }),
            65,
        ),
        _ => return json!({ "ok": false, "error": "unknown_request" }),
    };

    match post(port, &token, path, &body.to_string(), timeout) {
        Ok((status, body)) => {
            let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            json!({ "ok": status >= 200 && status < 300, "status": status, "body": parsed })
        }
        Err(_) => json!({ "ok": false, "error": "app_not_running" }),
    }
}

/// Resolve the app data directory the same way Tauri's `app_data_dir()` does, then
/// read `{ port, token }` from the handshake file the running app wrote.
fn read_handshake() -> Option<(u16, String)> {
    let path = handshake_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let port = v.get("port").and_then(Value::as_u64)? as u16;
    let token = v.get("token").and_then(Value::as_str)?.to_string();
    Some((port, token))
}

fn handshake_path() -> Option<PathBuf> {
    let base = app_data_dir()?;
    Some(base.join(APP_IDENTIFIER).join(HANDSHAKE_FILE))
}

#[cfg(target_os = "windows")]
fn app_data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn app_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn app_data_dir() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(x));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
}

/// Minimal HTTP/1.1 POST over a localhost TCP socket. Returns (status, body).
fn post(port: u16, token: &str, path: &str, body: &str, timeout: u64) -> io::Result<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(timeout)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         X-NaraVault-Token: {token}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        len = body.len(),
    );
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw);

    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("");
    let body = parts.next().unwrap_or("").to_string();

    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);

    Ok((status, body))
}
