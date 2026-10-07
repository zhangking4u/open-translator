"use strict";

// Bilingual reading mode (网页翻译): inserts a translation under each page
// block while keeping the original text. Passive by design — it never opens
// the selection/typing bubbles, never replaces the original, and never writes
// translation history (record: false). Driven by the popup toggle
// (`readingEnabled`) and the per-site opt-out (`disabledSites`); top frame
// only in v1 (see docs/READING_TRANSLATION.md §5).

(() => {
  const api = globalThis.browser ?? globalThis.chrome;

  const DEFAULTS = {
    readingEnabled: false,
    target: "zh",
    disabledSites: [],
  };

  const PRIMARY =
    "h1, h2, h3, h4, h5, h6, p, li, blockquote, td, th, figcaption, dd, dt, summary";
  const CONTAINER = "div, section, article, main";
  const SKIP_CLOSEST =
    "[data-ot-reading], footer, aside, pre, code, script, style, noscript, textarea, select, [contenteditable], [translate=no], .ot-reading-translation";
  const MIN_CHARS = 12;
  const MIN_HEADING_CHARS = 3;
  const MIN_NAV_CHARS = 4;
  const MAX_BLOCK_CHARS = 1200;
  const MAX_NEW_PER_BATCH = 20;
  const LINK_DENSITY = 0.6;
  const CJK_TARGET = /^(zh|ja|ko)/;

  const STYLE_ID = "ot-reading-style";
  const STYLE = `
.ot-reading-translation {
  display: block;
  margin: 2px 0 10px;
  padding: 2px 0 2px 10px;
  border-left: 3px solid #4a90d9;
  color: #3c3c40;
  font-size: 0.96em;
  line-height: 1.6;
}
.ot-reading-translation .ot-reading-actions {
  margin-left: 8px;
  opacity: 0;
  transition: opacity .12s ease;
  user-select: none;
}
.ot-reading-translation:hover .ot-reading-actions,
.ot-reading-translation:focus-within .ot-reading-actions {
  opacity: 1;
}
.ot-reading-translation .ot-reading-actions button {
  font: inherit;
  font-size: 0.85em;
  margin-left: 4px;
  padding: 1px 8px;
  cursor: pointer;
  color: #3c3c40;
  background: rgba(120, 120, 128, .12);
  border: 0;
  border-radius: 7px;
}
.ot-reading-translation .ot-reading-actions button:hover {
  background: rgba(120, 120, 128, .22);
}
.ot-reading-translation.ot-reading-pending {
  color: #8e8e93;
  border-left-color: #c7c7cc;
}
.ot-reading-translation.ot-reading-error {
  color: #d70015;
  border-left-color: #d70015;
}
.ot-reading-translation.ot-reading-error .ot-reading-text {
  cursor: pointer;
}
.ot-reading-translation.ot-reading-compact {
  margin: 0 0 2px;
  padding-left: 6px;
  border-left-width: 2px;
  font-size: 0.85em;
  line-height: 1.4;
}
@media (prefers-color-scheme: dark) {
  .ot-reading-translation { color: #c9c9ce; border-left-color: #5b9bd5; }
  .ot-reading-translation .ot-reading-actions button { color: #c9c9ce; background: rgba(235, 235, 245, .16); }
  .ot-reading-translation .ot-reading-actions button:hover { background: rgba(235, 235, 245, .28); }
  .ot-reading-translation.ot-reading-pending { color: #8e8e93; border-left-color: #48484a; }
  .ot-reading-translation.ot-reading-error { color: #ff6961; border-left-color: #ff6961; }
}
`;

  let enabled = DEFAULTS.readingEnabled;
  let target = DEFAULTS.target;
  let disabledSites = [];
  let started = false;
  let observer = null;
  let intersection = null;

  const cache = new Map();
  const queue = [];
  let queuedElements = new WeakSet();
  const offscreen = new Map();
  let inFlight = false;

  // Streaming transport: one `translate` port shared by the serialized queue.
  // Deltas render into the pending block; the port falls back to the one-shot
  // message when the background cannot stream.
  let port = null;
  let portSeq = 0;
  let stream = null;

  const stats = {
    queued: 0,
    done: 0,
    failed: 0,
    cacheHits: 0,
    pruned: 0,
    batches: 0,
  };

  // Test hook (isolated world): the CDP e2e waits on these numbers.
  globalThis.__OT_READING__ = {
    stats,
    get enabled() {
      return enabled;
    },
    get active() {
      return started;
    },
    get pending() {
      return queue.length + (inFlight ? 1 : 0);
    },
  };

  function siteDisabled() {
    return disabledSites.includes(location.hostname);
  }

  function shouldRun() {
    return enabled && !siteDisabled() && window.top === window;
  }

  function cjkRatio(text) {
    const letters = text.match(/\p{Letter}/gu) || [];
    if (letters.length === 0) return 0;
    const cjk = text.match(/[\u3400-\u9fff\uf900-\ufaff]/g) || [];
    return cjk.length / letters.length;
  }

  function inViewport(element) {
    const rect = element.getBoundingClientRect();
    return rect.bottom > 0 && rect.top < window.innerHeight && rect.height > 0;
  }

  function targetLanguageBlock(element) {
    const lang = (element.lang || document.documentElement.lang || "").toLowerCase();
    if (!lang) return false;

    const primary = target.toLowerCase().split(/[-_]/)[0];
    return lang.split(/[-_]/)[0] === primary;
  }

  function nodeFor(element) {
    if (element.tagName === "LI" || element.tagName === "TD") {
      return (
        [...element.children].find(
          (child) => child.dataset.otReadingTranslation === "1"
        ) || null
      );
    }

    const next = element.nextElementSibling;
    return next && next.dataset.otReadingTranslation === "1" ? next : null;
  }

  /// Creates the injection skeleton ("翻译中…") next to the block.
  function skeletonFor(element) {
    let node = nodeFor(element);
    if (node) return node;

    node = document.createElement("div");
    node.className = "ot-reading-translation ot-reading-pending";
    node.dataset.otReadingTranslation = "1";
    node.lang = target;
    if (element.closest("nav, header")) node.classList.add("ot-reading-compact");

    const text = document.createElement("span");
    text.className = "ot-reading-text";
    text.textContent = "翻译中…";

    const actions = document.createElement("span");
    actions.className = "ot-reading-actions";

    const copy = document.createElement("button");
    copy.type = "button";
    copy.textContent = "复制";

    const retry = document.createElement("button");
    retry.type = "button";
    retry.textContent = "重译";

    actions.append(copy, retry);
    node.append(text, actions);

    copy.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      const translation = text.textContent;
      if (!translation) return;
      navigator.clipboard.writeText(translation).then(() => {
        copy.textContent = "已复制";
        setTimeout(() => {
          copy.textContent = "复制";
        }, 1200);
      });
    });
    retry.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      retryEntry(element);
    });

    if (element.tagName === "LI" || element.tagName === "TD") {
      element.appendChild(node);
    } else {
      element.after(node);
    }

    return node;
  }

  function retryEntry(element) {
    const text = (element.innerText || "").replace(/\s+/g, " ").trim();
    if (!text) return;

    const node = nodeFor(element);
    if (node) {
      node.classList.remove("ot-reading-error");
      node.classList.add("ot-reading-pending");
      node.querySelector(".ot-reading-text").textContent = "翻译中…";
    }

    cache.delete(text);
    queuedElements.add(element);
    enqueue([
      {
        element,
        text,
        viewport: inViewport(element),
        queuedAt: performance.now(),
      },
    ]);
  }

  /// Renders the final translation into the block's node.
  function applyTranslation(element, text, translation) {
    const node = skeletonFor(element);
    node.dataset.otReadingSource = text;
    node.classList.remove("ot-reading-pending", "ot-reading-error");
    node.querySelector(".ot-reading-text").textContent = translation;
  }

  function applyError(element, message) {
    const node = skeletonFor(element);
    node.classList.remove("ot-reading-pending");
    node.classList.add("ot-reading-error");
    node.querySelector(".ot-reading-text").textContent =
      "翻译失败" + (message ? "：" + message : "") + "（点击重译）";
  }

  function ensurePort() {
    if (port) return port;

    try {
      port = api.runtime.connect({ name: "translate" });
    } catch (error) {
      port = null;
      return null;
    }

    port.onMessage.addListener(onPortMessage);
    port.onDisconnect.addListener(() => {
      port = null;

      if (stream) {
        const pending = stream;
        stream = null;
        pending.resolve({ ok: false, error: null });
      }
    });

    return port;
  }

  function onPortMessage(message) {
    if (!message || !stream || message.requestId !== stream.requestId) return;

    if (message.type === "delta") {
      stream.buffer += message.delta || "";
      const node = skeletonFor(stream.entry.element);
      node.querySelector(".ot-reading-text").textContent = stream.buffer;
    } else if (message.type === "done") {
      const pending = stream;
      stream = null;
      pending.resolve({
        ok: true,
        translation: message.translation || pending.buffer,
      });
    } else if (message.type === "error") {
      const pending = stream;
      stream = null;
      pending.resolve({ ok: false, error: message.message });
    }
  }

  /// Resolves `{ok, translation?, error?}`; streams deltas into the block when
  /// the port is available, otherwise falls back to the one-shot message.
  function translateEntry(entry) {
    if (!ensurePort()) {
      return api.runtime
        .sendMessage({ type: "translate", text: entry.text, record: false })
        .catch(() => null)
        .then((result) => result || { ok: false, error: null });
    }

    return new Promise((resolve) => {
      const requestId = ++portSeq;
      stream = { requestId, entry, buffer: "", resolve };

      try {
        port.postMessage({
          type: "translate",
          requestId,
          text: entry.text,
          record: false,
        });
      } catch (error) {
        stream = null;
        resolve({ ok: false, error: null });
      }
    });
  }

  /// Drops metadata the engine may copy from its own instructions.
  function cleanTranslation(raw) {
    const text = String(raw || "").trim();
    if (!text) return text;

    const lines = text.split("\n");
    const cleaned = lines.filter((line, index) => {
      const edge = index === 0 || index === lines.length - 1;
      return !(edge && /^(术语表|必须使用以下术语|Terminology \()/.test(line.trim()));
    });

    const result = cleaned.join("\n").trim();
    return result || text;
  }

  function noteSkip(reason) {
    stats["skip_" + reason] = (stats["skip_" + reason] || 0) + 1;
  }

  /// Collects one block; null when skipped or already handled. Idempotent:
  /// a block whose injected node no longer matches its text re-queues.
  function consider(element) {
    if (queuedElements.has(element)) return null;
    if (element.closest(SKIP_CLOSEST)) {
      noteSkip("ancestor");
      return null;
    }
    if (element.offsetParent === null) {
      noteSkip("hidden");
      return null;
    }

    const text = (element.innerText || "").replace(/\s+/g, " ").trim();
    if (!text) {
      noteSkip("empty");
      return null;
    }
    if (text.length < minCharsFor(element)) {
      noteSkip("short");
      return null;
    }
    if (text.length > MAX_BLOCK_CHARS) {
      noteSkip("too-long");
      return null;
    }
    if (CJK_TARGET.test(target) ? cjkRatio(text) > 0.5 : targetLanguageBlock(element)) {
      noteSkip("target-language");
      return null;
    }
    if (element.matches(CONTAINER) && linkDensity(element, text) > LINK_DENSITY) {
      noteSkip("links");
      return null;
    }

    const existing = nodeFor(element);
    if (existing && existing.dataset.otReadingSource === text) return null;

    element.dataset.otReadingBlock = "1";
    queuedElements.add(element);
    return {
      element,
      text,
      viewport: inViewport(element),
      queuedAt: performance.now(),
    };
  }

  /// Share of the block's text that lives in links; containers that are mostly
  /// link clusters (menus, card grids) are UI, not prose.
  function linkDensity(element, text) {
    if (!text) return 0;

    let linkChars = 0;
    for (const anchor of element.querySelectorAll("a")) {
      linkChars += (anchor.innerText || "").length;
    }

    return linkChars / text.length;
  }

  /// Headings and navigation labels are meaningful even when short.
  function minCharsFor(element) {
    if (element.matches("h1, h2, h3, h4, h5, h6, th, summary, dt, figcaption")) {
      return MIN_HEADING_CHARS;
    }
    if (element.closest("nav, header")) return MIN_NAV_CHARS;
    return MIN_CHARS;
  }

  function enqueue(entries) {
    for (const entry of entries) {
      queue.push(entry);
      stats.queued += 1;
    }
    if (entries.length > 0 && !inFlight) worker();
  }

  /// Viewport blocks go first; the rest are picked up by the intersection
  /// observer when they approach the viewport, so a long article never
  /// queues everything at once (the engine is serialized).
  function schedule(entry) {
    if (entry.viewport) {
      enqueue([entry]);
      return;
    }

    offscreen.set(entry.element, entry);
    intersection.observe(entry.element);
  }

  /// Post-order walk: innermost blocks are claimed first, so a container is
  /// translated as a whole only when no descendant block was claimed. This
  /// covers tag-soup pages without semantic paragraphs (div-based text) while
  /// never injecting twice into the same text.
  function walkCandidates(element) {
    let claimed = false;
    for (const child of element.children) {
      if (walkCandidates(child)) claimed = true;
    }

    const isPrimary = element.matches(PRIMARY);
    const isContainer = !isPrimary && element.matches(CONTAINER);
    if (!isPrimary && !isContainer) return claimed;
    if (claimed) return true;

    const text = (element.innerText || "").replace(/\s+/g, " ").trim();
    const existing = nodeFor(element);
    if (existing && existing.dataset.otReadingSource === text) return true;

    const entry = consider(element);
    if (entry) {
      schedule(entry);
      return true;
    }

    return false;
  }

  function scan(root) {
    if (root instanceof HTMLElement && root !== document.body) {
      walkCandidates(root);
      return;
    }
    if (document.body) walkCandidates(document.body);
  }

  // CSS-only visibility changes (tabs, accordions, lazy skins) leave the DOM
  // untouched, so a debounced rescan of the nearest block ancestor picks the
  // newly visible text up.
  let rescanRoots = new Set();
  let rescanTimer = null;

  function scheduleRescan(root) {
    rescanRoots.add(root);

    if (rescanTimer) return;
    rescanTimer = setTimeout(() => {
      rescanTimer = null;
      const roots = [...rescanRoots];
      rescanRoots = new Set();

      if (!started) return;
      for (const item of roots) {
        if (item.isConnected) walkCandidates(item);
      }
    }, 300);
  }

  function pruneOrphans() {
    for (const node of document.querySelectorAll("[data-ot-reading-translation]")) {
      const parent = node.parentElement;
      const inside = parent && parent.dataset.otReadingBlock === "1";
      const after =
        node.previousElementSibling &&
        node.previousElementSibling.dataset.otReadingBlock === "1";

      if (!inside && !after) {
        node.remove();
        stats.pruned += 1;
      }
    }
  }

  async function worker() {
    if (inFlight) return;
    inFlight = true;

    try {
      while (started && queue.length > 0) {
        const entry = queue.shift();

        if (!entry.element.isConnected) {
          queuedElements.delete(entry.element);
          continue;
        }

        const cached = cache.get(entry.text);
        if (cached !== undefined) {
          stats.cacheHits += 1;
          applyTranslation(entry.element, entry.text, cached);
          queuedElements.delete(entry.element);
          continue;
        }

        skeletonFor(entry.element);

        const result = await translateEntry(entry);

        if (!started) return;

        if (!result || !result.ok || !result.translation) {
          stats.failed += 1;
          applyError(entry.element, result && result.error);
          queuedElements.delete(entry.element);
          continue;
        }

        const translation = cleanTranslation(result.translation);
        cache.set(entry.text, translation);
        stats.done += 1;
        applyTranslation(entry.element, entry.text, translation);
        queuedElements.delete(entry.element);
      }
    } finally {
      inFlight = false;
      if (started && queue.length > 0) worker();
    }
  }

  function observe() {
    observer = new MutationObserver((records) => {
      stats.batches += 1;

      const additions = [];
      for (const record of records) {
        if (record.type === "attributes") {
          const target = record.target;
          if (!(target instanceof HTMLElement)) continue;
          if (target.closest(SKIP_CLOSEST)) continue;

          const selector = PRIMARY + ", " + CONTAINER;
          const root = target.matches(selector)
            ? target
            : target.closest(selector);
          if (root) scheduleRescan(root);
          continue;
        }

        for (const node of record.addedNodes) {
          if (!(node instanceof HTMLElement)) continue;
          if (
            node.dataset.otReadingTranslation === "1" ||
            node.closest("[data-ot-reading-block]")
          ) {
            continue;
          }
          additions.push(node);
        }
      }

      let budget = MAX_NEW_PER_BATCH;
      for (const element of additions) {
        if (budget <= 0) break;
        budget -= 1;
        walkCandidates(element);
      }

      if (records.length > 0) pruneOrphans();
    });

    // Observe the document element: SPA frameworks (GitHub Turbo included)
    // replace `document.body`, which would orphan an observer attached there.
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["class", "style", "hidden"],
    });

    intersection = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          intersection.unobserve(entry.target);
          const item = offscreen.get(entry.target);
          if (item) {
            offscreen.delete(entry.target);
            enqueue([item]);
          }
        }
      },
      { rootMargin: "300px 0px" }
    );
  }

  function injectStyle() {
    if (document.getElementById(STYLE_ID)) return;

    const style = document.createElement("style");
    style.id = STYLE_ID;
    style.textContent = STYLE;
    (document.head || document.documentElement).appendChild(style);
  }

  function start() {
    if (started || !shouldRun()) return;

    started = true;
    injectStyle();
    observe();
    scan(document.body);
  }

  function stop() {
    if (!started) return;

    started = false;

    if (observer) {
      observer.disconnect();
      observer = null;
    }
    if (intersection) {
      intersection.disconnect();
      intersection = null;
    }

    queue.length = 0;
    offscreen.clear();
    queuedElements = new WeakSet();
    rescanRoots = new Set();

    if (rescanTimer) {
      clearTimeout(rescanTimer);
      rescanTimer = null;
    }

    if (stream) {
      const pending = stream;
      stream = null;
      pending.resolve({ ok: false, error: null });
    }
    if (port) {
      try {
        port.postMessage({ type: "cancel" });
      } catch (error) {
        port = null;
      }
    }

    for (const element of document.querySelectorAll("[data-ot-reading-block]")) {
      delete element.dataset.otReadingBlock;
    }
    for (const node of document.querySelectorAll("[data-ot-reading-translation]")) {
      node.remove();
    }

    const style = document.getElementById(STYLE_ID);
    if (style) style.remove();

    cache.clear();
  }

  async function loadSettings() {
    const settings = await api.storage.local.get(DEFAULTS);
    enabled = Boolean(settings.readingEnabled);
    target = settings.target || DEFAULTS.target;
    disabledSites = Array.isArray(settings.disabledSites) ? settings.disabledSites : [];
  }

  api.storage.onChanged.addListener((changes, area) => {
    if (area !== "local") return;

    let restart = false;

    if (changes.readingEnabled) {
      enabled = Boolean(changes.readingEnabled.newValue);
      restart = true;
    }
    if (changes.target && changes.target.newValue) {
      if (started && changes.target.newValue !== target) restart = true;
      target = changes.target.newValue;
    }
    if (changes.disabledSites && Array.isArray(changes.disabledSites.newValue)) {
      disabledSites = changes.disabledSites.newValue;
      restart = true;
    }

    if (!restart) return;

    stop();
    start();
  });

  loadSettings().then(() => start());
})();
