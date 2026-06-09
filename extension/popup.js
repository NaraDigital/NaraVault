// NaraVault Autofill — popup.
//
// Three surfaces, matching the Nara Vault design handoff:
//   • Vault     — site-matched logins from the running desktop app (metadata
//                 only; clicking Fill asks the page's content script to perform
//                 the fill, so secrets are requested + used in the page and never
//                 linger here).
//   • Cards     — every saved card (metadata only: brand + last-4 + expiry).
//                 Fill asks the content script to fill the detected card form.
//   • Generator — a fully client-side password generator (works offline).
//
// The popup can also INSERT / EDIT a login or card. It NEVER reads existing
// secrets and NEVER unlocks the vault — the desktop app re-prompts the user
// before persisting, and on edit the old secret stays in the app (blank secret
// field = "keep current"). Decryption stays with the desktop app alone.

const appEl = document.getElementById("app");
const tabVaultBtn = document.getElementById("tabVault");
const tabCardsBtn = document.getElementById("tabCards");
const tabGenBtn = document.getElementById("tabGen");
const refreshBtn = document.getElementById("refreshBtn");

let tab = "vault"; // "vault" | "cards" | "generator"
let vaultState = { phase: "loading" }; // cached result of the last vault query
let cardsState = { phase: "loading" }; // cached result of the last cards query
let editor = null; // unified add/edit overlay state, or null when closed
let vaultQuery = ""; // free-text filter for the Vault tab
let cardsQuery = ""; // free-text filter for the Cards tab
// The list re-renders on every keystroke (innerHTML), which recreates the search
// <input>. These remember whether to refocus it and where the caret was, so
// typing stays smooth.
let searchFocus = false;
let searchCaret = null;

/* ----------------------------- icons ----------------------------- */
const ICONS = {
  lock: '<path d="M5 11h14v10H5z"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>',
  shield: '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>',
  plug: '<path d="M12 2v6"/><path d="M5 10h14l-1 5a6 6 0 0 1-12 0z"/><path d="M9 21v-3"/><path d="M15 21v-3"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a14 14 0 0 1 0 18 14 14 0 0 1 0-18z"/>',
  login: '<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><path d="M10 17l5-5-5-5"/><path d="M15 12H3"/>',
  card: '<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M2 10h20"/>',
  copy: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>',
  refresh: '<path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>',
  key: '<circle cx="8" cy="15" r="4"/><path d="M10.85 12.15 19 4"/><path d="M18 5l2 2"/><path d="M15 8l2 2"/>',
  edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4z"/>',
  plus: '<path d="M12 5v14"/><path d="M5 12h14"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  chevron: '<path d="m9 18 6-6-6-6"/>',
  close: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
};
function icon(name, size = 16) {
  return `<svg viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${ICONS[name] || ""}</svg>`;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]),
  );
}

/* ----------------------------- search bar ----------------------------- */
function searchBar(id, value, placeholder) {
  return `
    <div class="pop-search">
      <span class="pop-search-ic">${icon("search", 14)}</span>
      <input id="${id}" class="pop-search-input" type="text" value="${escapeHtml(value)}"
             placeholder="${escapeHtml(placeholder)}" autocomplete="off" autocapitalize="off" spellcheck="false" />
      ${value ? `<button class="pop-search-clear" id="${id}Clear" title="Clear">${icon("close", 14)}</button>` : ""}
    </div>`;
}

// Wire a search input: keep `getQ`/`setQ` in sync, re-render, and restore focus +
// caret afterwards so typing isn't interrupted by the innerHTML swap.
function wireSearch(id, getQ, setQ) {
  const el = document.getElementById(id);
  if (!el) return;
  if (searchFocus) {
    el.focus();
    const pos = searchCaret == null ? el.value.length : searchCaret;
    try {
      el.setSelectionRange(pos, pos);
    } catch (_e) {
      /* non-text input — ignore */
    }
  }
  el.addEventListener("input", () => {
    searchFocus = true;
    searchCaret = el.selectionStart;
    setQ(el.value);
    render();
  });
  const clear = document.getElementById(id + "Clear");
  if (clear)
    clear.addEventListener("click", () => {
      searchFocus = true;
      searchCaret = 0;
      setQ("");
      render();
    });
}

function matchText(q, ...fields) {
  if (!q) return true;
  const needle = q.trim().toLowerCase();
  if (!needle) return true;
  return fields.some((f) => String(f || "").toLowerCase().includes(needle));
}

