"use strict";

const api = globalThis.browser ?? globalThis.chrome;

const statusEl = document.getElementById("status");
const originalEl = document.getElementById("original");
const copyButton = document.getElementById("copy");
const closeButton = document.getElementById("close");

async function run() {
  const stored = await api.storage.local.get({ pendingResult: null });
  const pending = stored.pendingResult;
  await api.storage.local.remove("pendingResult");

  if (!pending || !pending.text) {
    statusEl.classList.add("error");
    statusEl.textContent = "没有待翻译的文本。";
    return;
  }

  originalEl.textContent = pending.text;

  const response = await api.runtime.sendMessage({
    type: "translate",
    text: pending.text,
  });

  if (response && response.ok) {
    const translation = response.translation || "";
    statusEl.textContent = translation;
    copyButton.disabled = !translation;
    copyButton.addEventListener("click", () => {
      navigator.clipboard.writeText(translation).then(() => {
        copyButton.textContent = "已复制";
        setTimeout(() => {
          copyButton.textContent = "复制译文";
        }, 1200);
      });
    });
  } else {
    statusEl.classList.add("error");
    statusEl.textContent = (response && response.error) || "翻译失败。";
  }
}

closeButton.addEventListener("click", () => window.close());
run();
