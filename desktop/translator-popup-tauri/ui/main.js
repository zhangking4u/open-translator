const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const views = {
  translator: document.getElementById("translator-view"),
  settings: document.getElementById("settings-view"),
  history: document.getElementById("history-view"),
};

const source = document.getElementById("source");
const sourceSelect = document.getElementById("source-select");
const targetSelect = document.getElementById("target-select");
const swapButton = document.getElementById("swap");
const detectedLabel = document.getElementById("detected");
const translation = document.getElementById("translation");
const status = document.getElementById("status");
const copy = document.getElementById("copy");
const retranslate = document.getElementById("retranslate");
const replace = document.getElementById("replace");
const speak = document.getElementById("speak");
const pin = document.getElementById("pin");
const close = document.getElementById("close");
const dot = document.getElementById("dot");

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
let speakBackend = "webview";
let speakSupported = "speechSynthesis" in window;
let speakVoiceAvailable = true;
let speechGeneration = 0;
let hotkeyLabel = "Ctrl+Alt+T";

async function checkSpeakVoice(lang) {
  try {
    speakVoiceAvailable = await invoke("tts_voice_available", { lang });

    if (!speakVoiceAvailable) {
      speak.hidden = true;
    }
  } catch (error) {
    speakVoiceAvailable = true;
  }
}

let current = "";
let lastSource = "";
let streaming = false;
let replaceable = false;
let languageOptions = [];
let languageState = {
  source: "auto",
  target: "zh",
  detected: null,
  recent_targets: [],
};

function labelOf(tag) {
  const option = languageOptions.find((entry) => entry.tag === tag);
  return option ? option.label : tag;
}

function applyLanguageState(state) {
  languageState = state;
  sourceSelect.value = state.source;
  targetSelect.value = state.target;
  detectedLabel.textContent = state.detected ? "检测：" + labelOf(state.detected) : "";

  if (globalThis.OTSelect) {
    OTSelect.sync(sourceSelect);
    OTSelect.sync(targetSelect);
  }
}

async function initLanguages() {
  languageOptions = await invoke("get_languages");

  for (const select of [sourceSelect, targetSelect]) {
    select.innerHTML = "";

    for (const option of languageOptions) {
      if (select === targetSelect && option.tag === "auto") {
        continue;
      }

      const element = document.createElement("option");
      element.value = option.tag;
      element.textContent = option.label;
      select.appendChild(element);
    }
  }

  if (globalThis.OTSelect) {
    OTSelect.inject();
    OTSelect.enhance(sourceSelect, { title: "源语言" });
    OTSelect.enhance(targetSelect, { title: "目标语言" });
  }

  applyLanguageState(await invoke("get_language_state"));
  resize();
}

sourceSelect.addEventListener("change", () => {
  invoke("set_source", { source: sourceSelect.value });
});

targetSelect.addEventListener("change", () => {
  invoke("set_target", { target: targetSelect.value });
});

swapButton.addEventListener("click", () => {
  invoke("swap_languages");
});
let pinned = false;
let view = "translator";

function setStatus(text) {
  status.textContent = text;
}

function setDot(state) {
  dot.className = "dot";

  if (state !== "ready") {
    dot.classList.add(state);
  }

  dot.title =
    state === "loading" ? "正在准备翻译引擎" : state === "error" ? "翻译引擎出错" : "就绪";
}

function stopSpeaking() {
  if (!speakSupported) return;

  if (speakBackend === "spd-say") {
    if (speak.classList.contains("active")) {
      speechGeneration += 1;
      invoke("stop_speaking");
    }
  } else {
    window.speechSynthesis.cancel();
  }

  speak.classList.remove("active");
  speak.title = "朗读译文";
}

function pickVoice(lang) {
  const target = lang.toLowerCase();
  const base = target.split("-")[0];
  const voices = window.speechSynthesis.getVoices();

  return (
    voices.find((voice) => voice.lang.toLowerCase() === target) ||
    voices.find((voice) => voice.lang.toLowerCase().startsWith(base)) ||
    null
  );
}

