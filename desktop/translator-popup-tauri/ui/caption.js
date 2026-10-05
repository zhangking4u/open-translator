// Live caption overlay: render the newest segment, keep the previous one dim,
// and fade out after a few seconds of silence.

const { listen } = window.__TAURI__.event;

const caption = document.getElementById("caption");
const prev = document.getElementById("prev");
const line = document.getElementById("line");
const hint = document.getElementById("hint");

const IDLE_HIDE_MS = 6000;
let hideTimer = null;

function scheduleHide() {
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    document.body.dataset.state = "hidden";
    prev.textContent = "";
    line.textContent = "";
  }, IDLE_HIDE_MS);
}

listen("caption-segment", (event) => {
  const text = (event.payload && event.payload.text) || "";
  if (!text) return;

  prev.textContent = line.textContent;
  line.textContent = text;
  hint.textContent = "";
  document.body.dataset.state = "visible";
  scheduleHide();
});

listen("caption-status", (event) => {
  const payload = event.payload || {};

  if (payload.state === "error") {
    clearTimeout(hideTimer);
    line.textContent = "实时字幕不可用";
    hint.textContent = payload.message || "";
    document.body.dataset.state = "error";
    return;
  }

  if (payload.state === "starting") {
    line.textContent = "";
    hint.textContent = payload.message || "正在加载语音模型…";
    document.body.dataset.state = "visible";
    return;
  }

  // "listening": hide the hint but keep whatever caption is on screen.
  hint.textContent = "";
});
