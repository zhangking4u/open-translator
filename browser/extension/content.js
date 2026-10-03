"use strict";

const api = globalThis.browser ?? globalThis.chrome;

const LANGUAGES = globalThis.OT_LANGUAGES || [];
const SOURCE_LANGUAGES = globalThis.OT_SOURCE_LANGUAGES || [
  ["auto", "自动检测"],
  ...LANGUAGES,
];

const DEFAULTS = {
  source: "auto",
  target: "zh",
  autoTranslate: false,
  disabledSites: [],
};

let host = null;
let statusEl = null;
let spinnerEl = null;
let copyButton = null;
let sourceSelect = null;
let targetSelect = null;
let swapButton = null;
let retryButton = null;
let cancelButton = null;
let siteButton = null;

let currentTranslation = "";
let currentText = "";
let targetLanguage = DEFAULTS.target;
let sourceLanguage = DEFAULTS.source;
let autoTranslate = false;
let disabledSites = [];
let autoTimer = null;
let copyTimer = null;
let loadingPlaceholder = false;
let activeRequest = 0;
let requestSeq = 0;
let port = null;
let repositionQueued = false;

function siteDisabled() {
  return disabledSites.includes(location.hostname);
}

function autoTranslateActive() {
  return autoTranslate && !siteDisabled();
}

function syncControls() {
  if (sourceSelect) sourceSelect.value = sourceLanguage;
  if (targetSelect) targetSelect.value = targetLanguage;
  if (swapButton) swapButton.disabled = sourceLanguage === "auto";
  updateSiteButton();
}

async function loadSettings() {
  const settings = await api.storage.local.get(DEFAULTS);
  targetLanguage = settings.target || DEFAULTS.target;
  sourceLanguage = settings.source || DEFAULTS.source;
  autoTranslate = Boolean(settings.autoTranslate);
  disabledSites = Array.isArray(settings.disabledSites) ? settings.disabledSites : [];
  syncControls();
}

api.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;

  if (changes.target && changes.target.newValue) {
    targetLanguage = changes.target.newValue;
  }
  if (changes.source && changes.source.newValue) {
    sourceLanguage = changes.source.newValue;
  }
  if (changes.autoTranslate) {
    autoTranslate = Boolean(changes.autoTranslate.newValue);
  }
  if (changes.disabledSites && Array.isArray(changes.disabledSites.newValue)) {
    disabledSites = changes.disabledSites.newValue;
  }

  syncControls();
});

function connectPort() {
  if (port) return port;

  port = api.runtime.connect({ name: "translate" });
  port.onMessage.addListener(onPortMessage);
  port.onDisconnect.addListener(() => {
    port = null;
    if (activeRequest) {
      activeRequest = 0;
      showError("与后台的连接已断开，请重试。");
    }
  });

  return port;
}

function post(message) {
  try {
    connectPort().postMessage(message);
  } catch (error) {
    showError("翻译请求失败，请重试。");
  }
}

function onPortMessage(message) {
  if (!message || message.requestId !== activeRequest) return;

  if (message.type === "delta") {
    if (loadingPlaceholder) {
      loadingPlaceholder = false;
      statusEl.textContent = "";
    }
    currentTranslation += message.delta || "";
    statusEl.textContent = currentTranslation;
  } else if (message.type === "done") {
    activeRequest = 0;
    showResult(message.translation || currentTranslation);
  } else if (message.type === "error") {
    activeRequest = 0;
    showError(message.message || "翻译失败，请重试。");
  }
}

function beginTranslate(text) {
  currentText = text;
  currentTranslation = "";
  requestSeq += 1;
  activeRequest = requestSeq;
  showLoading();
  post({ type: "translate", requestId: activeRequest, text });
}

function cancelActive() {
  if (!activeRequest) return;
  activeRequest = 0;
  post({ type: "cancel" });
}

function buildOptions(select, options) {
  for (const [code, name] of options) {
    const option = document.createElement("option");
    option.value = code;
    option.textContent = name;
    select.append(option);
  }
}