function toggleSpeech() {
  if (!speakSupported || !speakVoiceAvailable || !current) return;

  if (speakBackend === "spd-say") {
    if (speak.classList.contains("active")) {
      stopSpeaking();
      return;
    }

    speak.classList.add("active");
    speak.title = "停止朗读";
    const generation = ++speechGeneration;
    invoke("speak_text", {
      text: current,
      lang: languageState.target,
      generation,
    }).catch((message) => {
      stopSpeaking();
      setStatus(String(message));
      window.setTimeout(() => setStatus(""), 3000);
    });
    return;
  }

  const synth = window.speechSynthesis;

  if (synth.speaking) {
    stopSpeaking();
    return;
  }

  const lang = SPEECH_LANGS[languageState.target] || languageState.target;
  const voice = pickVoice(lang);

  if (!voice) {
    setStatus("未找到" + labelOf(languageState.target) + "语音，请先安装系统语音包");
    window.setTimeout(() => setStatus(""), 3000);
    return;
  }

  const utterance = new SpeechSynthesisUtterance(current);
  utterance.voice = voice;
  utterance.lang = voice.lang;
  utterance.onend = stopSpeaking;
  utterance.onerror = stopSpeaking;
  speak.classList.add("active");
  speak.title = "停止朗读";
  synth.speak(utterance);
}

function render(html) {
  translation.innerHTML = html;
  resize();
}

async function resize() {
  const margin = document.body.classList.contains("os-windows") ? 0 : 20;
  const height =
    Math.ceil(document.querySelector(".card").getBoundingClientRect().height) + margin;
  await invoke("resize_window", { height });
}

function showView(next) {
  view = next;

  for (const [name, element] of Object.entries(views)) {
    element.hidden = name !== next;
  }

  resize();
}

function setCopyEnabled(enabled) {
  copy.disabled = !enabled;
}

function showWaiting() {
  current = "";
  lastSource = "";
  source.hidden = true;
  source.textContent = "";
  retranslate.hidden = true;
  replace.hidden = true;
  speak.hidden = true;
  setCopyEnabled(false);
  stopSpeaking();
  render(
    '<div class="empty-state">' +
      '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
      '<path d="M4 5h9" />' +
      '<path d="M4 9h6" />' +
      '<path d="M4 13h4" />' +
      '<path d="m13 11 7 3-3 1-1 3z" />' +
      "</svg>" +
      '<p class="hint">等待划词</p>' +
      '<p class="sub">按 ' +
      hotkeyLabel +
      " 翻译选中文本</p>" +
      "</div>"
  );
}

function showEmpty() {
  current = "";
  source.hidden = true;
  source.textContent = "";
  retranslate.hidden = true;
  replace.hidden = true;
  speak.hidden = true;
  setCopyEnabled(false);
  stopSpeaking();
  render('<p class="hint">未选中文本</p><p class="sub">请在其它应用中选中要翻译的内容</p>');
}

function showSource(text, canReplace) {
  lastSource = text;
  replaceable = canReplace;
  // A new source starts a fresh translation: never append to the old one.
  current = "";
  streaming = false;
  setCopyEnabled(false);
  source.hidden = false;
  source.textContent = text;
  retranslate.hidden = false;
  replace.hidden = !replaceable;
  speak.hidden = true;
  stopSpeaking();
  setDot("loading");
  render('<p class="hint">正在翻译…</p>');
}

function showProgress(downloaded, total) {
  current = "";
  streaming = false;
  setCopyEnabled(false);
  speak.hidden = true;
  stopSpeaking();
  setDot("loading");

  const megabytes = (value) => (value / 1000000).toFixed(0);
  const percent = total ? Math.round((downloaded / total) * 100) : 0;

  render(
    '<p class="hint">正在下载模型</p>' +
      '<progress max="100" value="' + percent + '"></progress>' +
      '<p class="sub">' +
      megabytes(downloaded) +
      " / " +
      (total ? megabytes(total) + " MB · " : "MB · ") +
      "完成后自动翻译</p>"
  );
}

