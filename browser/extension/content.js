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
  typeTranslate: false,
  typeTranslateDelay: 500,
  typeTranslateMinLength: 2,
  recentTargets: [],
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

// Shared Apple-flavoured design tokens, injected into both shadow roots.
const UI_TOKENS_CSS = [
  ":host { color-scheme: light dark;",
  "  --ot-label: rgba(0,0,0,.85); --ot-label-2: rgba(0,0,0,.5); --ot-label-3: rgba(0,0,0,.26);",
  "  --ot-fill: rgba(120,120,128,.12); --ot-fill-hover: rgba(120,120,128,.2);",
  "  --ot-accent: #007aff; --ot-separator: rgba(60,60,67,.14);",
  "  --ot-bg: rgba(255,255,255,.78); --ot-bg-solid: #ffffff;",
  "  --ot-hairline: rgba(0,0,0,.08);",
  "  --ot-shadow: 0 1px 2px rgba(0,0,0,.06), 0 12px 32px rgba(0,0,0,.12);",
  "  --ot-radius: 12px; --ot-radius-sm: 8px;",
  "  --ot-font: -apple-system, BlinkMacSystemFont, \"SF Pro Text\", \"Segoe UI\", system-ui, sans-serif;",
  "}",
  "@media (prefers-color-scheme: dark) { :host {",
  "  --ot-label: rgba(255,255,255,.85); --ot-label-2: rgba(255,255,255,.55); --ot-label-3: rgba(255,255,255,.3);",
  "  --ot-fill: rgba(120,120,128,.24); --ot-fill-hover: rgba(120,120,128,.36);",
  "  --ot-accent: #0a84ff; --ot-separator: rgba(255,255,255,.12);",
  "  --ot-bg: rgba(30,30,32,.72); --ot-bg-solid: #2c2c2e;",
  "  --ot-hairline: rgba(255,255,255,.12);",
  "  --ot-shadow: 0 1px 2px rgba(0,0,0,.4), 0 16px 40px rgba(0,0,0,.5);",
  "} }",
].join("\n");

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
  settings: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><g transform="scale(0.6667)" stroke-width="2.25"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></g></svg>',
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

let typeTranslate = false;
let typeDelay = DEFAULTS.typeTranslateDelay;
let typeMinLength = DEFAULTS.typeTranslateMinLength;
let typeHost = null;
let typeStatusEl = null;
let typeHintEl = null;
let typeHintKeysEl = null;
let typeHintTextEl = null;
let typeSelect = null;
let typeSelectController = null;
let typeSettingsEl = null;
let typePort = null;
let typeSession = null;
let typeTimer = null;
let typeRequestSeq = 0;
let typeActiveRequest = 0;
let typeTranslation = "";
let typeComposing = false;
let typeApplying = false;
let typeLastText = "";
let typeAnchorRect = null;
let typeRepositionQueued = false;
let typeRecentTargets = [];
let typeNotice = "";

function siteDisabled() {
  return disabledSites.includes(location.hostname);
}

function autoTranslateActive() {
  return autoTranslate && !siteDisabled();
}

function syncControls() {
  if (targetSelect) {
    targetSelect.value = targetLanguage;
    if (globalThis.OTSelect) OTSelect.sync(targetSelect);
  }
  if (typeSelect) {
    typeSelect.value = targetLanguage;
    if (globalThis.OTSelect) OTSelect.sync(typeSelect);
  }
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
  typeTranslate = Boolean(settings.typeTranslate);
  typeDelay = Number(settings.typeTranslateDelay) || DEFAULTS.typeTranslateDelay;
  typeMinLength =
    Number(settings.typeTranslateMinLength) || DEFAULTS.typeTranslateMinLength;
  typeRecentTargets = Array.isArray(settings.recentTargets) ? settings.recentTargets : [];
  syncControls();
}

