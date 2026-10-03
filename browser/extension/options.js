"use strict";

const api = globalThis.browser ?? globalThis.chrome;
const LANGUAGES = globalThis.OT_LANGUAGES || [];
const SOURCE_LANGUAGES = globalThis.OT_SOURCE_LANGUAGES || [
  ["auto", "自动检测"],
  ...LANGUAGES,
];

const DEFAULTS = {
  serviceUrl: "http://127.0.0.1:17890",
  source: "auto",
  target: "zh",
  autoTranslate: false,
  autoTranslateDelay: 400,
  autoTranslateMinLength: 2,
  disabledSites: [],
};

const form = document.getElementById("form");
const serviceUrlInput = document.getElementById("serviceUrl");
const sourceInput = document.getElementById("source");
const targetInput = document.getElementById("target");
const autoTranslateInput = document.getElementById("autoTranslate");
const autoMinLengthInput = document.getElementById("autoMinLength");
const autoDelayInput = document.getElementById("autoDelay");
const statusEl = document.getElementById("status");
const sitesEl = document.getElementById("sites");

let disabledSites = [];

function fillSelect(select, options) {
  for (const [code, name] of options) {
    const option = document.createElement("option");
    option.value = code;
    option.textContent = name;
    select.append(option);
  }
}

function renderSites() {
  sitesEl.textContent = "";

  if (!disabledSites.length) {
    const item = document.createElement("li");
    item.textContent = "（无）";
    sitesEl.append(item);
    return;
  }

  for (const hostname of disabledSites) {
    const item = document.createElement("li");
    const name = document.createElement("span");
    name.textContent = hostname;

    const remove = document.createElement("button");
    remove.type = "button";
    remove.textContent = "移除";
    remove.addEventListener("click", async () => {
      disabledSites = disabledSites.filter((value) => value !== hostname);
      await api.storage.local.set({ disabledSites });
      renderSites();
    });

    item.append(name, remove);
    sitesEl.append(item);
  }
}

fillSelect(sourceInput, SOURCE_LANGUAGES);
fillSelect(targetInput, LANGUAGES);

api.storage.local.get(DEFAULTS).then((settings) => {
  serviceUrlInput.value = settings.serviceUrl;
  sourceInput.value = settings.source;
  targetInput.value = settings.target;
  autoTranslateInput.checked = Boolean(settings.autoTranslate);
  autoMinLengthInput.value = String(settings.autoTranslateMinLength);
  autoDelayInput.value = String(settings.autoTranslateDelay);
  disabledSites = Array.isArray(settings.disabledSites) ? settings.disabledSites : [];
  renderSites();
});

form.addEventListener("submit", (event) => {
  event.preventDefault();

  const minLength = Math.min(
    50,
    Math.max(1, Number(autoMinLengthInput.value) || DEFAULTS.autoTranslateMinLength)
  );

  api.storage.local
    .set({
      serviceUrl: serviceUrlInput.value.trim() || DEFAULTS.serviceUrl,
      source: sourceInput.value,
      target: targetInput.value,
      autoTranslate: autoTranslateInput.checked,
      autoTranslateMinLength: minLength,
      autoTranslateDelay: Number(autoDelayInput.value) || DEFAULTS.autoTranslateDelay,
    })
    .then(() => {
      autoMinLengthInput.value = String(minLength);
      statusEl.textContent = "已保存。";
    });
});

document.getElementById("test").addEventListener("click", async () => {
  statusEl.textContent = "测试中…";

  const base = (serviceUrlInput.value.trim() || DEFAULTS.serviceUrl).replace(
    /\/+$/,
    ""
  );

  try {
    const response = await fetch(base + "/health");
    if (!response.ok) {
      statusEl.textContent = "连接失败：HTTP " + response.status;
      return;
    }
    const payload = await response.json();
    statusEl.textContent =
      "服务正常：" + payload.engine + " / " + (payload.model || "-");
  } catch (error) {
    statusEl.textContent = "连接失败：无法访问 " + base;
  }
});