function showError(message, retryable) {
  current = "";
  streaming = false;
  setCopyEnabled(false);
  speak.hidden = true;
  stopSpeaking();
  setDot("error");
  retranslate.hidden = !retryable;

  const card = document.createElement("div");
  card.className = "error-card";

  const title = document.createElement("p");
  title.className = "error-title";
  title.textContent = "出错了";

  const body = document.createElement("p");
  body.className = "error-body";
  body.textContent = message;

  card.append(title, body);
  translation.innerHTML = "";
  translation.appendChild(card);
  resize();
}

function textParagraph() {
  let paragraph = translation.querySelector("p.text");

  if (!paragraph) {
    paragraph = document.createElement("p");
    paragraph.className = "text";
    translation.replaceChildren(paragraph);
  }

  return paragraph;
}

function appendDelta(piece) {
  current += piece;
  streaming = true;
  const paragraph = textParagraph();
  paragraph.classList.add("streaming");
  paragraph.textContent = current;
  setCopyEnabled(false);
  resize();
}

function finish(translationText) {
  current = translationText;
  streaming = false;
  const paragraph = textParagraph();
  paragraph.classList.remove("streaming");
  paragraph.textContent = current;
  setCopyEnabled(current.length > 0);
  speak.hidden = !speakSupported;

  if (speakSupported && speakBackend === "spd-say") {
    checkSpeakVoice(languageState.target);
  }

  setDot("ready");
  resize();
}

new ResizeObserver(resize).observe(document.body);

close.addEventListener("click", () => {
  invoke("hide_window");
});

document.getElementById("open-history").addEventListener("click", () => {
  openHistory();
});

async function togglePin() {
  pinned = !pinned;
  await invoke("set_pinned", { pinned });
  pin.classList.toggle("active", pinned);
  pin.title = pinned ? "取消固定（取消置顶）" : "固定窗口（保持最前）";
  close.hidden = pinned;
}

pin.addEventListener("click", togglePin);

document.querySelector(".titlebar").addEventListener("dblclick", (event) => {
  if (event.target.closest("button")) return;
  togglePin();
});

speak.addEventListener("click", toggleSpeech);

const contextMenu = document.getElementById("context-menu");

function hideContextMenu() {
  contextMenu.hidden = true;
}

translation.addEventListener("contextmenu", (event) => {
  if (!current) return;

  event.preventDefault();
  contextMenu.querySelector('[data-action="speak"]').hidden = !speakSupported;
  contextMenu.hidden = false;

  const rect = contextMenu.getBoundingClientRect();
  const x = Math.min(event.clientX, window.innerWidth - rect.width - 8);
  const y = Math.min(event.clientY, window.innerHeight - rect.height - 8);
  contextMenu.style.left = Math.max(4, x) + "px";
  contextMenu.style.top = Math.max(4, y) + "px";
});

contextMenu.addEventListener("click", (event) => {
  const action = event.target.closest("[data-action]")?.dataset.action;
  if (!action) return;

  if (action === "copy") {
    const selected = window.getSelection().toString();
    const text = selected || current;
    if (text) invoke("copy_text", { text });
  } else if (action === "select-all") {
    const paragraph = translation.querySelector("p.text");

    if (paragraph) {
      const range = document.createRange();
      range.selectNodeContents(paragraph);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }
  } else if (action === "speak") {
    toggleSpeech();
  }

  hideContextMenu();
});

document.addEventListener("click", hideContextMenu);

copy.addEventListener("click", async () => {
  if (!current) {
    return;
  }

  await invoke("copy_text", { text: current });
  copy.classList.add("copied");
  window.setTimeout(() => copy.classList.remove("copied"), 1200);
});

retranslate.addEventListener("click", () => {
  invoke("retranslate");
});

replace.addEventListener("click", async () => {
  if (!current) {
    return;
  }

  try {
    await invoke("replace_text", { text: current });
    setStatus("已替换");
    window.setTimeout(() => setStatus(""), 1500);
  } catch (message) {
    setStatus(String(message));
  }
});

