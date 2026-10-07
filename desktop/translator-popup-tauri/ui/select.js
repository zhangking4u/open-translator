// Region selector: a transparent click layer over the live screen. It reports
// the dragged rectangle (CSS pixels) to the backend, which hides the selector,
// captures the screen and maps the rectangle to capture pixels.

const { invoke } = window.__TAURI__.core;

const rect = document.getElementById("rect");
const hint = document.getElementById("hint");

let dragging = false;
let start = { x: 0, y: 0 };

window.addEventListener("mousedown", (event) => {
  if (event.button !== 0) return;

  dragging = true;
  start = { x: event.clientX, y: event.clientY };
  rect.hidden = false;
  updateRect(event.clientX, event.clientY);
});

window.addEventListener("mousemove", (event) => {
  if (!dragging) return;
  updateRect(event.clientX, event.clientY);
});

window.addEventListener("mouseup", async (event) => {
  if (!dragging) return;
  dragging = false;

  const x = Math.min(start.x, event.clientX);
  const y = Math.min(start.y, event.clientY);
  const width = Math.abs(event.clientX - start.x);
  const height = Math.abs(event.clientY - start.y);

  if (width < 3 || height < 3) {
    rect.hidden = true;
    return;
  }

  try {
    await invoke("shot_region", {
      x,
      y,
      width,
      height,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
    });
  } catch (error) {
    rect.hidden = true;
    hint.textContent = String(error);
  }
});

window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") invoke("shot_cancel");
});

window.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  invoke("shot_cancel");
});

function updateRect(x, y) {
  rect.style.left = Math.min(start.x, x) + "px";
  rect.style.top = Math.min(start.y, y) + "px";
  rect.style.width = Math.abs(x - start.x) + "px";
  rect.style.height = Math.abs(y - start.y) + "px";
}
