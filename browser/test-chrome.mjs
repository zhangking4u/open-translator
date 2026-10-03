#!/usr/bin/env node
// Parameterized CDP smoke test for the Chrome MV3 extension build.
//
// Usage:
//   node test-chrome.mjs --browser http://127.0.0.1:9222 \
//     --extension browser/dist/chrome \
//     --page http://127.0.0.1:8099/test-page.html

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    args[argv[i].replace(/^--/, "")] = argv[i + 1];
  }
  return args;
}

const args = parseArgs(process.argv.slice(2));
if (!args.browser || !args.extension || !args.page) {
  console.error("usage: test-chrome.mjs --browser URL --extension DIR --page URL");
  process.exit(2);
}

const versionResponse = await fetch(new URL("/json/version", args.browser));
const { webSocketDebuggerUrl } = await versionResponse.json();

const ws = new WebSocket(webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});

let nextId = 0;
const pending = new Map();
const listeners = [];

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id !== undefined && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
    return;
  }
  for (const listener of [...listeners]) listener(message);
};

function send(method, params = {}, sessionId) {
  return new Promise((resolve) => {
    const id = ++nextId;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
}

async function evaluate(sessionId, expression, extra = {}) {
  const result = await send(
    "Runtime.evaluate",
    { expression, awaitPromise: true, returnByValue: true, ...extra },
    sessionId
  );
  if (result.error) throw new Error(JSON.stringify(result.error));
  if (result.result.exceptionDetails) {
    throw new Error(JSON.stringify(result.result.exceptionDetails));
  }
  return result.result.result.value;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const BUBBLE_READER = `(() => {
  const host = [...document.documentElement.children].find((el) => el.shadowRoot);
  const status = host && host.shadowRoot.querySelector(".status");
  return status ? status.textContent : null;
})()`;

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures += 1;
}

try {
  const loaded = await send("Extensions.loadUnpacked", { path: args.extension });
  if (loaded.error) {
    throw new Error(`Extensions.loadUnpacked failed: ${JSON.stringify(loaded.error)}`);
  }
  const extensionId = loaded.result.id;
  check("extension loaded", true, extensionId);

  let workerTarget = null;
  for (let attempt = 0; attempt < 50 && !workerTarget; attempt++) {
    const targets = (await send("Target.getTargets")).result.targetInfos;
    workerTarget = targets.find((target) =>
      target.url.startsWith(`chrome-extension://${extensionId}/`)
    );
    if (!workerTarget) await sleep(200);
  }
  if (!workerTarget) throw new Error("extension service worker target not found");
  check("service worker present", true, workerTarget.url);

  const workerSession = (
    await send("Target.attachToTarget", { targetId: workerTarget.targetId, flatten: true })
  ).result.sessionId;
  await send("Runtime.enable", {}, workerSession);

  const translate = JSON.parse(
    await evaluate(workerSession, `(async () => JSON.stringify(await translate("kernel panic")))()`)
  );
  check(
    "worker translate",
    translate.ok === true &&
      typeof translate.translation === "string" &&
      translate.translation.length > 0,
    JSON.stringify(translate)
  );

  const settings = JSON.parse(
    await evaluate(workerSession, `(async () => JSON.stringify(await getSettings()))()`)
  );
  check("settings loaded", typeof settings.serviceUrl === "string", settings.serviceUrl);

  const contextEvents = [];
  listeners.push((message) => {
    if (message.method === "Runtime.executionContextCreated") contextEvents.push(message);
  });

  const created = await send("Target.createTarget", { url: args.page });
  const pageTargetId = created.result.targetId;
  const pageSession = (
    await send("Target.attachToTarget", { targetId: pageTargetId, flatten: true })
  ).result.sessionId;

  await send("Runtime.enable", {}, pageSession);
  await sleep(1500);

  const contexts = contextEvents
    .filter((event) => event.sessionId === pageSession)
    .map((event) => event.params.context);
  const isolated = contexts.find(
    (context) =>
      context.auxData &&
      context.auxData.isDefault === false &&
      context.origin?.startsWith("chrome-extension://")
  );
  check("content script isolated world", Boolean(isolated), isolated?.name ?? "");

  if (isolated) {
    const hasStart = await evaluate(pageSession, "typeof start", { contextId: isolated.id });
    check("content script loaded", hasStart === "function");

    await evaluate(
      pageSession,
      `(() => {
         const range = document.createRange();
         range.selectNodeContents(document.getElementById("t"));
         const selection = getSelection();
         selection.removeAllRanges();
         selection.addRange(range);
         return true;
       })()`
    );

    await evaluate(pageSession, "start()", { contextId: isolated.id });

    let bubble = null;
    for (let attempt = 0; attempt < 150; attempt++) {
      bubble = await evaluate(pageSession, BUBBLE_READER, { contextId: isolated.id });
      if (bubble && bubble !== "翻译中…" && bubble !== "正在准备翻译服务…") break;
      await sleep(200);
    }
    check(
      "bubble shows translation",
      Boolean(bubble) && bubble.length > 0 && !bubble.includes("翻译失败"),
      bubble ?? "<empty>"
    );

    const iconState = await evaluate(
      pageSession,
      `(() => {
         const host = [...document.documentElement.children].find((el) => el.shadowRoot);
         const root = host && host.shadowRoot;
         if (!root) return null;
         const copy = root.querySelector(".copy-button");
         const more = root.querySelector(".more-button");
         const close = root.querySelector(".close-button");
         return {
           copyIcon: Boolean(copy && copy.querySelector("svg")),
           moreIcon: Boolean(more && more.querySelector("svg")),
           closeIcon: Boolean(close && close.querySelector("svg")),
           copyText: copy ? copy.textContent.trim() : null,
         };
       })()`,
      { contextId: isolated.id }
    );
    check(
      "iconified controls",
      Boolean(
        iconState &&
          iconState.copyIcon &&
          iconState.moreIcon &&
          iconState.closeIcon &&
          iconState.copyText === ""
      ),
      JSON.stringify(iconState)
    );

    let statusClass = null;
    for (let attempt = 0; attempt < 150; attempt++) {
      statusClass = await evaluate(
        pageSession,
        `(() => {
           const host = [...document.documentElement.children].find((el) => el.shadowRoot);
           const status = host && host.shadowRoot.querySelector(".status");
           return status ? status.className : null;
         })()`,
        { contextId: isolated.id }
      );
      if (statusClass === "status") break;
      await sleep(200);
    }
    check(
      "streaming indicator cleared after done",
      statusClass === "status",
      statusClass ?? "<none>"
    );

    const selected = await evaluate(
      pageSession,
      `(() => {
         const host = [...document.documentElement.children].find((el) => el.shadowRoot);
         const select = host && host.shadowRoot.querySelector(".target-select");
         if (!select) return null;
         select.value = "ja";
         select.dispatchEvent(new Event("change", { bubbles: true }));
         return select.value;
       })()`,
      { contextId: isolated.id }
    );
    check("bubble language switch", selected === "ja", selected ?? "<no select>");

    const stored = await evaluate(
      pageSession,
      `(async () => {
         const api = globalThis.browser ?? globalThis.chrome;
         return (await api.storage.local.get({ target: "zh" })).target;
       })()`,
      { contextId: isolated.id }
    );
    check("bubble target persisted", stored === "ja", String(stored));

    let switched = bubble;
    for (let attempt = 0; attempt < 150; attempt++) {
      const current = await evaluate(pageSession, BUBBLE_READER, { contextId: isolated.id });
      if (current && current !== "翻译中…" && current !== "正在准备翻译服务…") {
        switched = current;
        break;
      }
      await sleep(200);
    }
    check(
      "bubble translation after switch",
      Boolean(switched) && switched.length > 0 && !switched.includes("翻译失败"),
      switched ?? "<empty>"
    );
    console.log(
      `bubble translation ${switched === bubble ? "unchanged (mock engine?)" : "updated after switch"}`
    );

    const historyCount = await evaluate(
      pageSession,
      `(async () => {
         const api = globalThis.browser ?? globalThis.chrome;
         const stored = await api.storage.local.get({ history: [] });
         return Array.isArray(stored.history) ? stored.history.length : 0;
       })()`,
      { contextId: isolated.id }
    );
    check("history recorded", historyCount > 0, String(historyCount));

    await evaluate(
      pageSession,
      `(() => {
         const textarea = document.getElementById("ta");
         textarea.focus();
         textarea.setSelectionRange(0, textarea.value.length);
         return true;
       })()`
    );
    await evaluate(pageSession, "start()", { contextId: isolated.id });

    let moreReady = false;
    for (let attempt = 0; attempt < 150; attempt++) {
      moreReady = await evaluate(
        pageSession,
        `(() => {
           const host = [...document.documentElement.children].find((el) => el.shadowRoot);
           const button = host && host.shadowRoot.querySelector(".more-button");
           return Boolean(button && !button.hidden && !button.disabled);
         })()`,
        { contextId: isolated.id }
      );
      if (moreReady) break;
      await sleep(200);
    }
    check("more menu offered after translation", moreReady);

    if (moreReady) {
      await evaluate(
        pageSession,
        `(() => {
           const host = [...document.documentElement.children].find((el) => el.shadowRoot);
           host.shadowRoot.querySelector(".more-button").click();
           return true;
         })()`,
        { contextId: isolated.id }
      );

      const replaceState = await evaluate(
        pageSession,
        `(() => {
           const host = [...document.documentElement.children].find((el) => el.shadowRoot);
           const item = host && host.shadowRoot.querySelector('[data-action="replace"]');
           return {
             ready: Boolean(item && !item.hidden && !item.disabled),
             icon: Boolean(item && item.querySelector("svg")),
             label: item && item.querySelector(".menu-label") ? item.querySelector(".menu-label").textContent : null,
           };
         })()`,
        { contextId: isolated.id }
      );
      check(
        "replace action offered for input selection",
        Boolean(replaceState && replaceState.ready),
        JSON.stringify(replaceState)
      );
      check(
        "menu item icon and label",
        Boolean(replaceState && replaceState.icon && replaceState.label === "替换原文"),
        JSON.stringify(replaceState)
      );

      if (replaceState && replaceState.ready) {
        await evaluate(
          pageSession,
          `(() => {
             const host = [...document.documentElement.children].find((el) => el.shadowRoot);
             host.shadowRoot.querySelector('[data-action="replace"]').click();
             return true;
           })()`,
          { contextId: isolated.id }
        );
        await sleep(300);
        const replacedText = await evaluate(pageSession, "document.getElementById('ta').value");
        check(
          "replace original in textarea",
          typeof replacedText === "string" &&
            replacedText.length > 0 &&
            replacedText !== "Hello world",
          replacedText
        );
      }
    }

    // Auto-translate: enable the setting, hide the bubble, select text and
    // dispatch a mouseup; the content script should translate after ~400ms.
    await evaluate(
      pageSession,
      `(async () => {
         const api = globalThis.browser ?? globalThis.chrome;
         await api.storage.local.set({ autoTranslate: true });
         return true;
       })()`,
      { contextId: isolated.id }
    );

    await evaluate(
      pageSession,
      `(() => {
         document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
         return true;
       })()`
    );

    await evaluate(
      pageSession,
      `(() => {
         const range = document.createRange();
         range.selectNodeContents(document.getElementById("t"));
         const selection = getSelection();
         selection.removeAllRanges();
         selection.addRange(range);
         document.getElementById("t").dispatchEvent(
           new MouseEvent("mouseup", { bubbles: true })
         );
         return true;
       })()`
    );

    let auto = null;
    for (let attempt = 0; attempt < 100; attempt++) {
      auto = await evaluate(
        pageSession,
        `(() => {
           const host = [...document.documentElement.children].find((el) => el.shadowRoot);
           const status = host && host.shadowRoot.querySelector(".status");
           return host && status
             ? { text: status.textContent, visible: host.style.display !== "none" }
             : null;
         })()`,
        { contextId: isolated.id }
      );
      if (auto && auto.visible && auto.text && auto.text !== "翻译中…") break;
      await sleep(200);
    }
    check(
      "auto translate on selection",
      Boolean(auto) && auto.visible && auto.text.length > 0 && !auto.text.includes("翻译失败"),
      auto ? auto.text : "<no bubble>"
    );
  }

  await send("Target.closeTarget", { targetId: pageTargetId });
} catch (error) {
  console.error("ERROR:", error.message ?? error);
  failures += 1;
}

ws.close();
process.exit(failures === 0 ? 0 : 1);
