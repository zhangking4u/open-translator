const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const isInputWindow =
  window.__TAURI__.window?.getCurrentWindow().label === "input";

const views = {
  translator: document.getElementById("translator-view"),
  input: document.getElementById("input-view"),
  settings: document.getElementById("settings-view"),
  settingsTranslation: document.getElementById("settings-translation-view"),
  settingsShortcuts: document.getElementById("settings-shortcuts-view"),
  settingsVoice: document.getElementById("settings-voice-view"),
  settingsVoicePacks: document.getElementById("settings-voice-packs-view"),
  settingsSystem: document.getElementById("settings-system-view"),
  about: document.getElementById("about-view"),
  history: document.getElementById("history-view"),
};

const source = document.getElementById("source");
const sourceSelect = document.getElementById("source-select");
const targetSelect = document.getElementById("target-select");
const swapButton = document.getElementById("swap");
const detectedLabel = document.getElementById("detected");
const translation = document.getElementById("translation");
const status = document.getElementById("status");
const copy = document.getElementById("copy");
const retranslate = document.getElementById("retranslate");
const replace = document.getElementById("replace");
const speak = document.getElementById("speak");
const pin = document.getElementById("pin");
const close = document.getElementById("close");
const dot = document.getElementById("dot");
const inputView = document.getElementById("input-view");
const inputSource = document.getElementById("input-source");
const inputTarget = document.getElementById("input-target");
const inputSwap = document.getElementById("input-swap");
const inputClear = document.getElementById("input-clear");
const inputText = document.getElementById("input-text");
const inputCompose = document.getElementById("input-compose");
const inputTranslate = document.getElementById("input-translate");
const inputError = document.getElementById("input-error");
const inputResult = document.getElementById("input-result");
const inputSourcePreview = document.getElementById("input-source-preview");
const inputSourceText = document.getElementById("input-source-text");
const inputResultText = document.getElementById("input-result-text");
const inputCopy = document.getElementById("input-copy");
const inputRetranslate = document.getElementById("input-retranslate");
const inputSaved = document.getElementById("input-saved");
const inputAlternate = document.getElementById("input-alternate");
const inputAlternativesContainer = document.getElementById("input-alternatives");
const inputSpeak = document.getElementById("input-speak");
const inputMore = document.getElementById("input-more");
const inputActionsMenu = document.getElementById("input-actions-menu");
const onboarding = document.getElementById("onboarding");
const onboardingDone = document.getElementById("onboarding-done");
const hotkeySheet = document.getElementById("hotkey-sheet");
const hotkeySheetKeys = document.getElementById("hotkey-sheet-keys");
const hotkeySheetHint = document.getElementById("hotkey-sheet-hint");
const hotkeySheetError = document.getElementById("hotkey-sheet-error");
const hotkeySheetCancel = document.getElementById("hotkey-sheet-cancel");
const srStatus = document.getElementById("sr-status");
const ttsTargetSelect = document.getElementById("tts-target");
const ttsRateSelect = document.getElementById("tts-rate");
const ttsVoiceSelect = document.getElementById("tts-voice");
const ttsVoiceRow = document.getElementById("tts-voice-row");
const ttsEngineSelect = document.getElementById("tts-engine");
const ttsEngineRow = document.getElementById("tts-engine-row");
const ttsLocalEngineSelect = document.getElementById("tts-local-engine");
const ttsLocalEngineRow = document.getElementById("tts-local-engine-row");
const ttsSupertonicModelRow = document.getElementById("tts-supertonic-model-row");
const ttsSupertonicModelStatus = document.getElementById("tts-supertonic-model-status");
const ttsSupertonicModelDownload = document.getElementById("tts-supertonic-model-download");
const ttsSupertonicModelDelete = document.getElementById("tts-supertonic-model-delete");
const ttsMultiSpeakerSelect = document.getElementById("tts-speaker-multi");
const ttsMultiPreview = document.getElementById("tts-multi-preview");
const ttsMultiSpeakerRow = document.getElementById("tts-multi-speaker-row");
const ttsVitsModelRow = document.getElementById("tts-vits-model-row");
const ttsVitsModelStatus = document.getElementById("tts-vits-model-status");
const ttsVitsModelDownload = document.getElementById("tts-vits-model-download");
const ttsVitsModelDelete = document.getElementById("tts-vits-model-delete");
const ttsKokoroModelRow = document.getElementById("tts-kokoro-model-row");
const ttsKokoroModelStatus = document.getElementById("tts-kokoro-model-status");
const ttsKokoroModelDownload = document.getElementById("tts-kokoro-model-download");
const ttsKokoroModelDelete = document.getElementById("tts-kokoro-model-delete");
const ttsSpeakerZhSelect = document.getElementById("tts-speaker-zh");
const ttsSpeakerEnSelect = document.getElementById("tts-speaker-en");
const ttsSpeakerZhRow = document.getElementById("tts-speaker-zh-row");
const ttsSpeakerEnRow = document.getElementById("tts-speaker-en-row");
const ttsTitle = document.getElementById("tts-title");
const ttsGroup = document.getElementById("tts-group");
const switchSelection = document.getElementById("switch-selection");
const selectionMethodSelect = document.getElementById("selection-method");
const selectionMethodRow = document.getElementById("selection-method-row");
const selectionSaved = document.getElementById("selection-saved");
const selectionGroup = document.getElementById("selection-group");
const selectionTitle = document.getElementById("selection-title");
const toolbarDisabledRow = document.getElementById("toolbar-disabled-row");
const toolbarDisabledList = document.getElementById("toolbar-disabled-list");
const switchAutostart = document.getElementById("switch-autostart");
const switchAutoCopy = document.getElementById("switch-auto-copy");
const openAbout = document.getElementById("open-about");
const aboutBack = document.getElementById("about-back");
const aboutVersionLabel = document.getElementById("about-version-label");
const modelStatus = document.getElementById("model-status");
const extensionStatus = document.getElementById("extension-status");
const switchSelectionRow = document.getElementById("switch-selection-row");
const openTranslationSettings = document.getElementById("open-translation-settings");
const openShortcutsSettings = document.getElementById("open-shortcuts-settings");
const openVoiceSettings = document.getElementById("open-voice-settings");
const openVoicePacksSettings = document.getElementById("open-voice-packs-settings");
const openSystemSettings = document.getElementById("open-system-settings");
const settingsModeSummary = document.getElementById("settings-mode-summary");
const settingsShortcutsSummary = document.getElementById("settings-shortcuts-summary");
const settingsVoiceSummary = document.getElementById("settings-voice-summary");
const settingsVoicePacksSummary = document.getElementById("settings-voice-packs-summary");
const settingsTranslationBack = document.getElementById("settings-translation-back");
const settingsShortcutsBack = document.getElementById("settings-shortcuts-back");
const settingsVoiceBack = document.getElementById("settings-voice-back");
const settingsVoicePacksBack = document.getElementById("settings-voice-packs-back");
const settingsSystemBack = document.getElementById("settings-system-back");
const ttsPacksGroup = document.getElementById("tts-packs-group");
let selectionSupported = false;
let lastSettingsPage = "";
let activeHotkeyRecorder = null;
let selectionMethodControl = null;
let selectionModeValue = "off";
let selectionMethodValue = "ball";
let selectionAutoSupported = false;
let selectionBallSupported = true;
let selectionToolbarSupported = false;
let toolbarDisabledApps = [];
let autoCopyEnabled = false;
let historyCache = [];
let historyFilter = "";
let historyFavoritesOnly = false;

const SPEECH_LANGS = {
  zh: "zh-CN",
  en: "en-US",
  ja: "ja-JP",
  ko: "ko-KR",
  fr: "fr-FR",
  de: "de-DE",
  es: "es-ES",
  ru: "ru-RU",
  th: "th-TH",
};
let speakBackend = "webview";
let speakSupported = "speechSynthesis" in window;
let speakVoiceAvailable = true;
let speechGeneration = 0;
let activeSpeechButton = null;
let pendingSpeechEnd = null;
let localTtsSupported = false;
let ttsEngineValue = "system";
let ttsLocalEngine = "fast";
let ttsSpeakerZh = 3;
let ttsSpeakerEn = 0;
let ttsModelInstalled = false;
let ttsKokoroInstalled = false;
let ttsVitsInstalled = false;
let ttsSupertonicInstalled = false;
let ttsModelDownloading = false;
let ttsDownloadingModel = "";
let ttsDownloadError = "";
let ttsDownloadErrorModel = "";
let ttsKokoroSize = 179300000;
let ttsVitsSize = 177500000;
let ttsSupertonicSize = 145300000;
let copyFeedbackTimer = 0;
let retryableError = false;
let announceTimer = 0;

function announce(text) {
  if (!srStatus) return;
  srStatus.textContent = "";
  window.clearTimeout(announceTimer);
  announceTimer = window.setTimeout(() => {
    srStatus.textContent = text;
  }, 30);
}
let ttsSpeakerMulti = 6;
let ttsMultiSpeakers = [];
let ttsPreviewTimer = 0;
let ttsPreviewGeneration = 0;
const ttsPreviewCache = new Map();
const TTS_PREVIEW_SAMPLES = {
  en: "Hello, this is a voice preview.",
  ja: "こんにちは、声の試聴です。",
  ko: "안녕하세요. 음성 미리듣기입니다.",
  fr: "Bonjour, ceci est un aperçu.",
  de: "Hallo, dies ist eine Stimmprobe.",
  es: "Hola, esta es una muestra de voz.",
  ru: "Здравствуйте, это пример голоса.",
};
let ttsSpeakers = [];
let localAudio = null;
let ttsTargetValue = "translation";
let ttsRateValue = 1;
let ttsVoiceName = "";
let ttsTargetControl = null;
let ttsRateControl = null;
let ttsEngineControl = null;
let ttsLocalEngineControl = null;
let hotkeyLabel = "Ctrl+Alt+T";

function systemTtsSupported() {
  return speakBackend === "spd-say" || "speechSynthesis" in window;
}

function updateSpeakSupport() {
  speakSupported = localTtsSupported || systemTtsSupported();
}

function languageBase(lang) {
  return String(lang || "").toLowerCase().split("-")[0];
}

function guessLanguage(text) {
  const value = String(text || "");

  if (/[\u3040-\u30ff]/.test(value)) {
    return "ja";
  }

  if (/[\uac00-\ud7af]/.test(value)) {
    return "ko";
  }

  if (/[\u4e00-\u9fff]/.test(value)) {
    return "zh";
  }

  return "en";
}

function localEngineFor(lang) {
  const base = languageBase(lang);

  if (base === "zh" || base === "cmn" || base === "yue") {
    if (ttsLocalEngine === "natural" && ttsKokoroInstalled) return "kokoro";
    if (ttsLocalEngine === "fast" && ttsVitsInstalled) return "vits";
    if (ttsKokoroInstalled) return "kokoro";
    if (ttsVitsInstalled) return "vits";
    return "";
  }

  if (ttsSupertonicInstalled) return "supertonic";

  if (base === "en" && ttsKokoroInstalled) return "kokoro";

  return "";
}

function localLanguageSupported(lang) {
  return Boolean(localEngineFor(lang));
}

function speakerForLanguage(lang) {
  const engine = localEngineFor(lang);
  const base = languageBase(lang);

  if (engine === "supertonic") return ttsSpeakerMulti;
  if (engine === "vits") return 0;
  if (base === "zh" || base === "cmn" || base === "yue") return ttsSpeakerZh;
  return ttsSpeakerEn;
}

function partsSupportLocal(parts) {
  return parts.every((part) => localLanguageSupported(part.lang));
}

function preloadLocalTtsModel() {
  if (!localTtsSupported || ttsEngineValue !== "local") {
    return;
  }

  const models = new Set();
  const sourceLang = languageState.detected || languageState.source;
  const targets = speechTargets(sourceLang, languageState.target);

  if (targets.source && sourceLang !== "auto") {
    const engine = localEngineFor(sourceLang);

    if (engine) models.add(engine);
  }

  if (targets.translation) {
    const engine = localEngineFor(languageState.target);

    if (engine) models.add(engine);
  }

  for (const model of models) {
    invoke("preload_tts_model", { model }).catch(() => {});
  }
}

async function checkSpeakVoice(lang) {
  try {
    speakVoiceAvailable = await invoke("tts_voice_available", { lang });

    if (!speakVoiceAvailable && ttsEngineValue !== "local") {
      speak.hidden = true;
    }
  } catch (error) {
    speakVoiceAvailable = true;
  }
}

let current = "";
let lastSource = "";
let streaming = false;
let replaceable = false;
let languageOptions = [];
let languageState = {
  source: "auto",
  target: "zh",
  detected: null,
  recent_targets: [],
};

function labelOf(tag) {
  const option = languageOptions.find((entry) => entry.tag === tag);
  return option ? option.label : tag;
}