api.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;

  if (changes.target && changes.target.newValue) {
    targetLanguage = changes.target.newValue;

    // Track recently used targets once (top frame) so the typing bubble can
    // offer a quick cycle; every frame sees the change but must not rewrite it.
    const previous = changes.target.oldValue;
    if (window.top === window && previous && previous !== targetLanguage) {
      typeRecentTargets = [
        previous,
        ...typeRecentTargets.filter((code) => code !== previous),
      ].slice(0, 3);
      api.storage.local.set({ recentTargets: typeRecentTargets });
    }
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
  if (changes.typeTranslate) {
    typeTranslate = Boolean(changes.typeTranslate.newValue);
    if (!typeTranslate) typeHide();
  }
  if (changes.typeTranslateDelay) {
    typeDelay =
      Number(changes.typeTranslateDelay.newValue) || DEFAULTS.typeTranslateDelay;
  }
  if (changes.typeTranslateMinLength) {
    typeMinLength =
      Number(changes.typeTranslateMinLength.newValue) || DEFAULTS.typeTranslateMinLength;
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
    UI_TOKENS_CSS,
    (globalThis.OTSelect && OTSelect.cssText) || "",
    "@keyframes ot-pulse { 0%, 100% { opacity: .45; } 50% { opacity: .95; } }",
    "@keyframes ot-caret { 0%, 100% { opacity: 1; } 50% { opacity: .15; } }",
    "@keyframes ot-menu-in { from { opacity: 0; transform: translateY(4px) scale(.98); } }",
    ".card { position: relative; font-family: var(--ot-font); font-size: 13px; line-height: 1.5;",
    "  -webkit-font-smoothing: antialiased; color: var(--ot-label);",
    "  background: var(--ot-bg-solid); border-radius: var(--ot-radius);",
    "  box-shadow: inset 0 0 0 .5px var(--ot-hairline), var(--ot-shadow);",
    "  max-width: 460px; min-width: 220px; padding: 10px 12px; }",
    "@supports ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px))) {",
    "  .card { background: var(--ot-bg); -webkit-backdrop-filter: blur(20px) saturate(180%); backdrop-filter: blur(20px) saturate(180%); }",
    "}",
    ".status { font-size: 13.5px; line-height: 1.5; white-space: pre-wrap;",
    "  word-break: break-word; min-height: 1.5em; }",
    ".status.waiting { color: var(--ot-label-3); animation: ot-pulse 1.4s ease-in-out infinite; }",
    ".status.streaming::after { content: \"\"; display: inline-block; width: 2px; height: 1em;",
    "  margin-left: 2px; border-radius: 1px; background: currentColor; vertical-align: -.12em;",
    "  animation: ot-caret 1.1s ease-in-out infinite; }",
    ".status.error { color: #d70015; }",
    ".row { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; margin-top: 8px; }",
    ".spacer { flex: 1; }",
    ".row .ot-select { flex: none; }",
    "button { font: inherit; font-size: 12.5px; padding: 3px 8px; min-height: 24px; border: 0;",
    "  border-radius: var(--ot-radius-sm); background: none; color: var(--ot-label-2); cursor: pointer;",
    "  display: inline-flex; align-items: center; justify-content: center; gap: 6px;",
    "  transition: background-color .15s ease, color .15s ease; }",
    "button:hover { background: var(--ot-fill); }",
    "button:active { background: var(--ot-fill-hover); }",
    "button:disabled { color: var(--ot-label-3); }",
    "button[hidden] { display: none; }",
    "button:focus-visible { outline: 2px solid var(--ot-accent); outline-offset: 1px; }",
    ".close-button { opacity: 0; pointer-events: none; transition: opacity .15s ease; }",
    ".card:hover .close-button, .close-button:focus-visible { opacity: 1; pointer-events: auto; }",
    "@media (hover: none) { .close-button { opacity: 1; pointer-events: auto; } }",
    ".menu { position: absolute; right: 8px; bottom: 38px; z-index: 3; min-width: 150px;",
    "  display: flex; flex-direction: column; padding: 4px; background: var(--ot-bg-solid);",
    "  border-radius: 10px; box-shadow: inset 0 0 0 .5px var(--ot-hairline), var(--ot-shadow);",
    "  animation: ot-menu-in .16s cubic-bezier(.25,.1,.25,1); }",
    "@supports ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px))) {",
    "  .menu { background: var(--ot-bg); -webkit-backdrop-filter: blur(20px) saturate(180%); backdrop-filter: blur(20px) saturate(180%); }",
    "}",
    ".menu[hidden] { display: none; }",
    ".menu-item { justify-content: flex-start; text-align: left; white-space: nowrap; color: var(--ot-label); }",
    ".menu-item:hover { background: var(--ot-fill); }",
    "svg { flex: none; }",
    "details.original { margin-top: 8px; }",
    "details.original summary { cursor: pointer; color: var(--ot-label-2); font-size: 12px;",
    "  list-style: none; display: inline-flex; align-items: center; gap: 4px; }",
    "details.original summary::-webkit-details-marker { display: none; }",
    "details.original summary::after { content: \"\"; width: 4px; height: 4px;",
    "  border-right: 1.4px solid currentColor; border-bottom: 1.4px solid currentColor;",
    "  transform: rotate(-45deg); transition: transform .15s; }",
    "details.original[open] summary::after { transform: rotate(45deg); }",
    ".original-text { margin-top: 4px; color: var(--ot-label-2); white-space: pre-wrap;",
    "  word-break: break-word; max-height: 96px; overflow: auto;",
    "  scrollbar-width: thin; scrollbar-color: rgba(120,120,128,.4) transparent; }",
    ".original-text::-webkit-scrollbar { width: 6px; }",
    ".original-text::-webkit-scrollbar-thumb { background: rgba(120,120,128,.4); border-radius: 3px; }",
    ".original-text::-webkit-scrollbar-track { background: transparent; }",
    "@media (prefers-color-scheme: dark) {",
    "  .status.error { color: #ff453a; }",
    "}",
    "@media (prefers-reduced-motion: reduce) {",
    "  .status.waiting { animation: none; color: var(--ot-label-3); }",
    "  .status.streaming::after { animation: none; }",
    "  .close-button, button, details.original summary::after { transition: none; }",
    "  .menu { animation: none; }",
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

  const targetSelectController = globalThis.OTSelect
    ? OTSelect.enhance(targetSelect, { title: "目标语言" })
    : null;

  row.append(
    targetSelectController ? targetSelectController.element : targetSelect,
    spacer,
    retryButton,
    cancelButton,
    copyButton,
    moreButton,
    closeButton
  );
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

// --- 边写边译（输入时翻译，Tab 上屏）----------------------------------------

const TYPE_DELIMITERS = "。！？!?；;\n\r";

function connectTypePort() {
  if (typePort) return typePort;

  typePort = api.runtime.connect({ name: "translate" });
  typePort.onMessage.addListener(onTypePortMessage);
  typePort.onDisconnect.addListener(() => {
    typePort = null;
    if (typeActiveRequest) {
      typeActiveRequest = 0;
      typeHide();
    }
  });

  return typePort;
}

function typePost(message) {
  try {
    connectTypePort().postMessage(message);
  } catch (error) {
    void error;
  }
}

function onTypePortMessage(message) {
  if (!message || message.requestId !== typeActiveRequest) return;

  if (message.type === "delta") {
    typeTranslation += message.delta || "";
    typeRender(typeTranslation, "streaming");
  } else if (message.type === "retry") {
    typeRender("正在等待本地翻译服务…", "waiting");
  } else if (message.type === "done") {
    typeActiveRequest = 0;
    typeTranslation = message.translation || typeTranslation;
    typeRender(typeTranslation, "");
  } else if (message.type === "error") {
    typeActiveRequest = 0;
    typeHide();
  }
}

function languageLabel(code) {
  const match = LANGUAGES.find(([value]) => value === code);
  return match ? match[1] : code;
}

function updateTypeTargetLabel() {
  if (typeSelect && globalThis.OTSelect) OTSelect.sync(typeSelect);
}

function closeTypeMenu() {
  if (typeSelectController) typeSelectController.close();
}

/// Switches the target language from the typing bubble and retranslates the
/// current sentence immediately, so the edited field never loses focus.
function setTypeTarget(code) {
  if (!code || code === targetLanguage) return;

  targetLanguage = code;
  syncControls();
  updateTypeTargetLabel();

  api.storage.local.set({ target: code }).then(() => {
    if (!typeSession || !typeHost || typeHost.style.display === "none") return;

    typeLastText = "";
    typeNotice = "目标语言：" + languageLabel(code);
    runTypeTranslation(typeSession);
  });
}

const TYPE_FALLBACK_TARGETS = ["zh", "en", "ja", "ko"];

function typeTargetChoices() {
  const choices = [targetLanguage];

  for (const code of typeRecentTargets) {
    if (!choices.includes(code)) choices.push(code);
  }

  if (choices.length < 2) {
    for (const code of TYPE_FALLBACK_TARGETS) {
      if (!choices.includes(code)) choices.push(code);
    }
  }

  return choices;
}

function cycleTypeTarget() {
  if (!typeHost || typeHost.style.display === "none") return;

  const choices = typeTargetChoices();
  if (choices.length < 2) return;

  const index = choices.indexOf(targetLanguage);
  setTypeTarget(choices[(index + 1) % choices.length]);
}

function ensureTypeBubble() {
  if (typeHost) return;

  typeHost = document.createElement("div");
  typeHost.dataset.opentranslator = "type";
  typeHost.style.position = "fixed";
  typeHost.style.zIndex = "2147483647";
  typeHost.style.top = "0";
  typeHost.style.left = "0";
  typeHost.style.display = "none";
  typeHost.style.pointerEvents = "none";

  const shadow = typeHost.attachShadow({ mode: "open" });

  const style = document.createElement("style");
  style.textContent = [
    UI_TOKENS_CSS,
    (globalThis.OTSelect && OTSelect.cssText) || "",
    "@keyframes ot-type-pulse { 0%, 100% { opacity: .45; } 50% { opacity: .95; } }",
    "@keyframes ot-type-caret { 0%, 100% { opacity: 1; } 50% { opacity: .15; } }",
    ".type-card { position: relative; pointer-events: auto;",
    "  --ot-control-height: 22px; --ot-control-font: 12px;",
    "  font-family: var(--ot-font); font-size: 13px; line-height: 1.5;",
    "  -webkit-font-smoothing: antialiased; color: var(--ot-label);",
    "  background: var(--ot-bg-solid); border-radius: var(--ot-radius);",
    "  box-shadow: inset 0 0 0 .5px var(--ot-hairline), var(--ot-shadow);",
    "  max-width: 440px; padding: 10px 12px; }",
    "@supports ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px))) {",
    "  .type-card { background: var(--ot-bg); -webkit-backdrop-filter: blur(20px) saturate(180%); backdrop-filter: blur(20px) saturate(180%); }",
    "}",
    ".type-status { font-size: 13.5px; line-height: 1.5; white-space: pre-wrap;",
    "  word-break: break-word; max-height: 6.5em; overflow: hidden; }",
    ".type-status.overflowing { -webkit-mask-image: linear-gradient(#000 calc(100% - 1.5em), transparent);",
    "  mask-image: linear-gradient(#000 calc(100% - 1.5em), transparent); }",
    ".type-status.waiting { color: var(--ot-label-3); animation: ot-type-pulse 1.4s ease-in-out infinite; }",
    ".type-status.streaming::after { content: \"\"; display: inline-block; width: 2px; height: 1em;",
    "  margin-left: 2px; border-radius: 1px; background: currentColor; vertical-align: -.12em;",
    "  animation: ot-type-caret 1.1s ease-in-out infinite; }",
    ".type-row { display: flex; align-items: center; gap: 6px; margin-top: 8px; }",
    ".type-row .ot-select { flex: none; }",
    ".type-hint { flex: 1; min-width: 0; font-size: 11px; color: var(--ot-label-3);",
    "  white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }",
    ".type-hint kbd { font-family: inherit; font-size: 10.5px; line-height: 1;",
    "  padding: 2px 4px 3px; border-radius: 4px; background: var(--ot-fill);",
    "  color: var(--ot-label-2); box-shadow: inset 0 -.5px 0 var(--ot-hairline); }",
    ".type-hint [hidden] { display: none; }",
    ".type-settings { padding: 2px 5px; min-height: 22px; border: 0; border-radius: var(--ot-radius-sm);",
    "  background: none; color: var(--ot-label-2); cursor: pointer; display: inline-flex;",
    "  align-items: center; transition: background-color .15s ease; }",
    ".type-settings:hover { background: var(--ot-fill); }",
    ".ot-select-menu-inline { margin: 6px -6px -4px; padding: 5px 4px 4px;",
    "  border-top: .5px solid var(--ot-separator); max-height: 236px; }",
    "@media (prefers-reduced-motion: reduce) {",
    "  .type-status.waiting { animation: none; color: var(--ot-label-3); }",
    "  .type-status.streaming::after { animation: none; }",
    "  .type-settings { transition: none; }",
    "}",
  ].join("\n");

  const card = document.createElement("div");
  card.className = "type-card";

  // Keep the caret in the edited field while using the bubble controls; a
  // default-prevented mousedown does not move focus.
  card.addEventListener("mousedown", (event) => {
    event.preventDefault();
    event.stopPropagation();
  });

  typeStatusEl = document.createElement("div");
  typeStatusEl.className = "type-status";
  typeStatusEl.setAttribute("role", "status");
  typeStatusEl.setAttribute("aria-live", "polite");

  typeSelect = document.createElement("select");
  typeSelect.className = "type-select";
  typeSelect.title = "目标语言";
  buildOptions(typeSelect, LANGUAGES);
  if (!LANGUAGES.some(([code]) => code === targetLanguage)) {
    buildOptions(typeSelect, [[targetLanguage, targetLanguage]]);
  }
  typeSelect.value = targetLanguage;
  typeSelect.addEventListener("change", () => setTypeTarget(typeSelect.value));

  typeHintEl = document.createElement("span");
  typeHintEl.className = "type-hint";
  typeHintKeysEl = document.createElement("span");
  typeHintKeysEl.className = "type-hint-keys";
  const tabKey = document.createElement("kbd");
  tabKey.textContent = "Tab";
  const escKey = document.createElement("kbd");
  escKey.textContent = "Esc";
  typeHintKeysEl.append(
    tabKey,
    document.createTextNode(" 采用译文 · "),
    escKey,
    document.createTextNode(" 忽略")
  );
  typeHintTextEl = document.createElement("span");
  typeHintTextEl.className = "type-hint-text";
  typeHintTextEl.hidden = true;
  typeHintEl.append(typeHintKeysEl, typeHintTextEl);

  typeSettingsEl = document.createElement("button");
  typeSettingsEl.type = "button";
  typeSettingsEl.className = "type-settings";
  typeSettingsEl.title = "打开设置…";
  typeSettingsEl.setAttribute("aria-label", "打开设置");
  setButtonIcon(typeSettingsEl, "settings");
  typeSettingsEl.addEventListener("click", () => {
    closeTypeMenu();
    api.runtime.sendMessage({ type: "open-options" }).catch(() => {});
  });

  const row = document.createElement("div");
  row.className = "type-row";
  row.append(typeHintEl, typeSettingsEl);

  card.append(typeStatusEl, row);

  if (globalThis.OTSelect) {
    typeSelectController = OTSelect.enhance(typeSelect, {
      title: "目标语言（Alt+Shift+L 切换）",
      menuContainer: card,
      onToggle: () => typePosition(),
    });
    row.prepend(typeSelectController.element);
  } else {
    row.prepend(typeSelect);
  }

  shadow.append(style, card);
  document.documentElement.append(typeHost);
  updateTypeTargetLabel();
}

function updateTypeHint(text, state) {
  if (!typeHintKeysEl || !typeHintTextEl) return;

  if (typeNotice) {
    typeHintKeysEl.hidden = true;
    typeHintTextEl.hidden = false;
    typeHintTextEl.textContent = typeNotice;
    return;
  }

  const showKeys = Boolean(text) && state !== "waiting";
  typeHintKeysEl.hidden = !showKeys;
  typeHintTextEl.hidden = true;
}

function typeRender(text, state) {
  ensureTypeBubble();
  updateTypeTargetLabel();

  if (state !== "waiting" && state !== "streaming") typeNotice = "";
  typeStatusEl.className = state ? "type-status " + state : "type-status";
  typeStatusEl.textContent = text;
  updateTypeHint(text, state);

  typeHost.style.display = "block";
  typeStatusEl.classList.toggle(
    "overflowing",
    typeStatusEl.scrollHeight > typeStatusEl.clientHeight + 1
  );
  typePosition();
}

function typeHide() {
  if (typeTimer) {
    clearTimeout(typeTimer);
    typeTimer = null;
  }
  if (typeActiveRequest) {
    typePost({ type: "cancel" });
    typeActiveRequest = 0;
  }

  typeTranslation = "";
  typeSession = null;
  typeLastText = "";
  typeAnchorRect = null;
  typeNotice = "";

  if (!typeHost) return;

  closeTypeMenu();
  typeHost.style.display = "none";
  typeStatusEl.textContent = "";
  typeStatusEl.className = "type-status";
  typeHintKeysEl.hidden = true;
  typeHintTextEl.hidden = true;
  typeHintTextEl.textContent = "";
}

function isTextEntry(element) {
  if (!element || element.nodeType !== 1) return false;
  if (element.tagName === "TEXTAREA") return true;
  if (element.tagName !== "INPUT") return false;

  const type = (element.getAttribute("type") || "text").toLowerCase();
  return ["text", "search", "url", "email", "tel"].includes(type);
}

function editingHost(node) {
  let element = node && node.nodeType === 1 ? node : node && node.parentElement;
  if (!element || !element.isContentEditable) return null;

  while (element.parentElement && element.parentElement.isContentEditable) {
    element = element.parentElement;
  }

  return element;
}

/// Splits `text` around `caret` on sentence delimiters and returns the trimmed
/// sentence plus the offsets to replace (includes a trailing delimiter).
function sentenceAt(text, caret) {
  const clamped = Math.max(0, Math.min(caret, text.length));
  let start = clamped;
  while (start > 0 && !TYPE_DELIMITERS.includes(text[start - 1])) start -= 1;

  let end = clamped;
  while (end < text.length && !TYPE_DELIMITERS.includes(text[end])) end += 1;
  if (end < text.length) end += 1;

  const raw = text.slice(start, end);
  const leading = raw.length - raw.trimStart().length;
  start += leading;
  end -= raw.length - raw.trimEnd().length;

  return { start, end, text: text.slice(start, end) };
}

function rangeAtOffsets(root, start, end) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
  let node = null;
  let offset = 0;
  let startNode = null;
  let startOffset = 0;
  let endNode = null;
  let endOffset = 0;

  while ((node = walker.nextNode())) {
    const length = node.nodeValue.length;

    if (!startNode && start <= offset + length) {
      startNode = node;
      startOffset = start - offset;
    }
    if (!endNode && end <= offset + length) {
      endNode = node;
      endOffset = end - offset;
      break;
    }

    offset += length;
  }

  if (!startNode || !endNode) return null;

  try {
    const range = document.createRange();
    range.setStart(startNode, startOffset);
    range.setEnd(endNode, endOffset);
    return range;
  } catch (error) {
    return null;
  }
}

function fieldSession(element) {
  const start = element.selectionStart;
  const end = element.selectionEnd;
  if (typeof start !== "number" || typeof end !== "number" || start !== end) {
    return null;
  }

  const sentence = sentenceAt(element.value, start);
  if (!sentence.text) return null;

  return {
    kind: "field",
    element,
    start: sentence.start,
    end: sentence.end,
    text: sentence.text,
  };
}

function blockElement(node, host) {
  let element = node.nodeType === 1 ? node : node.parentElement;

  while (element && element !== host) {
    const display = getComputedStyle(element).display;
    if (display && !display.startsWith("inline")) return element;
    element = element.parentElement;
  }

  return host;
}

function richSession(host) {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || !selection.isCollapsed) return null;

  const caret = selection.getRangeAt(0);
  if (!host.contains(caret.startContainer)) return null;

  const block = blockElement(caret.startContainer, host);
  const blockText = block.textContent || "";

  const before = document.createRange();
  before.selectNodeContents(block);
  try {
    before.setEnd(caret.startContainer, caret.startOffset);
  } catch (error) {
    return null;
  }

  const sentence = sentenceAt(blockText, before.toString().length);
  if (!sentence.text) return null;

  const range = rangeAtOffsets(block, sentence.start, sentence.end);
  if (!range) return null;

  return { kind: "richtext", element: host, range, text: sentence.text };
}

