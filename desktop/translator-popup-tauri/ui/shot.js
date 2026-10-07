// Screenshot viewer: draws the translations over the captured pixels
// (camera-translation style), with a bilingual list mode, hover-original
// footer, copy, refresh (same region) and close.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const image = document.getElementById("image");
const boxes = document.getElementById("boxes");
const footer = document.getElementById("footer");
const stage = document.getElementById("stage");
const listBody = document.getElementById("list-body");
const modeButton = document.getElementById("mode");
const listButton = document.getElementById("list");
const copyButton = document.getElementById("copy");
const refreshButton = document.getElementById("refresh");
const closeButton = document.getElementById("close");

let blocks = [];
let showOriginal = false;
let listMode = false;

listen("shot-result", (event) => apply(event.payload));

function apply(payload) {
  blocks = Array.isArray(payload.blocks) ? payload.blocks : [];
  showOriginal = false;
  listMode = false;

  modeButton.textContent = "显示原文";
  listButton.textContent = "列表";
  listBody.hidden = true;
  stage.hidden = false;
  footer.textContent = payload.error || "";

  if (payload.image) {
    if (image.src === payload.image && image.complete) {
      render();
    } else {
      image.onload = render;
      image.src = payload.image;
    }
  } else {
    image.removeAttribute("src");
    boxes.textContent = "";
  }

  renderList();
}

// The window is hidden while the result arrives, so the first render can run
// with clientWidth 0; re-render as soon as layout gives the image a size, and
// whenever the window is resized.
new ResizeObserver(() => {
  if (image.clientWidth) render();
}).observe(image);

window.addEventListener("resize", () => render());

function quadBoundingBox(quad) {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const point of Array.isArray(quad) ? quad : []) {
    if (!Array.isArray(point) || point.length < 2) continue;

    const [x, y] = point;
    if (typeof x !== "number" || typeof y !== "number") continue;

    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x);
    maxY = Math.max(maxY, y);
  }

  if (!Number.isFinite(minX)) return null;
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

function render() {
  boxes.textContent = "";
  if (!image.clientWidth || !image.naturalWidth) return;

  const scale = image.clientWidth / image.naturalWidth;

  for (const block of blocks) {
    const box = quadBoundingBox(block.quad);
    if (!box) continue;

    const element = document.createElement("div");
    element.className = "box";

    const height = Math.max(10, box.height * scale);
    element.style.left = box.x * scale + "px";
    element.style.top = box.y * scale + "px";
    element.style.minHeight = height + "px";
    element.style.fontSize = Math.min(Math.max(11, height * 0.68), 20) + "px";
    element.textContent = showOriginal ? block.text || "" : block.translation || "";

    element.addEventListener("mouseenter", () => {
      if (listMode) return;
      footer.textContent =
        (showOriginal ? "译文：" : "原文：") +
        (showOriginal ? block.translation || "" : block.text || "");
    });

    element.addEventListener("click", () => {
      invoke("copy_text", { text: block.translation || "" });
      footer.textContent = "已复制译文";
    });

    boxes.append(element);
  }
}

function renderList() {
  listBody.textContent = "";

  for (const block of blocks) {
    const item = document.createElement("div");
    item.className = "item";

    const translation = document.createElement("div");
    translation.className = "translation";
    translation.textContent = block.translation || "";

    const original = document.createElement("div");
    original.className = "original";
    original.textContent = block.text || "";

    item.append(translation, original);
    listBody.append(item);
  }
}

modeButton.addEventListener("click", () => {
  showOriginal = !showOriginal;
  modeButton.textContent = showOriginal ? "显示译文" : "显示原文";
  render();
});

listButton.addEventListener("click", () => {
  listMode = !listMode;
  listButton.textContent = listMode ? "贴图" : "列表";
  listBody.hidden = !listMode;
  stage.hidden = listMode;
  footer.textContent = "";
  if (!listMode) render();
});

copyButton.addEventListener("click", () => {
  const text = blocks
    .map((block) => block.translation || "")
    .filter(Boolean)
    .join("\n");

  invoke("copy_text", { text });
  copyButton.textContent = "已复制";
  setTimeout(() => {
    copyButton.textContent = "复制译文";
  }, 1200);
});

refreshButton.addEventListener("click", () => invoke("shot_refresh"));
closeButton.addEventListener("click", () => invoke("shot_close"));

window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") invoke("shot_close");
});
