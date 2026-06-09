// NaraVault Autofill — background service worker.
//
// The ONLY component allowed to talk to the native-messaging host. It is a pure
// relay: content scripts and the popup send a {cmd,...} request; we translate it
// into a native message, forward it to `com.naravault.host`, and return the
// reply. No secrets are stored here — every response is passed straight back to
// the caller for immediate use.

const HOST_NAME = "com.naravault.host";

/**
 * Send one request to the native host and resolve with its parsed reply.
 * Resolves to { ok:false, error:"app_not_running" } when the host can't be
 * reached (app closed, host not installed) instead of rejecting.
 */
function callHost(message) {
  return new Promise((resolve) => {
    let settled = false;
    try {
      chrome.runtime.sendNativeMessage(HOST_NAME, message, (reply) => {
        if (settled) return;
        settled = true;
        if (chrome.runtime.lastError || !reply) {
          resolve({ ok: false, error: "app_not_running" });
          return;
        }
        resolve(reply);
      });
    } catch (_e) {
      if (!settled) {
        settled = true;
        resolve({ ok: false, error: "app_not_running" });
      }
    }
  });
}

// Normalize the various host replies into a flat shape the UI can switch on.
function interpret(reply) {
  if (!reply || reply.ok === false) {
    return { state: reply && reply.error === "app_not_running" ? "offline" : "error" };
  }
  if (reply.status === 423) return { state: "locked" };
  if (reply.status >= 200 && reply.status < 300) {
    return { state: "ok", body: reply.body || {} };
  }
  if (reply.status === 403) return { state: "origin_mismatch" };
  if (reply.status === 404) return { state: "not_found" };
  return { state: "error" };
}

// Collect the write fields shared by create + update. `itemType` is mapped to
// the bridge body's `type` by the native host (the message-level `type` is the
// host command name, so the item type travels under a different key).
function buildWriteFields(msg) {
  return {
    itemType: msg.itemType || "login",
    origin: msg.origin || "",
    name: msg.name || "",
    username: msg.username || "",
    password: msg.password || "",
    url: msg.url || "",
    totp: msg.totp || "",
    holder: msg.holder || "",
    number: msg.number || "",
    expiry: msg.expiry || "",
    cvv: msg.cvv || "",
    brand: msg.brand || "",
  };
}

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  (async () => {
    switch (msg && msg.cmd) {
      case "status": {
        sendResponse(interpret(await callHost({ type: "status" })));
        break;
      }
      case "match": {
        sendResponse(interpret(await callHost({ type: "match", origin: msg.origin })));
        break;
      }
      case "fill": {
        sendResponse(
          interpret(await callHost({ type: "fill", id: msg.id, origin: msg.origin })),
        );
        break;
      }
      case "cards": {
        // Non-secret list of all cards (no PAN / CVV).
        sendResponse(interpret(await callHost({ type: "cards" })));
        break;
      }
      case "item": {
        // Non-secret fields of one item, to pre-fill the edit form.
        sendResponse(interpret(await callHost({ type: "item", id: msg.id })));
        break;
      }
      case "create": {
        // INSERT a new login or card. The app re-prompts the user before
        // persisting; we just relay the fields. No secret is ever read back.
        sendResponse(
          interpret(await callHost({ type: "create", ...buildWriteFields(msg) })),
        );
        break;
      }
      case "update": {
        // UPDATE an existing item by id. Blank secret fields = keep current.
        sendResponse(
          interpret(
            await callHost({ type: "update", id: msg.id, ...buildWriteFields(msg) }),
          ),
        );
        break;
      }
      default:
        sendResponse({ state: "error" });
    }
  })();
  // Keep the message channel open for the async reply.
  return true;
});