function typeSessionFor(target) {
  if (isTextEntry(target)) {
    if (target.disabled || target.readOnly) return null;
    return fieldSession(target);
  }

  if (target && target.isContentEditable) {
    const host = editingHost(target);
    if (host) return richSession(host);
  }

  return null;
}

/// Measures the caret position inside an `<input>`/`<textarea>` with a hidden
/// mirror node; returns null when the measurement is not usable.
function fieldCaretRect(element) {
  try {
    const isInput = element.tagName === "INPUT";
    const style = getComputedStyle(element);
    const bounds = element.getBoundingClientRect();
    const mirror = document.createElement("div");

    mirror.style.position = "fixed";
    mirror.style.left = bounds.left + "px";
    mirror.style.top = bounds.top + "px";
    mirror.style.visibility = "hidden";
    mirror.style.pointerEvents = "none";
    mirror.style.whiteSpace = isInput ? "pre" : "pre-wrap";
    mirror.style.overflowWrap = "break-word";
    mirror.style.overflow = "hidden";
    mirror.style.margin = "0";

    const properties = [
      "fontFamily",
      "fontSize",
      "fontWeight",
      "fontStyle",
      "letterSpacing",
      "textTransform",
      "lineHeight",
      "textIndent",
      "width",
      "paddingTop",
      "paddingRight",
      "paddingBottom",
      "paddingLeft",
      "borderTopWidth",
      "borderRightWidth",
      "borderBottomWidth",
      "borderLeftWidth",
      "boxSizing",
    ];
    for (const property of properties) mirror.style[property] = style[property];

    mirror.style.borderTopStyle = "solid";
    mirror.style.borderRightStyle = "solid";
    mirror.style.borderBottomStyle = "solid";
    mirror.style.borderLeftStyle = "solid";
    mirror.style.borderTopColor = "transparent";
    mirror.style.borderRightColor = "transparent";
    mirror.style.borderBottomColor = "transparent";
    mirror.style.borderLeftColor = "transparent";

    mirror.textContent = element.value.slice(0, element.selectionStart || 0);

    const marker = document.createElement("span");
    marker.textContent = "\u200b";
    mirror.append(marker);

    if (!isInput) mirror.style.height = element.clientHeight + "px";

    document.documentElement.append(mirror);
    if (!isInput) mirror.scrollTop = element.scrollTop;

    const rect = marker.getBoundingClientRect();
    mirror.remove();

    if (!rect || (rect.width === 0 && rect.height === 0 && rect.top === 0)) return null;
    return rect;
  } catch (error) {
    return null;
  }
}

