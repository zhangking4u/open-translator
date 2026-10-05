// Live caption overlay: show the newest recognized segment, stream its
// translation into the second line, and fade out after a few seconds of
// silence. `caption-layout` (bilingual / translation / source) comes from the
// backend via `caption-config`.

const { listen } = window.__TAURI__.event;

const source = document.getElementById("source");
const translation = document.getElementById("translation");
const hint = document.getElementById("hint");

const IDLE_HIDE_MS = 6000;
let hideTimer = null;
let layout = "bilingual";

function scheduleHide() {
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    document.body.dataset.state = "hidden";
    source.textContent = "";
    translation.textContent = "";
  }, IDLE_HIDE_MS);
}

listen("caption-config", (event) => {
  layout = (event.payload && event.payload.layout) || "bilingual";
  document.body.dataset.layout = layout;
});

listen("caption-segment", (event) => {
  const text = (event.payload && event.payload.text) || "";
  if (!text) return;

  source.textContent = layout === "translation" ? "" : text;
  translation.textContent = "";
  hint.textContent = "";
  document.body.dataset.state = "visible";
  scheduleHide();
});

listen("caption-translation", (event) => {
  const payload = event.payload || {};
  if (typeof payload.text !== "string" || layout === "source") return;

  if (payload.done) {
    translation.textContent = payload.text;
    scheduleHide();
  } else {
    translation.textContent += payload.text;
  }
});

listen("caption-status", (event) => {
  const payload = event.payload || {};

  if (payload.state === "error") {
    clearTimeout(hideTimer);
    source.textContent = "实时字幕不可用";
    translation.textContent = "";
    hint.textContent = payload.message || "";
    document.body.dataset.state = "error";
    return;
  }

  if (payload.state === "starting") {
    source.textContent = "";
    translation.textContent = "";
    hint.textContent = payload.message || "正在加载语音模型…";
    document.body.dataset.state = "visible";
    return;
  }

  hint.textContent = "";
});