/* ----------------------------- toast ----------------------------- */
let toastWrap = null;
function toast(text) {
  if (!toastWrap) {
    toastWrap = document.createElement("div");
    toastWrap.className = "toast-wrap";
    document.body.appendChild(toastWrap);
  }
  const el = document.createElement("div");
  el.className = "toast";
  el.innerHTML = `${icon("check", 14)}<span></span>`;
  el.querySelector("span").textContent = text;
  toastWrap.appendChild(el);
  setTimeout(() => el.remove(), 1800);
}

/* ----------------------------- bridge ----------------------------- */
function bg(cmd, extra) {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ cmd, ...extra }, (resp) => {
      if (chrome.runtime.lastError || !resp) resolve({ state: "offline" });
      else resolve(resp);
    });
  });
}

async function activeTab() {
  const [t] = await chrome.tabs.query({ active: true, currentWindow: true });
  return t || null;
}

function originOf(url) {
  try {
    return new URL(url).origin;
  } catch (_e) {
    return "";
  }
}

// Map a write-path error state to a friendly message for the editor.
function writeErr(state) {
  if (state === "offline") return "NaraVault is closed — open the desktop app.";
  if (state === "locked") return "Unlock NaraVault on your desktop first.";
  if (state === "origin_mismatch") return "Save was declined in the app.";
  if (state === "not_found") return "That item no longer exists in the vault.";
  return "Couldn't save. Please try again.";
}

/* ----------------------------- vault query ----------------------------- */
async function loadVault() {
  vaultState = { phase: "loading" };
  if (tab === "vault" && !editor) render();

  const t = await activeTab();
  let origin = t ? originOf(t.url) : "";
  // Only http(s) pages are a valid autofill target. Other pages (chrome://,
  // new tab, …) still list every login under "Other logins" — just with no
  // "this site" section.
  let isWebsite = false;
  try {
    isWebsite = !!origin && /^https?:$/.test(new URL(origin).protocol || "");
  } catch (_e) {
    isWebsite = false;
  }
  if (!isWebsite) origin = "";
  let host = "—";
  try {
    if (origin) host = new URL(origin).host;
  } catch (_e) {
    /* keep dash */
  }

  const status = await bg("status");
  if (status.state === "offline") {
    vaultState = { phase: "offline" };
    if (tab === "vault" && !editor) render();
    return;
  }
  if (status.state === "locked" || (status.body && status.body.locked)) {
    vaultState = { phase: "locked", host };
    if (tab === "vault" && !editor) render();
    return;
  }

  const match = await bg("match", { origin });
  if (match.state === "locked") {
    vaultState = { phase: "locked", host };
  } else if (match.state !== "ok") {
    vaultState = { phase: "error", host };
  } else {
    const items = (match.body && match.body.items) || [];
    const others = (match.body && match.body.others) || [];
    vaultState = { phase: "ok", host, origin, isWebsite, items, others, tabId: t && t.id };
  }
  if (tab === "vault" && !editor) render();
}

async function fillItem(id) {
  if (vaultState.phase !== "ok" || !vaultState.tabId) return;
  try {
    await chrome.tabs.sendMessage(vaultState.tabId, { cmd: "fillFromPopup", id });
  } catch (_e) {
    /* content script not present on this page */
  }
  window.close();
}

/* ----------------------------- cards query ----------------------------- */
async function loadCards() {
  cardsState = { phase: "loading" };
  if (tab === "cards" && !editor) render();

  const status = await bg("status");
  if (status.state === "offline") {
    cardsState = { phase: "offline" };
  } else if (status.state === "locked" || (status.body && status.body.locked)) {
    cardsState = { phase: "locked" };
  } else {
    const res = await bg("cards");
    if (res.state === "locked") cardsState = { phase: "locked" };
    else if (res.state !== "ok") cardsState = { phase: "error" };
    else cardsState = { phase: "ok", items: (res.body && res.body.items) || [] };
  }
  if (tab === "cards" && !editor) render();
}

async function fillCardItem(id) {
  const t = await activeTab();
  if (!t || !t.id) {
    toast("Open a website to fill a card");
    return;
  }
  try {
    await chrome.tabs.sendMessage(t.id, { cmd: "fillCardFromPopup", id });
  } catch (_e) {
    /* content script not present on this page */
  }
  window.close();
}

/* ----------------------------- vault rendering ----------------------------- */
function lockScreen({ badge = "lock", warn = false, title, body }) {
  return `
    <div class="pop-lock">
      <div class="pop-lock-badge${warn ? " warn" : ""}">${icon(badge, 24)}</div>
      <h2>${escapeHtml(title)}</h2>
      <p>${body}</p>
    </div>`;
}

