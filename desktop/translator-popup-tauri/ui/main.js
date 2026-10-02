const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const source = document.getElementById("source");
const translation = document.getElementById("translation");
const status = document.getElementById("status");
const copy = document.getElementById("copy");
const retranslate = document.getElementById("retranslate");

let current = "";
let lastSource = "";
let streaming = false;

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
  const percent = total ? Math.round((downloaded / total) * 100) : null;

  render(
    '<p class="hint">正在下载模型</p>' +
      '<progress max="100" value="' + (percent ?? 0) + '"></progress>' +
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

  const paragraph = document.createElement("div");
  paragraph.className = "error-card";

  const title = document.createElement("p");
  title.className = "error-title";
  title.textContent = "出错了";

  const body = document.createElement("p");
  body.className = "error-body";
  body.textContent = message;

  paragraph.append(title, body);
  translation.innerHTML = "";
  translation.appendChild(paragraph);
  resize();
}

function appendDelta(piece) {
  current += piece;
  streaming = true;
  render('<p class="text">' + escapeHtml(current) + '</p>');
  setCopyEnabled(false);
}

function finish(translationText) {
  current = translationText;
  streaming = false;
  render('<p class="text">' + escapeHtml(current) + '</p>');
  setCopyEnabled(current.length > 0);
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

new ResizeObserver(resize).observe(document.body);

document.getElementById("close").addEventListener("click", () => {
  invoke("hide_window");
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
  if (event.key === "Escape") {
    invoke("hide_window");
  }
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
  if (!streaming && !current) {
    showWaiting();
  }
});

listen("model-error", (event) => {
  showError(event.payload.message, false);
});

showWaiting();
resize();
