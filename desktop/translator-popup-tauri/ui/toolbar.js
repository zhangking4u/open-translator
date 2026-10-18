const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const translate = document.getElementById("translate");
const copy = document.getElementById("copy");
const speak = document.getElementById("speak");
const more = document.getElementById("more");
const menu = document.getElementById("menu");
const disableApp = document.getElementById("disable-app");
const disableAll = document.getElementById("disable-all");
const toast = document.getElementById("toast");

let toastTimer = null;
let hideTimer = null;
let speaking = false;
let speechGeneration = 0;
let localAudio = null;

function showToast(text, duration = 1200) {
  toast.textContent = text;
  toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    toast.hidden = true;
  }, duration);
}

function setMenu(open) {
  menu.hidden = !open;
  more.setAttribute("aria-expanded", String(open));
  document.body.dataset.menu = open ? "open" : "closed";
  invoke("toolbar_expanded", { expanded: open }).catch(() => {});
}

function resetMenu() {
  const wasOpen = !menu.hidden;

  menu.hidden = true;
  more.setAttribute("aria-expanded", "false");
  document.body.dataset.menu = "closed";

  if (wasOpen) {
    // Collapse the native window too; otherwise the transparent area below
    // the pill keeps intercepting clicks until the auto-hide timer fires.
    invoke("toolbar_expanded", { expanded: false }).catch(() => {});
  }
}

function stopSpeech() {
  const wasActive = speaking || localAudio !== null;

  speechGeneration += 1;

  if (localAudio) {
    const audio = localAudio;
    localAudio = null;
    audio.pause();
    audio.removeAttribute("src");
    audio.load();
  }

  if ("speechSynthesis" in window) {
    window.speechSynthesis.cancel();
  }

  if (wasActive) {
    invoke("stop_local_tts").catch(() => {});
    invoke("stop_speaking").catch(() => {});
  }

  speaking = false;
  speak.querySelector("span").textContent = "朗读";
}

function finishSpeech() {
  speaking = false;
  localAudio = null;
  speak.querySelector("span").textContent = "朗读";
}

async function refreshMenuState() {
  const info = await invoke("toolbar_info").catch(() => null);
  disableApp.hidden = !info || !info.app;
}

async function translateSelection() {
  resetMenu();
  stopSpeech();

  try {
    await invoke("toolbar_action", { action: "translate" });
  } catch (message) {
    showToast(String(message));
  }
}

async function copySelection() {
  resetMenu();

  try {
    await invoke("toolbar_action", { action: "copy" });
    showToast("已复制", 700);
    clearTimeout(hideTimer);
    hideTimer = window.setTimeout(() => {
      invoke("toolbar_hide").catch(() => {});
    }, 700);
  } catch (message) {
    showToast(String(message));
  }
}

function pickVoice(lang) {
  const synth = window.speechSynthesis;
  const code = (lang || "").toLowerCase();
  const base = code.split("-")[0];

  return (
    synth
      .getVoices()
      .find((voice) => voice.lang.toLowerCase() === code) ||
    synth.getVoices().find((voice) => voice.lang.toLowerCase().startsWith(base)) ||
    null
  );
}

// The backend resolves the concrete engine from tts_local_engine and the
// installed models; this only picks the speaker that matches that choice and
// decides whether a local attempt is worthwhile. Vits ignores the speaker.
function localPlan(settings, lang) {
  const code = (lang || "").toLowerCase();
  const rate = Number.parseFloat(settings.tts_rate) || 1;

  if (code.startsWith("zh")) {
    if (!settings.tts_vits_installed && !settings.tts_kokoro_installed) {
      return null;
    }

    return { speaker: Number(settings.tts_speaker_zh) || 0, rate };
  }

  if (settings.tts_supertonic_installed) {
    return { speaker: Number(settings.tts_speaker_multi) || 0, rate };
  }

  if (code.startsWith("en") && settings.tts_kokoro_installed) {
    return { speaker: Number(settings.tts_speaker_en) || 0, rate };
  }

  return null;
}

