"use strict";

const api = globalThis.browser ?? globalThis.chrome;

const LANGUAGES = globalThis.OT_LANGUAGES || [];

const DEFAULTS = {
  source: "auto",
  target: "zh",
  autoTranslate: false,
  disabledSites: [],
  autoTranslateDelay: 400,
  autoTranslateMinLength: 2,
};

const SPEECH_LANGS = {
  zh: "zh-CN",
  en: "en-US",
  ja: "ja-JP",
  ko: "ko-KR",
  fr: "fr-FR",
  de: "de-DE",
  es: "es-ES",
  ru: "ru-RU",
  th: "th-TH",
};

const FALLBACK_RECT = { left: 40, top: 40, bottom: 60 };

const ICONS = {
  copy: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M10.5 3.5v-1a1 1 0 0 0-1-1h-7a1 1 0 0 0-1 1v7a1 1 0 0 0 1 1h1"/></svg>',
  check: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.5 8.5 6.5 11.5 12.5 4.5"/></svg>',
  doc: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 1.5h5l3 3v10H4z"/><path d="M9 1.5v3h3"/><path d="M6 8h4M6 10.5h4"/></svg>',
  more: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true"><circle cx="3" cy="8" r="1.3"/><circle cx="8" cy="8" r="1.3"/><circle cx="13" cy="8" r="1.3"/></svg>',
  close: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8"/></svg>',
  speak: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 6.5h2.5L9 4v8L5.5 9.5H3z"/><path d="M11.2 6.2a2.6 2.6 0 0 1 0 3.6"/><path d="M13 4.4a5.2 5.2 0 0 1 0 7.2"/></svg>',
  stop: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true"><rect x="4.5" y="4.5" width="7" height="7" rx="1"/></svg>',
  replace: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 5h9.5"/><path d="M9 2.5 11.5 5 9 7.5"/><path d="M14 11H4.5"/><path d="M7 8.5 4.5 11 7 13.5"/></svg>',
  retry: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M13.5 8a5.5 5.5 0 1 1-1.7-4"/><path d="M13.5 2.5V6H10"/></svg>',
  swap: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M11 3.5 13.5 6 11 8.5"/><path d="M13.5 6h-8"/><path d="M5 12.5 2.5 10 5 7.5"/><path d="M2.5 10h8"/></svg>',
  settings: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="M2.5 4.5h11M2.5 11.5h11"/><circle cx="6" cy="4.5" r="2" fill="currentColor" stroke="none"/><circle cx="10" cy="11.5" r="2" fill="currentColor" stroke="none"/></svg>',
};

const MENU_ITEMS = [
  ["bilingual", "复制双语", "copy"],
  ["original", "复制原文", "doc"],
  ["speak", "朗读", "speak"],
  ["replace", "替换原文", "replace"],
  ["retranslate", "重新翻译", "retry"],
  ["swap", "互换源/目标语言", "swap"],
  ["settings", "语言设置…", "settings"],
];

let host = null;
let statusEl = null;
let originalEl = null;
let copyButton = null;
let moreButton = null;
let menuEl = null;
let retryButton = null;
let cancelButton = null;
let targetSelect = null;

let currentTranslation = "";
let currentText = "";
let currentEditable = null;
let targetLanguage = DEFAULTS.target;
let sourceLanguage = DEFAULTS.source;
let autoTranslate = false;
let autoDelay = DEFAULTS.autoTranslateDelay;
let autoMinLength = DEFAULTS.autoTranslateMinLength;
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
  if (targetSelect) targetSelect.value = targetLanguage;
}

