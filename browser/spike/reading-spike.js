// S1/S2 spike prototype: paragraph-level bilingual reading translation.
//
// Loaded directly by the spike's test pages (not shipped in the extension); the
// page server proxies POST /translate to the local desktop client, so the
// scheduler/layout/lifecycle questions are measured without touching the
// extension build.
//
// URL parameters:
//   batch=block|join   per-block requests or consecutive blocks joined up to maxChars
//   maxChars=1200      join budget for batch=join
//   minChars=12        blocks shorter than this are skipped
//   delayMs=0          wait before collecting (lets the harness screenshot "before")
//
// Dynamic pages (S2): a MutationObserver collects blocks added later, a
// per-node marker plus an in-memory cache prevents duplicate work, injections
// are idempotent (a stale sibling is replaced when the source text changed)
// and orphaned injections are pruned after every mutation batch.
//
// Results are exposed on window.__OT_SPIKE__ (done flag and pending()
// included) for the CDP driver.

(() => {
  const params = new URLSearchParams(location.search);
  const BATCH = params.get("batch") === "join" ? "join" : "block";
  const MAX_CHARS = Number(params.get("maxChars") || 1200);
  const MIN_CHARS = Number(params.get("minChars") || 12);
  const DELAY_MS = Number(params.get("delayMs") || 0);

  const SELECTOR =
    "h1, h2, h3, h4, p, li, blockquote, td, figcaption";
  const SKIP_CLOSEST =
    "[data-ot-spike-block], nav, footer, header, pre, code, script, style, noscript, [translate=no]";
  const GLOSSARY = [
    { source: "context window", target: "上下文窗口" },
    { source: "quantization", target: "量化" },
  ];

  const metrics = (window.__OT_SPIKE__ = {
    batch: BATCH,
    maxChars: MAX_CHARS,
    minChars: MIN_CHARS,
    startedAt: performance.now(),
    candidates: 0,
    skipped: {},
    queued: 0,
    done: 0,
    failed: 0,
    cacheHits: 0,
    firstBlockMs: null,
    firstViewportMs: null,
    fullPageMs: null,
    done_flag: false,
    requests: [],
    blocks: [],
    errors: [],
    layout: {},
    observer: { batches: 0, added: 0, skippedBudget: 0, pruned: 0 },
  });

  const cache = new Map();
  const queue = [];
  const queuedElements = new WeakSet();
  let viewportRemaining = 0;
  let modelInFlight = false;

  metrics.pending = () => queue.length + (modelInFlight ? 1 : 0);

  function noteSkip(reason) {
    metrics.skipped[reason] = (metrics.skipped[reason] || 0) + 1;
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

  function injectedNode(element) {
    if (element.tagName === "LI" || element.tagName === "TD") {
      return (
        [...element.children].find(
          (child) => child.dataset.otSpikeTranslation === "1"
        ) || null
      );
    }
    const next = element.nextElementSibling;
    return next && next.dataset.otSpikeTranslation === "1" ? next : null;
  }

  function inject(element, text, translation) {
    const existing = injectedNode(element);
    const node = existing || document.createElement("div");

    node.className = "ot-spike-translation";
    node.dataset.otSpikeTranslation = "1";
    node.dataset.otSpikeSource = text;
    node.lang = "zh-Hans";
    node.textContent = translation;

    if (existing) return node;

    if (element.tagName === "LI" || element.tagName === "TD") {
      element.appendChild(node);
    } else {
      element.after(node);
    }
    return node;
  }

  /// Collects one block, or null when it is skipped/marked. Idempotent: a
  /// marked block whose injected sibling no longer matches its text is
  /// re-queued (the sibling stays until the new translation lands).
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
    if (text.length < MIN_CHARS) {
      noteSkip("short");
      return null;
    }
    if (cjkRatio(text) > 0.5) {
      noteSkip("target-language");
      return null;
    }

    const existing = injectedNode(element);
    if (existing && existing.dataset.otSpikeSource === text) return null;

    element.dataset.otSpikeBlock = "1";
    const entry = {
      element,
      text,
      chars: text.length,
      viewport: inViewport(element),
      queuedAt: performance.now(),
    };
    queuedElements.add(element);
    return entry;
  }

  function enqueue(entries) {
    for (const entry of entries) {
      queue.push(entry);
      metrics.queued += 1;
    }
    if (entries.length > 0 && !modelInFlight) worker();
  }

  function collect() {
    metrics.layout.before = snapshotLayout();

    const viewportOrder = [];
    const restOrder = [];

    for (const element of document.querySelectorAll(SELECTOR)) {
      const entry = consider(element);
      if (!entry) continue;
      (entry.viewport ? viewportOrder : restOrder).push(entry);
    }

    enqueue([...viewportOrder, ...restOrder]);
    viewportRemaining = viewportOrder.length;
    return viewportOrder.length + restOrder.length;
  }

  /// Drops injections whose source block is gone (React-style re-renders).
  function pruneOrphans() {
    for (const note of document.querySelectorAll("[data-ot-spike-translation]")) {
      const parent = note.parentElement;
      const inside = parent && parent.dataset.otSpikeBlock === "1";
      const after =
        note.previousElementSibling &&
        note.previousElementSibling.dataset.otSpikeBlock === "1";

      if (!inside && !after) {
        note.remove();
        metrics.observer.pruned += 1;
      }
    }
  }

  const MAX_NEW_PER_BATCH = 20;

  function observe() {
    const observer = new MutationObserver((records) => {
      metrics.observer.batches += 1;
      const additions = [];

      for (const record of records) {
        for (const node of record.addedNodes) {
          if (!(node instanceof HTMLElement)) continue;
          if (
            node.dataset.otSpikeTranslation === "1" ||
            node.closest("[data-ot-spike-block]")
          ) {
            continue;
          }
          if (node.matches(SELECTOR)) additions.push(node);
          additions.push(...node.querySelectorAll(SELECTOR));
        }
      }

      const entries = [];
      for (const element of additions) {
        if (entries.length >= MAX_NEW_PER_BATCH) {
          metrics.observer.skippedBudget += 1;
          continue;
        }
        metrics.observer.added += 1;
        const entry = consider(element);
        if (entry) entries.push(entry);
      }

      if (entries.length > 0) enqueue(entries);
      if (records.length > 0) pruneOrphans();
    });

    observer.observe(document.body, { childList: true, subtree: true });
  }

  function snapshotLayout() {
    return {
      height: document.body.scrollHeight,
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      blocks: document.querySelectorAll(".ot-spike-translation").length,
    };
  }

  async function requestTranslation(text) {
    const startedAt = performance.now();
    const response = await fetch("/translate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text, source: "auto", target: "zh", glossary: GLOSSARY }),
    });
    const elapsed = performance.now() - startedAt;
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    const payload = await response.json();
    if (typeof payload.translation !== "string") {
      throw new Error("malformed response");
    }
    return { translation: payload.translation, elapsed, bytes: text.length };
  }

  function recordBlock(entry, translation, elapsed, extra = {}) {
    const doneAt = performance.now();
    inject(entry.element, entry.text, translation);
    queuedElements.delete(entry.element);

    if (metrics.done === 0) {
      metrics.firstBlockMs = Math.round(doneAt - metrics.startedAt);
    }
    metrics.done += 1;

    if (entry.viewport && metrics.firstViewportMs === null) {
      viewportRemaining -= 1;
      if (viewportRemaining <= 0) {
        metrics.firstViewportMs = Math.round(doneAt - metrics.startedAt);
      }
    }

    const glossaryHits = GLOSSARY.filter(
      (term) =>
        entry.text.toLowerCase().includes(term.source) &&
        translation.includes(term.target)
    ).map((term) => term.source);

    metrics.blocks.push({
      index: metrics.blocks.length,
      chars: entry.chars,
      viewport: entry.viewport,
      queueDelayMs: Math.round(doneAt - entry.queuedAt - elapsed),
      latencyMs: Math.round(elapsed),
      glossaryHits,
      status: extra.status || "ok",
      original: entry.text.slice(0, 160),
      translation: translation.slice(0, 200),
    });
  }

  async function runBlock(entry) {
    const cached = cache.get(entry.text);
    if (cached !== undefined) {
      metrics.cacheHits += 1;
      recordBlock(entry, cached, 0, { status: "cache" });
      return;
    }

    const { translation, elapsed } = await requestTranslation(entry.text);
    cache.set(entry.text, translation);
    metrics.requests.push({ blocks: 1, chars: entry.chars, ms: Math.round(elapsed) });
    recordBlock(entry, translation, elapsed);
  }

  async function runJoin() {
    // Take consecutive queue entries while the joined text stays within budget.
    const group = [];
    let chars = 0;
    while (queue.length > 0) {
      const next = queue[0];
      if (group.length > 0 && chars + next.chars > MAX_CHARS) break;
      queue.shift();
      group.push(next);
      chars += next.chars;
      if (next.chars >= MAX_CHARS) break;
    }

    const joined = group.map((entry) => entry.text).join("\n");
    const startedAt = performance.now();
    let response;
    try {
      response = await requestTranslation(joined);
    } catch (error) {
      metrics.errors.push(String(error));
      for (const entry of group) {
        metrics.failed += 1;
        recordBlock(entry, "（翻译失败）", performance.now() - startedAt, {
          status: "error",
        });
      }
      return;
    }

    const lines = response.translation
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0);

    metrics.requests.push({
      blocks: group.length,
      chars: joined.length,
      ms: Math.round(response.elapsed),
    });

    if (lines.length === group.length) {
      group.forEach((entry, index) => {
        cache.set(entry.text, lines[index]);
        recordBlock(entry, lines[index], response.elapsed, { status: "joined" });
      });
      return;
    }

    // Line count mismatch: redo the group per block (same fallback as the
    // image endpoint uses for its joined request).
    metrics.errors.push(
      `join split mismatch: ${group.length} blocks, ${lines.length} lines`
    );
    for (const entry of group) {
      const single = await requestTranslation(entry.text);
      cache.set(entry.text, single.translation);
      metrics.requests.push({
        blocks: 1,
        chars: entry.chars,
        ms: Math.round(single.elapsed),
      });
      recordBlock(entry, single.translation, single.elapsed, {
        status: "fallback",
      });
    }
  }

  async function worker() {
    if (modelInFlight) return;
    modelInFlight = true;

    try {
      while (queue.length > 0) {
        if (BATCH === "join") {
          await runJoin();
        } else {
          const entry = queue.shift();
          try {
            await runBlock(entry);
          } catch (error) {
            metrics.errors.push(`${String(error)} :: ${entry.text.slice(0, 40)}`);
            metrics.failed += 1;
            recordBlock(entry, "（翻译失败）", 0, { status: "error" });
          }
        }
      }
    } finally {
      modelInFlight = false;
      if (queue.length > 0) {
        worker();
        return;
      }
      metrics.layout.after = snapshotLayout();
      metrics.fullPageMs = Math.round(performance.now() - metrics.startedAt);
      metrics.done_flag = true;
    }
  }

  async function start() {
    if (DELAY_MS > 0) {
      await new Promise((resolve) => setTimeout(resolve, DELAY_MS));
    }
    observe();

    const count = collect();
    if (count === 0) {
      metrics.layout.after = snapshotLayout();
      metrics.fullPageMs = Math.round(performance.now() - metrics.startedAt);
      metrics.done_flag = true;
      return;
    }
    worker();
  }

  start();
})();