function applyLanguageState(state) {
  languageState = state;
  sourceSelect.value = state.source;
  targetSelect.value = state.target;

  // The input window starts from the card's current languages instead of
  // silently falling back to the first entry in each list.
  if ([...inputSource.options].some((option) => option.value === state.source)) {
    inputSource.value = state.source;
  }

  if ([...inputTarget.options].some((option) => option.value === state.target)) {
    inputTarget.value = state.target;
  }

  detectedLabel.textContent = state.detected ? "检测：" + labelOf(state.detected) : "";

  if (globalThis.OTSelect) {
    OTSelect.sync(sourceSelect);
    OTSelect.sync(targetSelect);
    syncInputLanguages();
  }

  if (speakSupported) {
    populateTtsVoices();
    speak.title = speakTitle();
  }

  preloadLocalTtsModel();
}

function syncInputLanguages() {
  if (globalThis.OTSelect) {
    OTSelect.sync(inputSource);
    OTSelect.sync(inputTarget);
  }
}

async function initLanguages() {
  languageOptions = await invoke("get_languages");

  for (const select of [sourceSelect, targetSelect, inputSource, inputTarget]) {
    select.innerHTML = "";
    const allowsAuto = select === sourceSelect || select === inputSource;

    for (const option of languageOptions) {
      if (option.tag === "auto" && !allowsAuto) {
        continue;
      }

      const element = document.createElement("option");
      element.value = option.tag;
      element.textContent = option.label;
      select.appendChild(element);
    }
  }

  const settings = await invoke("get_settings").catch(() => null);
  autoCopyEnabled = Boolean(settings && settings.auto_copy);

  if (globalThis.OTSelect) {
    OTSelect.inject();
    OTSelect.enhance(sourceSelect, { title: "源语言" });
    OTSelect.enhance(targetSelect, { title: "目标语言" });
    OTSelect.enhance(inputSource, { title: "源语言" });
    OTSelect.enhance(inputTarget, { title: "目标语言" });
  }

  // Short mode choices read better as segmented controls than as dropdowns.
  // The option removal must happen before the segments are built.
  removeUnsupportedSelectionOptions(settings);

  if (globalThis.OTSegmented) {
    selectionMethodControl = OTSegmented.enhance(selectionMethodSelect, { title: "翻译方式" });
    ttsEngineControl = OTSegmented.enhance(ttsEngineSelect, { title: "朗读引擎" });
    ttsLocalEngineControl = OTSegmented.enhance(ttsLocalEngineSelect, {
      title: "中文引擎",
    });
    ttsTargetControl = OTSegmented.enhance(ttsTargetSelect, { title: "朗读内容" });
    ttsRateControl = OTSegmented.enhance(ttsRateSelect, { title: "语速" });
  }

  if (settings) {
    await applyTtsSettings(settings);
  }

  applyLanguageState(await invoke("get_language_state"));

  if (settings) {
    applySelectionSettings(settings);
  }

  resize();
}

// "划词工具栏" and "立即翻译" are not offered where the watcher has no
// mouse-release anchor (Linux); the options must go before the segmented
// control is built.
function removeUnsupportedSelectionOptions(settings) {
  if (!settings) {
    return;
  }

  if (!settings.selection_toolbar_supported) {
    selectionMethodSelect.querySelector('option[value="toolbar"]')?.remove();

    if (selectionMethodSelect.value === "toolbar") {
      selectionMethodSelect.value = "ball";
    }
  }

  if (!settings.selection_auto_supported) {
    selectionMethodSelect.querySelector('option[value="auto"]')?.remove();

    if (selectionMethodSelect.value === "auto") {
      selectionMethodSelect.value = "ball";
    }
  }

  if (![...selectionMethodSelect.options].some((option) => option.value === selectionMethodValue)) {
    selectionMethodValue = "ball";
  }
}

sourceSelect.addEventListener("change", () => {
  invoke("set_source", { source: sourceSelect.value });
});

targetSelect.addEventListener("change", () => {
  invoke("set_target", { target: targetSelect.value });
});

swapButton.addEventListener("click", () => {
  invoke("swap_languages");
});
let pinned = false;
let view = "translator";

function setStatus(text) {
  status.textContent = text;
}

function setDot(state) {
  dot.className = "dot";

  if (state !== "ready") {
    dot.classList.add(state);
  }

  const label =
    state === "loading" ? "正在准备翻译引擎" : state === "error" ? "翻译引擎出错" : "就绪";
  dot.title = label;
  dot.setAttribute("role", "img");
  dot.setAttribute("aria-label", label);
}

function speakTitle() {
  if (ttsTargetValue === "source") return "朗读原文";
  if (ttsTargetValue === "both") return "朗读原文和译文";
  return "朗读译文";
}

function setSpeaking(active, button) {
  if (active) {
    if (activeSpeechButton && activeSpeechButton !== button) {
      activeSpeechButton.classList.remove("active");
      activeSpeechButton.title =
        activeSpeechButton === inputSpeak ? "朗读" : speakTitle();
    }

    activeSpeechButton = button;
    button.classList.add("active");
    button.title = "停止朗读";
  } else {
    if (activeSpeechButton) {
      activeSpeechButton.classList.remove("active");
      activeSpeechButton.title =
        activeSpeechButton === inputSpeak ? "朗读" : speakTitle();
    }

    activeSpeechButton = null;
  }
}

function stopSpeaking() {
  if (pendingSpeechEnd) {
    const resolve = pendingSpeechEnd;
    pendingSpeechEnd = null;
    resolve();
  }

  if (ttsEngineValue === "local" && localTtsSupported) {
    invoke("stop_local_tts").catch(() => {});
  }

  if (localAudio) {
    const audio = localAudio;
    localAudio = null;
    audio.pause();
    audio.removeAttribute("src");
    audio.load();
  }

  if (speakBackend === "spd-say") {
    invoke("stop_speaking").catch(() => {});
  } else if ("speechSynthesis" in window) {
    window.speechSynthesis.cancel();
  }

  speechGeneration += 1;
  setSpeaking(false);
}

function pickVoice(lang) {
  const target = lang.toLowerCase();
  const base = target.split("-")[0];
  const voices = window.speechSynthesis.getVoices();

  if (ttsVoiceName) {
    const selected = voices.find((voice) => {
      if (voice.name !== ttsVoiceName) return false;

      const code = voice.lang.toLowerCase();
      return code === target || code.startsWith(base);
    });

    if (selected) {
      return selected;
    }
  }

  return (
    voices.find((voice) => voice.lang.toLowerCase() === target) ||
    voices.find((voice) => voice.lang.toLowerCase().startsWith(base)) ||
    null
  );
}

function isChineseLang(lang) {
  const base = languageBase(lang);
  return base === "zh" || base === "cmn" || base === "yue";
}

function speechTargets(sourceLang, targetLang) {
  if (ttsTargetValue === "source") {
    return { source: true, translation: false };
  }

  if (ttsTargetValue === "translation") {
    return { source: false, translation: true };
  }

  if (ttsTargetValue === "both") {
    return { source: true, translation: true };
  }

  // Auto: read the non-Chinese side. If both sides are foreign, read both;
  // if both are Chinese, read the translation.
  const sourceKnown = sourceLang && sourceLang !== "auto";
  const sourceChinese = Boolean(sourceKnown && isChineseLang(sourceLang));
  const targetChinese = isChineseLang(targetLang);

  if (!sourceKnown) {
    return { source: false, translation: true };
  }

  if (!sourceChinese && targetChinese) {
    return { source: true, translation: false };
  }

  if (sourceChinese && !targetChinese) {
    return { source: false, translation: true };
  }

  if (sourceChinese && targetChinese) {
    return { source: false, translation: true };
  }

  return { source: true, translation: true };
}

function buildSpeechParts(sourceText, sourceLang, translation, targetLang) {
  const targets = speechTargets(sourceLang, targetLang);
  const parts = [];

  if (targets.source && sourceText && sourceText.trim()) {
    parts.push({ text: sourceText, lang: sourceLang });
  }

  if (targets.translation && translation && translation.trim()) {
    parts.push({ text: translation, lang: targetLang });
  }

  return parts;
}

// Local synthesis runs in one blocking call per chunk. The first part also
// splits at commas so the first sound arrives quickly; later parts keep whole
// sentences to avoid choppy prosody.
function splitLocalSpeech(text, splitCommas = false) {
  const trimmed = String(text || "").trim();

  if (!trimmed) {
    return [];
  }

  const sentences = splitCommas
    ? trimmed.match(/[^。！？!?；;\n.，,、]+[。！？!?；;.，,、]?/g) || [trimmed]
    : trimmed.match(/[^。！？!?；;\n.]+[。！？!?；;.]?/g) || [trimmed];
  const chunks = [];

  for (const sentence of sentences) {
    const value = sentence.trim();

    if (!value) {
      continue;
    }

    for (let index = 0; index < value.length; index += 60) {
      chunks.push(value.slice(index, index + 60));
    }
  }

  return chunks;
}

function speakWebviewParts(parts, button, reportError) {
  if (!("speechSynthesis" in window)) return;

  const synth = window.speechSynthesis;

  if (synth.speaking) {
    stopSpeaking();
    return;
  }

  const playable = [];

  for (const part of parts) {
    const voice = pickVoice(part.lang);

    if (voice) {
      playable.push({ ...part, voice });
    }
  }

  if (!playable.length) {
    reportError("未找到可用的系统语音，请先安装语音包");
    return;
  }

  setSpeaking(true, button);

  const speakAt = (index) => {
    if (index >= playable.length) {
      stopSpeaking();
      return;
    }

    const part = playable[index];
    const utterance = new SpeechSynthesisUtterance(part.text);
    utterance.voice = part.voice;
    utterance.lang = part.voice.lang;
    utterance.rate = ttsRateValue;
    utterance.onend = () => speakAt(index + 1);
    utterance.onerror = () => stopSpeaking();
    synth.speak(utterance);
  };

  speakAt(0);
}

async function speakSpdParts(parts, button, reportError) {
  setSpeaking(true, button);

  for (const part of parts) {
    if (!activeSpeechButton) return;

    const generation = ++speechGeneration;

    await new Promise((resolve) => {
      pendingSpeechEnd = resolve;

      invoke("speak_text", {
        text: part.text,
        lang: part.lang,
        rate: ttsRateValue,
        generation,
      }).catch((message) => {
        pendingSpeechEnd = null;
        stopSpeaking();
        reportError(String(message));
        resolve();
      });
    });
  }

  stopSpeaking();
}

function playLocalAudio(wavBase64) {
  return new Promise((resolve, reject) => {
    const audio = new Audio("data:audio/wav;base64," + wavBase64);
    let settled = false;
    localAudio = audio;
    pendingSpeechEnd = resolve;

    const finish = (callback) => {
      if (settled) return;
      settled = true;

      if (localAudio === audio) localAudio = null;
      if (pendingSpeechEnd === resolve) pendingSpeechEnd = null;
      callback();
    };

    audio.onended = () => finish(resolve);
    audio.onerror = () => finish(() => reject(new Error("音频播放失败")));
    audio.play().catch((error) => finish(() => reject(error)));
  });
}

function startLocalSynthesis(chunk, generation) {
  return invoke("tts_synthesize", {
    text: chunk.text,
    lang: chunk.lang,
    rate: ttsRateValue,
    speaker: speakerForLanguage(chunk.lang),
    localEngine: ttsLocalEngine,
    generation,
  })
    .then((audio) => ({ audio }))
    .catch((error) => ({ error }));
}

async function speakLocalParts(parts, button, reportError) {
  const generation = ++speechGeneration;
  const chunks = [];
  let firstPart = true;

  for (const part of parts) {
    const partChunks = splitLocalSpeech(part.text, firstPart);

    if (partChunks.length) {
      firstPart = false;
    }

    for (const text of partChunks) {
      chunks.push({ text, lang: part.lang });
    }
  }

  if (!chunks.length) {
    return;
  }

  setSpeaking(true, button);

  // Synthesize the next chunk while the current one is playing. Synthesis is
  // serialized in Rust, so this only overlaps playback with generation.
  let pending = startLocalSynthesis(chunks[0], generation);
  let playedAny = false;

  for (let index = 0; index < chunks.length; index += 1) {
    const result = await pending;

    if (generation !== speechGeneration) return;

    if (result.error) {
      if (!playedAny && systemTtsSupported()) {
        stopSpeaking();
        reportError("本地朗读失败，已使用系统语音");
        speakSystemParts(parts, button, reportError);
        return;
      }

      stopSpeaking();
      reportError(String(result.error));
      return;
    }

    if (index + 1 < chunks.length) {
      pending = startLocalSynthesis(chunks[index + 1], generation);
    }

    playedAny = true;

    try {
      await playLocalAudio(result.audio.wav_base64);
    } catch (error) {
      if (generation !== speechGeneration) return;
      stopSpeaking();
      reportError("音频播放失败：" + (error?.message || error));
      return;
    }

    if (generation !== speechGeneration) return;
  }

  stopSpeaking();
}

function speakSystemParts(parts, button, reportError) {
  if (speakBackend === "spd-say") {
    speakSpdParts(parts, button, reportError);
  } else {
    speakWebviewParts(parts, button, reportError);
  }
}