// One login row. `canFill` adds the Fill action (only for logins that match the
// current site — cross-origin fill is blocked server-side, so "Other logins"
// rows are edit-only).
function loginRow(it, canFill) {
  const name = it.name || "Login";
  const letter = escapeHtml((name[0] || "•").toUpperCase());
  const open = canFill
    ? `data-open="${escapeHtml(it.id)}"`
    : `data-edit-open="${escapeHtml(it.id)}"`;
  return `
    <div class="match-row" data-id="${escapeHtml(it.id)}">
      <div class="item-glyph" style="width:32px;height:32px">
        <span class="glyph-letter">${letter}</span>
      </div>
      <button class="li-text" ${open}>
        <span class="li-name">${escapeHtml(name)}</span>
        <span class="li-sub">${escapeHtml(it.username || "")}</span>
      </button>
      ${it.hasTotp ? '<span class="li-badge">TOTP</span>' : ""}
      <button class="icon-btn row-edit" data-edit="${escapeHtml(it.id)}" title="Edit">${icon("edit", 14)}</button>
      ${canFill ? `<button class="match-fill" data-fill="${escapeHtml(it.id)}">${icon("login", 13)} Fill</button>` : ""}
    </div>`;
}

function sectionHead(iconName, title, caption, count, counterWord) {
  const counter = `${count} ${counterWord}${count === 1 ? "" : "s"}`;
  return `
    <div class="site-ctx-head">
      <div class="site-favi">${icon(iconName, 13)}</div>
      <div>
        <div class="sc-host">${escapeHtml(title)}</div>
        <div class="sc-cap">${escapeHtml(caption)}</div>
      </div>
      <span class="sc-count">${counter}</span>
    </div>`;
}

function renderVault() {
  const s = vaultState;
  switch (s.phase) {
    case "loading":
      return `<div class="pop-empty"><div class="pop-empty-ic">${icon("shield", 22)}</div><p>Checking NaraVault…</p></div>`;
    case "offline":
      return lockScreen({
        badge: "plug",
        warn: true,
        title: "NaraVault is closed",
        body: "Start the NaraVault desktop app, then press refresh.",
      });
    case "locked":
      return lockScreen({
        badge: "lock",
        title: "Vault is locked",
        body: "Unlock NaraVault on your desktop with the master password, then press refresh.",
      });
    case "error":
      return lockScreen({
        badge: "plug",
        warn: true,
        title: "Couldn't reach NaraVault",
        body: "Make sure the desktop app is open and unlocked, then refresh.",
      });
    case "ok": {
      const q = vaultQuery.trim();
      const searching = q.length > 0;
      const flt = (arr) =>
        searching ? arr.filter((it) => matchText(q, it.name, it.username)) : arr;
      const matched = flt(s.items || []);
      const others = flt(s.others || []);
      const totalAll = (s.items || []).length + (s.others || []).length;

      const parts = [];
      // Search bar — only worth showing once there's something to filter.
      if (totalAll > 0)
        parts.push(searchBar("vaultSearch", vaultQuery, "Search logins…"));

      // Section 1 — logins for the current site (only on a real website, and
      // its empty / add affordance is hidden while searching).
      if (s.isWebsite) {
        if (matched.length > 0) {
          parts.push(`
            <div class="site-ctx">
              ${sectionHead("login", s.host, "Logins for this site", matched.length, "match")}
              ${matched.map((it) => loginRow(it, true)).join("")}
            </div>`);
          if (!searching)
            parts.push(`
              <div class="pop-pad">
                <button class="btn btn-ghost af-add" id="addLoginBtn">${icon("login", 13)} Add a login for this site</button>
              </div>`);
        } else if (!searching) {
          parts.push(`
            <div class="site-ctx">
              ${sectionHead("login", s.host, "Logins for this site", 0, "match")}
              <div class="pop-mini-empty">
                <p>Nothing matches <span class="mono">${escapeHtml(s.host)}</span> yet.</p>
                <button class="btn btn-primary af-add" id="addLoginBtn">${icon("login", 13)} Add a login for this site</button>
              </div>
            </div>`);
        }
      }

      // Section 2 — every other login in the vault (edit-only).
      if (others.length > 0) {
        parts.push(`
          <div class="site-ctx">
            ${sectionHead("shield", "Other logins", s.isWebsite ? "From the rest of your vault" : "All saved logins", others.length, "login")}
            ${others.map((it) => loginRow(it, false)).join("")}
          </div>`);
      }

      // No-results state while searching.
      if (searching && matched.length === 0 && others.length === 0) {
        parts.push(`
          <div class="pop-empty">
            <div class="pop-empty-ic">${icon("search", 22)}</div>
            <h3>No matches</h3>
            <p>No login matches “${escapeHtml(q)}”.</p>
          </div>`);
      }

      // Add button when there's no current-site section to host it (and not
      // filtering).
      if (!s.isWebsite && !searching) {
        parts.push(`
          <div class="pop-pad">
            <button class="btn ${totalAll === 0 ? "btn-primary" : "btn-ghost"} af-add" id="addLoginBtn">${icon("login", 13)} Add a login</button>
          </div>`);
      }

      return parts.join("");
    }
    default:
      return "";
  }
}

