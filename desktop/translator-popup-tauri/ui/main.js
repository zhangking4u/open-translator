const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const views = {
  translator: document.getElementById("translator-view"),
  settings: document.getElementById("settings-view"),
  history: document.getElementById("history-view"),
};

const source = document.getElementById("source");
const translation = document.getElementById("translation");
const status = document.getElementById("status");
const copy = document.getElementById("copy");
const retranslate = document.getElementById("retranslate");
const pin = document.getElementById("pin");
const close = document.getElementById("close");

let current = "";
let lastSource = "";
let streaming = false;
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
  const height = document.querySelector(".card").getBoundingClientRect().height + 20;
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
  setCopyEnabled(false);
  render('<p class="hint">等待划词</p><p class="sub">按 Ctrl+Alt+T 翻译选中文本</p>');
}

function showEmpty() {
  current = "";
  source.hidden = true;
  source.textContent = "";
  retranslate.hidden = true;
  setCopyEnabled(false);
  render('<p class="hint">未选中文本</p><p class="sub">请在其它应用中选中要翻译的内容</p>');
}

function showSource(text) {
  lastSource = text;
  source.hidden = false;
  source.textContent = text;
  retranslate.hidden = false;
  resize();
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
  pin.title = pinned ? "取消固定" : "固定窗口";
  close.hidden = pinned;
});

copy.addEventListener("click", async () => {
  if (!current) {
    return;
  }

  await invoke("copy_text", { text: current });
  setStatus("已复制");
  window.setTimeout(() => setStatus(""), 1500);
});

retranslate.addEventListener("click", () => {
  invoke("retranslate");
});

window.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") {
    return;
  }

  if (view !== "translator") {
    showView("translator");
  } else {
    invoke("hide_window");
  }
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

document.getElementById("hotkey-apply").addEventListener("click", async () => {
  const input = document.getElementById("hotkey-input");
  const error = document.getElementById("hotkey-error");

  try {
    await invoke("save_hotkey", { spec: input.value });
    error.hidden = true;
  } catch (message) {
    error.textContent = String(message);
    error.hidden = false;
  }

  resize();
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

  document.getElementById("hotkey-input").value = settings.hotkey;
  document.getElementById("hotkey-error").hidden = true;
  document.getElementById("model-input").value = settings.model_path;
  document.getElementById("switch-auto").checked = settings.auto_download;
  document.getElementById("switch-updates").checked = settings.check_updates;
  document.getElementById("switch-extension").checked = settings.serve_extension;
  document.getElementById("config-path").textContent = settings.config_path ?? "";

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
  showSource(event.payload.text);
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

listen("history-changed", () => {
  if (view === "history") {
    renderHistory();
  }
});

showWaiting();
resize();

invoke("get_initial_view").then((initial) => {
  if (initial === "settings") {
    openSettings();
  } else if (initial === "history") {
    openHistory();
  }
});
