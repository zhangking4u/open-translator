"use strict";

const api = globalThis.browser ?? globalThis.chrome;
const action = api.action ?? api.browserAction;

const DEFAULTS = {
  serviceUrl: "http://127.0.0.1:17890",
  source: "auto",
  target: "zh",
  autoTranslate: false,
  disabledSites: [],
};

const HEALTH_ALARM = "open-translator-health";
const HISTORY_LIMIT = 20;
const STREAM_RETRIES = 3;
const RETRY_DELAY_MS = 1500;

function baseUrl(serviceUrl) {
  return (serviceUrl || DEFAULTS.serviceUrl).replace(/\/+$/, "");
}

async function getSettings() {
  const stored = await api.storage.local.get(DEFAULTS);
  return Object.assign({}, DEFAULTS, stored);
}

function connectionError(serviceUrl) {
  return (
    "无法连接本地翻译服务（" +
    baseUrl(serviceUrl) +
    "）。请确认桌面客户端或核心服务已启动。"
  );
}

function friendlyError(error, status) {
  const kind = (error && error.kind) || "";
  const message = (error && error.message) || "";

  if (kind === "timeout") return "翻译超时，请重试。";
  if (kind === "engine_unavailable") return "翻译引擎暂不可用，请稍后重试。";
  if (kind === "ocr_unavailable") {
    return "截图文字识别未启用：请确认桌面客户端已启动且 OCR 模型可用。";
  }
  if (kind === "ocr_failed") {
    return "截图文字识别失败：" + (message || "请重试。");
  }

  if (kind === "invalid_request") {
    const tooLong = /text is too long: (\d+) chars \(max (\d+)\)/.exec(message);
    if (tooLong) {
      return `文本过长（${tooLong[1]} 字符，上限 ${tooLong[2]}）。请缩短后重试。`;
    }
    return message || "请求无效。";
  }

  if (message) return message;
  return "翻译失败（HTTP " + status + "）。";
}

function isAbort(error) {
  return Boolean(error) && error.name === "AbortError";
}

function delay(ms, signal) {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    if (signal) {
      signal.addEventListener(
        "abort",
        () => {
          clearTimeout(timer);
          resolve();
        },
        { once: true }
      );
    }
  });
}

function send(port, message) {
  try {
    port.postMessage(message);
  } catch (error) {
    void error;
  }
}

async function recordHistory(text, translation, settings) {
  if (!text || !translation) return;

  const stored = await api.storage.local.get({ history: [] });
  const history = Array.isArray(stored.history) ? stored.history : [];
  const entry = {
    text,
    translation,
    source: settings.source,
    target: settings.target,
    at: Date.now(),
  };

  const next = [
    entry,
    ...history.filter(
      (item) => !(item.text === text && item.target === entry.target)
    ),
  ].slice(0, HISTORY_LIMIT);

  await api.storage.local.set({ history: next });
}

async function translate(text, record = true) {
  const settings = await getSettings();
  const url = baseUrl(settings.serviceUrl) + "/translate";

  let response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        text,
        source: settings.source,
        target: settings.target,
      }),
    });
  } catch (error) {
    return { ok: false, error: connectionError(settings.serviceUrl) };
  }

  let payload = null;
  try {
    payload = await response.json();
  } catch (error) {
    return { ok: false, error: "本地服务返回了无效响应。" };
  }

  if (!response.ok) {
    return {
      ok: false,
      error: friendlyError(payload && payload.error, response.status),
    };
  }

  if (record) recordHistory(text, payload.translation || "", settings);
  return { ok: true, translation: payload.translation };
}

