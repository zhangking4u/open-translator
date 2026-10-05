// Live caption overlay: show the newest recognized segment, stream its
// translation into the second line, and fade out after a few seconds of
// silence. `caption-layout` (bilingual / translation / source) comes from the
// backend via `caption-config`.

const { listen } = window.__TAURI__.event;
const { invoke } = window.__TAURI__.core;

const caption = document.getElementById("caption");
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
  translation.classList.remove("streaming");
  translation.classList.add("pending");
  hint.textContent = "";
  document.body.dataset.state = "visible";
  scheduleHide();
});

listen("caption-translation", (event) => {
  const payload = event.payload || {};
  if (typeof payload.text !== "string" || layout === "source") return;

  translation.classList.remove("pending");

  if (payload.done) {
    translation.textContent = payload.text;
    translation.classList.remove("streaming");
    scheduleHide();
  } else {
    translation.classList.add("streaming");
    translation.textContent += payload.text;
  }
});

listen("caption-status", (event) => {
  const payload = event.payload || {};

  if (payload.state === "error") {
    clearTimeout(hideTimer);
    source.textContent = "实时字幕不可用";
    translation.textContent = "";
    translation.classList.remove("pending", "streaming");
    hint.textContent = payload.message || "";
    document.body.dataset.state = "error";
    return;
  }

  if (payload.state === "starting" || payload.state === "downloading") {
    source.textContent = "";
    translation.textContent = "";
    translation.classList.remove("pending", "streaming");
    hint.textContent =
      payload.message ||
      (payload.state === "starting" ? "正在加载语音模型…" : "正在下载语音模型…");
    document.body.dataset.state = "visible";
    return;
  }

  hint.textContent = "";
});

// Repositioning mode: the backend turns off click-through for 30 seconds (or
// until the drag ends) so the overlay can receive mouse events.
listen("caption-editing", (event) => {
  const active = !!(event.payload && event.payload.active);
  document.body.dataset.editing = active ? "true" : "false";

  if (active) {
    clearTimeout(hideTimer);
    hint.textContent = "拖动字幕条调整位置，松开后自动恢复点击穿透";
    document.body.dataset.state = "visible";
  } else {
    hint.textContent = "";
    scheduleHide();
  }
});

let dragging = false;
let lastX = 0;
let lastY = 0;

caption.addEventListener("pointerdown", (event) => {
  if (document.body.dataset.editing !== "true" || event.button !== 0) return;

  dragging = true;
  lastX = event.screenX;
  lastY = event.screenY;

  try {
    // Keep receiving moves even when the pointer leaves the overlay.
    caption.setPointerCapture(event.pointerId);
  } catch (error) {
    // pointer capture is best-effort
  }

  event.preventDefault();
});

caption.addEventListener("pointermove", (event) => {
  if (!dragging) return;

  const dx = event.screenX - lastX;
  const dy = event.screenY - lastY;

  if (dx === 0 && dy === 0) return;

  lastX = event.screenX;
  lastY = event.screenY;
  invoke("move_caption_by", { dx, dy });
});

function endDrag() {
  if (!dragging) return;

  dragging = false;
  invoke("save_caption_position");
  invoke("finish_caption_move");
}

caption.addEventListener("pointerup", endDrag);
caption.addEventListener("pointercancel", endDrag);