async function loadSettings() {
  const settings = await api.storage.local.get(DEFAULTS);
  targetLanguage = settings.target || DEFAULTS.target;
  sourceLanguage = settings.source || DEFAULTS.source;
  autoTranslate = Boolean(settings.autoTranslate);
  autoDelay = Number(settings.autoTranslateDelay) || DEFAULTS.autoTranslateDelay;
  autoMinLength =
    Number(settings.autoTranslateMinLength) || DEFAULTS.autoTranslateMinLength;
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
  if (changes.autoTranslateDelay) {
    autoDelay = Number(changes.autoTranslateDelay.newValue) || DEFAULTS.autoTranslateDelay;
  }
  if (changes.autoTranslateMinLength) {
    autoMinLength =
      Number(changes.autoTranslateMinLength.newValue) || DEFAULTS.autoTranslateMinLength;
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
      statusEl.classList.remove("waiting");
      statusEl.classList.add("streaming");
      statusEl.textContent = "";
    }
    currentTranslation += message.delta || "";
    statusEl.textContent = currentTranslation;
  } else if (message.type === "retry") {
    statusEl.textContent =
      "正在等待本地服务启动…（第 " + message.attempt + "/" + message.total + " 次尝试）";
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
  originalEl.textContent = text;
  closeMenu();
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

function createIcon(name) {
  const markup = ICONS[name];
  if (!markup) return document.createTextNode("");

  const doc = new DOMParser().parseFromString(markup, "image/svg+xml");
  return document.importNode(doc.documentElement, true);
}

function setButtonIcon(button, name) {
  button.replaceChildren(createIcon(name));
}

function renderMenuItem(item, label) {
  const span = document.createElement("span");
  span.className = "menu-label";
  span.textContent = label;
  item.replaceChildren(createIcon(item.dataset.icon), span);
}

function flashButton(button, restoreIcon, delay = 1200) {
  setButtonIcon(button, "check");
  clearTimeout(copyTimer);
  copyTimer = setTimeout(() => {
    if (button) setButtonIcon(button, restoreIcon);
  }, delay);
}

function flashMenuItem(item, restoreLabel) {
  const previousIcon = item.dataset.icon;
  item.dataset.icon = "check";
  renderMenuItem(item, "已复制");
  clearTimeout(copyTimer);
  copyTimer = setTimeout(() => {
    if (!item.isConnected) return;
    item.dataset.icon = previousIcon;
    renderMenuItem(item, restoreLabel);
  }, 1200);
}

function writeClipboard(text, onDone) {
  if (!text) return;

  const done = () => onDone();

  navigator.clipboard.writeText(text).then(done, () => {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    document.documentElement.appendChild(textarea);
    textarea.select();
    document.execCommand("copy");
    textarea.remove();
    done();
  });
}

function toggleSpeech() {
  if (!("speechSynthesis" in window) || !currentTranslation) return;

  const synth = window.speechSynthesis;
  if (synth.speaking) {
    synth.cancel();
    return;
  }

  const utterance = new SpeechSynthesisUtterance(currentTranslation);
  utterance.lang = SPEECH_LANGS[targetLanguage] || targetLanguage;
  utterance.onend = updateMenuState;
  utterance.onerror = updateMenuState;
  synth.speak(utterance);
}

function replaceOriginal() {
  if (!currentEditable || !currentTranslation) return;

  try {
    if (currentEditable.type === "input") {
      const element = currentEditable.element;
      element.focus();
      element.setRangeText(
        currentTranslation,
        currentEditable.start,
        currentEditable.end,
        "end"
      );
      element.dispatchEvent(new Event("input", { bubbles: true }));
    } else {
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(currentEditable.range);
      document.execCommand("insertText", false, currentTranslation);
    }
  } catch (error) {
    showError("无法替换原文。");
  }
}

async function swapLanguages() {
  if (sourceLanguage === "auto") return;

  const previousSource = sourceLanguage;
  sourceLanguage = targetLanguage;
  targetLanguage = previousSource;
  await api.storage.local.set({ source: sourceLanguage, target: targetLanguage });
  syncControls();
  if (currentText) beginTranslate(currentText);
}

function openMenu() {
  updateMenuState();
  menuEl.hidden = false;
}

function closeMenu() {
  if (menuEl) menuEl.hidden = true;
}

function updateMenuState() {
  if (!menuEl) return;

  const bilingual = menuEl.querySelector('[data-action="bilingual"]');
  const original = menuEl.querySelector('[data-action="original"]');
  const speak = menuEl.querySelector('[data-action="speak"]');
  const replace = menuEl.querySelector('[data-action="replace"]');
  const retranslate = menuEl.querySelector('[data-action="retranslate"]');
  const swap = menuEl.querySelector('[data-action="swap"]');

  bilingual.disabled = !currentTranslation;
  original.disabled = !currentText;
  speak.hidden = !("speechSynthesis" in window);
  speak.disabled = !currentTranslation;
  const speaking = Boolean(window.speechSynthesis && window.speechSynthesis.speaking);
  speak.dataset.icon = speaking ? "stop" : "speak";
  renderMenuItem(speak, speaking ? "停止朗读" : "朗读");
  replace.hidden = !currentEditable || !currentTranslation;
  replace.disabled = !currentTranslation;
  retranslate.disabled = !currentText;
  swap.disabled = sourceLanguage === "auto";
}

function menuAction(item) {
  const action = item.dataset.action;

  if (action === "bilingual") {
    writeClipboard(currentText + "\n\n" + currentTranslation, () => {
      flashMenuItem(item, "复制双语");
    });
  } else if (action === "original") {
    writeClipboard(currentText, () => {
      flashMenuItem(item, "复制原文");
    });
  } else if (action === "speak") {
    toggleSpeech();
    updateMenuState();
  } else if (action === "replace") {
    closeMenu();
    replaceOriginal();
  } else if (action === "retranslate") {
    closeMenu();
    if (currentText) beginTranslate(currentText);
  } else if (action === "swap") {
    closeMenu();
    swapLanguages();
  } else if (action === "settings") {
    closeMenu();
    api.runtime.sendMessage({ type: "open-options" }).catch(() => {});
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
    "@keyframes ot-shimmer { to { background-position: -200% 0; } }",
    "@keyframes ot-blink { 50% { opacity: 0; } }",
    "@keyframes ot-menu-in { from { opacity: 0; transform: translateY(4px) scale(.98); } }",
    ".card { position: relative; font: 13px/1.5 system-ui, sans-serif; color: #1f2937;",
    "  background: #fff; border: 1px solid #d1d5db; border-radius: 10px;",
    "  box-shadow: 0 10px 30px rgba(0,0,0,.16); max-width: 460px; min-width: 220px;",
    "  padding: 10px 12px; }",
    ".status { font-size: 14px; line-height: 1.55; white-space: pre-wrap;",
    "  word-break: break-word; min-height: 1.5em; }",
    ".status.waiting { background-image: linear-gradient(90deg, #9ca3af 0%, #374151 50%, #9ca3af 100%);",
    "  background-size: 200% 100%; -webkit-background-clip: text; background-clip: text;",
    "  color: transparent; animation: ot-shimmer 1.4s linear infinite; }",
    ".status.streaming::after { content: \"▍\"; margin-left: 1px;",
    "  animation: ot-blink 1s steps(1) infinite; }",
    ".status.error { color: #b91c1c; }",
    ".row { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; margin-top: 8px; }",
    ".spacer { flex: 1; }",
    ".select-wrap { position: relative; display: inline-flex; align-items: center; }",
    ".select-wrap::after { content: \"\"; position: absolute; right: 7px; top: 50%; width: 5px;",
    "  height: 5px; pointer-events: none; border-right: 1.6px solid #9096a0;",
    "  border-bottom: 1.6px solid #9096a0; transform: translateY(-70%) rotate(45deg); }",
    "select { appearance: none; -webkit-appearance: none; font: inherit; font-size: 12.5px;",
    "  color: #4b5563; background-color: rgba(0,0,0,.05); border: 0; border-radius: 6px;",
    "  padding: 3px 20px 3px 7px; cursor: pointer; }",
    "select:hover { background-color: rgba(0,0,0,.08); }",
    "button { font: inherit; padding: 3px 8px; min-height: 24px; border: 0; border-radius: 6px;",
    "  background: none; color: #4b5563; cursor: pointer; display: inline-flex;",
    "  align-items: center; justify-content: center; gap: 6px; }",
    "button:hover { background: rgba(0,0,0,.06); }",
    "button:disabled { color: #c3c8cf; }",
    "button[hidden] { display: none; }",
    "button:focus-visible, select:focus-visible { outline: 2px solid #4f7cff; outline-offset: 1px; }",
    ".close-button { opacity: 0; pointer-events: none; transition: opacity .15s; }",
    ".card:hover .close-button, .close-button:focus-visible { opacity: 1; pointer-events: auto; }",
    "@media (hover: none) { .close-button { opacity: 1; pointer-events: auto; } }",
    ".menu { position: absolute; right: 8px; bottom: 38px; z-index: 3; min-width: 150px;",
    "  display: flex; flex-direction: column; padding: 4px; background: #fff;",
    "  border: 1px solid #e5e7eb; border-radius: 10px; box-shadow: 0 10px 26px rgba(0,0,0,.16);",
    "  animation: ot-menu-in .12s ease-out; }",
    ".menu[hidden] { display: none; }",
    ".menu-item { justify-content: flex-start; text-align: left; white-space: nowrap; }",
    "svg { flex: none; }",
    "details.original { margin-top: 8px; }",
    "details.original summary { cursor: pointer; color: #6b7280; font-size: 12px;",
    "  list-style: none; display: inline-flex; align-items: center; gap: 4px; }",
    "details.original summary::-webkit-details-marker { display: none; }",
    "details.original summary::after { content: \"\"; width: 4px; height: 4px;",
    "  border-right: 1.4px solid currentColor; border-bottom: 1.4px solid currentColor;",
    "  transform: rotate(-45deg); transition: transform .15s; }",
    "details.original[open] summary::after { transform: rotate(45deg); }",
    ".original-text { margin-top: 4px; color: #6b7280; white-space: pre-wrap;",
    "  word-break: break-word; max-height: 96px; overflow: auto; }",
    "@media (prefers-color-scheme: dark) {",
    "  .card { color: #e5e7eb; background: #2b2d31; border-color: #3a3d42; }",
    "  select { color: #e5e7eb; background-color: rgba(255,255,255,.1); }",
    "  select:hover { background-color: rgba(255,255,255,.14); }",
    "  button { color: #d1d5db; }",
    "  button:hover { background: rgba(255,255,255,.08); }",
    "  button:disabled { color: #6b7280; }",
    "  .menu { background: #232529; border-color: #3a3d42; }",
    "  .status.waiting { background-image: linear-gradient(90deg, #6b7280 0%, #e5e7eb 50%, #6b7280 100%); }",
    "  .status.error { color: #fca5a5; }",
    "  details.original summary, .original-text { color: #9ca3af; }",
    "}",
    "@media (prefers-reduced-motion: reduce) {",
    "  .status.waiting { animation: none; background-image: none; color: #6b7280; }",
    "  .status.streaming::after { animation: none; }",
    "  .close-button { transition: none; }",
    "  .menu { animation: none; }",
    "  details.original summary::after { transition: none; }",
    "}",
  ].join("\n");

  const card = document.createElement("div");
  card.className = "card";
  card.setAttribute("role", "region");
  card.setAttribute("aria-label", "OpenTranslator 译文");

  statusEl = document.createElement("div");
  statusEl.className = "status";
  statusEl.setAttribute("role", "status");
  statusEl.setAttribute("aria-live", "polite");

  const originalDetails = document.createElement("details");
  originalDetails.className = "original";
  const originalSummary = document.createElement("summary");
  originalSummary.textContent = "原文";
  originalEl = document.createElement("div");
  originalEl.className = "original-text";
  originalDetails.append(originalSummary, originalEl);

  const row = document.createElement("div");
  row.className = "row";

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
    cancelButton.hidden = true;
    retryButton.hidden = !currentText;
    statusEl.classList.remove("error");
    statusEl.classList.remove("waiting");
    statusEl.classList.remove("streaming");
    statusEl.textContent = "已取消。";
  });

  copyButton = document.createElement("button");
  copyButton.className = "copy-button";
  setButtonIcon(copyButton, "copy");
  copyButton.title = "复制译文";
  copyButton.setAttribute("aria-label", "复制译文");
  copyButton.hidden = true;
  copyButton.addEventListener("click", () => {
    if (!currentTranslation) return;
    writeClipboard(currentTranslation, () => {
      flashButton(copyButton, "copy");
    });
  });

  moreButton = document.createElement("button");
  moreButton.className = "more-button";
  setButtonIcon(moreButton, "more");
  moreButton.title = "更多操作";
  moreButton.setAttribute("aria-label", "更多操作");
  moreButton.hidden = true;
  moreButton.addEventListener("click", () => {
    if (menuEl.hidden) {
      openMenu();
    } else {
      closeMenu();
    }
  });

  const closeButton = document.createElement("button");
  closeButton.className = "close-button";
  setButtonIcon(closeButton, "close");
  closeButton.title = "关闭";
  closeButton.setAttribute("aria-label", "关闭");
  closeButton.addEventListener("click", hide);

  menuEl = document.createElement("div");
  menuEl.className = "menu";
  menuEl.hidden = true;
  menuEl.setAttribute("role", "menu");

  for (const [action, label, icon] of MENU_ITEMS) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "menu-item";
    item.dataset.action = action;
    item.dataset.icon = icon;
    item.setAttribute("role", "menuitem");
    renderMenuItem(item, label);
    menuEl.append(item);
  }

  menuEl.addEventListener("click", (event) => {
    const item = event.target.closest("[data-action]");
    if (!item || item.disabled || item.hidden) return;
    menuAction(item);
  });

  const selectWrap = document.createElement("span");
  selectWrap.className = "select-wrap";
  selectWrap.append(targetSelect);

  row.append(selectWrap, spacer, retryButton, cancelButton, copyButton, moreButton, closeButton);
  card.append(statusEl, originalDetails, row, menuEl);
  shadow.append(style, card);
  document.documentElement.append(host);

  syncControls();
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
  if ("speechSynthesis" in window) window.speechSynthesis.cancel();
  closeMenu();
  host.style.display = "none";
  currentTranslation = "";
  currentText = "";
  currentEditable = null;
  loadingPlaceholder = false;
  setButtonIcon(copyButton, "copy");
  copyButton.hidden = true;
  moreButton.hidden = true;
  retryButton.hidden = true;
  cancelButton.hidden = true;
  statusEl.classList.remove("error");
  statusEl.classList.remove("waiting");
  statusEl.classList.remove("streaming");
  statusEl.textContent = "";
}