function ensureBubble() {
  if (host) return;

  host = document.createElement("div");
  host.style.position = "fixed";
  host.style.zIndex = "2147483647";
  host.style.top = "0";
  host.style.left = "0";
  host.style.display = "none";

  const shadow = host.attachShadow({ mode: "open" });

  const style = document.createElement("style");
  style.textContent = [
    "@keyframes ot-spin { to { transform: rotate(360deg); } }",
    ".card { font: 13px/1.5 system-ui, sans-serif; color: #1f2937; background: #fff;",
    "  border: 1px solid #d1d5db; border-radius: 8px; box-shadow: 0 6px 24px rgba(0,0,0,.18);",
    "  max-width: 460px; min-width: 240px; padding: 10px 12px; }",
    ".status { white-space: pre-wrap; word-break: break-word; min-height: 1.5em; }",
    ".status.error { color: #b91c1c; }",
    ".status-row { display: flex; align-items: flex-start; gap: 6px; }",
    ".spinner { flex: none; width: 11px; height: 11px; margin-top: 4px; border: 2px solid #d1d5db;",
    "  border-top-color: #6b7280; border-radius: 50%; animation: ot-spin .8s linear infinite; }",
    ".row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin-top: 8px; }",
    ".spacer { flex: 1; }",
    "select { font: inherit; padding: 2px 6px; border-radius: 6px; border: 1px solid #d1d5db;",
    "  background: #fff; }",
    "button { font: inherit; padding: 3px 10px; border-radius: 6px; border: 1px solid #d1d5db;",
    "  background: #f9fafb; cursor: pointer; }",
    "button:hover { background: #f3f4f6; }",
    "button:disabled { color: #9ca3af; cursor: default; }",
    "button[hidden] { display: none; }",
    ".site-toggle { margin-top: 6px; padding: 0; border: 0; background: none; color: #6b7280;",
    "  font-size: 12px; text-decoration: underline; cursor: pointer; }",
  ].join("\n");

  const card = document.createElement("div");
  card.className = "card";
  card.setAttribute("role", "region");
  card.setAttribute("aria-label", "OpenTranslator 译文");

  const statusRow = document.createElement("div");
  statusRow.className = "status-row";

  spinnerEl = document.createElement("span");
  spinnerEl.className = "spinner";
  spinnerEl.hidden = true;

  statusEl = document.createElement("div");
  statusEl.className = "status";
  statusEl.setAttribute("role", "status");
  statusEl.setAttribute("aria-live", "polite");

  statusRow.append(spinnerEl, statusEl);

  const row = document.createElement("div");
  row.className = "row";

  sourceSelect = document.createElement("select");
  sourceSelect.className = "source-select";
  sourceSelect.title = "源语言";
  buildOptions(sourceSelect, SOURCE_LANGUAGES);
  if (!SOURCE_LANGUAGES.some(([code]) => code === sourceLanguage)) {
    buildOptions(sourceSelect, [[sourceLanguage, sourceLanguage]]);
  }
  sourceSelect.value = sourceLanguage;
  sourceSelect.addEventListener("change", async () => {
    sourceLanguage = sourceSelect.value;
    await api.storage.local.set({ source: sourceLanguage });
    if (currentText) beginTranslate(currentText);
  });

  swapButton = document.createElement("button");
  swapButton.textContent = "⇄";
  swapButton.title = "互换源语言和目标语言";
  swapButton.addEventListener("click", async () => {
    if (sourceLanguage === "auto") return;
    const previousSource = sourceLanguage;
    sourceLanguage = targetLanguage;
    targetLanguage = previousSource;
    await api.storage.local.set({ source: sourceLanguage, target: targetLanguage });
    syncControls();
    if (currentText) beginTranslate(currentText);
  });

  targetSelect = document.createElement("select");
  targetSelect.className = "target-select";
  targetSelect.title = "目标语言";
  buildOptions(targetSelect, LANGUAGES);
  if (!LANGUAGES.some(([code]) => code === targetLanguage)) {
    buildOptions(targetSelect, [[targetLanguage, targetLanguage]]);
  }
  targetSelect.value = targetLanguage;
  targetSelect.addEventListener("change", async () => {
    targetLanguage = targetSelect.value;
    await api.storage.local.set({ target: targetLanguage });
    if (currentText) beginTranslate(currentText);
  });

  const spacer = document.createElement("div");
  spacer.className = "spacer";

  retryButton = document.createElement("button");
  retryButton.textContent = "重试";
  retryButton.hidden = true;
  retryButton.addEventListener("click", () => {
    if (currentText) beginTranslate(currentText);
  });

  cancelButton = document.createElement("button");
  cancelButton.textContent = "停止";
  cancelButton.hidden = true;
  cancelButton.addEventListener("click", () => {
    cancelActive();
    loadingPlaceholder = false;
    spinnerEl.hidden = true;
    cancelButton.hidden = true;
    retryButton.hidden = !currentText;
    statusEl.classList.remove("error");
    statusEl.textContent = "已取消。";
  });

  copyButton = document.createElement("button");
  copyButton.textContent = "复制";
  copyButton.disabled = true;
  copyButton.addEventListener("click", () => {
    if (!currentTranslation) return;

    const done = () => {
      copyButton.textContent = "已复制";
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        if (copyButton) copyButton.textContent = "复制";
      }, 2000);
    };

    navigator.clipboard.writeText(currentTranslation).then(done, () => {
      const textarea = document.createElement("textarea");
      textarea.value = currentTranslation;
      document.documentElement.appendChild(textarea);
      textarea.select();
      document.execCommand("copy");
      textarea.remove();
      done();
    });
  });

  const closeButton = document.createElement("button");
  closeButton.textContent = "关闭";
  closeButton.addEventListener("click", hide);

  row.append(
    sourceSelect,
    swapButton,
    targetSelect,
    spacer,
    retryButton,
    cancelButton,
    copyButton,
    closeButton
  );

  siteButton = document.createElement("button");
  siteButton.className = "site-toggle";
  siteButton.hidden = true;
  siteButton.addEventListener("click", async () => {
    const hostname = location.hostname;
    const next = disabledSites.includes(hostname)
      ? disabledSites.filter((item) => item !== hostname)
      : disabledSites.concat(hostname);
    disabledSites = next;
    await api.storage.local.set({ disabledSites: next });
    updateSiteButton();
  });

  card.append(statusRow, row, siteButton);
  shadow.append(style, card);
  document.documentElement.append(host);

  syncControls();
}

