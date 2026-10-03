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

async function translate(text) {
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
    return { ok: false, error: friendlyError(payload && payload.error, response.status) };
  }

  return { ok: true, translation: payload.translation };
}

function isAbort(error) {
  return Boolean(error) && error.name === "AbortError";
}

function send(port, message) {
  try {
    port.postMessage(message);
  } catch (error) {
    void error;
  }
}

async function streamTranslation(port, requestId, text, signal) {
  const settings = await getSettings();
  const url = baseUrl(settings.serviceUrl) + "/translate/stream";

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
      signal,
    });
  } catch (error) {
    if (isAbort(error)) return;
    send(port, { type: "error", requestId, message: connectionError(settings.serviceUrl) });
    refreshHealth();
    return;
  }

  if (response.status === 404 || response.status === 405) {
    await sendOneShot(port, requestId, text);
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
    await sendOneShot(port, requestId, text);
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
    await sendOneShot(port, requestId, text);
  }
}

async function sendOneShot(port, requestId, text) {
  const result = await translate(text);
  if (result.ok) {
    send(port, { type: "done", requestId, translation: result.translation });
  } else {
    send(port, { type: "error", requestId, message: result.error });
  }
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
      streamTranslation(port, message.requestId, message.text, controller.signal);
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

api.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId !== "translate-selection") return;
  if (!tab || tab.id === undefined) return;
  api.tabs.sendMessage(tab.id, { type: "start-translate" }).catch(() => {});
});

api.commands.onCommand.addListener((command) => {
  if (command !== "translate-selection") return;

  api.tabs.query({ active: true, currentWindow: true }).then((tabs) => {
    const tab = tabs[0];
    if (tab && tab.id !== undefined) {
      api.tabs.sendMessage(tab.id, { type: "start-translate" }).catch(() => {});
    }
  });
});

api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (!message || message.type !== "health") return;

  refreshHealth().then((health) => sendResponse(health));
  return true;
});