function typeAnchor(session) {
  if (session.kind === "field") {
    return fieldCaretRect(session.element) || session.element.getBoundingClientRect();
  }

  const rects = session.range.getClientRects();
  if (rects.length) return rects[rects.length - 1];
  return session.range.getBoundingClientRect();
}

function typePosition() {
  if (!typeHost || typeHost.style.display === "none") return;

  const anchor = typeAnchorRect || (typeSession ? typeAnchor(typeSession) : null);
  if (!anchor) return;

  const margin = 8;
  const width = typeHost.offsetWidth || 280;
  const height = typeHost.offsetHeight || 52;
  const topEdge = typeof anchor.top === "number" ? anchor.top : anchor.bottom;
  const bottomEdge = typeof anchor.bottom === "number" ? anchor.bottom : anchor.top;

  let left = anchor.left;
  let top = bottomEdge + 6;

  if (left + width > window.innerWidth - margin) {
    left = window.innerWidth - width - margin;
  }
  if (left < margin) left = margin;
  if (top + height > window.innerHeight - margin) {
    top = Math.max(margin, topEdge - height - 8);
  }

  typeHost.style.left = left + "px";
  typeHost.style.top = top + "px";
}

function typeQueueReposition() {
  if (typeRepositionQueued) return;

  typeRepositionQueued = true;
  requestAnimationFrame(() => {
    typeRepositionQueued = false;
    if (typeHost && typeHost.style.display !== "none") typePosition();
  });
}