function wireVault() {
  wireSearch("vaultSearch", () => vaultQuery, (v) => (vaultQuery = v));
  appEl.querySelectorAll("[data-fill]").forEach((b) =>
    b.addEventListener("click", () => fillItem(b.getAttribute("data-fill"))),
  );
  // Clicking a matched login's name acts as a Fill shortcut (no detail view —
  // the popup never holds item secrets).
  appEl.querySelectorAll("[data-open]").forEach((b) =>
    b.addEventListener("click", () => fillItem(b.getAttribute("data-open"))),
  );
  // For "other" logins, clicking the name opens the editor (fill is blocked
  // cross-origin), matching the explicit Edit button.
  appEl.querySelectorAll("[data-edit-open]").forEach((b) =>
    b.addEventListener("click", () =>
      openEditor({ mode: "edit", type: "login", id: b.getAttribute("data-edit-open") }),
    ),
  );
  appEl.querySelectorAll("[data-edit]").forEach((b) =>
    b.addEventListener("click", () =>
      openEditor({ mode: "edit", type: "login", id: b.getAttribute("data-edit") }),
    ),
  );
  appEl.querySelectorAll("#addLoginBtn").forEach((b) =>
    b.addEventListener("click", () => openEditor({ mode: "add", type: "login" })),
  );
}

/* ----------------------------- cards rendering ----------------------------- */
function renderCards() {
  const s = cardsState;
  switch (s.phase) {
    case "loading":
      return `<div class="pop-empty"><div class="pop-empty-ic">${icon("card", 22)}</div><p>Loading cards…</p></div>`;
    case "offline":
      return lockScreen({
        badge: "plug",
        warn: true,
        title: "NaraVault is closed",
        body: "Start the NaraVault desktop app, then press refresh.",
      });
    case "locked":
      return lockScreen({
        badge: "lock",
        title: "Vault is locked",
        body: "Unlock NaraVault on your desktop with the master password, then press refresh.",
      });
    case "error":
      return lockScreen({
        badge: "plug",
        warn: true,
        title: "Couldn't reach NaraVault",
        body: "Make sure the desktop app is open and unlocked, then refresh.",
      });
    case "ok": {
      const all = s.items || [];
      const addBtn = `
        <div class="pop-pad">
          <button class="btn ${all.length === 0 ? "btn-primary" : "btn-ghost"} af-add" id="addCardBtn">${icon("plus", 13)} Add a card</button>
        </div>`;
      if (all.length === 0) {
        return `
          <div class="pop-empty">
            <div class="pop-empty-ic">${icon("card", 22)}</div>
            <h3>No saved cards</h3>
            <p>Add a card to autofill it on checkout pages.</p>
          </div>
          ${addBtn}`;
      }
      const q = cardsQuery.trim();
      const searching = q.length > 0;
      const items = searching
        ? all.filter((it) => matchText(q, it.name, it.brand, it.last4, it.holder))
        : all;
      const search = searchBar("cardsSearch", cardsQuery, "Search cards…");
      if (items.length === 0) {
        return `
          ${search}
          <div class="pop-empty">
            <div class="pop-empty-ic">${icon("search", 22)}</div>
            <h3>No matches</h3>
            <p>No card matches “${escapeHtml(q)}”.</p>
          </div>`;
      }
      const rows = items
        .map((it) => {
          const name = it.name || "Card";
          const brand = it.brand || "Card";
          const sub = it.last4
            ? `${escapeHtml(brand)} •• ${escapeHtml(it.last4)}`
            : escapeHtml(brand);
          return `
          <div class="match-row" data-id="${escapeHtml(it.id)}">
            <div class="item-glyph item-glyph-card" style="width:32px;height:32px">${icon("card", 16)}</div>
            <button class="li-text" data-cardfill="${escapeHtml(it.id)}">
              <span class="li-name">${escapeHtml(name)}</span>
              <span class="li-sub">${sub}${it.expiry ? " · " + escapeHtml(it.expiry) : ""}</span>
            </button>
            <button class="icon-btn row-edit" data-cardedit="${escapeHtml(it.id)}" title="Edit">${icon("edit", 14)}</button>
            <button class="match-fill" data-cardfill="${escapeHtml(it.id)}">${icon("card", 13)} Fill</button>
          </div>`;
        })
        .join("");
      return `
        ${search}
        <div class="site-ctx">
          <div class="site-ctx-head">
            <div class="site-favi">${icon("card", 13)}</div>
            <div>
              <div class="sc-host">Your cards</div>
              <div class="sc-cap">Fill on checkout pages</div>
            </div>
            <span class="sc-count">${items.length} card${items.length > 1 ? "s" : ""}</span>
          </div>
          ${rows}
        </div>
        ${searching ? "" : addBtn}`;
    }
    default:
      return "";
  }
}

