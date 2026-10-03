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
const pin = document.getElementById("pin");
const close = document.getElementById("close");

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

function render(html) {
  translation.innerHTML = html;
  resize();
}

async function resize() {
  const margin = document.body.classList.contains("os-windows") ? 0 : 20;
  const height = document.querySelector(".card").getBoundingClientRect().height + margin;
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
  setCopyEnabled(false);
  render('<p class="hint">等待划词</p><p class="sub">按 Ctrl+Alt+T 翻译选中文本</p>');
}

function showEmpty() {
  current = "";
  source.hidden = true;
  source.textContent = "";
  retranslate.hidden = true;
  replace.hidden = true;
  setCopyEnabled(false);
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
  render('<p class="hint">正在翻译…</p>');
}

function showProgress(downloaded, total) {
  current = "";
  streaming = false;
  setCopyEnabled(false);

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

function appendDelta(piece) {
  current += piece;
  streaming = true;
  render('<p class="text">' + escapeHtml(current) + "</p>");
  setCopyEnabled(false);
}

function finish(translationText) {
  current = translationText;
  streaming = false;
  render('<p class="text">' + escapeHtml(current) + "</p>");
  setCopyEnabled(current.length > 0);
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

new ResizeObserver(resize).observe(document.body);

close.addEventListener("click", () => {
  invoke("hide_window");
});

pin.addEventListener("click", async () => {
  pinned = !pinned;
  await invoke("set_pinned", { pinned });
  pin.classList.toggle("active", pinned);
  pin.title = pinned ? "取消固定（取消置顶）" : "固定窗口（保持最前）";
  close.hidden = pinned;
});

copy.addEventListener("click", async () => {
  if (!current) {
    return;
  }

  await invoke("copy_text", { text: current });
  copy.classList.add("copied");
  window.setTimeout(() => copy.classList.remove("copied"), 1200);
  setStatus("已复制");
  window.setTimeout(() => setStatus(""), 1500);
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
  if (event.ctrlKey && !event.shiftKey && ["1", "2", "3"].includes(event.key)) {
    const target = languageState.recent_targets?.[Number(event.key) - 1];

    if (target) {
      invoke("set_target", { target });
      event.preventDefault();
    }

    return;
  }

  if (event.key !== "Escape") {
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

  updateNotes.hidden = !hasVersion || phase === "downloading";
  updateNotes.textContent = canInstall ? "更新说明" : "前往下载";
  updateCheck.hidden = phase === "downloading";
  updateCheck.disabled = phase === "checking";
  updateCheck.textContent = phase === "checking" ? "检查中…" : "检查更新";
  updateAction.hidden = !(
    canInstall &&
    (phase === "available" || phase === "install-error" || phase === "check-error")
  );
  updateAction.textContent = phase === "install-error" ? "重试" : "更新";
  updateProgress.hidden = phase !== "downloading";
  updateStatus.hidden = phase === "idle";
  updateStatus.classList.toggle("error", failed);

  if (phase === "checking") {
    updateStatus.textContent = "正在检查更新…";
  } else if (phase === "latest") {
    updateStatus.textContent = "已是最新版本";
  } else if (phase === "available") {
    updateStatus.textContent =
      "发现新版本 v" + version + (canInstall ? "" : "，请前往发布页下载");
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

document.getElementById("model-save").addEventListener("click", async () => {
  const value = document.getElementById("model-input").value;
  await invoke("save_model_path", { value });

  const saved = document.getElementById("model-saved");
  saved.hidden = false;
  window.setTimeout(() => (saved.hidden = true), 1500);
});

const hotkeyRecorder = document.getElementById("hotkey-recorder");
const hotkeyApply = document.getElementById("hotkey-apply");
const hotkeyReset = document.getElementById("hotkey-reset");
const hotkeyHint = document.getElementById("hotkey-hint");
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
  confirmed: false,
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
  let placeholder = "";

  if (!hotkeyState.recording) {
    parts = hotkeyParts(hotkeyState.current);
    if (!parts.length) placeholder = "未设置";
  } else if (hotkeyState.candidate) {
    parts = hotkeyParts(hotkeyState.candidate);
  } else {
    placeholder = "请按下快捷键…";
  }

  const nodes = [];
  for (const part of parts) {
    const keycap = document.createElement("kbd");
    keycap.textContent = part;
    nodes.push(keycap);
  }
  if (placeholder) {
    const span = document.createElement("span");
    span.className = "placeholder";
    span.textContent = placeholder;
    nodes.push(span);
  }
  hotkeyRecorder.replaceChildren(...nodes);

  if (!hotkeyState.recording) {
    hotkeyHint.textContent = "点击左侧按钮后按下新快捷键，再按一次相同组合确认；Esc 取消";
  } else if (!hotkeyState.candidate) {
    hotkeyHint.textContent = "请按下新快捷键（需包含 Ctrl/Alt/Super，可加 Shift）";
  } else if (!hotkeyState.confirmed) {
    hotkeyHint.textContent = "请再按一次相同组合确认";
  } else {
    hotkeyHint.textContent = "已确认，点击「应用」或按 Enter 生效";
  }

  hotkeyApply.disabled = !(hotkeyState.recording && hotkeyState.confirmed);
  resize();
}

function startHotkeyRecording() {
  hotkeyState.recording = true;
  hotkeyState.confirmed = false;
  hotkeyState.candidate = "";
  hotkeyError.hidden = true;
  hotkeySaved.hidden = true;
  hotkeyRecorder.focus();
  renderHotkey();
}

function stopHotkeyRecording() {
  hotkeyState.recording = false;
  hotkeyState.confirmed = false;
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
    hotkeyState.confirmed = false;
    hotkeyState.candidate = "";
    hotkeySaved.hidden = false;
    window.setTimeout(() => (hotkeySaved.hidden = true), 1500);
  } catch (message) {
    hotkeyError.textContent = String(message);
    hotkeyError.hidden = false;
  }

  renderHotkey();
}

hotkeyRecorder.addEventListener("click", () => {
  startHotkeyRecording();
});

hotkeyRecorder.addEventListener("blur", () => {
  if (hotkeyState.recording && !hotkeyState.confirmed) {
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

  if (event.key === "Enter" && hotkeyState.confirmed) {
    applyHotkey(hotkeyState.candidate);
    return;
  }

  if (event.key === "Backspace" || event.key === "Delete") {
    hotkeyState.candidate = "";
    hotkeyState.confirmed = false;
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
  hotkeyState.confirmed = hotkeyState.candidate === next;
  hotkeyState.candidate = next;
  renderHotkey();
});

hotkeyApply.addEventListener("click", () => {
  if (hotkeyState.candidate) {
    applyHotkey(hotkeyState.candidate);
  }
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
    invoke("save_switch", { key, value: event.target.checked });
  });
}

async function openSettings() {
  const settings = await invoke("get_settings");

  hotkeyState.current = settings.hotkey;
  hotkeyState.recording = false;
  hotkeyState.confirmed = false;
  hotkeyState.candidate = "";
  hotkeyError.hidden = true;
  hotkeySaved.hidden = true;
  renderHotkey();
  document.getElementById("model-input").value = settings.model_path;
  document.getElementById("switch-auto").checked = settings.auto_download;
  document.getElementById("switch-updates").checked = settings.check_updates;
  document.getElementById("switch-extension").checked = settings.serve_extension;
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
}

async function openHistory() {
  await renderHistory();
  showView("history");
}

async function renderHistory() {
  const history = await invoke("get_history");
  const list = document.getElementById("history-list");
  list.innerHTML = "";

  if (history.length === 0) {
    const empty = document.createElement("p");
    empty.className = "sub";
    empty.textContent = "暂无历史";
    list.appendChild(empty);
    return;
  }

  history.forEach((entry, index) => {
    const button = document.createElement("button");
    button.className = "history-entry";
    button.textContent =
      entry.target + " · " + preview(entry.text, 16) + " → " + preview(entry.translation, 22);
    button.addEventListener("click", async () => {
      await invoke("load_history_entry", { index });
      showView("translator");
    });
    list.appendChild(button);
  });
}

function preview(value, max) {
  return value.length > max ? value.slice(0, max) + "…" : value;
}

document.getElementById("history-clear").addEventListener("click", async () => {
  await invoke("clear_history");
  await renderHistory();
  resize();
});

listen("source", (event) => {
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
resize();
initLanguages();

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
