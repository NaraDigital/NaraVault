// NaraVault Autofill — popup.
//
// Shows the lock/connection state and the accounts that match the active tab.
// Clicking an account asks that tab's content script to perform the fill, so the
// secret is requested + used inside the page context and never lingers here.

const statusEl = document.getElementById("status");
const listEl = document.getElementById("list");
const originEl = document.getElementById("origin");

function showStatus(big, small) {
  statusEl.className = "status show";
  statusEl.innerHTML = small
    ? `<span class="big"></span><span class="sm"></span>`
    : `<span class="big"></span>`;
  statusEl.querySelector(".big").textContent = big;
  if (small) statusEl.querySelector(".sm").textContent = small;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]),
  );
}

function bg(cmd, extra) {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ cmd, ...extra }, (resp) => {
      if (chrome.runtime.lastError || !resp) resolve({ state: "offline" });
      else resolve(resp);
    });
  });
}

async function activeTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab || null;
}

function originOf(url) {
  try {
    return new URL(url).origin;
  } catch (_e) {
    return "";
  }
}

async function main() {
  const tab = await activeTab();
  const origin = tab ? originOf(tab.url) : "";
  originEl.textContent = origin ? new URL(origin).host : "—";

  if (!origin || !/^https?:$/.test(new URL(origin).protocol)) {
    showStatus("Not a website", "Open a normal http(s) page to autofill.");
    return;
  }

  const status = await bg("status");
  if (status.state === "offline") {
    showStatus("NaraVault not running", "Start the NaraVault desktop app, then reopen this popup.");
    return;
  }
  if (status.state === "locked" || (status.body && status.body.locked)) {
    showStatus("Vault is locked", "Unlock NaraVault (master password), then reopen this popup.");
    return;
  }

  const match = await bg("match", { origin });
  if (match.state === "locked") {
    showStatus("Vault is locked", "Unlock NaraVault, then reopen this popup.");
    return;
  }
  if (match.state !== "ok") {
    showStatus("Couldn't reach NaraVault", "Make sure the app is open and unlocked.");
    return;
  }

  const items = (match.body && match.body.items) || [];
  if (items.length === 0) {
    showStatus("No saved logins", `Nothing in your vault matches ${escapeHtml(originEl.textContent)}.`);
    return;
  }

  listEl.innerHTML = items
    .map(
      (it) => `
      <button class="row" data-id="${escapeHtml(it.id)}">
        <span class="name">${escapeHtml(it.name || "Login")}</span>
        <span class="user">${escapeHtml(it.username || "")}</span>
        ${it.hasTotp ? '<span class="badge">TOTP</span>' : ""}
      </button>`,
    )
    .join("");

  listEl.querySelectorAll(".row").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const id = btn.getAttribute("data-id");
      if (tab) {
        try {
          await chrome.tabs.sendMessage(tab.id, { cmd: "fillFromPopup", id });
        } catch (_e) {
          /* content script not present on this page */
        }
      }
      window.close();
    });
  });
}

main();