function wireCards() {
  wireSearch("cardsSearch", () => cardsQuery, (v) => (cardsQuery = v));
  appEl.querySelectorAll("[data-cardfill]").forEach((b) =>
    b.addEventListener("click", () => fillCardItem(b.getAttribute("data-cardfill"))),
  );
  appEl.querySelectorAll("[data-cardedit]").forEach((b) =>
    b.addEventListener("click", () =>
      openEditor({ mode: "edit", type: "card", id: b.getAttribute("data-cardedit") }),
    ),
  );
  const addBtn = document.getElementById("addCardBtn");
  if (addBtn)
    addBtn.addEventListener("click", () => openEditor({ mode: "add", type: "card" }));
}

/* ----------------------------- unified editor (add/edit × login/card) -------
// The popup can INSERT a new item or EDIT non-secret fields of an existing one.
// On edit, secret fields (password / TOTP / card number / CVV) load BLANK — the
// existing secret is never read back into the extension. A blank secret on save
// means "keep the current value"; typing a replacement overwrites it. The
// desktop app re-prompts the user before anything is persisted.
---------------------------------------------------------------------------- */
function detectBrand(num) {
  const n = (num || "").replace(/\D/g, "");
  if (/^4/.test(n)) return "Visa";
  if (/^(34|37)/.test(n)) return "Amex";
  if (/^(5[1-5]|2[2-7])/.test(n)) return "Mastercard";
  if (/^6(011|5)/.test(n)) return "Discover";
  return "";
}

async function openEditor({ mode, type, id }) {
  editor = {
    mode,
    type,
    id: id || "",
    name: "",
    url: "",
    username: "",
    password: "",
    totp: "",
    holder: "",
    number: "",
    expiry: "",
    cvv: "",
    brand: "",
    hasPassword: false,
    hasTotp: false,
    hasNumber: false,
    hasCvv: false,
    loading: mode === "edit",
    busy: false,
    err: "",
  };

  if (mode === "add" && type === "login") {
    const host = vaultState.host && vaultState.host !== "—" ? vaultState.host : "";
    editor.name = host;
    editor.url = vaultState.origin || host;
  }
  render();

  if (mode === "edit") {
    const res = await bg("item", { id });
    if (!editor || editor.id !== id) return; // editor changed underneath us
    editor.loading = false;
    if (res.state !== "ok" || !res.body) {
      editor.err =
        res.state === "locked"
          ? "Unlock NaraVault on your desktop first."
          : res.state === "offline"
            ? "NaraVault is closed — open the desktop app."
            : "Couldn't load this item.";
      render();
      return;
    }
    const b = res.body;
    editor.type = b.type || type;
    editor.name = b.name || "";
    if (editor.type === "login") {
      editor.url = b.url || "";
      editor.username = b.username || "";
      editor.hasPassword = !!b.hasPassword;
      editor.hasTotp = !!b.hasTotp;
    } else {
      editor.holder = b.holder || "";
      editor.expiry = b.expiry || "";
      editor.brand = b.brand || "";
      editor.hasNumber = !!b.hasNumber;
      editor.hasCvv = !!b.hasCvv;
    }
    render();
  }
}

function closeEditor() {
  editor = null;
  render();
}

function field(id, label, value, attrs = "", placeholder = "") {
  return `
    <div class="af-field">
      <label class="af-label" for="${id}">${escapeHtml(label)}</label>
      <input class="af-input" id="${id}" type="text" value="${escapeHtml(value)}" placeholder="${escapeHtml(placeholder)}" ${attrs} />
    </div>`;
}