function toggleSpeechFor(button, parts, reportError) {
  if (!speakSupported) return;

  if (activeSpeechButton) {
    stopSpeaking();
    return;
  }

  if (localAudio) {
    stopSpeaking();
  }

  if (!parts.length) return;

  if (ttsEngineValue === "local" && localTtsSupported) {
    if (!partsSupportLocal(parts)) {
      if (systemTtsSupported()) {
        reportError("当前语言没有本地语音模型，已使用系统语音");
        speakSystemParts(parts, button, reportError);
      } else {
        reportError("当前语言没有本地语音模型");
      }

      return;
    }

    if (!ttsModelInstalled) {
      if (systemTtsSupported()) {
        reportError("本地语音模型未下载，已使用系统语音");
        speakSystemParts(parts, button, reportError);
      } else {
        reportError("本地语音模型未下载，请在设置 → 语音中下载");
      }

      return;
    }

    speakLocalParts(parts, button, reportError);
    return;
  }

  if (!systemTtsSupported()) {
    if (localTtsSupported && ttsModelInstalled) {
      reportError("系统语音不可用，已使用本地模型");
      speakLocalParts(parts, button, reportError);
    } else {
      reportError(
        localTtsSupported
          ? "本地语音模型未下载，请在设置 → 语音中下载"
          : "当前系统没有可用的语音引擎",
      );
    }

    return;
  }

  speakSystemParts(parts, button, reportError);
}

function toggleCardSpeech() {
  if (ttsEngineValue !== "local" && !speakVoiceAvailable) return;

  const sourceLang =
    languageState.detected ||
    (languageState.source !== "auto"
      ? languageState.source
      : guessLanguage(lastSource)) ||
    languageState.target;

  toggleSpeechFor(
    speak,
    buildSpeechParts(lastSource, sourceLang, current, languageState.target),
    (message) => {
      setStatus(message);
      window.setTimeout(() => setStatus(""), 3000);
    },
  );
}

function toggleInputSpeech() {
  if (!inputResultContext) return;

  const target = inputResultContext.target;
  const source =
    inputResultContext.source && inputResultContext.source !== "auto"
      ? inputResultContext.source
      : guessLanguage(inputResultContext.text);

  toggleSpeechFor(
    inputSpeak,
    buildSpeechParts(
      inputResultContext.text,
      source,
      inputResultText.textContent,
      target,
    ),
    (message) => {
      showInputError(message);
      window.setTimeout(() => showInputError(""), 3000);
    },
  );
}

function updateInputSpeak() {
  inputSpeak.hidden = !speakSupported || !inputResultContext;
}

function populateTtsVoices(selected = ttsVoiceName) {
  if (speakBackend === "spd-say" || !("speechSynthesis" in window)) {
    return;
  }

  const lang = (SPEECH_LANGS[languageState.target] || languageState.target).toLowerCase();
  const base = lang.split("-")[0];
  const voices = window.speechSynthesis.getVoices().filter((voice) => {
    const code = voice.lang.toLowerCase();
    return code === lang || code.startsWith(base);
  });

  ttsVoiceSelect.replaceChildren();

  const auto = document.createElement("option");
  auto.value = "";
  auto.textContent = "自动（按语言）";
  ttsVoiceSelect.append(auto);

  for (const voice of voices) {
    const option = document.createElement("option");
    option.value = voice.name;
    option.textContent = voice.name + " · " + voice.lang;
    ttsVoiceSelect.append(option);
  }

  ttsVoiceSelect.value = [...ttsVoiceSelect.options].some(
    (option) => option.value === selected,
  )
    ? selected
    : "";
}

async function loadTtsSpeakers() {
  if (!localTtsSupported) {
    return;
  }

  if (!ttsSpeakers.length) {
    ttsSpeakers = await invoke("tts_speakers").catch(() => []);
  }

  if (!ttsMultiSpeakers.length) {
    ttsMultiSpeakers = await invoke("tts_multi_speakers").catch(() => []);
  }
}

function fillSpeakerSelect(select, language, selected, source = ttsSpeakers) {
  select.replaceChildren();

  const voices = source.filter((speaker) => speaker.language === language);

  for (const speaker of voices) {
    const option = document.createElement("option");
    option.value = String(speaker.id);
    option.textContent = speaker.name;
    select.append(option);
  }

  const available = voices.some((speaker) => speaker.id === selected);

  if (!available && Number.isInteger(selected) && selected >= 0 && selected <= 102) {
    const option = document.createElement("option");
    option.value = String(selected);
    option.textContent = "音色 " + selected;
    select.append(option);
  } else if (!available) {
    selected = voices.length ? voices[0].id : language === "zh" ? 3 : 0;
  }

  select.value = String(selected);
  return selected;
}

function populateTtsSpeakers() {
  ttsSpeakerZh = fillSpeakerSelect(ttsSpeakerZhSelect, "zh", ttsSpeakerZh);
  ttsSpeakerEn = fillSpeakerSelect(ttsSpeakerEnSelect, "en", ttsSpeakerEn);
  ttsSpeakerMulti = fillSpeakerSelect(
    ttsMultiSpeakerSelect,
    "multi",
    ttsSpeakerMulti,
    ttsMultiSpeakers,
  );
}

function previewSpeaker(kind) {
  if (!localTtsSupported || ttsEngineValue !== "local") {
    return;
  }

  let speaker;
  let lang;
  let text;
  let localEngine = ttsLocalEngine;
  let cacheKey;

  if (kind === "multi") {
    if (!ttsSupertonicInstalled) {
      return;
    }

    const target = languageBase(languageState.target);
    lang = TTS_PREVIEW_SAMPLES[target] ? target : "en";
    text = TTS_PREVIEW_SAMPLES[lang];
    speaker = ttsSpeakerMulti;
    cacheKey = `multi:${speaker}:${lang}`;
  } else if (kind === "zh") {
    if (!ttsKokoroInstalled || ttsLocalEngine !== "natural") {
      return;
    }

    lang = "zh";
    text = "你好，这是音色试听。";
    speaker = ttsSpeakerZh;
    localEngine = "natural";
    cacheKey = `zh:${speaker}`;
  } else {
    if (!ttsKokoroInstalled || ttsSupertonicInstalled) {
      return;
    }

    lang = "en";
    text = TTS_PREVIEW_SAMPLES.en;
    speaker = ttsSpeakerEn;
    localEngine = "natural";
    cacheKey = `en:${speaker}`;
  }

  window.clearTimeout(ttsPreviewTimer);
  stopSpeaking();

  const generation = ++ttsPreviewGeneration;
  const cached = ttsPreviewCache.get(cacheKey);

  if (cached) {
    playLocalAudio(cached).catch(() => {});
    return;
  }

  invoke("tts_synthesize", {
    text,
    lang,
    rate: 1,
    speaker,
    localEngine,
    generation,
  })
    .then((audio) => {
      if (generation !== ttsPreviewGeneration) {
        return;
      }

      ttsPreviewCache.set(cacheKey, audio.wav_base64);

      if (ttsPreviewCache.size > 24) {
        ttsPreviewCache.delete(ttsPreviewCache.keys().next().value);
      }

      return playLocalAudio(audio.wav_base64);
    })
    .catch(() => {});
}

function scheduleSpeakerPreview(kind) {
  window.clearTimeout(ttsPreviewTimer);

  const model = kind === "multi" ? "supertonic" : "kokoro";
  const installed =
    kind === "multi" ? ttsSupertonicInstalled : ttsKokoroInstalled;

  if (installed) {
    invoke("preload_tts_model", { model }).catch(() => {});
  }

  ttsPreviewTimer = window.setTimeout(() => previewSpeaker(kind), 350);
}

function cancelScheduledSpeakerPreview() {
  window.clearTimeout(ttsPreviewTimer);
}

function applyModelStatus(status) {
  if (!status) {
    return;
  }

  ttsKokoroInstalled = Boolean(status.kokoro_installed);
  ttsVitsInstalled = Boolean(status.vits_installed);
  ttsSupertonicInstalled = Boolean(status.supertonic_installed);
  ttsModelInstalled = Boolean(status.installed);
  ttsKokoroSize = Number(status.kokoro_size) || ttsKokoroSize;
  ttsVitsSize = Number(status.vits_size) || ttsVitsSize;
  ttsSupertonicSize = Number(status.supertonic_size) || ttsSupertonicSize;
}

function setModelRow(statusElement, button, deleteButton, installed, size, model) {
  if (ttsModelDownloading && ttsDownloadingModel === model) {
    statusElement.textContent = "下载中…";
    statusElement.title = "";
  } else if (ttsDownloadErrorModel === model && ttsDownloadError) {
    statusElement.textContent = "下载失败";
    statusElement.title = ttsDownloadError;
  } else if (installed) {
    statusElement.textContent = "已下载";
    statusElement.title = "";
  } else {
    statusElement.textContent =
      "未下载 · " + Math.round(size / 1000000) + " MB";
    statusElement.title = "";
  }

  button.disabled = ttsModelDownloading;
  button.textContent = installed ? "校验" : "下载";
  deleteButton.hidden = !installed;
  deleteButton.disabled = ttsModelDownloading;
}

function syncTtsRows() {
  const local = ttsEngineValue === "local" && localTtsSupported;
  const natural = ttsLocalEngine === "natural";

  ttsEngineRow.hidden = !localTtsSupported;
  ttsLocalEngineRow.hidden = !local;
  ttsSupertonicModelRow.hidden = !local;
  ttsMultiSpeakerRow.hidden = !local || !ttsSupertonicInstalled;
  ttsVitsModelRow.hidden = !local;
  ttsKokoroModelRow.hidden = !local;
  ttsSpeakerZhRow.hidden = !local || !natural || !ttsKokoroInstalled;
  ttsSpeakerEnRow.hidden =
    !local || ttsSupertonicInstalled || !ttsKokoroInstalled;
  ttsVoiceRow.hidden =
    local || speakBackend === "spd-say" || !("speechSynthesis" in window);

  setModelRow(
    ttsSupertonicModelStatus,
    ttsSupertonicModelDownload,
    ttsSupertonicModelDelete,
    ttsSupertonicInstalled,
    ttsSupertonicSize,
    "supertonic",
  );
  setModelRow(
    ttsVitsModelStatus,
    ttsVitsModelDownload,
    ttsVitsModelDelete,
    ttsVitsInstalled,
    ttsVitsSize,
    "vits",
  );
  setModelRow(
    ttsKokoroModelStatus,
    ttsKokoroModelDownload,
    ttsKokoroModelDelete,
    ttsKokoroInstalled,
    ttsKokoroSize,
    "kokoro",
  );

  if (local && !ttsModelDownloading) {
    if (!natural && !ttsVitsInstalled) {
      ttsVitsModelStatus.textContent += ttsKokoroInstalled
        ? "（当前回退 Kokoro）"
        : "（当前回退系统语音）";
    }

    if (natural && !ttsKokoroInstalled) {
      ttsKokoroModelStatus.textContent += ttsVitsInstalled
        ? "（当前回退 MeloTTS）"
        : "（当前回退系统语音）";
    }
  }

  ttsModelInstalled =
    ttsKokoroInstalled || ttsVitsInstalled || ttsSupertonicInstalled;

  updateSpeakSupport();
  ttsTitle.hidden = !speakSupported;
  ttsGroup.hidden = !speakSupported;
  ttsPacksGroup.hidden = !speakSupported;
  speak.title = speakTitle();
  updateInputSpeak();
  updateSettingsSummaries();
}

async function downloadLocalTtsModel(model) {
  if (ttsModelDownloading) return;

  ttsModelDownloading = true;
  ttsDownloadingModel = model;
  ttsDownloadError = "";
  ttsDownloadErrorModel = "";
  syncTtsRows();

  try {
    const status = await invoke("download_tts_model", { model });
    applyModelStatus(status);
    ttsPreviewCache.clear();
    preloadLocalTtsModel();
  } catch (error) {
    ttsDownloadError = String(error);
    ttsDownloadErrorModel = model;
  } finally {
    ttsModelDownloading = false;
    ttsDownloadingModel = "";
    syncTtsRows();
  }
}

function render(html) {
  translation.innerHTML = html;
  resize();
}

let resizeTimer = 0;
let resizeWaiters = [];
let lastWindowHeight = 0;

// Coalesce the many layout changes a translation causes (ResizeObserver plus
// explicit render calls) into one native resize. The card keeps a stable
// height while tokens stream; the final resize happens in finish().
function resize() {
  return new Promise((resolve) => {
    resizeWaiters.push(resolve);
    clearTimeout(resizeTimer);
    resizeTimer = window.setTimeout(async () => {
      const waiters = resizeWaiters;
      resizeWaiters = [];
      await applyResize();
      waiters.forEach((done) => done());
    }, 80);
  });
}

async function applyResize() {
  if (!isInputWindow && streaming) {
    return;
  }

  const margin = document.body.classList.contains("os-windows") ? 0 : 20;
  // Keep long settings/history pages inside the work area; the views scroll
  // internally instead of growing the native window past the screen.
  const screenHeight = Number(window.screen && window.screen.availHeight) || 0;

  if (screenHeight) {
    const viewMax = Math.max(180, screenHeight - 200);
    document.documentElement.style.setProperty("--ot-view-max", viewMax + "px");
  }

  // Settings/about use one stable window height; pages scroll inside instead
  // of resizing the native window on every navigation.
  if (document.body.classList.contains("settings-mode")) {
    const height = Math.min(560, screenHeight ? screenHeight - 48 : 560);

    if (Math.abs(height - lastWindowHeight) >= 1) {
      lastWindowHeight = height;
      await invoke("resize_window", { height });
    }

    return;
  }

  const cardHeight = Math.ceil(
    document.querySelector(".card").getBoundingClientRect().height
  );
  const maxHeight = screenHeight
    ? Math.max(240, screenHeight - 48)
    : cardHeight + margin;
  const height = Math.min(cardHeight + margin, maxHeight);

  if (Math.abs(height - lastWindowHeight) < 1) {
    return;
  }

  lastWindowHeight = height;
  await invoke("resize_window", { height });
}

