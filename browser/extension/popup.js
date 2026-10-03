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
const settingsButton = document.getElementById("openSettings");

let hostname = null;
let disabledSites = [];

for (const [code, name] of LANGUAGES) {
  const option = document.createElement("option");
  option.value = code;
  option.textContent = name;
  targetSelect.append(option);
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

settingsButton.addEventListener("click", () => api.runtime.openOptionsPage());