window.addEventListener("keydown", (event) => {
  if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "h") {
    event.preventDefault();
    openHistory();
    return;
  }

  if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key === ",") {
    event.preventDefault();
    openSettings();
    return;
  }

  if (event.ctrlKey && !event.shiftKey && ["1", "2", "3"].includes(event.key)) {
    const target = languageState.recent_targets?.[Number(event.key) - 1];

    if (target) {
      invoke("set_target", { target });
      event.preventDefault();
    }

    return;
  }

  if (event.ctrlKey && event.shiftKey && !event.altKey && event.key.toLowerCase() === "c") {
    if (!copy.disabled) {
      event.preventDefault();
      copy.click();
    }

    return;
  }

  if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key === "Enter") {
    if (!retranslate.hidden) {
      event.preventDefault();
      retranslate.click();
    }

    return;
  }

  if (event.key !== "Escape") {
    return;
  }

  if (!contextMenu.hidden) {
    hideContextMenu();
    return;
  }

  if (!confirmDialog.hidden) {
    closeConfirm();
    return;
  }

  if (view !== "translator") {
    showView("translator");
  } else {
    invoke("hide_window");
  }
});

const appVersion = document.getElementById("app-version");
const updateStatus = document.getElementById("update-status");
const updateProgress = document.getElementById("update-progress");
const updateNotes = document.getElementById("update-notes");
const updateCheck = document.getElementById("update-check");
const updateAction = document.getElementById("update-action");

let updateState = { phase: "idle", version: "", canInstall: false, message: "" };

function formatUpdateProgress(downloaded, total) {
  const megabytes = (value) => (value / 1000000).toFixed(0);

  if (total && total > 0) {
    return (
      "正在下载更新：" +
      Math.round((downloaded / total) * 100) +
      "%（" +
      megabytes(downloaded) +
      "/" +
      megabytes(total) +
      " MB）"
    );
  }

  return "正在下载更新：已下载 " + megabytes(downloaded) + " MB";
}

function renderUpdate() {
  const { phase, version, canInstall } = updateState;
  const hasVersion = version.length > 0;
  const failed = phase === "install-error" || phase === "check-error";
  const canAct =
    hasVersion &&
    (phase === "available" || phase === "install-error" || phase === "check-error");

  updateNotes.hidden = !hasVersion || phase === "downloading";
  updateNotes.textContent = "更新说明";
  updateCheck.hidden = phase === "downloading" || hasVersion;
  updateCheck.disabled = phase === "checking";
  updateCheck.textContent = phase === "checking" ? "检查中…" : "检查更新";
  updateAction.hidden = !canAct;
  updateAction.textContent =
    phase === "install-error" ? "重试" : canInstall ? "更新到 v" + version : "前往下载";
  updateProgress.hidden = phase !== "downloading";
  updateStatus.hidden = phase === "idle" || phase === "available";
  updateStatus.classList.toggle("error", failed);

  if (phase === "checking") {
    updateStatus.textContent = "正在检查更新…";
  } else if (phase === "latest") {
    updateStatus.textContent = "已是最新版本";
  } else if (phase === "install-error") {
    updateStatus.textContent = "更新失败：" + updateState.message;
  } else if (phase === "check-error") {
    updateStatus.textContent = "检查更新失败：" + updateState.message;
  }

  resize();
}

updateCheck.addEventListener("click", () => {
  updateState = { ...updateState, phase: "checking", message: "" };
  renderUpdate();
  invoke("check_update_now");
});

updateAction.addEventListener("click", () => {
  if (!updateState.canInstall) {
    invoke("open_release_page");
    return;
  }

  updateState = { ...updateState, phase: "downloading", message: "" };
  updateStatus.textContent = "正在准备更新…";
  updateProgress.max = 100;
  updateProgress.value = 0;
  renderUpdate();
  invoke("start_update");
});

updateNotes.addEventListener("click", () => {
  invoke("open_release_page");
});

document.getElementById("settings-back").addEventListener("click", () => showView("translator"));
document.getElementById("history-back").addEventListener("click", () => showView("translator"));

document.getElementById("open-config").addEventListener("click", () => {
  invoke("open_config_dir");
});

const modelInput = document.getElementById("model-input");
let savedModelPath = "";

async function saveModelPath() {
  const value = modelInput.value;
  if (value === savedModelPath) return;

  const saved = document.getElementById("model-saved");
  await invoke("save_model_path", { value });
  savedModelPath = value;
  saved.hidden = false;
  window.setTimeout(() => (saved.hidden = true), 1500);
  resize();
}

