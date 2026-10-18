const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const toolbarEl = document.querySelector(".toolbar");
const translate = document.getElementById("translate");
const copy = document.getElementById("copy");
const speak = document.getElementById("speak");
const more = document.getElementById("more");
const grip = document.getElementById("grip");
const closeButton = document.getElementById("close");
const menu = document.getElementById("menu");
const disableApp = document.getElementById("disable-app");
const disableAll = document.getElementById("disable-all");
const toast = document.getElementById("toast");

let toastTimer = null;
let hideTimer = null;
let holdTimer = null;
let speechHoldTimer = null;
let speaking = false;
let speechGeneration = 0;
let pendingSpeechEnd = null;
let localAudio = null;

// The native auto-hide timer is extended while the pointer rests on the pill
// or its menu. The interval is deliberately shorter than the 10 s native
// timeout so a held toolbar never disappears under the pointer.
function holdToolbar() {
  if (holdTimer !== null) {
    return;
  }

  invoke("toolbar_hold").catch(() => {});
  holdTimer = window.setInterval(() => {
    invoke("toolbar_hold").catch(() => {});
  }, 4000);
}

function releaseToolbar() {
  if (holdTimer !== null) {
    window.clearInterval(holdTimer);
    holdTimer = null;
  }

  // Restart the normal timeout as soon as the pointer leaves.
  invoke("toolbar_hold").catch(() => {});
}

// Long readings outlive the toolbar's 10 s auto-hide; hiding would emit
// toolbar-reset and cut the speech off, and it would remove the stop control.
// Keep re-arming the native timeout until playback finishes.
function startSpeechHold() {
  if (speechHoldTimer !== null) {
    return;
  }

  invoke("toolbar_hold").catch(() => {});
  speechHoldTimer = window.setInterval(() => {
    invoke("toolbar_hold").catch(() => {});
  }, 4000);
}

function stopSpeechHold() {
  if (speechHoldTimer === null) {
    return;
  }

  window.clearInterval(speechHoldTimer);
  speechHoldTimer = null;
  invoke("toolbar_hold").catch(() => {});
}

// The toolbar is non-focusable, so dragging drives the native window through
// move_toolbar_by (screen coordinates) instead of relying on the compositor's
// window move loop.
const DRAG_THRESHOLD = 5;
let toolbarPressed = false;
let toolbarDragging = false;
let toolbarPressX = 0;
let toolbarPressY = 0;
let toolbarLastX = 0;
let toolbarLastY = 0;

function resetToolbarDrag() {
  toolbarPressed = false;
  toolbarDragging = false;
}

grip.addEventListener("mousedown", (event) => {
  if (event.button !== 0) {
    return;
  }

  toolbarPressed = true;
  toolbarDragging = false;
  toolbarPressX = event.screenX;
  toolbarPressY = event.screenY;
  toolbarLastX = event.screenX;
  toolbarLastY = event.screenY;
  event.preventDefault();
});

window.addEventListener("mousemove", (event) => {
  if (!toolbarPressed) {
    return;
  }

  if ((event.buttons & 1) === 0) {
    resetToolbarDrag();
    return;
  }

  if (!toolbarDragging) {
    if (
      Math.hypot(event.screenX - toolbarPressX, event.screenY - toolbarPressY) <
      DRAG_THRESHOLD
    ) {
      return;
    }

    toolbarDragging = true;
    toolbarLastX = event.screenX;
    toolbarLastY = event.screenY;
  }

  const dx = event.screenX - toolbarLastX;
  const dy = event.screenY - toolbarLastY;

  if (dx === 0 && dy === 0) {
    return;
  }

  toolbarLastX = event.screenX;
  toolbarLastY = event.screenY;
  invoke("move_toolbar_by", { dx, dy }).catch(() => {});
});

window.addEventListener("mouseup", resetToolbarDrag);

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

  if (pendingSpeechEnd) {
    const resolve = pendingSpeechEnd;
    pendingSpeechEnd = null;
    resolve();
  }

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
  stopSpeechHold();
  speak.querySelector("span").textContent = "朗读";
}

