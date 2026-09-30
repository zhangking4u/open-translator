"use strict";

const api = globalThis.browser ?? globalThis.chrome;

const DEFAULTS = {
  source: "en",
  target: "zh",
  autoTranslate: false,
};

const LANGUAGES = [
  ["zh", "中文"],
  ["en", "英语"],
  ["ja", "日语"],
  ["ko", "韩语"],
  ["fr", "法语"],
  ["de", "德语"],
  ["es", "西班牙语"],
  ["ru", "俄语"],
];

let host = null;
let statusEl = null;
let copyButton = null;
let languageSelect = null;
let currentTranslation = "";
let currentText = "";
let targetLanguage = DEFAULTS.target;
let autoTranslate = false;
let autoTimer = null;

async function loadSettings() {
  const settings = await api.storage.local.get(DEFAULTS);
  targetLanguage = settings.target;
  autoTranslate = Boolean(settings.autoTranslate);
  if (languageSelect) languageSelect.value = targetLanguage;
}

api.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;

  if (changes.target && changes.target.newValue) {
    targetLanguage = changes.target.newValue;
    if (languageSelect) languageSelect.value = targetLanguage;
  }

  if (changes.autoTranslate) {
    autoTranslate = Boolean(changes.autoTranslate.newValue);
  }
});

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
    ".card { font: 13px/1.5 system-ui, sans-serif; color: #1f2937; background: #fff;",
    "  border: 1px solid #d1d5db; border-radius: 8px; box-shadow: 0 6px 24px rgba(0,0,0,.18);",
    "  max-width: 420px; min-width: 220px; padding: 10px 12px; }",
    ".status { white-space: pre-wrap; word-break: break-word; }",
    ".status.error { color: #b91c1c; }",
    ".row { display: flex; align-items: center; gap: 8px; margin-top: 8px; }",
    ".spacer { flex: 1; }",
    "select { font: inherit; padding: 2px 6px; border-radius: 6px; border: 1px solid #d1d5db;",
    "  background: #fff; }",
    "button { font: inherit; padding: 3px 10px; border-radius: 6px; border: 1px solid #d1d5db;",
    "  background: #f9fafb; cursor: pointer; }",
    "button:hover { background: #f3f4f6; }",
    "button:disabled { color: #9ca3af; cursor: default; }",
  ].join("\n");

  const card = document.createElement("div");
  card.className = "card";

  statusEl = document.createElement("div");
  statusEl.className = "status";

  const row = document.createElement("div");
  row.className = "row";

  languageSelect = document.createElement("select");
  for (const [code, name] of LANGUAGES) {
    const option = document.createElement("option");
    option.value = code;
    option.textContent = name;
    languageSelect.append(option);
  }
  if (!LANGUAGES.some(([code]) => code === targetLanguage)) {
    const option = document.createElement("option");
    option.value = targetLanguage;
    option.textContent = targetLanguage;
    languageSelect.append(option);
  }
  languageSelect.value = targetLanguage;
  languageSelect.addEventListener("change", async () => {
    targetLanguage = languageSelect.value;
    await api.storage.local.set({ target: targetLanguage });
    if (currentText) beginTranslate(currentText);
  });

  const spacer = document.createElement("div");
  spacer.className = "spacer";

  copyButton = document.createElement("button");
  copyButton.textContent = "复制";
  copyButton.disabled = true;
  copyButton.addEventListener("click", () => {
    if (!currentTranslation) return;
    navigator.clipboard.writeText(currentTranslation).then(
      () => {
        copyButton.textContent = "已复制";
      },
      () => {
        const textarea = document.createElement("textarea");
        textarea.value = currentTranslation;
        document.documentElement.appendChild(textarea);
        textarea.select();
        document.execCommand("copy");
        textarea.remove();
        copyButton.textContent = "已复制";
      }
    );
  });

  const closeButton = document.createElement("button");
  closeButton.textContent = "关闭";
  closeButton.addEventListener("click", hide);

  row.append(languageSelect, spacer, copyButton, closeButton);
  card.append(statusEl, row);
  shadow.append(style, card);
  document.documentElement.append(host);
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
  host.style.display = "none";
  currentTranslation = "";
  currentText = "";
  copyButton.textContent = "复制";
  copyButton.disabled = true;
  statusEl.classList.remove("error");
}

function showLoading() {
  currentTranslation = "";
  copyButton.textContent = "复制";
  copyButton.disabled = true;
  statusEl.classList.remove("error");
  statusEl.textContent = "翻译中…";
  show();
}

function showResult(translation) {
  currentTranslation = translation;
  copyButton.disabled = false;
  statusEl.classList.remove("error");
  statusEl.textContent = translation;
  show();
}

function showError(message) {
  currentTranslation = "";
  copyButton.disabled = true;
  statusEl.classList.add("error");
  statusEl.textContent = message;
  show();
}

function beginTranslate(text) {
  currentText = text;
  showLoading();

  api.runtime
    .sendMessage({ type: "translate", text })
    .then((response) => {
      if (!response) {
        showError("翻译请求失败。");
      } else if (response.ok) {
        showResult(response.translation);
      } else {
        showError(response.error);
      }
    })
    .catch(() => showError("翻译请求失败。"));
}

function start() {
  const selection = window.getSelection();
  const text = selection ? selection.toString().trim() : "";

  ensureBubble();

  if (!text) {
    showError("未选中文本。");
    return;
  }

  const range = selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
  const rect = range
    ? range.getBoundingClientRect()
    : { left: 40, top: 40, bottom: 60 };

  beginTranslate(text);
  positionBubble(rect);
}

api.runtime.onMessage.addListener((message) => {
  if (message && message.type === "start-translate") start();
});

document.addEventListener(
  "keydown",
  (event) => {
    if (event.key === "Escape") hide();
  },
  true
);

document.addEventListener("mouseup", (event) => {
  if (!autoTranslate) return;
  if (host && event.composedPath().includes(host)) return;

  const target = event.target;
  if (target && typeof target.closest === "function" && target.closest("input, textarea, [contenteditable]")) {
    return;
  }

  const selection = window.getSelection();
  const text = selection ? selection.toString().trim() : "";
  if (!text) return;

  clearTimeout(autoTimer);
  autoTimer = setTimeout(() => {
    const fresh = window.getSelection();
    const freshText = fresh ? fresh.toString().trim() : "";
    if (freshText !== text) return;

    ensureBubble();

    const range = fresh.rangeCount > 0 ? fresh.getRangeAt(0) : null;
    const rect = range
      ? range.getBoundingClientRect()
      : { left: 40, top: 40, bottom: 60 };

    beginTranslate(text);
    positionBubble(rect);
  }, 400);
});

loadSettings();