modelInput.addEventListener("blur", saveModelPath);
modelInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    saveModelPath();
  }
});

const hotkeyRecorder = document.getElementById("hotkey-recorder");
const hotkeyReset = document.getElementById("hotkey-reset");
const hotkeyError = document.getElementById("hotkey-error");
const hotkeySaved = document.getElementById("hotkey-saved");

const DEFAULT_HOTKEY = "Ctrl+Alt+T";
const MODIFIER_EVENT_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);
const KEY_LABELS = {
  Control: "Ctrl",
  Meta: "Super",
  " ": "Space",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
};

const hotkeyState = {
  recording: false,
  candidate: "",
  current: "",
};

function hotkeyParts(spec) {
  return spec ? spec.split("+") : [];
}

function keyLabel(event) {
  if (KEY_LABELS[event.key]) {
    return KEY_LABELS[event.key];
  }
  return event.key.length === 1 ? event.key.toUpperCase() : event.key;
}

function specFromEvent(event) {
  const modifiers = [];
  if (event.ctrlKey) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Super");

  if (MODIFIER_EVENT_KEYS.has(event.key)) {
    return { modifiers, key: "" };
  }

  return { modifiers, key: keyLabel(event) };
}

function renderHotkey() {
  hotkeyRecorder.classList.toggle("recording", hotkeyState.recording);

  let parts = [];
  let state = "";

  if (!hotkeyState.recording) {
    parts = hotkeyParts(hotkeyState.current);
    if (!parts.length) state = "未设置";
  } else if (hotkeyState.candidate) {
    parts = hotkeyParts(hotkeyState.candidate);
    state = "再按一次";
  } else {
    state = "按下新组合键…";
  }

  const nodes = [];
  for (const part of parts) {
    const keycap = document.createElement("kbd");
    keycap.textContent = part;
    nodes.push(keycap);
  }
  if (state) {
    const span = document.createElement("span");
    span.className = "placeholder";
    span.textContent = state;
    nodes.push(span);
  }
  hotkeyRecorder.replaceChildren(...nodes);
  hotkeyReset.hidden = !hotkeyState.recording;

  resize();
}

function startHotkeyRecording() {
  hotkeyState.recording = true;
  hotkeyState.candidate = "";
  hotkeyError.hidden = true;
  hotkeySaved.hidden = true;
  hotkeyRecorder.focus();
  renderHotkey();
}

function stopHotkeyRecording() {
  hotkeyState.recording = false;
  hotkeyState.candidate = "";
  renderHotkey();
}

async function applyHotkey(spec) {
  hotkeyError.hidden = true;
  hotkeySaved.hidden = true;

  try {
    await invoke("save_hotkey", { spec });
    hotkeyState.current = spec;
    hotkeyState.recording = false;
    hotkeyState.candidate = "";
    hotkeySaved.hidden = false;
    window.setTimeout(() => (hotkeySaved.hidden = true), 1500);
    hotkeyLabel = spec;

    if (view === "translator" && !current && !streaming) {
      showWaiting();
    }
  } catch (message) {
    hotkeyError.textContent = String(message);
    hotkeyError.hidden = false;
  }

  renderHotkey();
}

hotkeyRecorder.addEventListener("click", () => {
  startHotkeyRecording();
});

hotkeyRecorder.addEventListener("blur", (event) => {
  if (event.relatedTarget === hotkeyReset) return;
  if (hotkeyState.recording) {
    stopHotkeyRecording();
  }
});

hotkeyRecorder.addEventListener("keydown", (event) => {
  if (!hotkeyState.recording) return;
  if (event.isComposing) return;

  event.preventDefault();
  event.stopPropagation();

  if (event.key === "Escape") {
    stopHotkeyRecording();
    return;
  }

  if (event.key === "Backspace" || event.key === "Delete") {
    hotkeyState.candidate = "";
    renderHotkey();
    return;
  }

  if (event.repeat || MODIFIER_EVENT_KEYS.has(event.key)) return;

  const { modifiers, key } = specFromEvent(event);
  const hasCommandModifier = modifiers.some((modifier) => modifier !== "Shift");

  if (!hasCommandModifier) {
    hotkeyError.textContent = "快捷键需要包含 Ctrl、Alt 或 Super";
    hotkeyError.hidden = false;
    return;
  }

  hotkeyError.hidden = true;

  const next = modifiers.concat(key).join("+");

  if (hotkeyState.candidate === next) {
    applyHotkey(next);
    return;
  }

  hotkeyState.candidate = next;
  renderHotkey();
});