function finishSpeech() {
  speaking = false;
  localAudio = null;
  stopSpeechHold();
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


function baseLanguage(lang) {
  return String(lang || "").toLowerCase().split("-")[0];
}

function isChineseLanguage(lang) {
  const base = baseLanguage(lang);
  return base === "zh" || base === "cmn" || base === "yue";
}

// See ui/main.js: mixed Chinese/English text is split at script boundaries so
// each run gets a voice that can pronounce it, and common framework names are
// respelled the way people say them.
const SPEECH_TERMS = [
  [/\bMongoDB\b/gi, "Mongo D B"],
  [/\bPostgreSQL\b/gi, "Postgres Q L"],
  [/\bMySQL\b/gi, "My S Q L"],
  [/\bSQLite\b/gi, "sequel lite"],
  [/\bNginx\b/gi, "engine X"],
  [/\bNode\.js\b/gi, "Node J S"],
  [/\.NET\b/g, "dot net"],
  [/\bVue\b/g, "view"],
  [/\biOS\b/g, "i O S"],
  [/\bmacOS\b/g, "Mac O S"],
  [/\bOpenAI\b/g, "Open A I"],
  [/\bChatGPT\b/g, "Chat G P T"],
  [/\bnpm\b/gi, "N P M"],
  [/\bHTTPS\b/gi, "H T T P S"],
  [/\bHTTP\b/gi, "H T T P"],
  [/\bAPI\b/gi, "A P I"],
  [/\bJSON\b/gi, "Jason"],
  [/\bGUI\b/gi, "gooey"],
  [/\bURL\b/gi, "U R L"],
  [/\bLLM\b/gi, "L L M"],
  [/\bSQL\b/gi, "S Q L"],
];

function normalizeSpeechTerms(text) {
  let value = text;

  for (const [pattern, replacement] of SPEECH_TERMS) {
    value = value.replace(pattern, replacement);
  }

  return value;
}

function mergeSpeechRuns(runs) {
  const merged = [];

  for (const run of runs) {
    const text = run.text;

    if (!text) {
      continue;
    }

    const previous = merged[merged.length - 1];

    if (previous && previous.lang === run.lang) {
      previous.text += text;
    } else {
      merged.push({ text, lang: run.lang });
    }
  }

  return merged
    .map((run) => ({ ...run, text: run.text.trim() }))
    .filter((run) => /[A-Za-z0-9\u3400-\u4dbf\u4e00-\u9fff]/.test(run.text));
}

function splitMixedRuns(text, lang) {
  const value = String(text || "");
  const base = baseLanguage(lang);

  if (!value.trim()) {
    return [];
  }

  if (isChineseLanguage(base)) {
    const runs = [];
    const token = /[A-Za-z][A-Za-z0-9+#._/-]*/g;
    let cursor = 0;

    for (const match of value.matchAll(token)) {
      const trailing = match[0].match(/[./_-]+$/);
      const word = trailing ? match[0].slice(0, -trailing[0].length) : match[0];

      if (!word) {
        continue;
      }

      const start = match.index;
      const end = start + word.length;

      if (start > cursor) {
        runs.push({ text: value.slice(cursor, start), lang: base });
      }

      runs.push({ text: normalizeSpeechTerms(word), lang: "en" });
      cursor = end;
    }

    if (cursor < value.length) {
      runs.push({ text: value.slice(cursor), lang: base });
    }

    return mergeSpeechRuns(runs);
  }

  if (base === "en") {
    const runs = [];
    const han = /[\u3400-\u4dbf\u4e00-\u9fff]+/g;
    let cursor = 0;

    for (const match of value.matchAll(han)) {
      const start = match.index;
      const end = start + match[0].length;

      if (start > cursor) {
        runs.push({
          text: normalizeSpeechTerms(value.slice(cursor, start)),
          lang: base,
        });
      }

      runs.push({ text: match[0], lang: "zh" });
      cursor = end;
    }

    if (cursor < value.length) {
      runs.push({
        text: normalizeSpeechTerms(value.slice(cursor)),
        lang: base,
      });
    }

    return mergeSpeechRuns(runs);
  }

  return [{ text: value, lang }];
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

// Keep runs the installed models cannot pronounce inside a neighbouring
// supported run, so a Chinese-only pack still reads mixed selections locally.
function localPlans(runs, settings) {
  const plans = [];
  let prefix = "";

  for (const run of runs) {
    const plan = localPlan(settings, run.lang);

    if (!plan) {
      if (plans.length) {
        plans[plans.length - 1].run.text += " " + run.text;
      } else {
        prefix = (prefix + " " + run.text).trim();
      }
      continue;
    }

    const text = prefix ? prefix + " " + run.text : run.text;
    prefix = "";
    plans.push({ run: { text, lang: run.lang }, plan });
  }

  return plans;
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

async function speakSystem(runs, settings) {
  const parsedRate = Number.parseFloat(settings?.tts_rate);
  const rate = Number.isFinite(parsedRate) && parsedRate > 0 ? parsedRate : 0.95;
  const generation = speechGeneration;

  // WebKitGTK has no speechSynthesis; Linux uses speech-dispatcher.
  if (!("speechSynthesis" in window)) {
    let failed = false;

    for (const run of runs) {
      if (generation !== speechGeneration || !speaking) {
        return;
      }

      await new Promise((resolve) => {
        pendingSpeechEnd = resolve;

        invoke("speak_text", {
          text: run.text,
          lang: run.lang,
          rate,
          generation,
        }).catch((message) => {
          pendingSpeechEnd = null;
          failed = true;
          if (generation === speechGeneration) {
            finishSpeech();
            showToast(String(message));
          }
          resolve();
        });
      });

      if (failed) {
        return;
      }
    }

    if (generation === speechGeneration) {
      finishSpeech();
    }
    return;
  }

  const synth = window.speechSynthesis;

  for (const run of runs) {
    if (generation !== speechGeneration || !speaking) {
      return;
    }

    const voice = pickVoice(run.lang);

    await new Promise((resolve) => {
      const utterance = new SpeechSynthesisUtterance(run.text);

      if (voice) {
        utterance.voice = voice;
        utterance.lang = voice.lang;
      } else {
        utterance.lang = run.lang;
      }

      utterance.rate = rate;
      utterance.onend = resolve;
      utterance.onerror = resolve;
      synth.speak(utterance);
    });
  }

  if (generation === speechGeneration) {
    finishSpeech();
  }
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
  const runs = splitMixedRuns(text, lang);
  const generation = ++speechGeneration;

  if (settings?.tts_engine === "local" && settings.local_tts_supported) {
    const plans = localPlans(runs, settings);

    if (plans.length) {
      speaking = true;
      startSpeechHold();
      speak.querySelector("span").textContent = "停止";

      const synthesize = ({ run, plan }) =>
        invoke("tts_synthesize", {
          text: run.text,
          lang: run.lang,
          rate: plan.rate,
          speaker: plan.speaker,
          localEngine: settings.tts_local_engine,
          generation,
        });

      try {
        let pendingAudio = synthesize(plans[0]);

        // A stopped reading leaves this promise unawaited; keep a rejection
        // from surfacing as an unhandled error.
        pendingAudio.catch(() => {});

        for (let index = 0; index < plans.length; index += 1) {
          if (generation !== speechGeneration) {
            return;
          }

          const audio = await pendingAudio;

          if (generation !== speechGeneration) {
            return;
          }

          if (index + 1 < plans.length) {
            pendingAudio = synthesize(plans[index + 1]);
            pendingAudio.catch(() => {});
          }

          await playLocalAudio(audio.wav_base64);

          if (generation !== speechGeneration) {
            return;
          }
        }

        finishSpeech();
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

  speaking = true;
  startSpeechHold();
  speak.querySelector("span").textContent = "停止";
  speakSystem(runs, settings);
}

translate.addEventListener("click", translateSelection);
copy.addEventListener("click", copySelection);
speak.addEventListener("click", speakSelection);

closeButton.addEventListener("click", () => {
  resetMenu();
  stopSpeech();
  invoke("toolbar_hide").catch(() => {});
});

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

for (const element of [toolbarEl, menu]) {
  element.addEventListener("pointerenter", holdToolbar);
  element.addEventListener("pointerleave", releaseToolbar);
}

listen("toolbar-reset", () => {
  clearTimeout(hideTimer);
  resetToolbarDrag();
  releaseToolbar();
  stopSpeech();
  resetMenu();
  toast.hidden = true;
  refreshMenuState();
});

listen("speech-ended", (event) => {
  if (event.payload?.generation !== speechGeneration) {
    return;
  }

  if (pendingSpeechEnd) {
    const resolve = pendingSpeechEnd;
    pendingSpeechEnd = null;
    resolve();
    return;
  }

  finishSpeech();
});

window.addEventListener("blur", () => {
  resetToolbarDrag();
  stopSpeech();
});
