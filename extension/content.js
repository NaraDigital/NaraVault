// NaraVault Autofill — content script.
//
// Detects login forms, shows a small in-page chooser of matching accounts, and
// fills credentials + TOTP only on an explicit user click (never automatically,
// to avoid clickjacking / silent harvesting). All secrets are requested
// on-demand from the background relay and used immediately — nothing is cached.

(() => {
  "use strict";

  const ORIGIN = location.origin;
  const ICON_URL = chrome.runtime.getURL("icons/icon128.png");
  let menuHost = null; // shadow-DOM overlay element
  const decorated = new WeakSet();

  /* ---------------------------- messaging ---------------------------- */

  function send(cmd, extra) {
    return new Promise((resolve) => {
      try {
        chrome.runtime.sendMessage({ cmd, origin: ORIGIN, ...extra }, (resp) => {
          if (chrome.runtime.lastError || !resp) {
            resolve({ state: "offline" });
            return;
          }
          resolve(resp);
        });
      } catch (_e) {
        resolve({ state: "offline" });
      }
    });
  }

  /* ------------------------- field detection ------------------------- */

  function isVisible(el) {
    if (!el) return false;
    const r = el.getBoundingClientRect();
    if (r.width < 8 || r.height < 8) return false;
    const cs = getComputedStyle(el);
    return cs.visibility !== "hidden" && cs.display !== "none";
  }

  function passwordFields() {
    return [...document.querySelectorAll('input[type="password"]')].filter(isVisible);
  }

  // Find the username/email field associated with a password field: prefer inputs
  // in the same form, otherwise scan the document, picking the closest text/email
  // input that appears before the password.
  function usernameFor(pwEl) {
    const scope = pwEl.form || document;
    const candidates = [...scope.querySelectorAll("input")].filter((el) => {
      const t = (el.getAttribute("type") || "text").toLowerCase();
      return (
        isVisible(el) &&
        ["text", "email", "tel", ""].includes(t) &&
        el !== pwEl
      );
    });
    if (candidates.length === 0) return null;
    // Heuristic: the last candidate positioned before the password element.
    let best = null;
    for (const el of candidates) {
      const rel = pwEl.compareDocumentPosition(el);
      if (rel & Node.DOCUMENT_POSITION_PRECEDING) best = el;
    }
    return best || candidates[0];
  }

  function totpField() {
    const all = [...document.querySelectorAll("input")].filter(isVisible);
    return (
      all.find((el) => (el.getAttribute("autocomplete") || "").toLowerCase() === "one-time-code") ||
      all.find((el) => {
        const hay = `${el.name} ${el.id} ${el.getAttribute("aria-label") || ""}`.toLowerCase();
        return /(otp|totp|2fa|mfa|one[-_ ]?time|auth.?code|verification)/.test(hay);
      }) ||
      null
    );
  }

  /* ------------------------------ filling ---------------------------- */

  // Set a value in a way frameworks (React/Vue) notice.
  function setValue(el, value) {
    if (!el || value == null) return;
    const proto = el instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    el.focus();
    if (setter) setter.call(el, value);
    else el.value = value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  // Like setValue but also handles <select> (expiry month/year dropdowns), where
  // the native value setter doesn't apply.
  function setAnyValue(el, value) {
    if (!el || value == null) return;
    if (el.tagName === "SELECT") {
      el.focus();
      el.value = String(value);
      if (el.value !== String(value)) {
        const want = String(value).toLowerCase();
        const opt = [...el.options].find(
          (o) =>
            o.value.toLowerCase() === want ||
            o.text.trim().toLowerCase() === want ||
            o.value.toLowerCase().endsWith(want),
        );
        if (opt) el.value = opt.value;
      }
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return;
    }
    setValue(el, value);
  }

  async function fillItem(id, anchorPw) {
    const resp = await send("fill", { id });
    if (resp.state !== "ok") {
      toast(messageFor(resp.state));
      return;
    }
    const { username, password, totp } = resp.body;
    const pw = anchorPw || passwordFields()[0];
    if (pw) setValue(pw, password);
    const userEl = pw ? usernameFor(pw) : null;
    if (userEl && username) setValue(userEl, username);

    if (totp) {
      const otp = totpField();
      if (otp) {
        setValue(otp, totp);
        toast("Filled login + TOTP");
      } else {
        try {
          await navigator.clipboard.writeText(totp);
          toast("Login filled · TOTP copied to clipboard");
        } catch (_e) {
          toast("Login filled");
        }
      }
    } else {
      toast("Login filled");
    }
  }

  /* ----------------------- credit-card detection --------------------- */

  function acToken(el) {
    return (el.getAttribute("autocomplete") || "").toLowerCase();
  }
  function fieldHay(el) {
    return `${el.name || ""} ${el.id || ""} ${el.getAttribute("placeholder") || ""} ${
      el.getAttribute("aria-label") || ""
    }`.toLowerCase();
  }

  // Locate the credit-card fields on the page (autocomplete tokens first, then
  // name/id/placeholder heuristics). Any field may be absent.
  function detectCardFields() {
    const inputs = [...document.querySelectorAll("input, select")].filter(isVisible);
    const byAc = (tok) => inputs.find((el) => acToken(el).includes(tok));
    const byRe = (re) => inputs.find((el) => re.test(fieldHay(el)));

    const number =
      byAc("cc-number") || byRe(/card.?number|cardnum|ccnum|credit.?card|\bpan\b/);
    const name =
      byAc("cc-name") || byAc("cc-given-name") || byRe(/card.?holder|name.?on.?card|cc.?name/);
    const cvv = byAc("cc-csc") || byRe(/cvv|cvc|csc|security.?code|card.?code/);
    const expMonth = byAc("cc-exp-month") || byRe(/exp.?month|expmonth|exp.?mm/);
    const expYear = byAc("cc-exp-year") || byRe(/exp.?year|expyear|exp.?yy/);
    let expFull = byAc("cc-exp") || byRe(/(card.?)?exp(iry|iration)?(.?date)?/);
    if (expFull && (expFull === expMonth || expFull === expYear)) expFull = null;
    return { number, name, cvv, expFull, expMonth, expYear };
  }

  function hasCardForm() {
    const f = detectCardFields();
    return !!(f.number || f.expFull || (f.cvv && f.name));
  }

  async function fillCard(id) {
    if (!hasCardForm()) {
      toast("No card fields detected on this page");
      return;
    }
    const resp = await send("fill", { id });
    if (resp.state !== "ok") {
      toast(messageFor(resp.state));
      return;
    }
    const c = resp.body;
    if (!c || c.type !== "card") {
      toast("Couldn't fill card");
      return;
    }
    const f = detectCardFields();
    const digits = String(c.number || "").replace(/\D/g, "");
    if (f.number) setAnyValue(f.number, digits);
    if (f.name && c.holder) setAnyValue(f.name, c.holder);
    if (f.cvv && c.cvv) setAnyValue(f.cvv, c.cvv);

    const m = String(c.expiry || "").match(/(\d{1,2})\s*\/\s*(\d{2,4})/);
    if (f.expFull) {
      setAnyValue(f.expFull, c.expiry || "");
    } else if (m) {
      const mm = m[1].padStart(2, "0");
      const yyRaw = m[2];
      const yy = yyRaw.length === 4 ? yyRaw.slice(-2) : yyRaw;
      const yyyy = yyRaw.length === 2 ? "20" + yyRaw : yyRaw;
      if (f.expMonth) setAnyValue(f.expMonth, mm);
      if (f.expYear) {
        // Try 4-digit first (common for <select>), fall back to 2-digit.
        setAnyValue(f.expYear, yyyy);
        if (!f.expYear.value) setAnyValue(f.expYear, yy);
      }
    }
    toast("Card filled");
  }

  /* --------------------------- chooser menu -------------------------- */

  function closeMenu() {
    if (menuHost) {
      menuHost.remove();
      menuHost = null;
    }
  }

  function toast(text) {
    const host = document.createElement("div");
    const shadow = host.attachShadow({ mode: "closed" });
    shadow.innerHTML = `
      <style>
        .t{position:fixed;z-index:2147483647;right:18px;bottom:18px;background:#1c1d22;color:#fff;
           font:500 13px/1.4 system-ui,sans-serif;padding:10px 14px;border-radius:10px;
           box-shadow:0 8px 30px rgba(0,0,0,.35);max-width:280px}
      </style>
      <div class="t">${escapeHtml(text)}</div>`;
    document.documentElement.appendChild(host);
    setTimeout(() => host.remove(), 2600);
  }

  function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]),
    );
  }

  async function openMenu(anchorEl) {
    closeMenu();
    const resp = await send("match", {});

    menuHost = document.createElement("div");
    const shadow = menuHost.attachShadow({ mode: "closed" });
    const rect = anchorEl.getBoundingClientRect();
    const top = window.scrollY + rect.bottom + 6;
    const left = window.scrollX + rect.left;

    let inner;
    if (resp.state === "ok" && resp.body.items && resp.body.items.length) {
      inner = resp.body.items
        .map(
          (it) => `
          <button class="row" data-id="${escapeHtml(it.id)}">
            <span class="name">${escapeHtml(it.name || "Login")}</span>
            <span class="user">${escapeHtml(it.username || "")}${it.hasTotp ? " · TOTP" : ""}</span>
          </button>`,
        )
        .join("");
    } else {
      inner = `<div class="empty">${escapeHtml(messageFor(resp.state, true))}</div>`;
    }

    shadow.innerHTML = `
      <style>
        .panel{position:absolute;z-index:2147483647;top:${top}px;left:${left}px;min-width:240px;
          max-width:340px;background:#fff;color:#1c1d22;border:1px solid #e3e3e8;border-radius:12px;
          box-shadow:0 12px 40px rgba(0,0,0,.18);overflow:hidden;font:13px/1.4 system-ui,sans-serif}
        .head{display:flex;align-items:center;gap:8px;padding:9px 12px;border-bottom:1px solid #eee;
          font-weight:600;font-size:12px;color:#555}
        .head img{width:16px;height:16px;border-radius:4px}
        .row{display:flex;flex-direction:column;gap:2px;width:100%;text-align:left;background:none;
          border:0;padding:9px 12px;cursor:pointer}
        .row:hover{background:#f3f3f7}
        .name{font-weight:600}
        .user{color:#777;font-size:12px}
        .empty{padding:14px 12px;color:#777}
      </style>
      <div class="panel">
        <div class="head"><img src="${ICON_URL}" alt=""/> NaraVault</div>
        ${inner}
      </div>`;

    shadow.querySelectorAll(".row").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.getAttribute("data-id");
        closeMenu();
        fillItem(id, anchorEl.type === "password" ? anchorEl : passwordFields()[0]);
      });
    });

    document.documentElement.appendChild(menuHost);
  }

  // Card chooser: cards aren't origin-bound, so we list ALL saved cards and let
  // the user pick. Picking one triggers a consent prompt in the app before the
  // PAN/CVV is released.
  async function openCardMenu(anchorEl) {
    closeMenu();
    const resp = await send("cards", {});

    menuHost = document.createElement("div");
    const shadow = menuHost.attachShadow({ mode: "closed" });
    const rect = anchorEl.getBoundingClientRect();
    const top = window.scrollY + rect.bottom + 6;
    const left = window.scrollX + rect.left;

    let inner;
    if (resp.state === "ok" && resp.body.items && resp.body.items.length) {
      inner = resp.body.items
        .map((it) => {
          const meta = [it.brand, it.last4 ? "•• " + it.last4 : ""]
            .filter(Boolean)
            .join("  ");
          return `
          <button class="row" data-id="${escapeHtml(it.id)}">
            <span class="name">${escapeHtml(it.name || "Card")}</span>
            <span class="user">${escapeHtml(meta || it.holder || "")}</span>
          </button>`;
        })
        .join("");
    } else if (resp.state === "ok") {
      inner = `<div class="empty">${escapeHtml("No saved cards yet.")}</div>`;
    } else {
      inner = `<div class="empty">${escapeHtml(messageFor(resp.state, true))}</div>`;
    }

    shadow.innerHTML = `
      <style>
        .panel{position:absolute;z-index:2147483647;top:${top}px;left:${left}px;min-width:240px;
          max-width:340px;background:#fff;color:#1c1d22;border:1px solid #e3e3e8;border-radius:12px;
          box-shadow:0 12px 40px rgba(0,0,0,.18);overflow:hidden;font:13px/1.4 system-ui,sans-serif}
        .head{display:flex;align-items:center;gap:8px;padding:9px 12px;border-bottom:1px solid #eee;
          font-weight:600;font-size:12px;color:#555}
        .head img{width:16px;height:16px;border-radius:4px}
        .row{display:flex;flex-direction:column;gap:2px;width:100%;text-align:left;background:none;
          border:0;padding:9px 12px;cursor:pointer}
        .row:hover{background:#f3f3f7}
        .name{font-weight:600}
        .user{color:#777;font-size:12px}
        .empty{padding:14px 12px;color:#777}
      </style>
      <div class="panel">
        <div class="head"><img src="${ICON_URL}" alt=""/> NaraVault · Cards</div>
        ${inner}
      </div>`;

    shadow.querySelectorAll(".row").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.getAttribute("data-id");
        closeMenu();
        fillCard(id);
      });
    });

    document.documentElement.appendChild(menuHost);
  }

  function messageFor(state, short) {
    switch (state) {
      case "locked":
        return short ? "Vault locked — unlock NaraVault" : "NaraVault is locked. Unlock it first.";
      case "offline":
        return short ? "NaraVault not running" : "Open the NaraVault app first.";
      case "origin_mismatch":
        return "This site doesn't match the saved login.";
      case "not_found":
        return "Saved item not found.";
      default:
        return "No matching logins for this site.";
    }
  }

  /* --------------------------- field badge --------------------------- */

  // Add a clickable NaraVault key icon inside a detected field. `opener` is the
  // chooser to open on click (login menu for password fields, card menu for
  // card-number fields).
  function decorate(targetEl, opener) {
    if (decorated.has(targetEl)) return;
    decorated.add(targetEl);

    const badge = document.createElement("button");
    badge.type = "button";
    badge.setAttribute("aria-label", "NaraVault autofill");
    badge.style.cssText =
      "all:unset;cursor:pointer;position:absolute;width:22px;height:22px;z-index:2147483646;" +
      "background-image:url(" + ICON_URL + ");background-size:contain;background-repeat:no-repeat;" +
      "background-position:center;opacity:.75;border-radius:5px";
    badge.addEventListener("mouseenter", () => (badge.style.opacity = "1"));
    badge.addEventListener("mouseleave", () => (badge.style.opacity = ".75"));
    badge.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      opener(targetEl);
    });

    function place() {
      const r = targetEl.getBoundingClientRect();
      if (r.width < 8) {
        badge.style.display = "none";
        return;
      }
      badge.style.display = "block";
      badge.style.top = window.scrollY + r.top + (r.height - 22) / 2 + "px";
      badge.style.left = window.scrollX + r.right - 28 + "px";
    }
    place();
    document.documentElement.appendChild(badge);

    window.addEventListener("scroll", place, { passive: true });
    window.addEventListener("resize", place, { passive: true });
    targetEl.addEventListener("focus", place);
  }

  function scan() {
    passwordFields().forEach((el) => decorate(el, openMenu));
    const cardNum = detectCardFields().number;
    if (cardNum) decorate(cardNum, openCardMenu);
  }

  /* ----------------------------- wiring ------------------------------ */

  // Popup can ask us to fill a specific item it picked, or report whether this
  // page has a card form (so the popup can show/hide its "Fill here" affordance).
  chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
    if (!msg) return false;
    if (msg.cmd === "fillFromPopup") {
      fillItem(msg.id, passwordFields()[0]);
      sendResponse({ ok: true });
      return false;
    }
    if (msg.cmd === "fillCardFromPopup") {
      fillCard(msg.id);
      sendResponse({ ok: true });
      return false;
    }
    if (msg.cmd === "hasCardForm") {
      sendResponse({ ok: true, hasCardForm: hasCardForm() });
      return false;
    }
    return false;
  });

  document.addEventListener("click", (e) => {
    if (menuHost && !e.composedPath().includes(menuHost)) closeMenu();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeMenu();
  });

  scan();
  // Re-scan as SPAs mutate the DOM (debounced).
  let pending = null;
  const mo = new MutationObserver(() => {
    if (pending) return;
    pending = setTimeout(() => {
      pending = null;
      scan();
    }, 600);
  });
  mo.observe(document.documentElement, { childList: true, subtree: true });
})();