hotkeyReset.addEventListener("click", () => {
  applyHotkey(DEFAULT_HOTKEY);
});

for (const [id, key] of [
  ["switch-auto", "auto_download"],
  ["switch-updates", "check_updates"],
  ["switch-extension", "serve_extension"],
]) {
  document.getElementById(id).addEventListener("change", (event) => {
    event.target.setAttribute("aria-checked", String(event.target.checked));
    invoke("save_switch", { key, value: event.target.checked });
  });
}

function setSwitch(id, checked) {
  const input = document.getElementById(id);
  input.checked = checked;
  input.setAttribute("aria-checked", String(checked));
}

async function openSettings() {
  const settings = await invoke("get_settings");

  hotkeyState.current = settings.hotkey;
  hotkeyState.recording = false;
  hotkeyState.candidate = "";
  hotkeyError.hidden = true;
  hotkeySaved.hidden = true;
  renderHotkey();
  modelInput.value = settings.model_path;
  savedModelPath = settings.model_path;
  setSwitch("switch-auto", settings.auto_download);
  setSwitch("switch-updates", settings.check_updates);
  setSwitch("switch-extension", settings.serve_extension);
  document.getElementById("config-path").textContent = settings.config_path ?? "";
  appVersion.textContent = "当前版本 v" + settings.app_version;

  if (updateState.phase === "idle") {
    const state = await invoke("get_update_state");

    if (state.version) {
      updateState = {
        phase: "available",
        version: state.version,
        canInstall: state.can_install,
        message: "",
      };
    }
  }

  renderUpdate();
  showView("settings");
  await resize();
  await invoke("center_window");
}

async function openHistory() {
  await renderHistory();
  showView("history");
  await resize();
  await invoke("center_window");
}

async function renderHistory() {
  const history = await invoke("get_history");
  const list = document.getElementById("history-list");
  const empty = document.getElementById("history-empty");
  const clear = document.getElementById("history-clear");

  list.replaceChildren();
  empty.hidden = history.length > 0;
  clear.hidden = history.length === 0;

  document.getElementById("history-empty-hint").textContent =
    "选中文字按 " + hotkeyLabel + " 开始";

  history.forEach((entry, index) => {
    const button = document.createElement("button");
    button.className = "history-entry";
    button.type = "button";
    button.title = entry.text + "\n" + entry.translation;

    const text = document.createElement("span");
    text.className = "history-text";
    text.textContent = entry.text;

    const time = document.createElement("span");
    time.className = "history-time";
    const relative = relativeTime(entry.at);
    // The language tag only earns its place when it differs from the target
    // the user is translating into right now.
    time.textContent =
      entry.target && entry.target !== languageState.target
        ? relative
          ? entry.target + " · " + relative
          : entry.target
        : relative;

    const translation = document.createElement("span");
    translation.className = "history-translation";
    translation.textContent = entry.translation;

    button.append(text, time, translation);
    button.addEventListener("click", async () => {
      await invoke("load_history_entry", { index });
      showView("translator");
    });
    list.appendChild(button);
  });
}

function relativeTime(at) {
  if (!at) return "";

  const seconds = Math.max(0, Math.floor((Date.now() - at) / 1000));
  if (seconds < 60) return "刚刚";

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return minutes + " 分钟前";

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return hours + " 小时前";

  const days = Math.floor(hours / 24);
  if (days === 1) return "昨天";
  if (days < 30) return days + " 天前";

  const date = new Date(at);
  return date.getMonth() + 1 + "月" + date.getDate() + "日";
}

document.getElementById("history-clear").addEventListener("click", () => {
  openConfirm(async () => {
    await invoke("clear_history");
    await renderHistory();
    resize();
  });
});

const confirmDialog = document.getElementById("confirm-dialog");
let confirmAction = null;

