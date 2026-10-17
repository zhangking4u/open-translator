// Screenshot viewer: draws the translations over the captured pixels
// (camera-translation style), with a bilingual list mode, hover-original
// footer, copy, refresh (same region) and close.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// The OS move loop behind data-tauri-drag-region / start_dragging does not
// engage for this always-on-top tool window on Windows, so the Windows drag
// is driven from the page like the docked ball (`move_viewer_by`): mousedown
// records the pointer, mousemove sends the screen delta, and the pointer
// capture WebView2 holds keeps the events coming. The drag region attributes
// stay in the markup for Linux, whose compositor owns window moves.
const IS_WINDOWS = navigator.userAgent.includes("Windows");

if (IS_WINDOWS) {
  for (const element of document.querySelectorAll("[data-tauri-drag-region]")) {
    element.removeAttribute("data-tauri-drag-region");
  }
}

let dragging = false;
let lastScreenX = 0;
let lastScreenY = 0;

function startWindowDrag(event) {
  if (event.button !== 0 || !IS_WINDOWS) return;

  dragging = true;
  lastScreenX = event.screenX;
  lastScreenY = event.screenY;
  event.preventDefault();
}

window.addEventListener("mousemove", (event) => {
  if (!dragging) return;

  const dx = event.screenX - lastScreenX;
  const dy = event.screenY - lastScreenY;

  if (dx === 0 && dy === 0) return;

  lastScreenX = event.screenX;
  lastScreenY = event.screenY;
  invoke("move_viewer_by", { dx, dy }).catch(() => {});
});

window.addEventListener("mouseup", () => {
  dragging = false;
});

// If the pointer grab is lost (e.g. a system dialog takes focus), stop the
// drag rather than applying a jump on the next move.
window.addEventListener("blur", () => {
  dragging = false;
});

const toolbar = document.getElementById("toolbar");

const image = document.getElementById("image");
const boxes = document.getElementById("boxes");
const footer = document.getElementById("footer");
const stage = document.getElementById("stage");
const listBody = document.getElementById("list-body");
const modeButton = document.getElementById("mode");
const inPlaceButton = document.getElementById("inplace");
const listButton = document.getElementById("list");
const copyButton = document.getElementById("copy");
const refreshButton = document.getElementById("refresh");
const closeButton = document.getElementById("close");

for (const handle of [toolbar, footer]) {
  handle.addEventListener("mousedown", startWindowDrag);
}

let blocks = [];
let showOriginal = false;
let listMode = false;
let inPlaceMode = false;
let sampleSource = "";
let sampleCanvas = null;
let sampleContext = null;

listen("shot-result", (event) => apply(event.payload));