function renderEditor() {
  const e = editor;
  const isLogin = e.type === "login";
  const isEdit = e.mode === "edit";
  const backLabel = isLogin ? "Back to logins" : "Back to cards";
  const title =
    (isEdit ? "Edit " : "Add ") + (isLogin ? "login" : "card");

  if (e.loading) {
    return `
      <div class="pop-pad add-wrap">
        <button class="af-back" id="afBack">${icon("chevron", 14)}<span>${backLabel}</span></button>
        <div class="pop-empty"><div class="pop-empty-ic">${icon("shield", 22)}</div><p>Loading…</p></div>
      </div>`;
  }

  const keepHint = "Leave blank to keep current";
  let body;
  if (isLogin) {
    body = `
      ${field("edName", "Name", e.name, "", "e.g. GitHub")}
      ${field("edUrl", "Website", e.url, "", "https://example.com")}
      ${field("edUser", "Username", e.username, 'autocomplete="off" autocapitalize="off" spellcheck="false"', "you@example.com")}
      <div class="af-field">
        <label class="af-label" for="edPass">Password</label>
        <div class="af-pw">
          <input class="af-input" id="edPass" type="text" value="${escapeHtml(e.password)}" placeholder="${isEdit && e.hasPassword ? keepHint : "Password"}" autocomplete="off" autocapitalize="off" spellcheck="false" />
          <button class="icon-btn" id="edGen" title="Generate a strong password">${icon("key", 16)}</button>
        </div>
      </div>
      <div class="af-field">
        <label class="af-label" for="edTotp">TOTP secret <span class="af-opt">(optional)</span></label>
        <input class="af-input" id="edTotp" type="text" value="${escapeHtml(e.totp)}" placeholder="${isEdit && e.hasTotp ? keepHint : "otpauth:// or base32 secret"}" autocomplete="off" autocapitalize="off" spellcheck="false" />
      </div>`;
  } else {
    body = `
      ${field("edName", "Name", e.name, "", "e.g. Personal Visa (optional)")}
      ${field("edHolder", "Cardholder", e.holder, 'autocapitalize="words"', "Name on card")}
      <div class="af-field">
        <label class="af-label" for="edNumber">Card number</label>
        <input class="af-input" id="edNumber" type="text" value="${escapeHtml(e.number)}" placeholder="${isEdit && e.hasNumber ? keepHint : "•••• •••• •••• ••••"}" inputmode="numeric" autocomplete="off" spellcheck="false" />
      </div>
      <div class="af-grid2">
        ${field("edExpiry", "Expiry", e.expiry, 'inputmode="numeric"', "MM/YY")}
        <div class="af-field">
          <label class="af-label" for="edCvv">CVV</label>
          <input class="af-input" id="edCvv" type="text" value="${escapeHtml(e.cvv)}" placeholder="${isEdit && e.hasCvv ? keepHint : "•••"}" inputmode="numeric" autocomplete="off" spellcheck="false" />
        </div>
      </div>
      ${field("edBrand", "Brand", e.brand, "", "Visa / Mastercard …")}`;
  }

  return `
    <div class="pop-pad add-wrap">
      <button class="af-back" id="afBack">${icon("chevron", 14)}<span>${backLabel}</span></button>
      <div class="add-form">
        <div class="af-title">${escapeHtml(title)}</div>
        ${body}
        ${e.err ? `<div class="af-err">${escapeHtml(e.err)}</div>` : ""}
        <div class="af-actions">
          <button class="btn btn-ghost" id="edCancel">Cancel</button>
          <button class="btn btn-primary" id="edSave"${e.busy ? " disabled" : ""}>${e.busy ? "Saving…" : isEdit ? "Save changes" : isLogin ? "Save login" : "Save card"}</button>
        </div>
        <p class="af-hint">NaraVault will ask you to confirm in the desktop app before saving.</p>
      </div>
    </div>`;
}

