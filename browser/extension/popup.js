"use strict";

const api = globalThis.browser ?? globalThis.chrome;
const LANGUAGES = globalThis.OT_LANGUAGES || [];

const DEFAULTS = {
  target: "zh",
  autoTranslate: false,
  disabledSites: [],
};

const statusEl = document.getElementById("status");
const targetSelect = document.getElementById("target");
const autoInput = document.getElementById("autoTranslate");
const siteRow = document.getElementById("siteRow");
const siteInput = document.getElementById("siteDisabled");
const historyEl = document.getElementById("history");
const clearHistoryButton = document.getElementById("clearHistory");
const settingsButton = document.getElementById("openSettings");

let hostname = null;
let disabledSites = [];

for (const [code, name] of LANGUAGES) {
  const option = document.createElement("option");
  option.value = code;
  option.textContent = name;
  targetSelect.append(option);
}

async function renderHistory() {
  const stored = await api.storage.local.get({ history: [] });
  const history = Array.isArray(stored.history) ? stored.history : [];

  historyEl.textContent = "";

  if (!history.length) {
    const item = document.createElement("li");
    item.textContent = "（无）";
    historyEl.append(item);
    return;
  }

  for (const entry of history.slice(0, 10)) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "entry";
    button.title = (entry.text || "") + "\n" + (entry.translation || "");
    button.textContent = entry.translation || "";

    button.addEventListener("click", () => {
      navigator.clipboard.writeText(entry.translation || "").then(() => {
        button.textContent = "已复制";
        setTimeout(() => {
          button.textContent = entry.translation || "";
        }, 1200);
      });
    });

    item.append(button);
    historyEl.append(item);
  }
}

async function loadSite() {
  try {
    const tabs = await api.tabs.query({ active: true, currentWindow: true });
    const tab = tabs && tabs[0];
    if (!tab || tab.id === undefined) return;

    const info = await api.tabs.sendMessage(tab.id, { type: "get-site-info" });
    hostname = info && info.hostname ? info.hostname : null;
  } catch (error) {
    hostname = null;
  }

  if (!hostname) return;

  siteRow.hidden = false;
  siteInput.checked = disabledSites.includes(hostname);
}

api.storage.local.get(DEFAULTS).then((settings) => {
  targetSelect.value = settings.target || DEFAULTS.target;
  autoInput.checked = Boolean(settings.autoTranslate);
  disabledSites = Array.isArray(settings.disabledSites) ? settings.disabledSites : [];
  loadSite();
  renderHistory();
});

api.runtime
  .sendMessage({ type: "health" })
  .then((health) => {
    if (health && health.ok) {
      statusEl.classList.add("ok");
      statusEl.textContent =
        "服务正常：" + health.engine + (health.model ? " / " + health.model : "");
    } else {
      statusEl.classList.add("bad");
      statusEl.textContent = "无法连接本地翻译服务，请先启动客户端";
    }
  })
  .catch(() => {
    statusEl.classList.add("bad");
    statusEl.textContent = "无法连接本地翻译服务，请先启动客户端";
  });

targetSelect.addEventListener("change", () => {
  api.storage.local.set({ target: targetSelect.value });
});

autoInput.addEventListener("change", () => {
  api.storage.local.set({ autoTranslate: autoInput.checked });
});

siteInput.addEventListener("change", () => {
  if (!hostname) return;

  const next = siteInput.checked
    ? Array.from(new Set(disabledSites.concat(hostname)))
    : disabledSites.filter((item) => item !== hostname);

  disabledSites = next;
  api.storage.local.set({ disabledSites: next });
});

clearHistoryButton.addEventListener("click", async () => {
  await api.storage.local.set({ history: [] });
  renderHistory();
});

settingsButton.addEventListener("click", () => api.runtime.openOptionsPage());
