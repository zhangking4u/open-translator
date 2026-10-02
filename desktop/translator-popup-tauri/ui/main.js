const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const source = document.getElementById("source");
const translation = document.getElementById("translation");
const status = document.getElementById("status");
const copy = document.getElementById("copy");

let current = "";

function setTranslation(text) {
  current = text;
  translation.innerHTML = "";
  const paragraph = document.createElement("p");
  paragraph.className = "text";
  paragraph.textContent = text;
  translation.appendChild(paragraph);
  copy.disabled = text.length === 0;
}

function setWaiting() {
  current = "";
  translation.innerHTML =
    '<p class="hint">等待划词</p>' +
    '<p class="sub">按 Ctrl+Alt+T 翻译选中文本</p>';
  copy.disabled = true;
}

async function resize() {
  const height = document.querySelector(".card").getBoundingClientRect().height + 20;
  await invoke("resize_window", { height });
}

new ResizeObserver(resize).observe(document.body);

document.getElementById("close").addEventListener("click", () => {
  invoke("hide_window");
});

copy.addEventListener("click", async () => {
  if (!current) {
    return;
  }

  await navigator.clipboard.writeText(current);
  status.textContent = "已复制";
  window.setTimeout(() => {
    status.textContent = "";
  }, 1500);
});

window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    invoke("hide_window");
  }
});

listen("translate", () => {
  source.hidden = true;
  status.textContent = "";
  translation.innerHTML = '<p class="hint">正在翻译…</p>';
  copy.disabled = true;
  resize();
});

setWaiting();
resize();