function showView(next) {
  view = next;
  const settingsLike = next === "about" || next.startsWith("settings");
  document.body.classList.toggle("settings-mode", settingsLike);

  for (const [name, element] of Object.entries(views)) {
    element.hidden = name !== next;
  }

  if (settingsLike && views[next]) {
    views[next].scrollTop = 0;
  }

  if (isInputWindow) {
    const title = document.getElementById("title");
    if (title) {
      const titles = { input: "输入翻译", history: "历史", about: "关于" };
      title.textContent = titles[next] || (settingsLike ? "设置" : "输入翻译");
    }
  }

  resize();
}

// The primary window returns to the input editor; the selection card returns
// to its translator view.
function showHomeView() {
  if (isInputWindow) {
    showView("input");
    inputText.focus();
  } else {
    showView("translator");
  }
}

function setCopyEnabled(enabled) {
  copy.disabled = !enabled;
}

function waitingHint() {
  if (selectionModeValue === "ball" && selectionBallSupported) {
    return "选中文字后移到小球上开始翻译";
  }

  if (selectionModeValue !== "off") {
    return "选中文字即可翻译";
  }

  return "按 " + hotkeyLabel + " 翻译选中文本";
}

function showWaiting() {
  current = "";
  streaming = false;
  retryableError = false;
  lastSource = "";
  source.hidden = true;
  source.textContent = "";
  retranslate.hidden = true;
  replace.hidden = true;
  speak.hidden = true;
  setCopyEnabled(false);
  stopSpeaking();
  translation.setAttribute("aria-busy", "false");
  render(
    '<div class="empty-state">' +
      '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
      '<path d="M4 5h9" />' +
      '<path d="M4 9h6" />' +
      '<path d="M4 13h4" />' +
      '<path d="m13 11 7 3-3 1-1 3z" />' +
      "</svg>" +
      '<p class="hint">等待划词</p>' +
      '<p class="sub">' +
      waitingHint() +
      "</p>" +
      "</div>"
  );
  announce("等待划词翻译");
}

function showEmpty() {
  current = "";
  streaming = false;
  retryableError = false;
  source.hidden = true;
  source.textContent = "";
  retranslate.hidden = true;
  replace.hidden = true;
  speak.hidden = true;
  setCopyEnabled(false);
  stopSpeaking();
  translation.setAttribute("aria-busy", "false");
  render('<p class="hint">未选中文本</p><p class="sub">请在其它应用中选中要翻译的内容</p>');
  announce("未选中文本");
}

function showSource(text, canReplace) {
  lastSource = text;
  replaceable = canReplace;
  // A new source starts a fresh translation: never append to the old one.
  current = "";
  retryableError = false;
  streaming = false;
  setCopyEnabled(false);
  source.hidden = false;
  source.textContent = text;
  retranslate.hidden = false;
  replace.hidden = !replaceable;
  speak.hidden = true;
  stopSpeaking();
  setDot("loading");
  translation.setAttribute("aria-busy", "true");
  render('<p class="hint">正在翻译…</p>');
  announce("正在翻译");
}

function showProgress(downloaded, total) {
  current = "";
  streaming = false;
  setCopyEnabled(false);
  speak.hidden = true;
  stopSpeaking();
  setDot("loading");
  translation.setAttribute("aria-busy", "true");

  const megabytes = (value) => (value / 1000000).toFixed(0);
  const percent = total ? Math.round((downloaded / total) * 100) : 0;

  render(
    '<p class="hint">正在下载模型</p>' +
      '<progress max="100" value="' + percent + '"></progress>' +
      '<p class="sub">' +
      megabytes(downloaded) +
      " / " +
      (total ? megabytes(total) + " MB · " : "MB · ") +
      "完成后自动翻译</p>"
  );
}

function showError(message, retryable) {
  current = "";
  streaming = false;
  translation.setAttribute("aria-busy", "false");
  announce("翻译失败：" + message);
  setCopyEnabled(false);
  speak.hidden = true;
  stopSpeaking();
  setDot("error");
  retryableError = retryable;
  retranslate.hidden = true;

  const card = document.createElement("div");
  card.className = "error-card";
  card.setAttribute("role", "alert");

  const title = document.createElement("p");
  title.className = "error-title";
  title.textContent = "出错了";

  const body = document.createElement("p");
  body.className = "error-body";
  body.textContent = message;

  const actions = document.createElement("div");
  actions.className = "error-actions";

  const primary = document.createElement("button");
  primary.type = "button";
  primary.className = "error-action primary";
  primary.textContent = retryable ? "重试" : "关闭";
  primary.addEventListener("click", () => {
    if (retryable) {
      invoke("retranslate");
    } else {
      invoke("hide_window");
    }
  });

  actions.append(primary);
  card.append(title, body, actions);
  translation.innerHTML = "";
  translation.appendChild(card);
  resize();
}

function textParagraph() {
  let paragraph = translation.querySelector("p.text");

  if (!paragraph) {
    paragraph = document.createElement("p");
    paragraph.className = "text";
    translation.replaceChildren(paragraph);
  }

  return paragraph;
}

function appendDelta(piece) {
  current += piece;
  streaming = true;
  const paragraph = textParagraph();
  paragraph.classList.add("streaming");
  paragraph.textContent = current;
  setCopyEnabled(false);
  resize();
}

function finish(translationText) {
  current = translationText;
  streaming = false;
  retryableError = false;
  const paragraph = textParagraph();
  paragraph.classList.remove("streaming");
  paragraph.textContent = current;
  setCopyEnabled(current.length > 0);
  translation.setAttribute("aria-busy", "false");
  const spoken = current.length > 120 ? current.slice(0, 120) + "…" : current;
  announce(spoken ? "翻译完成：" + spoken : "翻译完成");
  speak.hidden = !speakSupported;

  if (
    speakSupported &&
    speakBackend === "spd-say" &&
    ttsEngineValue !== "local"
  ) {
    checkSpeakVoice(languageState.target);
  }

  setDot("ready");
  resize();
}

new ResizeObserver(resize).observe(document.body);

close.addEventListener("click", () => {
  invoke("hide_window");
});

inputTranslate.addEventListener("click", doInputTranslate);
inputRetranslate.addEventListener("click", doInputTranslate);

inputCopy.addEventListener("click", async () => {
  await invoke("copy_text", { text: inputResultText.textContent });
  flashInputSaved();
});

onboardingDone.addEventListener("click", dismissOnboarding);
onboarding.addEventListener("keydown", (event) => {
  if (event.key === "Tab") {
    event.preventDefault();
    onboardingDone.focus();
  } else if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    dismissOnboarding();
  }
});

function setInputMenu(open) {
  inputActionsMenu.hidden = !open;
  inputMore.setAttribute("aria-expanded", String(open));

  if (open) {
    const first = inputActionsMenu.querySelector(
      "button:not([hidden]):not(:disabled)"
    );
    if (first) first.focus();
  }
}

inputMore.addEventListener("click", (event) => {
  event.stopPropagation();
  setInputMenu(inputActionsMenu.hidden);
});

inputActionsMenu.addEventListener("click", (event) => {
  const item = event.target.closest("button");
  if (!item || item.disabled) return;
  // 换个说法 keeps the menu open so its progress text stays visible.
  if (item !== inputAlternate) setInputMenu(false);
});

document.addEventListener("click", (event) => {
  if (
    inputActionsMenu.hidden ||
    inputMore.contains(event.target) ||
    inputActionsMenu.contains(event.target)
  ) {
    return;
  }

  setInputMenu(false);
});

document.addEventListener(
  "keydown",
  (event) => {
    if (event.key !== "Escape" || inputActionsMenu.hidden) return;
    event.preventDefault();
    event.stopPropagation();
    setInputMenu(false);
    inputMore.focus();
  },
  true
);

inputAlternate.addEventListener("click", async () => {
  if (inputAlternateBusy || inputAlternatives.length >= 2 || !inputResultContext) {
    return;
  }

  const context = inputResultContext;
  const generation = inputGeneration;
  inputAlternateBusy = true;
  showInputError("");
  updateInputAlternateButton();

  try {
    const result = await invoke("translate_alternative", {
      text: context.text,
      source: context.source,
      target: context.target,
    });

    if (generation !== inputGeneration) {
      return;
    }

    if (
      !result ||
      result === inputResultText.textContent ||
      inputAlternatives.includes(result)
    ) {
      showInputError("这次没有生成新的译法，再试一次。");
      return;
    }

    inputAlternatives.push(result);
    renderInputAlternatives();
  } catch (message) {
    if (generation === inputGeneration) {
      showInputError(String(message));
    }
  } finally {
    inputAlternateBusy = false;
    updateInputAlternateButton();
  }
});

inputText.addEventListener("input", () => {
  showInputError("");
  updateInputActions();
});

inputClear.addEventListener("click", () => {
  inputText.value = "";
  showInputError("");
  showInputCompose();
  updateInputActions();
  inputText.focus();
});

inputSwap.addEventListener("click", () => {
  const sourceValue = inputSource.value;
  const targetValue = inputTarget.value;
  const detected = languageState.detected || languageState.target;

  if (sourceValue === "auto") {
    if (!detected || detected === targetValue) {
      return;
    }

    inputSource.value = targetValue;
    inputTarget.value = detected;
  } else {
    inputSource.value = targetValue;
    inputTarget.value = sourceValue;
  }

  syncInputLanguages();
  inputText.focus();

  if (!inputResult.hidden) doInputTranslate();
});

for (const select of [inputSource, inputTarget]) {
  select.addEventListener("change", () => {
    if (!inputResult.hidden) doInputTranslate();
  });
}

inputSourcePreview.addEventListener("click", () => {
  showInputCompose();
  updateInputActions();
  inputText.focus();
});

function showOnboardingIfNeeded() {
  if (!isInputWindow) {
    return;
  }

  let seen = false;

  try {
    seen = localStorage.getItem("ot.onboarding.done") === "1";
  } catch {
    seen = false;
  }

  if (seen) {
    return;
  }

  onboarding.hidden = false;
  onboardingDone.focus();
}

function dismissOnboarding() {
  onboarding.hidden = true;

  try {
    localStorage.setItem("ot.onboarding.done", "1");
  } catch {
    // The overlay still closes for this session when storage is unavailable.
  }

  inputText.focus();
}

function showInputHome() {
  setInputMenu(false);
  showView("input");
  inputText.focus();
}

function openInputCompose() {
  inputText.value = "";
  inputResultText.textContent = "";
  inputSourceText.textContent = "";
  inputSaved.hidden = true;
  showInputError("");
  showInputCompose();
  updateInputActions();
  showInputHome();
}

function showInputEntry(entry) {
  if ([...inputSource.options].some((option) => option.value === entry.source)) {
    inputSource.value = entry.source;
  }

  if ([...inputTarget.options].some((option) => option.value === entry.target)) {
    inputTarget.value = entry.target;
  }

  syncInputLanguages();
  inputText.value = entry.text;
  inputSourceText.textContent = entry.text.replace(/\s+/g, " ");
  inputSourcePreview.title = "编辑原文：" + entry.text;
  inputResultText.textContent = entry.translation;
  inputResultText.setAttribute("aria-busy", "false");
  inputSaved.hidden = true;
  showInputError("");
  inputCompose.hidden = true;
  inputResult.hidden = false;
  inputGeneration += 1;
  inputResultContext = {
    text: entry.text,
    source: entry.source,
    target: entry.target,
  };
  clearInputAlternatives();
  setInputMenu(false);
  updateInputActions();
  updateInputSpeak();
  announce("已载入历史翻译");
  showView("input");
}

function showInputCompose() {
  inputGeneration += 1;
  inputResultContext = null;
  clearInputAlternatives();
  setInputMenu(false);
  inputResultText.setAttribute("aria-busy", "false");
  inputCompose.hidden = false;
  inputResult.hidden = true;
}

function clearInputAlternatives() {
  inputAlternatives = [];
  inputAlternateBusy = false;
  inputAlternativesContainer.hidden = true;
  inputAlternativesContainer.replaceChildren();
  updateInputAlternateButton();
}

function updateInputAlternateButton() {
  const full = inputAlternatives.length >= 2;
  inputAlternate.disabled = inputAlternateBusy || full || !inputResultContext;
  inputAlternate.textContent = inputAlternateBusy
    ? "生成中…"
    : full
      ? "已生成 2 个备选"
      : "换个说法";
}