function closeConfirm() {
  confirmAction = null;
  confirmDialog.hidden = true;

  const clear = document.getElementById("history-clear");
  if (clear && !clear.hidden) clear.focus();
}

function openConfirm(action) {
  confirmAction = action;
  confirmDialog.hidden = false;
  document.getElementById("confirm-cancel").focus();
}

document.getElementById("confirm-cancel").addEventListener("click", closeConfirm);

document.getElementById("confirm-accept").addEventListener("click", async () => {
  const action = confirmAction;
  confirmAction = null;
  confirmDialog.hidden = true;

  if (action) await action();
});

confirmDialog.addEventListener("click", (event) => {
  if (event.target === confirmDialog) closeConfirm();
});

listen("source", (event) => {
  // A new translation always belongs in the translator view, even when the
  // card currently sits on the history or settings page.
  if (view !== "translator") showView("translator");
  showSource(event.payload.text, event.payload.replaceable === true);
});

listen("delta", (event) => {
  appendDelta(event.payload);
});

listen("done", (event) => {
  finish(event.payload);
});

listen("error", (event) => {
  showError(event.payload.message, lastSource.length > 0);
});

listen("empty", () => {
  showEmpty();
});

listen("model-progress", (event) => {
  showProgress(event.payload.downloaded, event.payload.total);
});

listen("model-ready", () => {
  setDot("ready");

  if (view === "translator" && !streaming && !current) {
    showWaiting();
  }
});

listen("model-error", (event) => {
  showError(event.payload.message, false);
});

listen("open-settings", () => {
  openSettings();
});

listen("open-history", () => {
  openHistory();
});

listen("language-state", (event) => {
  applyLanguageState(event.payload);
});

listen("history-changed", () => {
  if (view === "history") {
    renderHistory();
  }
});

listen("update-checking", () => {
  updateState = { ...updateState, phase: "checking", message: "" };
  renderUpdate();
});

listen("update-available", (event) => {
  updateState = {
    phase: "available",
    version: event.payload.version,
    canInstall: event.payload.can_install,
    message: "",
  };
  renderUpdate();
});

listen("update-none", () => {
  updateState = { phase: "latest", version: "", canInstall: false, message: "" };
  renderUpdate();
});

listen("update-check-failed", (event) => {
  updateState = { ...updateState, phase: "check-error", message: event.payload.message };
  renderUpdate();
});

listen("update-progress", (event) => {
  updateState = { ...updateState, phase: "downloading" };
  updateStatus.hidden = false;
  updateStatus.textContent = formatUpdateProgress(event.payload.downloaded, event.payload.total);

  if (event.payload.total && event.payload.total > 0) {
    updateProgress.max = event.payload.total;
    updateProgress.value = event.payload.downloaded;
  } else {
    updateProgress.removeAttribute("value");
  }

  renderUpdate();
});

listen("update-error", (event) => {
  updateState = { ...updateState, phase: "install-error", message: event.payload.message };
  renderUpdate();
});

showWaiting();
setDot("loading");
resize();
initLanguages();

invoke("get_settings").then((settings) => {
  hotkeyLabel = settings.hotkey || hotkeyLabel;

  if (settings.pinned) {
    pinned = true;
    pin.classList.add("active");
    pin.title = "取消固定（取消置顶）";
    close.hidden = true;
  }

  if (view === "translator" && !current && !streaming) {
    showWaiting();
  }
});

invoke("tts_backend").then((backend) => {
  speakBackend = backend;
  speakSupported =
    backend === "spd-say" || (backend === "webview" && "speechSynthesis" in window);

  if (speakSupported && current) {
    speak.hidden = false;
  }
});

listen("speech-ended", (event) => {
  if (event.payload.generation !== speechGeneration) {
    return;
  }

  speak.classList.remove("active");
  speak.title = "朗读译文";
});

invoke("platform").then((os) => {
  // Windows keeps the window opaque, so the card fills it instead of floating.
  document.body.classList.toggle("os-windows", os === "windows");
  resize();
});

invoke("get_initial_view").then((initial) => {
  if (initial === "settings") {
    openSettings();
  } else if (initial === "history") {
    openHistory();
  }
});
