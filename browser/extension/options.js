"use strict";

const api = globalThis.browser ?? globalThis.chrome;

const DEFAULTS = {
  serviceUrl: "http://127.0.0.1:17890",
  source: "en",
  target: "zh",
};

const form = document.getElementById("form");
const serviceUrlInput = document.getElementById("serviceUrl");
const sourceInput = document.getElementById("source");
const targetInput = document.getElementById("target");
const statusEl = document.getElementById("status");

api.storage.local.get(DEFAULTS).then((settings) => {
  serviceUrlInput.value = settings.serviceUrl;
  sourceInput.value = settings.source;
  targetInput.value = settings.target;
});

form.addEventListener("submit", (event) => {
  event.preventDefault();

  api.storage.local
    .set({
      serviceUrl: serviceUrlInput.value.trim() || DEFAULTS.serviceUrl,
      source: sourceInput.value.trim() || DEFAULTS.source,
      target: targetInput.value.trim() || DEFAULTS.target,
    })
    .then(() => {
      statusEl.textContent = "已保存。";
    });
});

document.getElementById("test").addEventListener("click", async () => {
  statusEl.textContent = "测试中…";

  const base = (serviceUrlInput.value.trim() || DEFAULTS.serviceUrl).replace(
    /\/+$/,
    ""
  );

  try {
    const response = await fetch(base + "/health");
    const payload = await response.json();
    statusEl.textContent =
      "服务正常：" + payload.engine + " / " + (payload.model || "-");
  } catch (error) {
    statusEl.textContent = "连接失败：" + error;
  }
});