function renderInputAlternatives() {
  inputAlternativesContainer.replaceChildren();

  inputAlternatives.forEach((alternative, index) => {
    const item = document.createElement("div");
    item.className = "input-alternative";

    const head = document.createElement("div");
    head.className = "input-alternative-head";

    const label = document.createElement("span");
    label.className = "input-alternative-label";
    label.textContent = "备选 " + (index + 1);

    const copy = document.createElement("button");
    copy.type = "button";
    copy.className = "input-alternative-copy";
    copy.title = "复制备选译文";
    copy.setAttribute("aria-label", "复制备选译文");
    copy.innerHTML =
      '<svg class="icon-copy" viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"></rect><path d="M5 15V5a2 2 0 0 1 2-2h10"></path></svg>' +
      '<svg class="icon-check" viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"></path></svg>';
    copy.addEventListener("click", async () => {
      await invoke("copy_text", { text: alternative });
      copy.classList.add("copied");
      window.setTimeout(() => copy.classList.remove("copied"), 1200);
    });

    const text = document.createElement("div");
    text.className = "input-alternative-text";
    text.textContent = alternative;

    head.append(label, copy);
    item.append(head, text);
    inputAlternativesContainer.appendChild(item);
  });

  inputAlternativesContainer.hidden = inputAlternatives.length === 0;
  updateInputAlternateButton();

  if (!inputAlternativesContainer.hidden) {
    setInputMenu(false);
  }
}

function updateInputActions() {
  const hasText = inputText.value.trim().length > 0;
  inputTranslate.disabled = !hasText;
  inputClear.disabled = inputText.value.length === 0;
}

function showInputError(message) {
  inputError.textContent = message;
  inputError.hidden = !message;
}

let inputSavedTimer = 0;
function flashInputSaved() {
  inputSaved.hidden = false;
  clearTimeout(inputSavedTimer);
  inputSavedTimer = setTimeout(() => {
    inputSaved.hidden = true;
  }, 1200);
}

let inputGeneration = 0;
let inputAlternatives = [];
let inputAlternateBusy = false;
let inputResultContext = null;

async function doInputTranslate() {
  const text = inputText.value.trim();
  if (!text) {
    inputText.focus();
    return;
  }

  const source = inputSource.value;
  const target = inputTarget.value;
  const generation = ++inputGeneration;
  inputResultContext = null;
  setInputMenu(false);
  clearInputAlternatives();
  inputTranslate.disabled = true;
  inputRetranslate.disabled = true;
  inputTranslate.textContent = "翻译中…";
  inputRetranslate.textContent = "翻译中…";
  showInputError("");
  inputResultText.setAttribute("aria-busy", "true");
  announce("正在翻译");

  try {
    const result = await invoke("translate_input", {
      text,
      source,
      target,
    });

    if (generation !== inputGeneration) {
      return;
    }

    inputResultText.textContent = result.translation;
    inputSourceText.textContent = text.replace(/\s+/g, " ");
    inputSourcePreview.title = "编辑原文：" + text;
    inputCompose.hidden = true;
    inputResult.hidden = false;
    inputResultContext = { text, source: result.source || source, target };
    inputResultText.setAttribute("aria-busy", "false");
    const spoken =
      result.translation.length > 120
        ? result.translation.slice(0, 120) + "…"
        : result.translation;
    announce(spoken ? "翻译完成：" + spoken : "翻译完成");
    updateInputAlternateButton();
    updateInputSpeak();

    if (autoCopyEnabled) {
      await invoke("copy_text", { text: result.translation });
      flashInputSaved();
    }
  } catch (message) {
    if (generation === inputGeneration) {
      inputResultText.setAttribute("aria-busy", "false");
      showInputError(String(message));
      announce("翻译失败：" + message);
    }
  } finally {
    if (generation === inputGeneration) {
      inputTranslate.textContent = "翻译";
      inputRetranslate.textContent = "重试";
      inputRetranslate.disabled = false;
      updateInputActions();
    }
  }
}

document.getElementById("open-history").addEventListener("click", () => {
  openHistory();
});

document.getElementById("open-settings").addEventListener("click", () => {
  openSettings();
});

async function togglePin() {
  pinned = !pinned;
  await invoke("set_pinned", { pinned });
  pin.classList.toggle("active", pinned);
  pin.title = pinned ? "取消固定（取消置顶）" : "固定窗口（保持最前）";
  close.hidden = pinned;
}

pin.addEventListener("click", togglePin);

document.querySelector(".titlebar").addEventListener("dblclick", (event) => {
  if (event.target.closest("button")) return;
  togglePin();
});

speak.addEventListener("click", toggleCardSpeech);
inputSpeak.addEventListener("click", toggleInputSpeech);

const contextMenu = document.getElementById("context-menu");

function hideContextMenu() {
  contextMenu.hidden = true;
}

translation.addEventListener("contextmenu", (event) => {
  if (!current) return;

  event.preventDefault();
  contextMenu.querySelector('[data-action="speak"]').hidden = !speakSupported;
  contextMenu.hidden = false;

  const rect = contextMenu.getBoundingClientRect();
  const x = Math.min(event.clientX, window.innerWidth - rect.width - 8);
  const y = Math.min(event.clientY, window.innerHeight - rect.height - 8);
  contextMenu.style.left = Math.max(4, x) + "px";
  contextMenu.style.top = Math.max(4, y) + "px";
});

contextMenu.addEventListener("click", (event) => {
  const action = event.target.closest("[data-action]")?.dataset.action;
  if (!action) return;

  if (action === "copy") {
    const selected = window.getSelection().toString();
    const text = selected || current;
    if (text) invoke("copy_text", { text });
  } else if (action === "select-all") {
    const paragraph = translation.querySelector("p.text");

    if (paragraph) {
      const range = document.createRange();
      range.selectNodeContents(paragraph);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }
  } else if (action === "speak") {
    toggleCardSpeech();
  }

  hideContextMenu();
});

document.addEventListener("click", hideContextMenu);

// Watcher popups are shown non-focusable (so they never interrupt the user's
// selection); the first click turns the card into a normal focusable window.
document.addEventListener(
  "pointerdown",
  () => {
    invoke("card_engaged");
  },
  { capture: true }
);

copy.addEventListener("click", async () => {
  if (!current) {
    return;
  }

  await invoke("copy_text", { text: current });
  const label = copy.querySelector("span");
  copy.classList.add("copied");
  copy.setAttribute("aria-label", "已复制");
  if (label) label.textContent = "已复制";
  announce("已复制译文");
  window.clearTimeout(copyFeedbackTimer);
  copyFeedbackTimer = window.setTimeout(() => {
    copy.classList.remove("copied");
    copy.setAttribute("aria-label", "复制译文");
    if (label) label.textContent = "复制";
  }, 1500);
});

retranslate.addEventListener("click", () => {
  invoke("retranslate");
});

replace.addEventListener("click", async () => {
  if (!current) {
    return;
  }

  try {
    await invoke("replace_text", { text: current });
    setStatus("已替换");
    window.setTimeout(() => setStatus(""), 1500);
  } catch (message) {
    setStatus(String(message));
  }
});

window.addEventListener("keydown", (event) => {
  // The input window is the primary UI: it hosts the editor plus the
  // settings/history views, so only the card-specific shortcuts are absent.
  if (isInputWindow) {
    if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "h") {
      event.preventDefault();
      openHistory();
      return;
    }

    if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key === ",") {
      event.preventDefault();
      openSettings();
      return;
    }

    if (
      view === "input" &&
      event.target === inputText &&
      event.key === "Enter" &&
      !event.shiftKey &&
      !event.isComposing
    ) {
      event.preventDefault();
      doInputTranslate();
      return;
    }

    if (event.key === "Escape") {
      event.preventDefault();

      if (view === "input") {
        invoke("hide_window");
      } else if (view === "settingsVoicePacks") {
        openSettingsPage("settingsVoice");
      } else if (view === "about") {
        showSettingsHome();
      } else if (view === "settings") {
        showHomeView();
      } else if (view.startsWith("settings")) {
        showSettingsHome();
      } else {
        showHomeView();
      }
    }

    return;
  }

  if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "h") {
    event.preventDefault();
    openHistory();
    return;
  }

  if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key === ",") {
    event.preventDefault();
    openSettings();
    return;
  }

  if (event.ctrlKey && !event.shiftKey && ["1", "2", "3"].includes(event.key)) {
    const target = languageState.recent_targets?.[Number(event.key) - 1];

    if (target) {
      invoke("set_target", { target });
      event.preventDefault();
    }

    return;
  }

  if (event.ctrlKey && event.shiftKey && !event.altKey && event.key.toLowerCase() === "c") {
    if (!copy.disabled) {
      event.preventDefault();
      copy.click();
    }

    return;
  }

  if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key === "Enter") {
    if (!retranslate.hidden || retryableError) {
      event.preventDefault();
      invoke("retranslate");
    }

    return;
  }

  if (event.key !== "Escape") {
    return;
  }

  if (!contextMenu.hidden) {
    hideContextMenu();
    return;
  }

  if (!confirmDialog.hidden) {
    closeConfirm();
    return;
  }

  if (view !== "translator") {
    showView("translator");
  } else {
    invoke("hide_window");
  }
});

const appVersion = document.getElementById("app-version");
const updateStatus = document.getElementById("update-status");
const updateProgress = document.getElementById("update-progress");
const updateNotes = document.getElementById("update-notes");
const updateCheck = document.getElementById("update-check");
const updateAction = document.getElementById("update-action");

let updateState = { phase: "idle", version: "", canInstall: false, message: "" };

function formatUpdateProgress(downloaded, total) {
  const megabytes = (value) => (value / 1000000).toFixed(0);

  if (total && total > 0) {
    return (
      "正在下载更新：" +
      Math.round((downloaded / total) * 100) +
      "%（" +
      megabytes(downloaded) +
      "/" +
      megabytes(total) +
      " MB）"
    );
  }

  return "正在下载更新：已下载 " + megabytes(downloaded) + " MB";
}

function renderUpdate() {
  const { phase, version, canInstall } = updateState;
  const hasVersion = version.length > 0;
  const failed = phase === "install-error" || phase === "check-error";
  const canAct =
    hasVersion &&
    (phase === "available" || phase === "install-error" || phase === "check-error");

  updateNotes.hidden = !hasVersion || phase === "downloading";
  updateNotes.textContent = "更新说明";
  updateCheck.hidden = phase === "downloading" || hasVersion;
  updateCheck.disabled = phase === "checking";
  updateCheck.textContent = phase === "checking" ? "检查中…" : "检查更新";
  updateAction.hidden = !canAct;
  updateAction.textContent =
    phase === "install-error" ? "重试" : canInstall ? "更新到 v" + version : "前往下载";
  updateProgress.hidden = phase !== "downloading";
  updateStatus.hidden = phase === "idle" || phase === "available";
  updateStatus.classList.toggle("error", failed);

  if (phase === "checking") {
    updateStatus.textContent = "正在检查更新…";
  } else if (phase === "latest") {
    updateStatus.textContent = "已是最新版本";
  } else if (phase === "install-error") {
    updateStatus.textContent = "更新失败：" + updateState.message;
  } else if (phase === "check-error") {
    updateStatus.textContent = "检查更新失败：" + updateState.message;
  }

  resize();
}

updateCheck.addEventListener("click", () => {
  updateState = { ...updateState, phase: "checking", message: "" };
  renderUpdate();
  invoke("check_update_now");
});

updateAction.addEventListener("click", () => {
  if (!updateState.canInstall) {
    invoke("open_release_page");
    return;
  }

  updateState = { ...updateState, phase: "downloading", message: "" };
  updateStatus.textContent = "正在准备更新…";
  updateProgress.max = 100;
  updateProgress.value = 0;
  renderUpdate();
  invoke("start_update");
});

updateNotes.addEventListener("click", () => {
  invoke("open_release_page");
});

document.getElementById("settings-back").addEventListener("click", showHomeView);
document.getElementById("history-back").addEventListener("click", showHomeView);

document.getElementById("open-config").addEventListener("click", () => {
  invoke("open_config_dir");
});

const modelInput = document.getElementById("model-input");
const modelReset = document.getElementById("model-reset");
const pickModel = document.getElementById("pick-model");
const pickDisabledApp = document.getElementById("pick-disabled-app");
let savedModelPath = "";
let defaultModelPath = "";

// The input always shows the effective path. An empty value (or the default
// path itself) means "use the default location" and is persisted as empty, so
// the config keeps falling back to the default model.
async function saveModelPath() {
  const value = modelInput.value.trim();
  const useDefault = value === "" || value === defaultModelPath;
  const effective = useDefault ? defaultModelPath : value;

  if (effective === savedModelPath) {
    modelInput.value = effective;
    return;
  }

  await invoke("save_model_path", { value: useDefault ? "" : value });
  savedModelPath = effective;
  modelInput.value = effective;
  modelReset.hidden = useDefault;

  const saved = document.getElementById("model-saved");
  saved.hidden = false;
  window.setTimeout(() => (saved.hidden = true), 1500);
  resize();
}

modelReset.addEventListener("click", async () => {
  modelInput.value = defaultModelPath;
  await saveModelPath();
});

document.getElementById("open-model").addEventListener("click", () => {
  invoke("open_model_location");
});

pickModel.addEventListener("click", async () => {
  try {
    const path = await invoke("pick_model_file");
    if (!path) return;
    modelInput.value = path;
    await saveModelPath();
  } catch (message) {
    setStatus(String(message));
    window.setTimeout(() => setStatus(""), 3000);
  }
});