function playLocalAudio(wavBase64) {
  return new Promise((resolve, reject) => {
    const audio = new Audio("data:audio/wav;base64," + wavBase64);
    localAudio = audio;
    audio.onended = () => {
      if (localAudio === audio) {
        localAudio = null;
      }

      resolve();
    };
    audio.onerror = () => {
      if (localAudio === audio) {
        localAudio = null;
      }

      reject(new Error("音频播放失败"));
    };
    audio.play().catch(reject);
  });
}

function speakSystem(text, lang, settings) {
  const parsedRate = Number.parseFloat(settings?.tts_rate);
  const rate = Number.isFinite(parsedRate) && parsedRate > 0 ? parsedRate : 0.95;

  // WebKitGTK has no speechSynthesis; Linux uses speech-dispatcher.
  if (!("speechSynthesis" in window)) {
    const generation = ++speechGeneration;

    speaking = true;
    speak.querySelector("span").textContent = "停止";
    invoke("speak_text", { text, lang, rate, generation }).catch((message) => {
      if (generation === speechGeneration) {
        finishSpeech();
      }

      showToast(String(message));
    });
    return;
  }

  const synth = window.speechSynthesis;
  const voice = pickVoice(lang);
  const utterance = new SpeechSynthesisUtterance(text);

  if (voice) {
    utterance.voice = voice;
    utterance.lang = voice.lang;
  } else {
    utterance.lang = lang;
  }

  utterance.rate = rate;
  utterance.onend = finishSpeech;
  utterance.onerror = finishSpeech;

  speaking = true;
  speak.querySelector("span").textContent = "停止";
  synth.speak(utterance);
}

async function speakSelection() {
  resetMenu();

  if (speaking) {
    stopSpeech();
    return;
  }

  const info = await invoke("toolbar_speech").catch(() => null);

  if (!info || !info.text.trim()) {
    showToast("没有可朗读的内容");
    return;
  }

  const text = info.text.trim();
  const lang = info.lang === "auto" ? navigator.language : info.lang;
  const settings = await invoke("get_settings").catch(() => null);
  const generation = ++speechGeneration;

  if (settings?.tts_engine === "local" && settings.local_tts_supported) {
    const plan = localPlan(settings, lang);

    if (plan) {
      speaking = true;
      speak.querySelector("span").textContent = "停止";

      try {
        const audio = await invoke("tts_synthesize", {
          text,
          lang,
          rate: plan.rate,
          speaker: plan.speaker,
          localEngine: settings.tts_local_engine,
          generation,
        });

        if (generation !== speechGeneration) {
          return;
        }

        await playLocalAudio(audio.wav_base64);

        if (generation === speechGeneration) {
          finishSpeech();
        }

        return;
      } catch {
        if (generation !== speechGeneration) {
          return;
        }

        // Local synthesis or playback failed: fall back to the system voice.
        speaking = false;
      }
    }
  }

  if (generation !== speechGeneration) {
    return;
  }

  speakSystem(text, lang, settings);
}

translate.addEventListener("click", translateSelection);
copy.addEventListener("click", copySelection);
speak.addEventListener("click", speakSelection);

more.addEventListener("click", () => {
  const open = menu.hidden;
  setMenu(open);

  if (open) {
    refreshMenuState();
  }
});

disableApp.addEventListener("click", async () => {
  try {
    await invoke("toolbar_disable_app");
  } catch (message) {
    showToast(String(message));
  }
});

disableAll.addEventListener("click", () => {
  invoke("toolbar_disable_all").catch(() => {});
});

listen("toolbar-reset", () => {
  clearTimeout(hideTimer);
  stopSpeech();
  resetMenu();
  toast.hidden = true;
  refreshMenuState();
});

listen("speech-ended", (event) => {
  if (event.payload?.generation === speechGeneration) {
    finishSpeech();
  }
});

window.addEventListener("blur", () => {
  stopSpeech();
});