function runTypeTranslation(session) {
  if (!typeTranslate || siteDisabled()) return;

  if (Array.from(session.text).length < Math.max(1, typeMinLength)) {
    if (!typeActiveRequest) typeHide();
    return;
  }

  if (session.text === typeLastText && (typeActiveRequest || typeTranslation)) return;

  if (typeActiveRequest) {
    typePost({ type: "cancel" });
    typeActiveRequest = 0;
  }

  typeSession = session;
  typeLastText = session.text;
  typeTranslation = "";
  typeAnchorRect = typeAnchor(session);
  typeRequestSeq += 1;
  typeActiveRequest = typeRequestSeq;
  typeRender("翻译中…", "waiting");
  typePost({
    type: "translate",
    requestId: typeActiveRequest,
    text: session.text,
    record: false,
  });
}

function typeCommit() {
  const session = typeSession;
  const translation = typeTranslation;
  if (!session || !translation) return;

  typeApplying = true;
  let committed = false;

  try {
    if (session.kind === "field") {
      const element = session.element;
      if (element.isConnected) {
        element.focus();
        element.setRangeText(translation, session.start, session.end, "end");
        element.dispatchEvent(new Event("input", { bubbles: true }));
        committed = true;
      }
    } else if (session.range.startContainer.isConnected) {
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(session.range);
      committed = document.execCommand("insertText", false, translation);
    }
  } catch (error) {
    committed = false;
  }

  setTimeout(() => {
    typeApplying = false;
  }, 0);

  if (!committed) return;

  api.runtime
    .sendMessage({ type: "record-history", text: session.text, translation })
    .catch(() => {});
  typeHide();
}