function showLoading() {
  loadingPlaceholder = true;
  currentTranslation = "";
  setButtonIcon(copyButton, "copy");
  copyButton.hidden = true;
  moreButton.hidden = true;
  retryButton.hidden = true;
  cancelButton.hidden = false;
  statusEl.classList.remove("error");
  statusEl.classList.add("waiting");
  statusEl.classList.remove("streaming");
  statusEl.textContent = "翻译中…";
  show();
}

function showResult(translation) {
  currentTranslation = translation || "";
  loadingPlaceholder = false;
  copyButton.hidden = false;
  copyButton.disabled = !currentTranslation;
  moreButton.hidden = false;
  retryButton.hidden = true;
  cancelButton.hidden = true;
  statusEl.classList.remove("error");
  statusEl.classList.remove("waiting");
  statusEl.classList.remove("streaming");
  statusEl.textContent = currentTranslation;
  show();
}

function showError(message) {
  currentTranslation = "";
  loadingPlaceholder = false;
  copyButton.hidden = true;
  moreButton.hidden = false;
  retryButton.hidden = !currentText;
  cancelButton.hidden = true;
  statusEl.classList.remove("waiting");
  statusEl.classList.remove("streaming");
  statusEl.classList.add("error");
  statusEl.textContent = message;
  show();
}