function updateSiteButton() {
  if (!siteButton) return;

  const showIt = autoTranslate || siteDisabled();
  siteButton.hidden = !showIt;
  if (!showIt) return;

  siteButton.textContent = siteDisabled()
    ? "本站已关闭自动翻译（点击恢复）"
    : "本站不再自动翻译";
}

function positionBubble(rect) {
  const margin = 8;
  const width = host.offsetWidth || 320;
  const height = host.offsetHeight || 80;

  let left = rect.left;
  let top = rect.bottom + 6;

  if (left + width > window.innerWidth - margin) {
    left = window.innerWidth - width - margin;
  }
  if (left < margin) left = margin;
  if (top + height > window.innerHeight - margin) {
    top = Math.max(margin, rect.top - height - 6);
  }

  host.style.left = left + "px";
  host.style.top = top + "px";
}

function show() {
  host.style.display = "block";
}

function hide() {
  if (!host) return;

  cancelActive();
  host.style.display = "none";
  currentTranslation = "";
  currentText = "";
  loadingPlaceholder = false;
  copyButton.textContent = "复制";
  copyButton.disabled = true;
  retryButton.hidden = true;
  cancelButton.hidden = true;
  spinnerEl.hidden = true;
  statusEl.classList.remove("error");
  statusEl.textContent = "";
}

function showLoading() {
  loadingPlaceholder = true;
  currentTranslation = "";
  copyButton.textContent = "复制";
  copyButton.disabled = true;
  retryButton.hidden = true;
  cancelButton.hidden = false;
  spinnerEl.hidden = false;
  statusEl.classList.remove("error");
  statusEl.textContent = "翻译中…";
  show();
}

function showResult(translation) {
  currentTranslation = translation || "";
  loadingPlaceholder = false;
  copyButton.disabled = !currentTranslation;
  retryButton.hidden = true;
  cancelButton.hidden = true;
  spinnerEl.hidden = true;
  statusEl.classList.remove("error");
  statusEl.textContent = currentTranslation;
  show();
}

function showError(message) {
  currentTranslation = "";
  loadingPlaceholder = false;
  copyButton.disabled = true;
  retryButton.hidden = !currentText;
  cancelButton.hidden = true;
  spinnerEl.hidden = true;
  statusEl.classList.add("error");
  statusEl.textContent = message;
  show();
}

function selectionInfo() {
  const active = document.activeElement;
  if (active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA")) {
    const start = active.selectionStart;
    const end = active.selectionEnd;
    const rect = active.getBoundingClientRect();
    if (typeof start === "number" && typeof end === "number" && end > start) {
      return { text: active.value.slice(start, end).trim(), rect };
    }
    return { text: "", rect };
  }

  const selection = window.getSelection();
  const text = selection ? selection.toString().trim() : "";
  const range = selection && selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
  return { text, rect: range ? range.getBoundingClientRect() : null };
}

function queueReposition() {
  if (repositionQueued) return;

  repositionQueued = true;
  requestAnimationFrame(() => {
    repositionQueued = false;
    if (!host || host.style.display === "none") return;

    const info = selectionInfo();
    if (info.text && info.text === currentText && info.rect) {
      positionBubble(info.rect);
    } else if (!info.text) {
      hide();
    }
  });
}

function start() {
  const info = selectionInfo();
  ensureBubble();

  if (!info.text) {
    currentText = "";
    showError("未选中文本。");
    return;
  }

  beginTranslate(info.text);
  positionBubble(info.rect || { left: 40, top: 40, bottom: 60 });
}

api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message && message.type === "get-site-info") {
    sendResponse({ hostname: location.hostname });
  } else if (message && message.type === "start-translate") {
    start();
  }
});

document.addEventListener(
  "keydown",
  (event) => {
    if (event.key === "Escape") hide();
  },
  true
);

document.addEventListener(
  "mousedown",
  (event) => {
    if (!host || host.style.display === "none") return;
    if (event.composedPath().includes(host)) return;
    hide();
  },
  true
);

document.addEventListener(
  "mouseup",
  (event) => {
    if (!autoTranslateActive()) return;
    if (host && event.composedPath().includes(host)) return;

    const info = selectionInfo();
    if (!info.text) return;

    clearTimeout(autoTimer);
    autoTimer = setTimeout(() => {
      const fresh = selectionInfo();
      if (!fresh.text || fresh.text !== info.text) return;

      ensureBubble();
      beginTranslate(fresh.text);
      positionBubble(fresh.rect || { left: 40, top: 40, bottom: 60 });
    }, 400);
  }
);

document.addEventListener("scroll", queueReposition, true);
window.addEventListener("resize", queueReposition);

loadSettings();