function wireEditor() {
  const e = editor;
  if (!e || e.loading) {
    const back = document.getElementById("afBack");
    if (back)
      back.addEventListener("click", () =>
        e && e.type === "card" ? backToCards() : backToVault(),
      );
    return;
  }
  const byId = (id) => document.getElementById(id);
  const isLogin = e.type === "login";

  const sync = () => {
    if (isLogin) {
      e.name = byId("edName").value;
      e.url = byId("edUrl").value;
      e.username = byId("edUser").value;
      e.password = byId("edPass").value;
      e.totp = byId("edTotp").value;
    } else {
      e.name = byId("edName").value;
      e.holder = byId("edHolder").value;
      e.number = byId("edNumber").value;
      e.expiry = byId("edExpiry").value;
      e.cvv = byId("edCvv").value;
      e.brand = byId("edBrand").value;
    }
  };
  const ids = isLogin
    ? ["edName", "edUrl", "edUser", "edPass", "edTotp"]
    : ["edName", "edHolder", "edNumber", "edExpiry", "edCvv", "edBrand"];
  ids.forEach((id) => {
    const el = byId(id);
    if (el) el.addEventListener("input", sync);
  });

  // For cards: auto-suggest brand from the number when the brand box is empty.
  if (!isLogin) {
    const numEl = byId("edNumber");
    if (numEl)
      numEl.addEventListener("input", () => {
        if (!byId("edBrand").value.trim()) {
          const b = detectBrand(numEl.value);
          if (b) byId("edBrand").value = b;
        }
      });
  }

  byId("afBack").addEventListener("click", () =>
    isLogin ? backToVault() : backToCards(),
  );
  byId("edCancel").addEventListener("click", () =>
    isLogin ? backToVault() : backToCards(),
  );
  const genBtn = byId("edGen");
  if (genBtn)
    genBtn.addEventListener("click", () => {
      sync();
      generatePassword();
      e.password = gen.pw;
      render();
      const p = byId("edPass");
      if (p) p.focus();
    });
  byId("edSave").addEventListener("click", submitEditor);
}

function backToVault() {
  editor = null;
  tab = "vault";
  render();
}
function backToCards() {
  editor = null;
  tab = "cards";
  render();
}

async function submitEditor() {
  const e = editor;
  const isLogin = e.type === "login";
  e.name = (e.name || "").trim();

  if (isLogin) {
    if (e.mode === "add" && !e.password) {
      e.err = "Password is required.";
      render();
      return;
    }
  } else {
    e.number = (e.number || "").trim();
    if (e.mode === "add" && !e.number) {
      e.err = "Card number is required.";
      render();
      return;
    }
  }

  e.err = "";
  e.busy = true;
  render();

  const origin = vaultState.origin || "";
  const fields = isLogin
    ? {
        itemType: "login",
        origin,
        name: e.name,
        username: e.username || "",
        password: e.password || "",
        url: (e.url || "").trim() || origin,
        totp: e.totp || "",
      }
    : {
        itemType: "card",
        origin,
        name: e.name,
        holder: e.holder || "",
        number: e.number || "",
        expiry: (e.expiry || "").trim(),
        cvv: e.cvv || "",
        brand: (e.brand || "").trim() || detectBrand(e.number),
      };

  const resp =
    e.mode === "edit"
      ? await bg("update", { id: e.id, ...fields })
      : await bg("create", fields);

  if (!editor) return; // view changed underneath us
  editor.busy = false;

  if (resp.state === "ok") {
    toast(e.mode === "edit" ? "Changes saved" : isLogin ? "Login saved" : "Card saved");
    if (isLogin) {
      editor = null;
      tab = "vault";
      render();
      loadVault();
    } else {
      editor = null;
      tab = "cards";
      render();
      loadCards();
    }
    return;
  }
  editor.err = writeErr(resp.state);
  render();
}

/* ----------------------------- generator ----------------------------- */
const gen = { length: 20, upper: true, lower: true, digits: true, symbols: true, pw: "" };

function generatePassword() {
  const sets = [];
  if (gen.upper) sets.push("ABCDEFGHJKLMNPQRSTUVWXYZ");
  if (gen.lower) sets.push("abcdefghijkmnopqrstuvwxyz");
  if (gen.digits) sets.push("23456789");
  if (gen.symbols) sets.push("!@#$%^&*()-_=+[]{}");
  if (sets.length === 0) {
    gen.pw = "";
    return;
  }
  const pool = sets.join("");
  const rnd = new Uint32Array(gen.length);
  crypto.getRandomValues(rnd);
  let out = "";
  // Guarantee at least one char from each enabled set, fill the rest from pool.
  for (let i = 0; i < gen.length; i++) {
    const src = i < sets.length ? sets[i] : pool;
    out += src[rnd[i] % src.length];
  }
  // Shuffle so the guaranteed chars aren't always at the front.
  const arr = out.split("");
  const sh = new Uint32Array(arr.length);
  crypto.getRandomValues(sh);
  for (let i = arr.length - 1; i > 0; i--) {
    const j = sh[i] % (i + 1);
    [arr[i], arr[j]] = [arr[j], arr[i]];
  }
  gen.pw = arr.join("");
}