function selectionInfo(preferTarget) {
  const active = document.activeElement;
  const focusInInput =
    active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA");
  if (focusInInput && (!preferTarget || preferTarget === active)) {
    const start = active.selectionStart;
    const end = active.selectionEnd;
    const rect = active.getBoundingClientRect();
    if (typeof start === "number" && typeof end === "number" && end > start) {
      const raw = active.value.slice(start, end);
      const leading = raw.length - raw.trimStart().length;
      const trailing = raw.length - raw.trimEnd().length;
      return {
        text: raw.trim(),
        rect,
        editable: {
          type: "input",
          element: active,
          start: start + leading,
          end: end - trailing,
        },
      };
    }
    return { text: "", rect, editable: null };
  }

  const selection = window.getSelection();
  const text = selection ? selection.toString().trim() : "";
  const range = selection && selection.rangeCount > 0 ? selection.getRangeAt(0) : null;

  let editable = null;
  if (range && !selection.isCollapsed) {
    const container = range.startContainer;
    const element = container.nodeType === 1 ? container : container.parentElement;
    if (element && element.isContentEditable) {
      editable = { type: "contenteditable", range: range.cloneRange() };
    }
  }

  return { text, rect: range ? range.getBoundingClientRect() : null, editable };
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
    currentEditable = null;
    if (window.top === window) showError("未选中文本。");
    return;
  }

  currentEditable = info.editable;
  beginTranslate(info.text);
  positionBubble(info.rect || FALLBACK_RECT);
}

