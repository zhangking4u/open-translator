const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const ball = document.getElementById("ball");
// A short dwell turns "the mouse passed over the ball" into "the user aimed at
// the ball"; leaving before it fires cancels the whole thing.
const HOVER_DELAY = 150;

let hoverTimer = null;
let dragging = false;
let lastScreenX = 0;
let lastScreenY = 0;

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

// Dragging the ball repositions the dock; a drag must never count as a hover
// commit.
ball.addEventListener("mousedown", (event) => {
  if (event.button !== 0) {
    return;
  }

  dragging = true;
  lastScreenX = event.screenX;
  lastScreenY = event.screenY;
  cancelHover();
  event.preventDefault();
});

window.addEventListener("mousemove", (event) => {
  if (!dragging) {
    return;
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

window.addEventListener("mouseup", () => {
  if (!dragging) {
    return;
  }

  dragging = false;
  invoke("save_ball_position");
});

// If the pointer grab is lost (e.g. the compositor cancels it), still persist
// where the ball ended up.
window.addEventListener("blur", () => {
  if (!dragging) {
    return;
  }

  dragging = false;
  invoke("save_ball_position");
});
