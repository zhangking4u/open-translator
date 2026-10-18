const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const ball = document.getElementById("ball");
// A short dwell turns "the mouse passed over the ball" into "the user aimed at
// the ball"; leaving before it fires cancels the whole thing.
const HOVER_DELAY = 150;
// A press that moves farther than this is a reposition drag, not a click.
const DRAG_THRESHOLD = 5;

let hoverTimer = null;
let pressed = false;
let dragging = false;
let lastScreenX = 0;
let lastScreenY = 0;
let pressScreenX = 0;
let pressScreenY = 0;

listen("ball-state", (event) => {
  document.body.dataset.state = event.payload || "idle";
});

function cancelHover() {
  if (hoverTimer !== null) {
    clearTimeout(hoverTimer);
    hoverTimer = null;
  }
}

ball.addEventListener("mouseenter", () => {
  if (dragging) {
    return;
  }

  cancelHover();
  hoverTimer = window.setTimeout(() => {
    hoverTimer = null;
    invoke("ball_hover");
  }, HOVER_DELAY);
});

ball.addEventListener("mouseleave", cancelHover);

// Clicking the ball is the deterministic commit. Dragging only starts after
// the pointer has moved past a small threshold, so a slightly shaky click is
// still a click.
ball.addEventListener("mousedown", (event) => {
  if (event.button !== 0) {
    return;
  }

  pressed = true;
  dragging = false;
  pressScreenX = event.screenX;
  pressScreenY = event.screenY;
  lastScreenX = event.screenX;
  lastScreenY = event.screenY;
  cancelHover();
  event.preventDefault();
});

window.addEventListener("mousemove", (event) => {
  if (!pressed) {
    return;
  }

  if (!dragging) {
    const movedX = event.screenX - pressScreenX;
    const movedY = event.screenY - pressScreenY;

    if (Math.hypot(movedX, movedY) < DRAG_THRESHOLD) {
      return;
    }

    dragging = true;
    lastScreenX = event.screenX;
    lastScreenY = event.screenY;
  }

  const dx = event.screenX - lastScreenX;
  const dy = event.screenY - lastScreenY;

  if (dx === 0 && dy === 0) {
    return;
  }

  lastScreenX = event.screenX;
  lastScreenY = event.screenY;
  invoke("move_ball_by", { dx, dy });
});

window.addEventListener("mouseup", (event) => {
  if (!pressed) {
    return;
  }

  const wasDragging = dragging;
  pressed = false;
  dragging = false;

  if (wasDragging) {
    invoke("save_ball_position");
    return;
  }

  const target = document.elementFromPoint(event.clientX, event.clientY);
  if (target && ball.contains(target)) {
    invoke("ball_click");
  }
});

// If the pointer grab is lost (e.g. the compositor cancels it), still persist
// where the ball ended up.
window.addEventListener("blur", () => {
  if (!pressed) {
    return;
  }

  const wasDragging = dragging;
  pressed = false;
  dragging = false;

  if (wasDragging) {
    invoke("save_ball_position");
  }
});

// Keep keyboard activation working if the platform ever exposes the ball to
// assistive technology; the docked ball itself is intentionally non-focusable.
ball.addEventListener("keydown", (event) => {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    invoke("ball_click");
  }
});
