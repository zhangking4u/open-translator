// Renders a short native <select> as a segmented control. The select stays in
// the DOM (hidden) as the source of truth, so existing value/change wiring
// keeps working. Intended for 2–3 short options; use OTSelect for long lists.
(function () {
  function enhance(select, options = {}) {
    if (!select || select.dataset.segmented === "1") {
      return null;
    }

    select.dataset.segmented = "1";
    select.classList.add("segmented-native");
    select.setAttribute("aria-hidden", "true");
    select.tabIndex = -1;

    const wrap = document.createElement("div");
    wrap.className = "segmented";
    wrap.setAttribute("role", "radiogroup");

    if (options.title) {
      wrap.setAttribute("aria-label", options.title);
    }

    const buttons = [];

    for (const option of Array.from(select.options)) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "segmented-item";
      button.setAttribute("role", "radio");
      button.dataset.value = option.value;
      button.textContent = option.textContent;

      // The option title becomes the segment's hover hint, so each segment can
      // explain itself.
      if (option.title) {
        button.title = option.title;
      }

      buttons.push(button);
      wrap.appendChild(button);
    }

    if (select.parentNode) {
      select.parentNode.insertBefore(wrap, select);
    }

    wrap.appendChild(select);

    function sync() {
      for (const button of buttons) {
        const active = button.dataset.value === select.value;
        button.classList.toggle("active", active);
        button.setAttribute("aria-checked", String(active));
        button.tabIndex = active ? 0 : -1;
      }
    }

    function choose(button) {
      if (!button || button.dataset.value === select.value) {
        return;
      }

      select.value = button.dataset.value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      sync();
    }

    wrap.addEventListener("click", (event) => {
      choose(event.target.closest(".segmented-item"));
    });

    wrap.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
        return;
      }

      event.preventDefault();
      const index = buttons.findIndex((button) => button.dataset.value === select.value);
      const step = event.key === "ArrowRight" ? 1 : -1;
      const next = buttons[(index + step + buttons.length) % buttons.length];
      choose(next);
      next.focus();
    });

    sync();

    return { sync, element: wrap };
  }

  window.OTSegmented = { enhance };
})();