async function streamTranslation(port, requestId, text, signal, record = true) {
  const settings = await getSettings();
  const url = baseUrl(settings.serviceUrl) + "/translate/stream";

  let response = null;
  for (let attempt = 1; attempt <= STREAM_RETRIES; attempt += 1) {
    try {
      response = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          text,
          source: settings.source,
          target: settings.target,
        }),
        signal,
      });
      break;
    } catch (error) {
      if (isAbort(error)) return;

      if (attempt >= STREAM_RETRIES) {
        send(port, {
          type: "error",
          requestId,
          message: connectionError(settings.serviceUrl),
        });
        refreshHealth();
        return;
      }

      send(port, {
        type: "retry",
        requestId,
        attempt: attempt + 1,
        total: STREAM_RETRIES,
      });

      await delay(RETRY_DELAY_MS, signal);
      if (signal.aborted) return;
    }
  }

  if (response.status === 404 || response.status === 405) {
    await sendOneShot(port, requestId, text, record);
    return;
  }

  if (!response.ok) {
    let payload = null;
    try {
      payload = await response.json();
    } catch (error) {
      void error;
    }
    send(port, {
      type: "error",
      requestId,
      message: friendlyError(payload && payload.error, response.status),
    });
    return;
  }

  if (!response.body || typeof response.body.getReader !== "function") {
    await sendOneShot(port, requestId, text, record);
    return;
  }

  setBadge(true);

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let dataLines = [];
  let terminal = false;

  const handleEvent = (data) => {
    let event = null;
    try {
      event = JSON.parse(data);
    } catch (error) {
      return;
    }

    if (!event || !event.type) return;

    if (event.type === "delta") {
      send(port, { type: "delta", requestId, delta: event.delta || "" });
    } else if (event.type === "done") {
      terminal = true;
      send(port, {
        type: "done",
        requestId,
        translation: event.translation || "",
        elapsed_ms: event.elapsed_ms,
      });
      if (record) recordHistory(text, event.translation || "", settings);
    } else if (event.type === "error") {
      terminal = true;
      send(port, { type: "error", requestId, message: friendlyError(event, 200) });
    }
  };

  const handleLine = (line) => {
    if (line === "") {
      if (dataLines.length) {
        handleEvent(dataLines.join("\n"));
        dataLines = [];
      }
      return;
    }
    if (line.startsWith("data:")) dataLines.push(line.slice(5).trimStart());
  };

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      const lines = buffer.split(/\r?\n/);
      buffer = lines.pop() ?? "";
      for (const line of lines) handleLine(line);

      if (terminal) break;
    }

    if (buffer) handleLine(buffer);
    if (dataLines.length) handleEvent(dataLines.join("\n"));
  } catch (error) {
    if (isAbort(error)) return;
    send(port, { type: "error", requestId, message: "翻译过程中断，请重试。" });
    return;
  } finally {
    try {
      reader.cancel();
    } catch (error) {
      void error;
    }
  }

  if (!terminal) {
    await sendOneShot(port, requestId, text, record);
  }
}

async function sendOneShot(port, requestId, text, record = true) {
  const result = await translate(text, record);
  if (result.ok) {
    send(port, { type: "done", requestId, translation: result.translation });
  } else {
    send(port, { type: "error", requestId, message: result.error });
  }
}

// Reads the visible tab, recognizes its text through the local service and
// hands the blocks to the page overlay. The service can be slow on the first
// request (OCR model download); the content script shows nothing until the
// result arrives.
async function translateImage(image) {
  const settings = await getSettings();
  const url = baseUrl(settings.serviceUrl) + "/translate/image";

  let response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        image,
        source: settings.source,
        target: settings.target,
      }),
    });
  } catch (error) {
    return { ok: false, error: connectionError(settings.serviceUrl) };
  }

  let payload = null;
  try {
    payload = await response.json();
  } catch (error) {
    return { ok: false, error: "本地服务返回了无效响应。" };
  }

  if (!response.ok) {
    return {
      ok: false,
      error: friendlyError(payload && payload.error, response.status),
    };
  }

  return {
    ok: true,
    image: payload.image || null,
    blocks: Array.isArray(payload.blocks) ? payload.blocks : [],
  };
}

async function sendScreenshotError(tab, message) {
  try {
    await api.tabs.sendMessage(tab.id, { type: "screenshot-error", message });
  } catch (error) {
    console.warn("[OpenTranslator] screenshot error:", message);
  }
}

async function deliverImageResult(tab, result) {
  if (!result.ok) {
    await sendScreenshotError(tab, result.error);
    return;
  }

  if (!result.blocks.length) {
    await sendScreenshotError(tab, "没有识别到可翻译的文字。");
    return;
  }

  try {
    await api.tabs.sendMessage(tab.id, {
      type: "screenshot-result",
      image: result.image,
      blocks: result.blocks,
    });
  } catch (error) {
    // No content script (PDF viewer, browser pages): fall back to the result
    // page with the recognized text.
    const text = result.blocks
      .map((block) => (block.text || "").trim())
      .filter(Boolean)
      .join("\n");
    if (text) openResultPage(text);
  }
}

async function screenshotTranslate(tab) {
  if (!tab || tab.id === undefined) return;

  let image = null;
  try {
    image = await api.tabs.captureVisibleTab(tab.windowId, { format: "png" });
  } catch (error) {
    await sendScreenshotError(tab, "截图失败：无法捕获当前页面。");
    return;
  }

  await deliverImageResult(tab, await translateImage(image));
}

