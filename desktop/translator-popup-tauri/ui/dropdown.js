"use strict";

// Shared Apple-style select for the desktop client: replaces a native <select>
// with a filled chip button and a checkmarked menu. The native select stays
// hidden in the DOM as the value holder, so existing `value` / `change` code
// keeps working.
//
// This is the desktop copy of browser/extension/dropdown.js; keep the two in
// sync (the extension version additionally embeds its cssText into shadow
// roots, the desktop injects it into the page).
//
// Usage:
//   const controller = OTSelect.enhance(select, { title });
//   OTSelect.sync(select);            // after setting select.value in code
//   OTSelect.inject();                // once, then enhance the selects
(function () {
  const CHECK =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.5 8.5 6.5 11.5 12.5 4.5"/></svg>';
  const CHEVRON =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 6.25 8 10.25 12 6.25"/></svg>';

  const cssText = [
    ".ot-select { position: relative; display: inline-flex; max-width: 100%; }",
    ".ot-select-native { position: absolute; width: 1px; height: 1px; opacity: 0;",
    "  pointer-events: none; }",
    ".ot-select-button { display: inline-flex; align-items: center; gap: 4px; max-width: 100%;",
    "  min-height: var(--ot-control-height, 24px); padding: 3px 8px; border: 0;",
    "  border-radius: var(--ot-radius-sm, 8px); background: var(--ot-fill, rgba(120,120,128,.12));",
    "  color: var(--ot-label, rgba(0,0,0,.85)); font: inherit;",
    "  font-size: var(--ot-control-font, 12.5px); line-height: 1.4; cursor: pointer;",
    "  white-space: nowrap; transition: background-color .15s ease; }",
    ".ot-select-button:hover { background: var(--ot-fill-hover, rgba(120,120,128,.2)); }",
    ".ot-select-button:focus-visible { outline: 2px solid var(--ot-accent, #007aff); outline-offset: 1px; }",
    ".ot-select-label { overflow: hidden; text-overflow: ellipsis; }",
    ".ot-select-button svg { flex: none; opacity: .65; }",
    ".ot-select-menu { position: absolute; left: 0; top: calc(100% + 6px); z-index: 5;",
    "  min-width: 100%; width: max-content; max-width: 240px; max-height: 240px;",
    "  overflow: auto; display: flex; flex-direction: column; padding: 4px;",
    "  background: var(--ot-bg-solid, #fff); border-radius: 10px;",
    "  box-shadow: inset 0 0 0 .5px var(--ot-hairline, rgba(0,0,0,.08)),",
    "    var(--ot-shadow, 0 1px 2px rgba(0,0,0,.06), 0 12px 32px rgba(0,0,0,.12));",
    "  scrollbar-width: thin; scrollbar-color: rgba(120,120,128,.4) transparent; }",
    ".ot-select-menu[hidden] { display: none; }",
    ".ot-select-menu-up { top: auto; bottom: calc(100% + 6px); }",
    ".ot-select-menu::-webkit-scrollbar { width: 6px; }",
    ".ot-select-menu::-webkit-scrollbar-thumb { background: rgba(120,120,128,.4); border-radius: 3px; }",
    ".ot-select-menu::-webkit-scrollbar-track { background: transparent; }",
    ".ot-select-menu-inline { position: static; min-width: 0; width: auto; max-width: none;",
    "  padding: 0; background: none; border-radius: 0; box-shadow: none; }",
    ".ot-select-item { display: flex; align-items: center; gap: 6px; font: inherit;",
    "  font-size: var(--ot-control-font, 12.5px); text-align: left; min-height: 26px;",
    "  padding: 4px 6px; border: 0; border-radius: 6px; background: none;",
    "  color: var(--ot-label, rgba(0,0,0,.85)); cursor: pointer; white-space: nowrap;",
    "  transition: background-color .15s ease; }",
    ".ot-select-item:hover, .ot-select-item.focused { background: var(--ot-fill, rgba(120,120,128,.12)); }",
    ".ot-select-item.active { color: var(--ot-accent, #007aff); }",
    ".ot-select-check { width: 12px; height: 12px; display: inline-flex; align-items: center;",
    "  justify-content: center; visibility: hidden; color: var(--ot-accent, #007aff); }",
    ".ot-select-check svg { width: 12px; height: 12px; }",
    ".ot-select-item.active .ot-select-check { visibility: visible; }",
    "@media (prefers-reduced-motion: reduce) {",
    "  .ot-select-button, .ot-select-item { transition: none; }",
    "}",
  ].join("\n");

  const controllers = new WeakMap();

  function icon(markup) {
    const doc = new DOMParser().parseFromString(markup, "image/svg+xml");
    return document.importNode(doc.documentElement, true);
  }

  function enhance(select, options = {}) {
    if (controllers.has(select)) return controllers.get(select);

    const wrap = document.createElement("span");
    wrap.className = "ot-select";

    const button = document.createElement("button");
    button.type = "button";
    button.className = "ot-select-button";
    if (options.title) button.title = options.title;
    button.setAttribute("aria-haspopup", "listbox");
    button.setAttribute("aria-label", options.title || select.title || "选择");

    const label = document.createElement("span");
    label.className = "ot-select-label";
    button.append(label, icon(CHEVRON));

    const menu = document.createElement("div");
    menu.className = "ot-select-menu" + (options.menuContainer ? " ot-select-menu-inline" : "");
    menu.hidden = true;
    menu.setAttribute("role", "listbox");

    const items = new Map();
    for (const option of Array.from(select.options)) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "ot-select-item";
      item.dataset.value = option.value;
      item.setAttribute("role", "option");

      const check = document.createElement("span");
      check.className = "ot-select-check";
      check.append(icon(CHECK));

      const text = document.createElement("span");
      text.className = "ot-select-item-label";
      text.textContent = option.textContent;

      item.append(check, text);
      items.set(option.value, item);
      menu.append(item);
    }

    const itemList = Array.from(items.values());

    select.classList.add("ot-select-native");
    select.setAttribute("aria-hidden", "true");
    select.tabIndex = -1;

    if (select.parentNode) select.parentNode.insertBefore(wrap, select);
    wrap.append(button, select);
    if (options.menuContainer) options.menuContainer.append(menu);
    else wrap.append(menu);

    let focusedIndex = -1;

    function setFocused(index) {
      focusedIndex = index;
      itemList.forEach((item, itemIndex) => {
        item.classList.toggle("focused", itemIndex === index);
      });
      if (itemList[index]) itemList[index].scrollIntoView({ block: "nearest" });
    }

    function sync() {
      const option = select.selectedOptions && select.selectedOptions[0];
      label.textContent = option ? option.textContent : "";
      for (const [value, item] of items) {
        item.classList.toggle("active", value === select.value);
      }
    }

    // Keeps the menu inside the viewport: flips it above the button when the
    // space below is too small, and always constrains its height (the menu
    // then scrolls). The inline variant follows the card, so it only limits
    // the height to the space left below it.
    function place() {
      const margin = 8;
      const preferred = 240;

      if (options.menuContainer) {
        const rect = menu.getBoundingClientRect();
        const spaceBelow = window.innerHeight - rect.top - margin;
        menu.style.maxHeight = Math.max(96, Math.min(preferred, spaceBelow)) + "px";
        return;
      }

      const rect = button.getBoundingClientRect();
      const spaceBelow = window.innerHeight - rect.bottom - margin - 6;
      const spaceAbove = rect.top - margin - 6;
      const flipped = spaceBelow < Math.min(preferred, spaceAbove);
      menu.classList.toggle("ot-select-menu-up", flipped);
      menu.style.maxHeight =
        Math.max(96, Math.min(preferred, flipped ? spaceAbove : spaceBelow)) + "px";
    }

    function close() {
      if (menu.hidden) return;
      menu.hidden = true;
      focusedIndex = -1;
      for (const item of itemList) item.classList.remove("focused");
      document.removeEventListener("mousedown", onOutside, true);
      document.removeEventListener("keydown", onKeydown, true);
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
      if (options.onToggle) options.onToggle(false);
    }

    function open() {
      if (!menu.hidden) return;
      menu.hidden = false;
      place();
      setFocused(itemList.findIndex((item) => item.dataset.value === select.value));
      document.addEventListener("mousedown", onOutside, true);
      document.addEventListener("keydown", onKeydown, true);
      window.addEventListener("resize", place);
      window.addEventListener("scroll", place, true);
      if (options.onToggle) options.onToggle(true);
      place();
    }

    function choose(item) {
      if (!item) return;
      select.value = item.dataset.value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      sync();
      close();
    }

    function onOutside(event) {
      // Shadow DOM retargets event.target to the host, so use composedPath:
      // plain `wrap.contains(event.target)` closes the menu on its own items.
      const path = event.composedPath();
      if (path.includes(wrap) || path.includes(menu)) return;
      close();
    }

    function onKeydown(event) {
      if (event.key === "Escape" && !menu.hidden) {
        event.stopPropagation();
        close();
        button.focus();
      }
    }

    button.addEventListener("click", () => {
      if (menu.hidden) open();
      else close();
    });

    button.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      if (menu.hidden) {
        open();
        return;
      }
      const step = event.key === "ArrowDown" ? 1 : -1;
      setFocused((focusedIndex + step + itemList.length) % itemList.length);
    });

    menu.addEventListener("click", (event) => {
      choose(event.target.closest(".ot-select-item"));
    });

    menu.addEventListener("keydown", (event) => {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : -1;
        setFocused((focusedIndex + step + itemList.length) % itemList.length);
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        choose(itemList[focusedIndex]);
      }
    });

    select.addEventListener("change", sync);

    const controller = {
      element: wrap,
      button,
      menu,
      sync,
      open,
      close,
      isOpen: () => !menu.hidden,
    };
    controllers.set(select, controller);
    sync();
    return controller;
  }

  function sync(select) {
    const controller = controllers.get(select);
    if (controller) controller.sync();
  }

  function inject() {
    if (document.getElementById("ot-select-styles")) return;
    const style = document.createElement("style");
    style.id = "ot-select-styles";
    style.textContent = cssText;
    document.head.append(style);
  }

  globalThis.OTSelect = { cssText, enhance, sync, inject };
})();