pickDisabledApp.addEventListener("click", async () => {
  try {
    const appName = await invoke("pick_toolbar_disabled_app");
    if (!appName) return;
    if (
      !toolbarDisabledApps.some(
        (entry) => entry.toLowerCase() === appName.toLowerCase()
      )
    ) {
      toolbarDisabledApps.push(appName);
    }
    renderToolbarDisabledApps();
    resize();
  } catch (message) {
    setStatus(String(message));
    window.setTimeout(() => setStatus(""), 3000);
  }
});

modelInput.addEventListener("blur", saveModelPath);
modelInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    saveModelPath();
  }
});

const DEFAULT_HOTKEY = "Ctrl+Alt+T";
const DEFAULT_INPUT_HOTKEY = "Ctrl+Alt+I";
const DEFAULT_OCR_HOTKEY = "Ctrl+Alt+Z";
const MODIFIER_EVENT_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);
const KEY_LABELS = {
  Control: "Ctrl",
  Meta: "Super",
  " ": "Space",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
};

function hotkeyParts(spec) {
  return spec ? spec.split("+") : [];
}

function keyLabel(event) {
  if (KEY_LABELS[event.key]) {
    return KEY_LABELS[event.key];
  }
  return event.key.length === 1 ? event.key.toUpperCase() : event.key;
}

function specFromEvent(event) {
  const modifiers = [];
  if (event.ctrlKey) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Super");

  if (MODIFIER_EVENT_KEYS.has(event.key)) {
    return { modifiers, key: "" };
  }

  return { modifiers, key: keyLabel(event) };
}

function renderHotkeySheet() {
  const active = activeHotkeyRecorder;

  if (!active) {
    hotkeySheet.hidden = true;
    return;
  }

  hotkeySheet.hidden = false;
  hotkeySheetKeys.replaceChildren();

  const parts = active.state.candidate ? hotkeyParts(active.state.candidate) : [];

  for (const part of parts) {
    const keycap = document.createElement("kbd");
    keycap.textContent = part;
    hotkeySheetKeys.append(keycap);
  }

  if (!parts.length) {
    const placeholder = document.createElement("span");
    placeholder.className = "placeholder";
    placeholder.textContent = active.state.candidate
      ? active.state.candidate
      : "按下新的组合键";
    hotkeySheetKeys.append(placeholder);
  }

  hotkeySheetHint.textContent = active.state.candidate
    ? "再按一次确认 · Esc 取消"
    : "Esc 取消";

  hotkeySheetError.textContent = active.config.error.textContent;
  hotkeySheetError.hidden = active.config.error.hidden;
}

function hideHotkeySheet() {
  activeHotkeyRecorder = null;
  hotkeySheet.hidden = true;
}

hotkeySheetCancel.addEventListener("click", () => {
  activeHotkeyRecorder?.stop();
});

function createHotkeyRecorder(config) {
  const state = { recording: false, candidate: "", current: "" };
  const controller = {
    state,
    config,
    stop() {
      stop();
    },
  };

  function render() {
    config.recorder.classList.toggle("recording", state.recording);

    let parts = [];
    let placeholder = "";

    if (!state.recording) {
      parts = hotkeyParts(state.current);
      if (!parts.length) placeholder = "未设置";
    } else if (state.candidate) {
      parts = hotkeyParts(state.candidate);
      placeholder = "再按一次";
    } else {
      placeholder = "按下新组合键…";
    }

    const nodes = [];
    for (const part of parts) {
      const keycap = document.createElement("kbd");
      keycap.textContent = part;
      nodes.push(keycap);
    }
    if (placeholder) {
      const span = document.createElement("span");
      span.className = "placeholder";
      span.textContent = placeholder;
      nodes.push(span);
    }
    config.recorder.replaceChildren(...nodes);
    config.reset.hidden = !state.recording;
    updateSettingsSummaries();

    if (activeHotkeyRecorder === controller) {
      renderHotkeySheet();
    }

    resize();
  }

  function start() {
    state.recording = true;
    state.candidate = "";
    config.error.hidden = true;
    config.saved.hidden = true;
    activeHotkeyRecorder = controller;
    config.recorder.focus();
    render();
    renderHotkeySheet();
  }

  function stop() {
    state.recording = false;
    state.candidate = "";

    if (activeHotkeyRecorder === controller) {
      hideHotkeySheet();
    }

    render();
  }

  async function apply(spec) {
    config.error.hidden = true;
    config.saved.hidden = true;

    try {
      await invoke(config.saveCommand, { spec });
      state.current = spec;
      state.recording = false;
      state.candidate = "";
      config.saved.hidden = false;
      window.setTimeout(() => (config.saved.hidden = true), 1500);
      config.onApplied?.(spec);

      if (activeHotkeyRecorder === controller) {
        hideHotkeySheet();
      }
    } catch (message) {
      config.error.textContent = String(message);
      config.error.hidden = false;
    }

    render();
  }

  config.recorder.addEventListener("click", start);

  config.recorder.addEventListener("blur", (event) => {
    if (event.relatedTarget === config.reset) return;
    if (state.recording) stop();
  });

  config.recorder.addEventListener("keydown", (event) => {
    if (!state.recording) return;
    if (event.isComposing) return;

    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      stop();
      return;
    }

    if (event.key === "Backspace" || event.key === "Delete") {
      state.candidate = "";
      render();
      return;
    }

    if (event.repeat || MODIFIER_EVENT_KEYS.has(event.key)) return;

    const { modifiers, key } = specFromEvent(event);
    const hasCommandModifier = modifiers.some((modifier) => modifier !== "Shift");

    if (!hasCommandModifier) {
      config.error.textContent = "快捷键需要包含 Ctrl、Alt 或 Super";
      config.error.hidden = false;
      return;
    }

    config.error.hidden = true;

    const next = modifiers.concat(key).join("+");

    if (state.candidate === next) {
      apply(next);
      return;
    }

    state.candidate = next;
    render();
  });

  config.reset.addEventListener("click", () => apply(config.defaultSpec));

  return {
    setCurrent(spec) {
      state.current = spec || "";
      state.recording = false;
      state.candidate = "";
      render();
    },
  };
}

const mainHotkeyRecorder = createHotkeyRecorder({
  recorder: document.getElementById("hotkey-recorder"),
  reset: document.getElementById("hotkey-reset"),
  error: document.getElementById("hotkey-error"),
  saved: document.getElementById("hotkey-saved"),
  defaultSpec: DEFAULT_HOTKEY,
  saveCommand: "save_hotkey",
  onApplied(spec) {
    hotkeyLabel = spec;

    if (view === "translator" && !current && !streaming) {
      showWaiting();
    }
  },
});

const inputHotkeyRecorder = createHotkeyRecorder({
  recorder: document.getElementById("input-hotkey-recorder"),
  reset: document.getElementById("input-hotkey-reset"),
  error: document.getElementById("input-hotkey-error"),
  saved: document.getElementById("input-hotkey-saved"),
  defaultSpec: DEFAULT_INPUT_HOTKEY,
  saveCommand: "save_input_hotkey",
});

const ocrHotkeyRecorder = createHotkeyRecorder({
  recorder: document.getElementById("ocr-hotkey-recorder"),
  reset: document.getElementById("ocr-hotkey-reset"),
  error: document.getElementById("ocr-hotkey-error"),
  saved: document.getElementById("ocr-hotkey-saved"),
  defaultSpec: DEFAULT_OCR_HOTKEY,
  saveCommand: "save_ocr_hotkey",
});

for (const [id, key] of [
  ["switch-updates", "check_updates"],
  ["switch-extension", "serve_extension"],
  ["switch-auto-copy", "auto_copy"],
]) {
  document.getElementById(id).addEventListener("change", (event) => {
    event.target.setAttribute("aria-checked", String(event.target.checked));
    invoke("save_switch", { key, value: event.target.checked });
  });
}

ttsTargetSelect.addEventListener("change", () => {
  ttsTargetValue = ttsTargetSelect.value;
  invoke("save_tts_setting", { key: "tts_target", value: ttsTargetValue });
  if (ttsTargetControl) ttsTargetControl.sync();
  if (speakSupported) speak.title = speakTitle();
  preloadLocalTtsModel();
});

ttsRateSelect.addEventListener("change", () => {
  ttsRateValue = Number(ttsRateSelect.value) || 1;
  invoke("save_tts_setting", { key: "tts_rate", value: String(ttsRateValue) });
  if (ttsRateControl) ttsRateControl.sync();
});

ttsVoiceSelect.addEventListener("change", () => {
  ttsVoiceName = ttsVoiceSelect.value;
  invoke("save_tts_setting", { key: "tts_voice", value: ttsVoiceName });
});

ttsEngineSelect.addEventListener("change", () => {
  ttsEngineValue = ttsEngineSelect.value === "local" ? "local" : "system";
  invoke("save_tts_setting", { key: "tts_engine", value: ttsEngineValue });
  if (ttsEngineControl) ttsEngineControl.sync();
  syncTtsRows();

  if (ttsEngineValue === "local") {
    preloadLocalTtsModel();
  } else {
    invoke("unload_tts_models").catch(() => {});
  }
});

ttsLocalEngineSelect.addEventListener("change", () => {
  ttsLocalEngine =
    ttsLocalEngineSelect.value === "natural" ? "natural" : "fast";
  invoke("save_tts_setting", {
    key: "tts_local_engine",
    value: ttsLocalEngine,
  });
  if (ttsLocalEngineControl) ttsLocalEngineControl.sync();
  syncTtsRows();
  preloadLocalTtsModel();
});

ttsSpeakerZhSelect.addEventListener("change", () => {
  ttsSpeakerZh = Number(ttsSpeakerZhSelect.value) || 3;
  invoke("save_tts_setting", {
    key: "tts_speaker_zh",
    value: String(ttsSpeakerZh),
  });
  previewSpeaker("zh");
});

ttsSpeakerEnSelect.addEventListener("change", () => {
  ttsSpeakerEn = Number(ttsSpeakerEnSelect.value) || 0;
  invoke("save_tts_setting", {
    key: "tts_speaker_en",
    value: String(ttsSpeakerEn),
  });
  previewSpeaker("en");
});

ttsVitsModelDownload.addEventListener("click", () => {
  downloadLocalTtsModel("vits");
});

ttsKokoroModelDownload.addEventListener("click", () => {
  downloadLocalTtsModel("kokoro");
});

ttsSupertonicModelDownload.addEventListener("click", () => {
  downloadLocalTtsModel("supertonic");
});

async function deleteLocalTtsModel(model, label) {
  try {
    const status = await invoke("delete_tts_model", { model });
    applyModelStatus(status);
    syncTtsRows();
    announce(label + "语音包已删除");
  } catch (message) {
    setStatus(String(message));
    announce("删除语音包失败：" + message);
    window.setTimeout(() => setStatus(""), 3000);
  }
}

ttsVitsModelDelete.addEventListener("click", () =>
  deleteLocalTtsModel("vits", "MeloTTS")
);
ttsKokoroModelDelete.addEventListener("click", () =>
  deleteLocalTtsModel("kokoro", "Kokoro")
);
ttsSupertonicModelDelete.addEventListener("click", () =>
  deleteLocalTtsModel("supertonic", "Supertonic")
);

ttsMultiSpeakerSelect.addEventListener("change", () => {
  ttsSpeakerMulti = Number(ttsMultiSpeakerSelect.value) || 6;
  invoke("save_tts_setting", {
    key: "tts_speaker_multi",
    value: String(ttsSpeakerMulti),
  });
  previewSpeaker("multi");
});

ttsMultiPreview.addEventListener("click", () => {
  previewSpeaker("multi");
});

ttsMultiSpeakerSelect.addEventListener("mouseenter", () => {
  scheduleSpeakerPreview("multi");
});
ttsMultiSpeakerSelect.addEventListener("mouseleave", cancelScheduledSpeakerPreview);
ttsMultiPreview.addEventListener("mouseenter", () => {
  scheduleSpeakerPreview("multi");
});
ttsMultiPreview.addEventListener("mouseleave", cancelScheduledSpeakerPreview);
ttsMultiSpeakerSelect.addEventListener("focus", () => {
  scheduleSpeakerPreview("multi");
});

ttsSpeakerZhSelect.addEventListener("mouseenter", () => {
  scheduleSpeakerPreview("zh");
});
ttsSpeakerZhSelect.addEventListener("mouseleave", cancelScheduledSpeakerPreview);

ttsSpeakerEnSelect.addEventListener("mouseenter", () => {
  scheduleSpeakerPreview("en");
});
ttsSpeakerEnSelect.addEventListener("mouseleave", cancelScheduledSpeakerPreview);

function setSwitch(id, checked) {
  const input = document.getElementById(id);
  input.checked = checked;
  input.setAttribute("aria-checked", String(checked));
}

let selectionSavedTimer = null;

function flashSelectionSaved() {
  selectionSaved.hidden = false;
  clearTimeout(selectionSavedTimer);
  selectionSavedTimer = window.setTimeout(() => (selectionSaved.hidden = true), 1500);
  resize();
}