async function checkHealth() {
  const settings = await getSettings();
  const url = baseUrl(settings.serviceUrl) + "/health";

  try {
    const response = await fetch(url, { cache: "no-store" });
    if (!response.ok) return { ok: false, error: "HTTP " + response.status };
    const payload = await response.json();
    return {
      ok: true,
      engine: payload.engine || "-",
      model: payload.model || "-",
    };
  } catch (error) {
    return { ok: false, error: String(error) };
  }
}

function setBadge(ok) {
  if (!action) return;

  if (ok) {
    action.setBadgeText({ text: "" });
    return;
  }

  action.setBadgeText({ text: "!" });
  action.setBadgeBackgroundColor({ color: "#b91c1c" });
}

async function refreshHealth() {
  const health = await checkHealth();
  setBadge(health.ok);
  return health;
}

async function openResultPage(text) {
  await api.storage.local.set({ pendingResult: { text, at: Date.now() } });
  api.windows.create({
    url: api.runtime.getURL("result.html"),
    type: "popup",
    width: 460,
    height: 380,
  });
}

api.runtime.onConnect.addListener((port) => {
  if (port.name !== "translate") return;

  let controller = null;

  port.onDisconnect.addListener(() => {
    if (controller) {
      controller.abort();
      controller = null;
    }
  });

  port.onMessage.addListener((message) => {
    if (!message) return;

    if (message.type === "translate") {
      if (controller) controller.abort();
      controller = new AbortController();
      streamTranslation(
        port,
        message.requestId,
        message.text,
        controller.signal,
        message.record !== false
      );
    } else if (message.type === "cancel") {
      if (controller) {
        controller.abort();
        controller = null;
      }
    }
  });
});

api.runtime.onInstalled.addListener((details) => {
  api.contextMenus.removeAll(() => {
    api.contextMenus.create({
      id: "translate-selection",
      title: "翻译选中文本（OpenTranslator）",
      contexts: ["selection"],
    });
    api.contextMenus.create({
      id: "screenshot-translate",
      title: "截图翻译页面（OpenTranslator）",
      contexts: ["page"],
    });
  });

  api.alarms.create(HEALTH_ALARM, { periodInMinutes: 1 });
  refreshHealth();

  if (details && details.reason === "install" && api.runtime.openOptionsPage) {
    api.runtime.openOptionsPage();
  }
});

if (api.runtime.onStartup) {
  api.runtime.onStartup.addListener(() => {
    api.alarms.create(HEALTH_ALARM, { periodInMinutes: 1 });
    refreshHealth();
  });
}

if (api.alarms && api.alarms.onAlarm) {
  api.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === HEALTH_ALARM) refreshHealth();
  });
}

api.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId === "screenshot-translate") {
    await screenshotTranslate(tab);
    return;
  }

  if (info.menuItemId !== "translate-selection") return;

  if (tab && tab.id !== undefined) {
    const options = info.frameId === undefined ? undefined : { frameId: info.frameId };
    try {
      await api.tabs.sendMessage(tab.id, { type: "start-translate" }, options);
      return;
    } catch (error) {
      void error;
    }
  }

  const selection = (info.selectionText || "").trim();
  if (selection) openResultPage(selection);
});

api.commands.onCommand.addListener((command) => {
  if (command === "screenshot-translate") {
    api.tabs.query({ active: true, currentWindow: true }).then((tabs) => {
      const tab = tabs[0];
      if (tab) screenshotTranslate(tab);
    });
    return;
  }

  let type = null;

  if (command === "translate-selection") {
    type = "start-translate";
  } else if (command === "translate-clipboard") {
    type = "translate-clipboard";
  } else if (command === "cycle-target") {
    type = "cycle-target";
  } else {
    return;
  }

  api.tabs.query({ active: true, currentWindow: true }).then((tabs) => {
    const tab = tabs[0];
    if (tab && tab.id !== undefined) {
      api.tabs.sendMessage(tab.id, { type }).catch(() => {});
    }
  });
});

api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (!message) return;

  if (message.type === "health") {
    refreshHealth().then((health) => sendResponse(health));
    return true;
  }

  if (message.type === "translate") {
    translate(message.text).then((result) => sendResponse(result));
    return true;
  }

  if (message.type === "record-history") {
    getSettings().then((settings) => {
      recordHistory(message.text, message.translation, settings);
    });
    return;
  }

  if (message.type === "open-options") {
    if (api.runtime.openOptionsPage) api.runtime.openOptionsPage();
  }
});