function onTypeInput(event) {
  if (!typeTranslate || typeApplying || siteDisabled()) return;
  if (event.isComposing || typeComposing) return;

  const target = event.target;
  const editable = isTextEntry(target) ? target : editingHost(target);
  if (!editable) return;

  closeTypeMenu();
  typeNotice = "";

  const session = typeSessionFor(target);
  if (!session) {
    typeHide();
    return;
  }

  typeQueueReposition();

  if (typeTimer) clearTimeout(typeTimer);
  typeTimer = setTimeout(() => {
    typeTimer = null;
    const fresh = typeSessionFor(target);
    if (fresh) {
      runTypeTranslation(fresh);
    } else {
      typeHide();
    }
  }, typeDelay);
}

api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message && message.type === "get-site-info") {
    sendResponse({ hostname: location.hostname });
  } else if (message && message.type === "start-translate") {
    start();
  } else if (message && message.type === "translate-clipboard") {
    translateClipboard();
  } else if (message && message.type === "cycle-target") {
    cycleTypeTarget();
  }
});

document.addEventListener(
  "keydown",
  (event) => {
    if (event.key !== "Escape") return;
    if (typeSelectController && typeSelectController.isOpen()) {
      typeSelectController.close();
      return;
    }
    typeHide();
    if (menuEl && !menuEl.hidden) {
      closeMenu();
      return;
    }
    hide();
  },
  true
);