function apply(payload) {
  blocks = Array.isArray(payload.blocks) ? payload.blocks : [];
  showOriginal = false;
  listMode = false;

  modeButton.textContent = "显示原文";
  modeButton.hidden = inPlaceMode;
  inPlaceButton.textContent = inPlaceMode ? "对照" : "原位";
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

function ensureSampleCanvas() {
  if (!image.naturalWidth || !image.naturalHeight) return false;
  if (sampleSource === image.src && sampleContext) return true;

  sampleSource = image.src;
  sampleCanvas = document.createElement("canvas");
  sampleCanvas.width = image.naturalWidth;
  sampleCanvas.height = image.naturalHeight;
  sampleContext = sampleCanvas.getContext("2d", { willReadFrequently: true });

  if (!sampleContext) return false;

  sampleContext.drawImage(image, 0, 0);
  return true;
}

// OCR quads are tight around the glyphs; the ring just outside them is the
// background. Taking the dominant quantized color is more robust than an
// average when the ring grazes a neighboring icon or line.
function sampleBackground(box) {
  if (!sampleContext) return "rgb(255, 255, 255)";

  const maxX = image.naturalWidth - 1;
  const maxY = image.naturalHeight - 1;
  const offset = 2;
  const step = Math.max(1, Math.floor(Math.min(box.width, box.height) / 8));
  const counts = new Map();

  function sample(x, y) {
    x = Math.round(Math.min(maxX, Math.max(0, x)));
    y = Math.round(Math.min(maxY, Math.max(0, y)));

    const data = sampleContext.getImageData(x, y, 1, 1).data;
    const r = data[0] & 0xf8;
    const g = data[1] & 0xf8;
    const b = data[2] & 0xf8;
    const key = (r << 16) | (g << 8) | b;
    const entry = counts.get(key) || { count: 0, r, g, b };
    entry.count += 1;
    counts.set(key, entry);
  }

  for (let x = box.x; x <= box.x + box.width; x += step) {
    sample(x, box.y - offset);
    sample(x, box.y + box.height + offset);
  }

  for (let y = box.y; y <= box.y + box.height; y += step) {
    sample(box.x - offset, y);
    sample(box.x + box.width + offset, y);
  }

  let best = null;

  for (const entry of counts.values()) {
    if (!best || entry.count > best.count) best = entry;
  }

  return best ? `rgb(${best.r}, ${best.g}, ${best.b})` : "rgb(255, 255, 255)";
}

function contrastColor(background) {
  const match = /rgb\((\d+),\s*(\d+),\s*(\d+)\)/.exec(background);
  if (!match) return "#ffffff";

  const [r, g, b] = match.slice(1).map(Number);
  const luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  return luminance > 0.6 ? "#111111" : "#ffffff";
}

// Shrink the translation until it wraps inside the original line height; if
// it still does not fit at the minimum size, the opaque cover grows downwards
// instead of clipping the text.
function fitInPlace(element, height) {
  const maxSize = Math.min(20, Math.max(10, height * 0.8));

  for (let size = maxSize; size >= 9; size -= 0.5) {
    element.style.fontSize = size + "px";

    if (element.scrollHeight <= height + 1) {
      break;
    }
  }

  element.style.minHeight = height + "px";
}

function addBoxHandlers(element, block) {
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
}

function render() {
  boxes.textContent = "";
  if (!image.clientWidth || !image.naturalWidth) return;

  const scale = image.clientWidth / image.naturalWidth;
  const canInPlace = inPlaceMode && ensureSampleCanvas();

  for (const block of blocks) {
    const box = quadBoundingBox(block.quad);
    if (!box) continue;

    const element = document.createElement("div");
    element.className = "box";
    element.title = block.translation || "";

    const height = Math.max(10, box.height * scale);

    if (canInPlace) {
      const inset = 2;
      const x = Math.max(0, box.x - inset);
      const y = Math.max(0, box.y - inset);
      const width = Math.min(image.naturalWidth - x, box.width + inset * 2);
      const coverHeight = Math.min(
        image.naturalHeight - y,
        box.height + inset * 2,
      );
      const background = sampleBackground(box);

      element.classList.add("inplace");
      element.style.left = x * scale + "px";
      element.style.top = y * scale + "px";
      element.style.setProperty("--box-width", width * scale + "px");
      element.style.setProperty("--box-height", coverHeight * scale + "px");
      element.style.setProperty("--box-bg", background);
      element.style.setProperty("--box-fg", contrastColor(background));
      element.textContent = block.translation || "";

      addBoxHandlers(element, block);
      boxes.append(element);
      fitInPlace(element, coverHeight * scale);
      continue;
    }

    element.style.left = box.x * scale + "px";
    element.style.top = box.y * scale + "px";
    element.style.minHeight = height + "px";
    element.style.fontSize = Math.min(Math.max(11, height * 0.68), 20) + "px";
    element.textContent = showOriginal ? block.text || "" : block.translation || "";

    addBoxHandlers(element, block);
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

inPlaceButton.addEventListener("click", () => {
  inPlaceMode = !inPlaceMode;

  if (inPlaceMode) {
    showOriginal = false;
    modeButton.textContent = "显示原文";
  }

  modeButton.hidden = inPlaceMode;
  inPlaceButton.textContent = inPlaceMode ? "对照" : "原位";
  footer.textContent = inPlaceMode ? "原位：已覆盖原文，点击译文可复制" : "";
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