// The master switch mirrors "selection_mode != off"; the method row only
// appears when the feature is on and the platform offers a choice.
function updateSelectionMethodRow() {
  selectionMethodRow.hidden = !(
    !selectionGroup.hidden &&
    (selectionAutoSupported || selectionToolbarSupported) &&
    switchSelection.checked
  );

  updateSettingsSummaries();
}

function updateSettingsSummaries() {
  if (switchSelectionRow) {
    switchSelectionRow.hidden = !selectionSupported;
  }

  if (openTranslationSettings) {
    openTranslationSettings.hidden =
      !selectionSupported || !switchSelection.checked;
  }

  if (settingsModeSummary) {
    const option = selectionMethodSelect.selectedOptions[0];
    settingsModeSummary.textContent = option ? option.textContent : "";
  }

  if (settingsShortcutsSummary) {
    settingsShortcutsSummary.textContent = hotkeyLabel
      ? hotkeyLabel + " 等"
      : "";
  }

  if (settingsVoiceSummary) {
    settingsVoiceSummary.textContent =
      ttsEngineValue === "local" && localTtsSupported ? "本地模型" : "系统语音";
  }

  if (openVoiceSettings) {
    openVoiceSettings.hidden = !speakSupported;
  }

  if (openVoicePacksSettings) {
    openVoicePacksSettings.hidden = !localTtsSupported;
  }

  if (settingsVoicePacksSummary) {
    const installed = [
      ttsKokoroInstalled,
      ttsVitsInstalled,
      ttsSupertonicInstalled,
    ].filter(Boolean).length;
    settingsVoicePacksSummary.textContent = installed
      ? installed + " 个已就绪"
      : "需要下载";
  }
}

function renderToolbarDisabledApps() {
  toolbarDisabledList.replaceChildren();
  toolbarDisabledRow.hidden = false;

  if (toolbarDisabledApps.length === 0) {
    const empty = document.createElement("span");
    empty.className = "dim-text";
    empty.textContent = "未禁用任何应用";
    toolbarDisabledList.append(empty);
  }

  for (const appName of toolbarDisabledApps) {
    const chip = document.createElement("span");
    chip.className = "toolbar-disabled-chip";
    chip.append(appName);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.title = "恢复在此应用显示工具栏";
    remove.setAttribute("aria-label", `恢复 ${appName}`);
    remove.textContent = "×";
    remove.addEventListener("click", async () => {
      await invoke("remove_toolbar_disabled_app", { appName }).catch(() => {});
      toolbarDisabledApps = toolbarDisabledApps.filter((entry) => entry !== appName);
      renderToolbarDisabledApps();
      resize();
    });

    chip.append(remove);
    toolbarDisabledList.append(chip);
  }
}

function applySelectionSettings(settings) {
  selectionModeValue = settings.selection_supported ? settings.selection_mode : "off";
  selectionMethodValue = ["ball", "toolbar", "auto"].includes(settings.selection_method)
    ? settings.selection_method
    : "ball";

  // The backend clamps modes it cannot support; guard here too so the select
  // can never hold a value without a matching option.
  if (!settings.selection_auto_supported && selectionMethodValue === "auto") {
    selectionMethodValue = "ball";
  }

  if (!settings.selection_toolbar_supported && selectionMethodValue === "toolbar") {
    selectionMethodValue = "ball";
  }

  selectionSupported = Boolean(settings.selection_supported);
  selectionAutoSupported = settings.selection_auto_supported;
  selectionBallSupported = settings.selection_ball_supported;
  selectionToolbarSupported = settings.selection_toolbar_supported;
  toolbarDisabledApps = Array.isArray(settings.toolbar_disabled_apps)
    ? settings.toolbar_disabled_apps
    : [];
  removeUnsupportedSelectionOptions(settings);
  selectionGroup.hidden = !settings.selection_supported;
  selectionTitle.hidden = !settings.selection_supported;

  switchSelection.checked = selectionModeValue !== "off";
  switchSelection.setAttribute("aria-checked", String(switchSelection.checked));
  selectionMethodSelect.value = selectionMethodValue;

  if (selectionMethodControl) {
    selectionMethodControl.sync();
  }

  renderToolbarDisabledApps();
  updateSelectionMethodRow();
}

async function applySelectionMode() {
  selectionModeValue = switchSelection.checked ? selectionMethodValue : "off";

  try {
    if (switchSelection.checked) {
      await invoke("save_selection_method", { value: selectionMethodValue });
    }

    await invoke("save_selection_mode", { mode: selectionModeValue });
    flashSelectionSaved();
  } catch (message) {
    setStatus(String(message));
    window.setTimeout(() => setStatus(""), 3000);
  }

  updateSelectionMethodRow();

  if (view === "translator" && !current && !streaming) {
    showWaiting();
  }
}

switchSelection.addEventListener("change", () => {
  switchSelection.setAttribute("aria-checked", String(switchSelection.checked));
  updateSettingsSummaries();
  applySelectionMode();
});

selectionMethodSelect.addEventListener("change", () => {
  selectionMethodValue = selectionMethodSelect.value;
  updateSettingsSummaries();
  applySelectionMode();
});

function updateModelStatus(settings) {
  const path = settings.model_path || settings.default_model_path;
  const name = path ? path.split(/[\\/]/).pop() : "";

  if (settings.model_exists) {
    modelStatus.textContent = "正在使用 · " + name;
  } else {
    modelStatus.textContent = settings.auto_download
      ? "未找到模型文件，启动时会自动下载默认模型。"
      : "未找到模型文件，请在配置文件中设置 model_path 或开启自动下载。";
  }
}

let extensionAddr = "";
let extensionRunning = false;

// The HTTP server starts once at launch, so the switch only takes effect on
// the next start; say so instead of claiming a live state.
function extensionStatusText(enabled) {
  if (!enabled) {
    return extensionRunning ? "已关闭（重启后停止）" : "已关闭";
  }

  return extensionRunning
    ? "运行中 · " + extensionAddr
    : "将在下次启动时运行 · " + extensionAddr;
}

function updateExtensionStatus(settings) {
  extensionStatus.textContent = extensionStatusText(settings.serve_extension);
}

document.getElementById("switch-extension").addEventListener("change", (event) => {
  extensionStatus.textContent = extensionStatusText(event.target.checked);
});

switchAutostart.addEventListener("change", async () => {
  switchAutostart.setAttribute("aria-checked", String(switchAutostart.checked));

  try {
    await invoke("set_autostart", { enabled: switchAutostart.checked });
  } catch (message) {
    setStatus(String(message));
    window.setTimeout(() => setStatus(""), 3000);
    switchAutostart.checked = !switchAutostart.checked;
    switchAutostart.setAttribute("aria-checked", String(switchAutostart.checked));
  }
});

aboutBack.addEventListener("click", () => {
  showSettingsHome();
});
openAbout.addEventListener("click", showAbout);
openTranslationSettings.addEventListener("click", () =>
  openSettingsPage("settingsTranslation")
);
openShortcutsSettings.addEventListener("click", () =>
  openSettingsPage("settingsShortcuts")
);
openVoiceSettings.addEventListener("click", () =>
  openSettingsPage("settingsVoice")
);
openVoicePacksSettings.addEventListener("click", () =>
  openSettingsPage("settingsVoicePacks")
);
openSystemSettings.addEventListener("click", () =>
  openSettingsPage("settingsSystem")
);
settingsTranslationBack.addEventListener("click", showSettingsHome);
settingsShortcutsBack.addEventListener("click", showSettingsHome);
settingsVoiceBack.addEventListener("click", showSettingsHome);
settingsVoicePacksBack.addEventListener("click", () =>
  openSettingsPage("settingsVoice")
);
settingsSystemBack.addEventListener("click", showSettingsHome);

async function applyTtsSettings(settings) {
  if (!settings) {
    return;
  }

  ttsTargetValue = settings.tts_target || "translation";
  ttsRateValue = Math.min(2, Math.max(0.5, Number(settings.tts_rate) || 1));
  ttsVoiceName = settings.tts_voice || "";
  localTtsSupported = Boolean(settings.local_tts_supported);
  ttsEngineValue =
    settings.tts_engine === "local" && localTtsSupported ? "local" : "system";
  ttsLocalEngine =
    settings.tts_local_engine === "natural" ? "natural" : "fast";
  ttsSpeakerZh = Number(settings.tts_speaker_zh) || 3;
  ttsSpeakerEn = Number(settings.tts_speaker_en);
  if (Number.isNaN(ttsSpeakerEn)) ttsSpeakerEn = 0;
  ttsSpeakerMulti = Number(settings.tts_speaker_multi);
  if (Number.isNaN(ttsSpeakerMulti)) ttsSpeakerMulti = 6;
  ttsKokoroInstalled = Boolean(settings.tts_kokoro_installed);
  ttsVitsInstalled = Boolean(settings.tts_vits_installed);
  ttsSupertonicInstalled = Boolean(settings.tts_supertonic_installed);
  ttsModelInstalled = Boolean(settings.tts_model_installed);
  ttsKokoroSize = Number(settings.tts_kokoro_size) || ttsKokoroSize;
  ttsVitsSize = Number(settings.tts_vits_size) || ttsVitsSize;
  ttsSupertonicSize =
    Number(settings.tts_supertonic_size) || ttsSupertonicSize;
  ttsTargetSelect.value = ttsTargetValue;
  ttsRateSelect.value = String(ttsRateValue);
  ttsEngineSelect.value = ttsEngineValue;
  ttsLocalEngineSelect.value = ttsLocalEngine;
  populateTtsVoices(ttsVoiceName);

  if (localTtsSupported) {
    await loadTtsSpeakers();
    populateTtsSpeakers();
  }

  if (ttsTargetControl) ttsTargetControl.sync();
  if (ttsRateControl) ttsRateControl.sync();
  if (ttsEngineControl) ttsEngineControl.sync();
  if (ttsLocalEngineControl) ttsLocalEngineControl.sync();
  syncTtsRows();
  preloadLocalTtsModel();
}

async function applySettingsToUi(settings) {
  mainHotkeyRecorder.setCurrent(settings.hotkey);
  inputHotkeyRecorder.setCurrent(settings.input_hotkey || DEFAULT_INPUT_HOTKEY);
  ocrHotkeyRecorder.setCurrent(settings.ocr_hotkey || DEFAULT_OCR_HOTKEY);
  defaultModelPath = settings.default_model_path || "";
  savedModelPath = settings.model_path || defaultModelPath;
  modelInput.value = savedModelPath;
  modelReset.hidden = !settings.model_path;
  extensionAddr = settings.extension_addr || "";
  extensionRunning = Boolean(settings.extension_running);
  setSwitch("switch-updates", settings.check_updates);
  setSwitch("switch-extension", settings.serve_extension);
  setSwitch("switch-auto-copy", Boolean(settings.auto_copy));
  autoCopyEnabled = Boolean(settings.auto_copy);
  await applyTtsSettings(settings);
  applySelectionSettings(settings);
  updateModelStatus(settings);
  updateExtensionStatus(settings);
  document.getElementById("config-path").textContent = settings.config_path ?? "";
  appVersion.textContent = "当前版本 v" + settings.app_version;
  aboutVersionLabel.textContent = "v" + settings.app_version;

  if (updateState.phase === "idle") {
    const state = await invoke("get_update_state");

    if (state.version) {
      updateState = {
        phase: "available",
        version: state.version,
        canInstall: state.can_install,
        message: "",
      };
    }
  }

  renderUpdate();
}

async function openSettingsPage(name) {
  lastSettingsPage = name;
  showView(name);
  await resize();

  const title = views[name]?.querySelector(".page-title");
  if (title) {
    title.setAttribute("tabindex", "-1");
    title.focus({ preventScroll: true });
  }
}

async function showSettingsHome({ center = false } = {}) {
  showView("settings");
  await resize();
  if (center) await invoke("center_window");

  const returnTarget = {
    settingsTranslation: "open-translation-settings",
    settingsShortcuts: "open-shortcuts-settings",
    settingsVoice: "open-voice-settings",
    settingsSystem: "open-system-settings",
    about: "open-about",
  }[lastSettingsPage];

  lastSettingsPage = "";
  const target = returnTarget && document.getElementById(returnTarget);
  const first = views.settings?.querySelector(".group input, .group button");

  if (target && !target.hidden) {
    target.focus({ preventScroll: true });
    target.scrollIntoView({ block: "nearest" });
  } else if (first) {
    first.focus({ preventScroll: true });
    first.scrollIntoView({ block: "nearest" });
  }
}

async function openSettings() {
  if (!onboarding.hidden) dismissOnboarding();
  const settings = await invoke("get_settings");
  await applySettingsToUi(settings);

  const autostart = await invoke("autostart_enabled").catch(() => false);
  setSwitch("switch-autostart", autostart);

  await showSettingsHome({ center: true });
}

async function showAbout() {
  lastSettingsPage = "about";
  const settings = await invoke("get_settings");
  await applySettingsToUi(settings);
  showView("about");
  await resize();
}

async function openHistory() {
  if (!onboarding.hidden) dismissOnboarding();
  await loadHistory();
  renderHistory();
  showView("history");
  await resize();
  await invoke("center_window");
}

async function loadHistory() {
  historyCache = await invoke("get_history");
}

