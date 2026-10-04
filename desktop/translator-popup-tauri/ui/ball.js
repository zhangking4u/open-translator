const { invoke } = window.__TAURI__.core;

const ball = document.getElementById("ball");
// A short dwell turns "the mouse passed over the ball" into "the user aimed at
// the ball"; leaving before it fires cancels the whole thing.
const HOVER_DELAY = 120;
let hoverTimer = null;

function cancelHover() {
  if (hoverTimer !== null) {
    clearTimeout(hoverTimer);
    hoverTimer = null;
  }
}

ball.addEventListener("mouseenter", () => {
  cancelHover();
  hoverTimer = window.setTimeout(() => {
    hoverTimer = null;
    invoke("ball_hover");
  }, HOVER_DELAY);
});

ball.addEventListener("mouseleave", cancelHover);

ball.addEventListener("click", (event) => {
  event.preventDefault();
  cancelHover();
  invoke("ball_click");
});