function strength(pw) {
  if (!pw) return { score: 0, label: "—" };
  let pool = 0;
  if (/[a-z]/.test(pw)) pool += 26;
  if (/[A-Z]/.test(pw)) pool += 26;
  if (/[0-9]/.test(pw)) pool += 10;
  if (/[^a-zA-Z0-9]/.test(pw)) pool += 20;
  const bits = pw.length * Math.log2(pool || 1);
  let score = 0;
  if (bits >= 40) score = 1;
  if (bits >= 60) score = 2;
  if (bits >= 80) score = 3;
  if (bits >= 110) score = 4;
  const labels = ["Weak", "Fair", "Good", "Strong", "Excellent"];
  const colors = ["#ff7070", "#f0b35e", "#e0d063", "#7fd18b", "#5fcb9c"];
  return { score, label: labels[score], color: colors[score] };
}

function renderGen() {
  const st = strength(gen.pw);
  const bars = [0, 1, 2, 3]
    .map(
      (i) =>
        `<span class="strength-bar" style="background:${i < st.score ? st.color : "var(--track)"}"></span>`,
    )
    .join("");
  const toggle = (k, label) =>
    `<button class="toggle-row${gen[k] ? " is-on" : ""}" data-toggle="${k}"><span>${label}</span><span class="switch"><span class="knob"></span></span></button>`;
  return `
    <div class="pop-pad">
      <div class="pop-gen">
        <div class="gen-out">
          <span class="gen-pw">${escapeHtml(gen.pw)}</span>
          <div class="gen-out-tools">
            <button class="icon-btn" id="genRegen" title="Regenerate">${icon("refresh", 16)}</button>
            <button class="icon-btn" id="genCopy" title="Copy">${icon("copy", 16)}</button>
          </div>
        </div>
        <div class="strength">
          <div class="strength-bars">${bars}</div>
          <span class="strength-label" style="color:${gen.pw ? st.color : "var(--text-faint)"}">${st.label}</span>
        </div>
        <div class="gen-controls">
          <div class="gen-row"><span class="gen-label">Length</span><span class="gen-val">${gen.length}</span></div>
          <input type="range" min="8" max="40" value="${gen.length}" class="gen-slider" id="genLen" />
          ${toggle("upper", "Uppercase  A-Z")}
          ${toggle("lower", "Lowercase  a-z")}
          ${toggle("digits", "Digits  0-9")}
          ${toggle("symbols", "Symbols  !@#$")}
        </div>
      </div>
    </div>`;
}

function wireGen() {
  const lenEl = document.getElementById("genLen");
  lenEl.addEventListener("input", () => {
    gen.length = +lenEl.value;
    generatePassword();
    render();
  });
  document.getElementById("genRegen").addEventListener("click", () => {
    generatePassword();
    render();
  });
  document.getElementById("genCopy").addEventListener("click", () => {
    if (!gen.pw) return;
    try {
      navigator.clipboard.writeText(gen.pw);
    } catch (_e) {
      /* clipboard may be unavailable */
    }
    toast("Password copied");
  });
  appEl.querySelectorAll("[data-toggle]").forEach((b) =>
    b.addEventListener("click", () => {
      const k = b.getAttribute("data-toggle");
      // Keep at least one set enabled.
      const enabled = ["upper", "lower", "digits", "symbols"].filter((x) => gen[x]);
      if (gen[k] && enabled.length === 1) return;
      gen[k] = !gen[k];
      generatePassword();
      render();
    }),
  );
}

/* ----------------------------- render ----------------------------- */
function render() {
  tabVaultBtn.classList.toggle("is-active", tab === "vault");
  tabCardsBtn.classList.toggle("is-active", tab === "cards");
  tabGenBtn.classList.toggle("is-active", tab === "generator");
  refreshBtn.style.visibility =
    !editor && (tab === "vault" || tab === "cards") ? "visible" : "hidden";

  if (editor) {
    appEl.innerHTML = renderEditor();
    wireEditor();
    return;
  }
  if (tab === "vault") {
    appEl.innerHTML = renderVault();
    wireVault();
  } else if (tab === "cards") {
    appEl.innerHTML = renderCards();
    wireCards();
  } else {
    appEl.innerHTML = renderGen();
    wireGen();
  }
}

/* ----------------------------- wiring ----------------------------- */
tabVaultBtn.addEventListener("click", () => {
  editor = null;
  searchFocus = false;
  tab = "vault";
  render();
});
tabCardsBtn.addEventListener("click", () => {
  editor = null;
  searchFocus = false;
  tab = "cards";
  render();
  loadCards(); // refresh on tab open — cheap metadata
});
tabGenBtn.addEventListener("click", () => {
  editor = null;
  searchFocus = false;
  tab = "generator";
  if (!gen.pw) generatePassword();
  render();
});
refreshBtn.addEventListener("click", () => {
  searchFocus = false;
  if (tab === "cards") loadCards();
  else loadVault();
});

generatePassword();
render();
loadVault();