function renderHistory() {
  const list = document.getElementById("history-list");
  const empty = document.getElementById("history-empty");
  const clear = document.getElementById("history-clear");
  const toolbar = document.getElementById("history-toolbar");
  const emptyTitle = document.getElementById("history-empty-title");
  const emptyHint = document.getElementById("history-empty-hint");
  const query = historyFilter.trim().toLowerCase();

  const rows = historyCache
    .map((entry, index) => ({ entry, index }))
    .filter(({ entry }) => {
      if (historyFavoritesOnly && !entry.favorited) {
        return false;
      }

      if (!query) {
        return true;
      }

      return [entry.text, entry.translation, entry.target, labelOf(entry.target)]
        .join("\n")
        .toLowerCase()
        .includes(query);
    });

  list.replaceChildren();
  toolbar.hidden = historyCache.length === 0;
  empty.hidden = rows.length > 0;
  clear.hidden = historyCache.length === 0;

  if (historyCache.length === 0) {
    emptyTitle.textContent = "暂无翻译记录";
    emptyHint.textContent = "选中文字按 " + hotkeyLabel + " 开始";
  } else {
    emptyTitle.textContent = "没有匹配的记录";
    emptyHint.textContent = historyFavoritesOnly
      ? "取消收藏筛选或换个关键词"
      : "换个关键词试试";
  }

  for (const { entry, index } of rows) {
    const row = document.createElement("div");
    row.className = "history-entry";

    const star = document.createElement("button");
    star.type = "button";
    star.className = "history-star";
    star.textContent = entry.favorited ? "★" : "☆";
    star.title = entry.favorited ? "取消收藏" : "收藏";
    star.setAttribute("aria-label", star.title);
    star.setAttribute("aria-pressed", String(entry.favorited));
    star.addEventListener("click", async () => {
      await invoke("toggle_history_favorite", { index });
      entry.favorited = !entry.favorited;
      renderHistory();
      resize();
    });

    const openButton = document.createElement("button");
    openButton.type = "button";
    openButton.className = "history-open";
    openButton.title = entry.text + "\n" + entry.translation;

    const text = document.createElement("span");
    text.className = "history-text";
    text.textContent = entry.text;

    const time = document.createElement("span");
    time.className = "history-time";
    const relative = relativeTime(entry.at);
    // The language tag only earns its place when it differs from the target
    // the user is translating into right now.
    time.textContent =
      entry.target && entry.target !== languageState.target
        ? relative
          ? entry.target + " · " + relative
          : entry.target
        : relative;

    const translation = document.createElement("span");
    translation.className = "history-translation";
    translation.textContent = entry.translation;

    const copyButton = document.createElement("button");
    copyButton.type = "button";
    copyButton.className = "history-copy";
    copyButton.title = "复制译文";
    copyButton.setAttribute("aria-label", "复制译文");
    copyButton.innerHTML =
      '<svg class="icon-copy" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"></rect><path d="M5 15V5a2 2 0 0 1 2-2h10"></path></svg>' +
      '<svg class="icon-check" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"></path></svg>';
    copyButton.addEventListener("click", async () => {
      await invoke("copy_text", { text: entry.translation });
      copyButton.classList.add("copied");
      window.setTimeout(() => copyButton.classList.remove("copied"), 1200);
    });

    const open = async () => {
      const loaded = await invoke("load_history_entry", { index });

      if (isInputWindow && loaded) {
        showInputEntry(loaded);
      } else {
        showView("translator");
      }
    };

    openButton.addEventListener("click", open);
    openButton.append(text, time, translation);
    row.addEventListener("click", (event) => {
      if (!event.target.closest("button")) {
        open();
      }
    });
    row.append(star, openButton, copyButton);
    list.appendChild(row);
  }
}

document.getElementById("history-search").addEventListener("input", (event) => {
  historyFilter = event.target.value;
  renderHistory();
});

document.getElementById("history-favorites").addEventListener("click", (event) => {
  historyFavoritesOnly = !historyFavoritesOnly;
  event.currentTarget.setAttribute("aria-pressed", String(historyFavoritesOnly));
  renderHistory();
});

function relativeTime(at) {
  if (!at) return "";

  const seconds = Math.max(0, Math.floor((Date.now() - at) / 1000));
  if (seconds < 60) return "刚刚";

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return minutes + " 分钟前";

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return hours + " 小时前";

  const days = Math.floor(hours / 24);
  if (days === 1) return "昨天";
  if (days < 30) return days + " 天前";

  const date = new Date(at);
  return date.getMonth() + 1 + "月" + date.getDate() + "日";
}

document.getElementById("history-clear").addEventListener("click", () => {
  openConfirm(async () => {
    await invoke("clear_history");
    historyCache = [];
    renderHistory();
    resize();
  });
});

const confirmDialog = document.getElementById("confirm-dialog");
let confirmAction = null;

function closeConfirm() {
  confirmAction = null;
  confirmDialog.hidden = true;

  const clear = document.getElementById("history-clear");
  if (clear && !clear.hidden) clear.focus();
}

function openConfirm(action) {
  confirmAction = action;
  confirmDialog.hidden = false;
  document.getElementById("confirm-cancel").focus();
}

document.getElementById("confirm-cancel").addEventListener("click", closeConfirm);

document.getElementById("confirm-accept").addEventListener("click", async () => {
  const action = confirmAction;
  confirmAction = null;
  confirmDialog.hidden = true;

  if (action) await action();
});

confirmDialog.addEventListener("click", (event) => {
  if (event.target === confirmDialog) closeConfirm();
});

confirmDialog.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    closeConfirm();
    return;
  }

  if (event.key !== "Tab") return;

  const cancel = document.getElementById("confirm-cancel");
  const accept = document.getElementById("confirm-accept");

  if (event.shiftKey && document.activeElement === cancel) {
    event.preventDefault();
    accept.focus();
  } else if (!event.shiftKey && document.activeElement === accept) {
    event.preventDefault();
    cancel.focus();
  }
});

// Card events are broadcast to every window; the input window must not react
// to them (they would switch its view and resize it mid-typing).
if (!isInputWindow) {
listen("source", (event) => {
  // A new translation always belongs in the translator view, even when the
  // card currently sits on the history or settings page.
  if (view !== "translator") showView("translator");
  showSource(event.payload.text, event.payload.replaceable === true);
});

listen("delta", (event) => {
  appendDelta(event.payload);
});

listen("done", (event) => {
  finish(event.payload);
});

listen("error", (event) => {
  showError(event.payload.message, lastSource.length > 0);
});

listen("empty", () => {
  showEmpty();
});

listen("model-progress", (event) => {
  showProgress(event.payload.downloaded, event.payload.total);
});

listen("model-ready", () => {
  setDot("ready");

  if (view === "translator" && !streaming && !current) {
    showWaiting();
  }
});

listen("model-error", (event) => {
  showError(event.payload.message, false);
});

}

listen("open-settings", () => {
  openSettings();
});

listen("input-toggle", openInputCompose);
listen("input-show", showInputHome);

listen("selection-mode", (event) => {
  // The tray submenu changed the mode; update the UI from the payload (no
  // config refetch) and do it in every view, so the About page and the
  // waiting hint stay in sync too.
  const mode = event.payload;

  if (mode === "off" || mode === "ball" || mode === "toolbar" || mode === "auto") {
    selectionModeValue = mode;

    if (
      mode === "ball" ||
      (mode === "toolbar" && selectionToolbarSupported) ||
      (mode === "auto" && selectionAutoSupported)
    ) {
      selectionMethodValue = mode;
    }
  }

  switchSelection.checked = selectionModeValue !== "off";
  switchSelection.setAttribute("aria-checked", String(switchSelection.checked));
  selectionMethodSelect.value = selectionMethodValue;

  if (selectionMethodControl) {
    selectionMethodControl.sync();
  }

  updateSelectionMethodRow();

  if (view === "translator" && !current && !streaming) {
    showWaiting();
  }
});

listen("open-history", () => {
  openHistory();
});

listen("language-state", (event) => {
  applyLanguageState(event.payload);
});

listen("history-changed", () => {
  if (view === "history") {
    loadHistory().then(renderHistory);
  }
});

listen("update-checking", () => {
  updateState = { ...updateState, phase: "checking", message: "" };
  renderUpdate();
});

listen("update-available", (event) => {
  updateState = {
    phase: "available",
    version: event.payload.version,
    canInstall: event.payload.can_install,
    message: "",
  };
  renderUpdate();
});

listen("update-none", () => {
  updateState = { phase: "latest", version: "", canInstall: false, message: "" };
  renderUpdate();
});

listen("update-check-failed", (event) => {
  updateState = { ...updateState, phase: "check-error", message: event.payload.message };
  renderUpdate();
});

listen("update-progress", (event) => {
  updateState = { ...updateState, phase: "downloading" };
  updateStatus.hidden = false;
  updateStatus.textContent = formatUpdateProgress(event.payload.downloaded, event.payload.total);

  if (event.payload.total && event.payload.total > 0) {
    updateProgress.max = event.payload.total;
    updateProgress.value = event.payload.downloaded;
  } else {
    updateProgress.removeAttribute("value");
  }

  renderUpdate();
});

listen("update-error", (event) => {
  updateState = { ...updateState, phase: "install-error", message: event.payload.message };
  renderUpdate();
});

if (!isInputWindow) {
  // The selection card is a result surface; settings and history live in the
  // primary input window.
  document.getElementById("open-history").hidden = true;
  document.getElementById("open-settings").hidden = true;
  showWaiting();
  setDot("loading");
  resize();
} else {
  document.body.classList.add("input-window");
  document.getElementById("title").textContent = "输入翻译";
  document.getElementById("pin").hidden = true;
  updateInputActions();
  showInputHome();

  invoke("get_settings")
    .then((settings) => {
      hotkeyLabel = settings.hotkey || hotkeyLabel;
      document.getElementById("onboarding-hotkey").textContent = hotkeyLabel;
      document.getElementById("onboarding-input-hotkey").textContent =
        settings.input_hotkey || "Ctrl+Alt+I";
      document.getElementById("onboarding-ocr-hotkey").textContent =
        settings.ocr_hotkey || "Ctrl+Alt+Z";
    })
    .catch(() => {});

  showOnboardingIfNeeded();
}

initLanguages();

if (!isInputWindow) {
invoke("get_settings").then((settings) => {
  hotkeyLabel = settings.hotkey || hotkeyLabel;
  selectionModeValue = settings.selection_supported ? settings.selection_mode : "off";
  selectionBallSupported = settings.selection_ball_supported;

  if (settings.pinned) {
    pinned = true;
    pin.classList.add("active");
    pin.title = "取消固定（取消置顶）";
    close.hidden = true;
  }

  if (view === "translator" && !current && !streaming) {
    showWaiting();
  }
});

}

Promise.all([
  invoke("tts_backend").catch(() => "webview"),
  invoke("tts_engine_supported").catch(() => false),
  invoke("tts_model_status").catch(() => null),
]).then(([backend, localSupported, modelStatus]) => {
  speakBackend = backend;
  localTtsSupported = Boolean(localSupported);

  if (modelStatus) {
    applyModelStatus(modelStatus);
    ttsModelDownloading = Boolean(modelStatus.downloading);
  }

  updateSpeakSupport();

  if (speakSupported && current) {
    speak.hidden = false;
  }

  populateTtsVoices();

  if (localTtsSupported) {
    loadTtsSpeakers().then(() => populateTtsSpeakers());
  }

  syncTtsRows();
  updateInputSpeak();
  preloadLocalTtsModel();
});

listen("tts-model-progress", (event) => {
  const { model, downloaded, total } = event.payload;
  const percent = total ? Math.floor((downloaded * 100) / total) : null;
  const statusElement =
    model === "vits"
      ? ttsVitsModelStatus
      : model === "supertonic"
        ? ttsSupertonicModelStatus
        : ttsKokoroModelStatus;

  statusElement.textContent =
    percent === null
      ? "下载中 · " + Math.round(downloaded / 1000000) + " MB"
      : "下载中 " + percent + "%";
});

listen("tts-model-status", (event) => {
  const modelStatus = event.payload;
  applyModelStatus(modelStatus);
  ttsModelDownloading = Boolean(modelStatus.downloading);
  syncTtsRows();
});

listen("speech-ended", (event) => {
  if (event.payload.generation !== speechGeneration) {
    return;
  }

  if (pendingSpeechEnd) {
    const resolve = pendingSpeechEnd;
    pendingSpeechEnd = null;
    resolve();
    return;
  }

  stopSpeaking();
});

if ("speechSynthesis" in window) {
  window.speechSynthesis.onvoiceschanged = () => populateTtsVoices();
}

invoke("platform").then((os) => {
  // Windows keeps the window opaque, so the card fills it instead of floating.
  document.body.classList.toggle("os-windows", os === "windows");
  document.getElementById("ocr-hotkey-row").hidden = os === "macos";
  document.getElementById("onboarding-ocr").hidden = os === "macos";
  resize();
});

// The input window is the primary UI, so it is the one that honors
// --settings/--history at startup.
if (isInputWindow) {
invoke("get_initial_view").then((initial) => {
  if (initial === "settings") {
    openSettings();
  } else if (initial === "history") {
    openHistory();
  }
});
}
