#!/usr/bin/env node
// Parameterized CDP smoke test for the Chrome MV3 extension build.
//
// Usage:
//   node test-chrome.mjs --browser http://127.0.0.1:9222 \
//     --extension browser/dist/chrome \
//     --page http://127.0.0.1:8099/test-page.html

import http from "node:http";

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
  const host = [...document.documentElement.children].find(
    (el) => el.shadowRoot && el.shadowRoot.querySelector(".status")
  );
  const status = host && host.shadowRoot.querySelector(".status");
  return status ? status.textContent : null;
})()`;

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures += 1;
}

// Mock /translate/image: one block, viewport-sized image, so the overlay
// mapping can be asserted. captureVisibleTab needs a user gesture (activeTab),
// which CDP cannot grant, so the test drives translateImage +
// deliverImageResult — everything after the capture.
const imageMock = http.createServer((request, response) => {
  if (request.method === "POST" && request.url === "/translate/image") {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(
        JSON.stringify({
          image: { width: 800, height: 600 },
          blocks: [
            {
              text: "START GAME",
              translation: "开始游戏",
              score: 0.98,
              quad: [
                [100, 80],
                [300, 80],
                [300, 120],
                [100, 120],
              ],
            },
          ],
        })
      );
    });
    return;
  }

  response.writeHead(404);
  response.end();
});

await new Promise((resolve) => imageMock.listen(0, "127.0.0.1", resolve));
const imageMockUrl = `http://127.0.0.1:${imageMock.address().port}`;

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
    workerTarget = targets.find(
      (target) =>
        target.url.startsWith(`chrome-extension://${extensionId}/`) &&
        target.url.endsWith("/background.js")
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

  const created = await send("Target.createTarget", {
    url: (() => {
      const url = new URL(args.page);
      url.searchParams.set("ot-e2e", Date.now().toString());
      return url.toString();
    })(),
  });
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

    // Trusted mouse clicks (not element.click()) so the real mousedown/click
    // path through the shadow DOM dropdown is exercised.
    const clickAt = async (x, y) => {
      await send(
        "Input.dispatchMouseEvent",
        { type: "mousePressed", x, y, button: "left", clickCount: 1 },
        pageSession
      );
      await send(
        "Input.dispatchMouseEvent",
        { type: "mouseReleased", x, y, button: "left", clickCount: 1 },
        pageSession
      );
    };
    const pointIn = (hostExpression, innerSelector) =>
      evaluate(
        pageSession,
        `(() => {
           const host = ${hostExpression};
           const element = host && host.shadowRoot.querySelector(${JSON.stringify(innerSelector)});
           if (!element) return null;
           const rect = element.getBoundingClientRect();
           return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
         })()`,
        { contextId: isolated.id }
      );
    const selectionHost = `[...document.documentElement.children].find(
      (el) => el.shadowRoot && el.shadowRoot.querySelector(".status"))`;
    const typingHost = `document.querySelector('[data-opentranslator="type"]')`;

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
           closeHidden: close
             ? getComputedStyle(close).opacity ===
               (window.matchMedia("(hover: hover)").matches ? "0" : "1")
             : null,
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
          iconState.copyText === "" &&
          iconState.closeHidden
      ),
      JSON.stringify(iconState)
    );

    const closeBox = await evaluate(
      pageSession,
      `(() => {
         const host = [...document.documentElement.children].find((el) => el.shadowRoot);
         const button = host && host.shadowRoot.querySelector(".close-button");
         if (!button) return null;
         const rect = button.getBoundingClientRect();
         return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
       })()`,
      { contextId: isolated.id }
    );
    if (closeBox) {
      await send(
        "Input.dispatchMouseEvent",
        { type: "mouseMoved", x: closeBox.x, y: closeBox.y, button: "none" },
        pageSession
      );
      await sleep(300);
      const closeOpacity = await evaluate(
        pageSession,
        `(() => {
           const host = [...document.documentElement.children].find((el) => el.shadowRoot);
           const button = host && host.shadowRoot.querySelector(".close-button");
           return button ? getComputedStyle(button).opacity : null;
         })()`,
        { contextId: isolated.id }
      );
      check("close button revealed on hover", closeOpacity === "1", closeOpacity ?? "<none>");
    }

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

    const chipPoint = await pointIn(selectionHost, ".ot-select-button");
    if (chipPoint) await clickAt(chipPoint.x, chipPoint.y);
    await sleep(200);

    const jaPoint = await pointIn(selectionHost, '.ot-select-item[data-value="ja"]');
    if (jaPoint) await clickAt(jaPoint.x, jaPoint.y);

    const selected = await evaluate(
      pageSession,
      `(() => {
         const host = ${selectionHost};
         const select = host && host.shadowRoot.querySelector(".target-select");
         return select ? select.value : null;
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

    // 边写边译: enable it, type into the textarea, wait for the inline bubble
    // and commit with Tab.
    await evaluate(
      pageSession,
      `(async () => {
         const api = globalThis.browser ?? globalThis.chrome;
         await api.storage.local.set({ typeTranslate: true });
         return true;
       })()`,
      { contextId: isolated.id }
    );
    await sleep(200);

    await evaluate(
      pageSession,
      `(() => {
         const textarea = document.getElementById("ta");
         textarea.focus();
         textarea.value = "Hello world";
         textarea.setSelectionRange(textarea.value.length, textarea.value.length);
         textarea.dispatchEvent(new Event("input", { bubbles: true }));
         return true;
       })()`
    );

    let typeBubble = null;
    for (let attempt = 0; attempt < 100; attempt++) {
      typeBubble = await evaluate(
        pageSession,
        `(() => {
           const host = document.querySelector('[data-opentranslator="type"]');
           const status = host && host.shadowRoot.querySelector(".type-status");
           if (!host || !status || host.style.display === "none") return null;
           const hint = host.shadowRoot.querySelector(".type-hint");
           return {
             text: status.textContent,
             hint: hint ? hint.textContent : "",
             streaming: status.className.includes("streaming"),
           };
         })()`,
        { contextId: isolated.id }
      );
      if (
        typeBubble &&
        typeBubble.text &&
        typeBubble.text !== "翻译中…" &&
        !typeBubble.streaming
      ) {
        break;
      }
      await sleep(200);
    }
    check(
      "type bubble shows translation",
      Boolean(typeBubble) &&
        typeBubble.text.length > 0 &&
        !typeBubble.text.includes("翻译失败"),
      typeBubble ? typeBubble.text : "<none>"
    );
    check(
      "type bubble shows Tab hint",
      Boolean(typeBubble) && typeBubble.hint.includes("Tab"),
      typeBubble ? typeBubble.hint : "<none>"
    );

    const typeControls = await evaluate(
      pageSession,
      `(() => {
         const host = document.querySelector('[data-opentranslator="type"]');
         const root = host && host.shadowRoot;
         if (!root) return null;
         const target = root.querySelector(".ot-select-button");
         const settings = root.querySelector(".type-settings");
         const menu = root.querySelector(".ot-select-menu");
         return {
           target: target ? target.textContent.trim() : null,
           settings: Boolean(settings && settings.querySelector("svg")),
           menuHidden: menu ? menu.hidden : null,
         };
       })()`,
      { contextId: isolated.id }
    );
    check(
      "type bubble language chip",
      Boolean(typeControls && typeControls.target && typeControls.menuHidden === true),
      JSON.stringify(typeControls)
    );
    check(
      "type bubble settings button",
      Boolean(typeControls && typeControls.settings),
      JSON.stringify(typeControls)
    );

    const menuState = await evaluate(
      pageSession,
      `(() => {
         const host = document.querySelector('[data-opentranslator="type"]');
         host.shadowRoot.querySelector(".ot-select-button").click();
         const root = host.shadowRoot;
         const menu = root.querySelector(".ot-select-menu");
         const status = root.querySelector(".type-status");
         if (!menu || menu.hidden) return null;
         return {
           menuTop: menu.getBoundingClientRect().top,
           statusBottom: status.getBoundingClientRect().bottom,
         };
       })()`,
      { contextId: isolated.id }
    );
    check(
      "type menu opens below the translation",
      Boolean(menuState && menuState.menuTop >= menuState.statusBottom),
      JSON.stringify(menuState)
    );

    const frPoint = await pointIn(typingHost, '.ot-select-menu [data-value="fr"]');
    if (frPoint) await clickAt(frPoint.x, frPoint.y);

    let switchedTarget = null;
    for (let attempt = 0; attempt < 50; attempt++) {
      switchedTarget = await evaluate(
        pageSession,
        `(async () => {
           const api = globalThis.browser ?? globalThis.chrome;
           return (await api.storage.local.get({ target: "zh" })).target;
         })()`,
        { contextId: isolated.id }
      );
      if (switchedTarget === "fr") break;
      await sleep(100);
    }
    check(
      "type bubble target switch persisted",
      switchedTarget === "fr",
      String(switchedTarget)
    );

    let switchedBubble = null;
    for (let attempt = 0; attempt < 100; attempt++) {
      switchedBubble = await evaluate(
        pageSession,
        `(() => {
           const host = document.querySelector('[data-opentranslator="type"]');
           const status = host && host.shadowRoot.querySelector(".type-status");
           if (!host || !status || host.style.display === "none") return null;
           const target = host.shadowRoot.querySelector(".ot-select-button");
           return {
             text: status.textContent,
             label: target ? target.textContent.trim() : "",
             streaming: status.className.includes("streaming"),
           };
         })()`,
        { contextId: isolated.id }
      );
      if (
        switchedBubble &&
        switchedBubble.text &&
        switchedBubble.text !== "翻译中…" &&
        !switchedBubble.streaming
      ) {
        break;
      }
      await sleep(200);
    }
    check(
      "type bubble retranslates after switch",
      Boolean(switchedBubble) &&
        switchedBubble.text.length > 0 &&
        !switchedBubble.text.includes("翻译失败"),
      switchedBubble ? switchedBubble.text : "<none>"
    );
    check(
      "type bubble chip shows new language",
      Boolean(switchedBubble && switchedBubble.label.includes("法语")),
      switchedBubble ? switchedBubble.label : "<none>"
    );

    const committedValue = await evaluate(
      pageSession,
      `(() => {
         const textarea = document.getElementById("ta");
         textarea.dispatchEvent(
           new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true })
         );
         return textarea.value;
       })()`
    );
    check(
      "Tab commits inline translation",
      typeof committedValue === "string" &&
        committedValue.length > 0 &&
        committedValue !== "Hello world",
      committedValue
    );

    const typeHidden = await evaluate(
      pageSession,
      `(() => {
         const host = document.querySelector('[data-opentranslator="type"]');
         return !host || host.style.display === "none";
       })()`,
      { contextId: isolated.id }
    );
    check("type bubble hidden after commit", typeHidden);

    // Same flow inside a contenteditable block (web mail/doc editors).
    await evaluate(
      pageSession,
      `(() => {
         const editor = document.getElementById("ce");
         editor.textContent = "Hello world";
         editor.focus();
         const range = document.createRange();
         range.selectNodeContents(editor);
         range.collapse(false);
         const selection = getSelection();
         selection.removeAllRanges();
         selection.addRange(range);
         editor.dispatchEvent(new Event("input", { bubbles: true }));
         return true;
       })()`
    );

    let richBubble = null;
    for (let attempt = 0; attempt < 100; attempt++) {
      richBubble = await evaluate(
        pageSession,
        `(() => {
           const host = document.querySelector('[data-opentranslator="type"]');
           const status = host && host.shadowRoot.querySelector(".type-status");
           if (!host || !status || host.style.display === "none") return null;
           return {
             text: status.textContent,
             streaming: status.className.includes("streaming"),
           };
         })()`,
        { contextId: isolated.id }
      );
      if (
        richBubble &&
        richBubble.text &&
        richBubble.text !== "翻译中…" &&
        !richBubble.streaming
      ) {
        break;
      }
      await sleep(200);
    }
    check(
      "contenteditable type bubble",
      Boolean(richBubble) &&
        richBubble.text.length > 0 &&
        !richBubble.text.includes("翻译失败"),
      richBubble ? richBubble.text : "<none>"
    );

    const richCommitted = await evaluate(
      pageSession,
      `(() => {
         const editor = document.getElementById("ce");
         editor.dispatchEvent(
           new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true })
         );
         return editor.textContent;
       })()`
    );
    check(
      "Tab commits in contenteditable",
      typeof richCommitted === "string" &&
        richCommitted.length > 0 &&
        richCommitted !== "Hello world",
      richCommitted
    );

    // cycle-target (the Alt+Shift+L path) switches the target through the
    // background's tab message while the bubble is visible.
    await evaluate(
      pageSession,
      `(() => {
         const textarea = document.getElementById("ta");
         textarea.focus();
         textarea.value = "Hello world";
         textarea.setSelectionRange(textarea.value.length, textarea.value.length);
         textarea.dispatchEvent(new Event("input", { bubbles: true }));
         return true;
       })()`
    );

    let cycleBubble = null;
    for (let attempt = 0; attempt < 100; attempt++) {
      cycleBubble = await evaluate(
        pageSession,
        `(() => {
           const host = document.querySelector('[data-opentranslator="type"]');
           const status = host && host.shadowRoot.querySelector(".type-status");
           if (!host || !status || host.style.display === "none") return null;
           return {
             text: status.textContent,
             streaming: status.className.includes("streaming"),
           };
         })()`,
        { contextId: isolated.id }
      );
      if (
        cycleBubble &&
        cycleBubble.text &&
        cycleBubble.text !== "翻译中…" &&
        !cycleBubble.streaming
      ) {
        break;
      }
      await sleep(200);
    }

    const tabId = await evaluate(
      workerSession,
      `(async () => {
         const tabs = await api.tabs.query({});
         const tab = tabs.find((entry) => (entry.url || "").includes("test-page.html"));
         return tab ? tab.id : null;
       })()`
    );

    if (tabId !== null && tabId !== undefined) {
      await evaluate(
        workerSession,
        `(async () => {
           await api.tabs.sendMessage(${tabId}, { type: "cycle-target" });
           return true;
         })()`
      );
    }

    let cycledTarget = null;
    for (let attempt = 0; attempt < 50; attempt++) {
      cycledTarget = await evaluate(
        pageSession,
        `(async () => {
           const api = globalThis.browser ?? globalThis.chrome;
           return (await api.storage.local.get({ target: "zh" })).target;
         })()`,
        { contextId: isolated.id }
      );
      if (cycledTarget === "ja") break;
      await sleep(100);
    }
    check(
      "cycle-target switches language",
      tabId !== null && tabId !== undefined && cycledTarget === "ja",
      String(cycledTarget)
    );

    let cycledBubble = null;
    for (let attempt = 0; attempt < 100; attempt++) {
      cycledBubble = await evaluate(
        pageSession,
        `(() => {
           const host = document.querySelector('[data-opentranslator="type"]');
           const status = host && host.shadowRoot.querySelector(".type-status");
           if (!host || !status || host.style.display === "none") return null;
           return {
             text: status.textContent,
             streaming: status.className.includes("streaming"),
           };
         })()`,
        { contextId: isolated.id }
      );
      if (
        cycledBubble &&
        cycledBubble.text &&
        cycledBubble.text !== "翻译中…" &&
        !cycledBubble.streaming
      ) {
        break;
      }
      await sleep(200);
    }
    check(
      "bubble retranslates after cycle",
      Boolean(cycledBubble) &&
        cycledBubble.text.length > 0 &&
        !cycledBubble.text.includes("翻译失败"),
      cycledBubble ? cycledBubble.text : "<none>"
    );

    await evaluate(
      pageSession,
      `(async () => {
         const api = globalThis.browser ?? globalThis.chrome;
         await api.storage.local.set({ typeTranslate: false });
         return true;
       })()`,
      { contextId: isolated.id }
    );

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
           const host = [...document.documentElement.children].find(
             (el) => el.shadowRoot && el.shadowRoot.querySelector(".status")
           );
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

    // Menu placement near the bottom of the viewport: the dropdown must flip
    // above the button (or constrain its height) so every language stays
    // reachable.
    await evaluate(
      pageSession,
      `(() => {
         if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
         document.getElementById("bottom").scrollIntoView({ block: "end" });
         return true;
       })()`
    );
    await sleep(400);

    await evaluate(
      pageSession,
      `(() => {
         const range = document.createRange();
         range.selectNodeContents(document.getElementById("bottom"));
         const selection = getSelection();
         selection.removeAllRanges();
         selection.addRange(range);
         return true;
       })()`
    );
    await evaluate(pageSession, "start()", { contextId: isolated.id });

    // Wait for the translated text: on a fast (mock) engine the card is still
    // swapping its controls when the chip would be clicked, which moves the
    // button under the captured point.
    let bottomBubble = null;
    for (let attempt = 0; attempt < 150; attempt++) {
      bottomBubble = await evaluate(
        pageSession,
        `(() => {
           const host = ${selectionHost};
           const status = host && host.shadowRoot.querySelector(".status");
           if (!host || !status || host.style.display === "none") return null;
           return {
             text: status.textContent,
             streaming: status.className.includes("streaming"),
           };
         })()`,
        { contextId: isolated.id }
      );
      if (
        bottomBubble &&
        bottomBubble.text &&
        bottomBubble.text !== "翻译中…" &&
        !bottomBubble.streaming
      ) {
        break;
      }
      await sleep(200);
    }
    check(
      "bottom selection translated",
      Boolean(bottomBubble) &&
        bottomBubble.text.length > 0 &&
        !bottomBubble.text.includes("翻译失败"),
      bottomBubble ? bottomBubble.text : "<none>"
    );

    const readMenuPlacement = () =>
      evaluate(
        pageSession,
        `(() => {
           const host = ${selectionHost};
           const menu = host && host.shadowRoot.querySelector(".ot-select-menu");
           if (!menu || menu.hidden) return null;
           const rect = menu.getBoundingClientRect();
           return {
             up: menu.classList.contains("ot-select-menu-up"),
             top: rect.top,
             bottom: rect.bottom,
             innerHeight: window.innerHeight,
           };
         })()`,
        { contextId: isolated.id }
      );

    // The chip itself does not move anymore, but retry the click defensively:
    // a miss leaves the menu closed and the placement checks meaningless.
    let menuPlacement = null;
    for (let attempt = 0; attempt < 10 && !menuPlacement; attempt++) {
      const bottomChip = await pointIn(selectionHost, ".ot-select-button");
      if (bottomChip) await clickAt(bottomChip.x, bottomChip.y);
      await sleep(250);
      menuPlacement = await readMenuPlacement();
    }

    check(
      "menu flips above near the viewport bottom",
      Boolean(menuPlacement && menuPlacement.up),
      JSON.stringify(menuPlacement)
    );
    check(
      "menu stays inside the viewport",
      Boolean(
        menuPlacement &&
          menuPlacement.top >= 0 &&
          menuPlacement.bottom <= menuPlacement.innerHeight + 0.5
      ),
      JSON.stringify(menuPlacement)
    );

    // --- Screenshot translation ---------------------------------------------
    const originalSettings = JSON.parse(
      await evaluate(workerSession, `(async () => JSON.stringify(await getSettings()))()`)
    );

    await evaluate(
      workerSession,
      `(async () => { await api.storage.local.set({ serviceUrl: ${JSON.stringify(
        imageMockUrl
      )}, source: "en", target: "zh" }); return true; })()`
    );

    const delivered = JSON.parse(
      await evaluate(
        workerSession,
        `(async () => {
           const tabs = await api.tabs.query({ active: true, currentWindow: true });
           const result = await translateImage("data:image/png;base64,AAAA");
           await deliverImageResult(tabs[0], result);
           return JSON.stringify({ ok: result.ok, blocks: (result.blocks || []).length });
         })()`
      )
    );
    check(
      "screenshot image translated via the service",
      delivered.ok === true && delivered.blocks === 1,
      JSON.stringify(delivered)
    );

    let shot = null;
    for (let attempt = 0; attempt < 50; attempt++) {
      shot = await evaluate(
        pageSession,
        `(() => {
           const host = document.querySelector('[data-opentranslator="shot"]');
           if (!host || host.style.display === "none") return null;
           const root = host.shadowRoot;
           const boxes = [...root.querySelectorAll(".shot-box")];
           return {
             boxes: boxes.length,
             text: boxes[0] ? boxes[0].textContent : null,
             toolbar: !root.querySelector(".shot-toolbar").hidden,
           };
         })()`,
        { contextId: isolated.id }
      );
      if (shot) break;
      await sleep(100);
    }
    check(
      "screenshot overlay renders the translated block",
      Boolean(shot) && shot.boxes === 1 && shot.text === "开始游戏" && shot.toolbar,
      JSON.stringify(shot)
    );

    await evaluate(
      pageSession,
      `(() => {
         document.querySelector('[data-opentranslator="shot"]').shadowRoot.querySelector(".shot-toggle").click();
         return true;
       })()`,
      { contextId: isolated.id }
    );
    const originalShot = await evaluate(
      pageSession,
      `(() => {
         const box = document.querySelector('[data-opentranslator="shot"]').shadowRoot.querySelector(".shot-box");
         return box ? box.textContent : null;
       })()`,
      { contextId: isolated.id }
    );
    check(
      "screenshot overlay toggles the original",
      originalShot === "START GAME",
      originalShot ?? "<none>"
    );

    await evaluate(
      pageSession,
      `(() => {
         document.querySelector('[data-opentranslator="shot"]').shadowRoot.querySelector(".shot-close").click();
         return true;
       })()`,
      { contextId: isolated.id }
    );
    const shotClosed = await evaluate(
      pageSession,
      `document.querySelector('[data-opentranslator="shot"]').style.display`,
      { contextId: isolated.id }
    );
    check("screenshot overlay closes", shotClosed === "none", shotClosed);

    // Error path: an unreachable service surfaces as a toast instead of a
    // silent failure.
    await evaluate(
      workerSession,
      `(async () => { await api.storage.local.set({ serviceUrl: "http://127.0.0.1:1" }); return true; })()`
    );
    await evaluate(
      workerSession,
      `(async () => {
         const tabs = await api.tabs.query({ active: true, currentWindow: true });
         const result = await translateImage("data:image/png;base64,AAAA");
         await deliverImageResult(tabs[0], result);
         return true;
       })()`
    );

    let toast = null;
    for (let attempt = 0; attempt < 50; attempt++) {
      toast = await evaluate(
        pageSession,
        `(() => {
           const host = document.querySelector('[data-opentranslator="shot"]');
           if (!host || host.style.display === "none") return null;
           const node = host.shadowRoot.querySelector(".shot-toast");
           return node && !node.hidden ? node.textContent : null;
         })()`,
        { contextId: isolated.id }
      );
      if (toast) break;
      await sleep(100);
    }
    check(
      "screenshot errors surface as a toast",
      typeof toast === "string" && toast.includes("无法连接本地翻译服务"),
      toast ?? "<none>"
    );

    await evaluate(
      workerSession,
      `(async () => { await api.storage.local.set({ serviceUrl: ${JSON.stringify(
        originalSettings.serviceUrl
      )} }); return true; })()`
    );
  }

  await send("Target.closeTarget", { targetId: pageTargetId });
} catch (error) {
  console.error("ERROR:", error.message ?? error);
  failures += 1;
}

ws.close();
process.exit(failures === 0 ? 0 : 1);