// Tab commits the inline translation while the 边写边译 bubble is visible;
// the listener runs in the capture phase so the page's own Tab handling
// (indentation, field navigation) does not fire first.
document.addEventListener(
  "keydown",
  (event) => {
    if (
      event.key !== "Tab" ||
      event.shiftKey ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey
    ) {
      return;
    }
    if (!typeSession || !typeTranslation || event.isComposing) return;

    const editable = typeSession.element;
    const target = event.target;
    const inSession =
      editable && (target === editable || (editable.contains && editable.contains(target)));
    if (!inSession) return;

    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
    typeCommit();
  },
  true
);

document.addEventListener("input", onTypeInput, true);

document.addEventListener(
  "compositionstart",
  () => {
    typeComposing = true;
  },
  true
);

document.addEventListener(
  "compositionend",
  (event) => {
    typeComposing = false;
    onTypeInput(event);
  },
  true
);

document.addEventListener(
  "focusout",
  (event) => {
    if (!typeSession) return;

    const editable = typeSession.element;
    if (event.target !== editable && !(editable.contains && editable.contains(event.target))) {
      return;
    }

    setTimeout(() => {
      if (!typeSession) return;
      const active = document.activeElement;
      if (
        active === typeSession.element ||
        (typeSession.element.contains && typeSession.element.contains(active)) ||
        (typeHost && typeHost.contains(active))
      ) {
        return;
      }
      typeHide();
    }, 120);
  },
  true
);

document.addEventListener(
  "mousedown",
  (event) => {
    if (!typeHost || typeHost.style.display === "none") return;
    if (event.composedPath().includes(typeHost)) return;
    typeHide();
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

document.addEventListener(
  "scroll",
  () => {
    queueReposition();
    typeQueueReposition();
  },
  true
);
window.addEventListener("resize", () => {
  queueReposition();
  typeQueueReposition();
});

loadSettings();
