"use strict";

const api = globalThis.browser ?? globalThis.chrome;

const DEFAULTS = {
  serviceUrl: "http://127.0.0.1:17890",
  source: "en",
  target: "zh",
};

async function getSettings() {
  const stored = await api.storage.local.get(DEFAULTS);
  return Object.assign({}, DEFAULTS, stored);
}

async function translate(text) {
  const settings = await getSettings();
  const url = settings.serviceUrl.replace(/\/+$/, "") + "/translate";

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
    return {
      ok: false,
      error:
        "无法连接本地翻译服务（" +
        url +
        "）。请先启动服务，或按一次桌面端快捷键自动拉起。\n" +
        error,
    };
  }

  let payload = null;
  try {
    payload = await response.json();
  } catch (error) {
    return { ok: false, error: "本地服务返回了无效响应。" };
  }

  if (!response.ok) {
    const message =
      payload && payload.error
        ? payload.error.kind + ": " + payload.error.message
        : "HTTP " + response.status;
    return { ok: false, error: message };
  }

  return { ok: true, translation: payload.translation };
}

api.runtime.onInstalled.addListener(() => {
  api.contextMenus.removeAll(() => {
    api.contextMenus.create({
      id: "translate-selection",
      title: "翻译选中文本（OpenTranslator）",
      contexts: ["selection"],
    });
  });
});

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
  if (!message || message.type !== "translate") return;

  translate(message.text).then(sendResponse);
  return true;
});