async function translateClipboard() {
  if (!document.hasFocus()) return;

  ensureBubble();

  let text = "";
  try {
    text = (await navigator.clipboard.readText()).trim();
  } catch (error) {
    text = readClipboardFallback();
  }

  if (!text) {
    currentText = "";
    currentEditable = null;
    showError("剪贴板为空或无法读取。");
    return;
  }

  currentEditable = null;
  beginTranslate(text);
  positionBubble({
    left: Math.max(8, window.innerWidth / 2 - 160),
    top: 60,
    bottom: 80,
  });
}

function readClipboardFallback() {
  const textarea = document.createElement("textarea");
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.documentElement.appendChild(textarea);
  textarea.focus();

  let text = "";
  try {
    if (document.execCommand("paste")) text = textarea.value;
  } catch (error) {
    text = "";
  }

  textarea.remove();
  return text.trim();
}

api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message && message.type === "get-site-info") {
    sendResponse({ hostname: location.hostname });
  } else if (message && message.type === "start-translate") {
    start();
  } else if (message && message.type === "translate-clipboard") {
    translateClipboard();
  }
});

document.addEventListener(
  "keydown",
  (event) => {
    if (event.key !== "Escape") return;
    if (menuEl && !menuEl.hidden) {
      closeMenu();
      return;
    }
    hide();
  },
  true
);

document.addEventListener(
  "mousedown",
  (event) => {
    if (!host || host.style.display === "none") return;
    if (event.composedPath().includes(host)) return;
    if (menuEl && !menuEl.hidden) {
      closeMenu();
      return;
    }
    hide();
  },
  true
);

document.addEventListener("mouseup", (event) => {
  if (!autoTranslateActive()) return;
  if (host && event.composedPath().includes(host)) return;

  const target = event.target;
  if (target && typeof target.closest === "function" && target.closest("pre, code")) {
    return;
  }

  const info = selectionInfo(event.target);
  if (!info.text) return;
  if ([...info.text].length < Math.max(1, autoMinLength)) return;

  clearTimeout(autoTimer);
  autoTimer = setTimeout(() => {
    const fresh = selectionInfo(event.target);
    if (!fresh.text || fresh.text !== info.text) return;
    if ([...fresh.text].length < Math.max(1, autoMinLength)) return;

    currentEditable = fresh.editable;
    ensureBubble();
    beginTranslate(fresh.text);
    positionBubble(fresh.rect || FALLBACK_RECT);
  }, autoDelay);
});

document.addEventListener("scroll", queueReposition, true);
window.addEventListener("resize", queueReposition);

loadSettings();
