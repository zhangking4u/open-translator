# Live Subtitle Translation (实时字幕翻译) — Research & Design Intent

Status: research and design intent, 2026-10-04. Nothing is implemented — the
tree has no audio capture, VAD or ASR code. Scope was decided with the
maintainer on 2026-10-04: **single-direction comprehension, overlay captions,
strictly local, Linux first**. This document is the reference for the feature;
it does not commit to a schedule.


## 1. Goal

Turn any audio playing on the machine — meeting apps, browsers, players — into
an always-on-top, two-line subtitle overlay whose second line is the target
language, with every stage (capture, ASR, translation) running on the local
machine.

The original motivation is meeting interpretation, but the capability is
deliberately general: anything that plays audio can be captioned and
translated. The system must not be bound to a specific meeting application.

In scope (v1):

- System output capture (PipeWire monitor on Linux) and local streaming ASR.
- Local translation through the existing llama.cpp engine (HY-MT GGUF).
- A non-focusable, always-on-top subtitle window with provisional/final states.
- Personal glossary and history export (trust features, see §6.5).

Out of scope (v1):

- Microphone and in-person conversation mode.
- Two-way interpretation: TTS, voice cloning, virtual-microphone output.
- Any cloud API, account system or telemetry.
- Platform integrations (meeting SDKs, bots joining meetings).


## 2. First principles

### 2.1 The need is comprehension, not translation

The user does not lack "a translation"; they cannot keep up with spoken content
in a language they do not understand, while it is happening. Subtitles and
translation are mechanisms, not the goal. Everything below is derived from that
sentence.

### 2.2 Three constraints that cannot be engineered away

1. **Meaning unfolds in time.** A sentence cannot be translated before enough
   of it has been heard. Any real-time system has a strictly positive latency
   floor; the only choice is how to allocate it.
2. **An unfinished sentence has no unique translation.** A real-time system must
   bet: emit a translation under uncertainty and revise it later. Errors are
   normal, not exceptional. Therefore the first-class problem is *commitment and
   revision* — when a unit is frozen, and how the UI communicates that it may
   change.
3. **Reading is faster than speech.** Reading runs at roughly 300–500 characters
   per minute against 120–180 spoken words per minute. Rendering is never the
   bottleneck; pipeline latency and revision churn are. Captions also compete
   with the original audio for the same comprehension channel.

Consequence: this is a **scheduling and interaction problem with a model inside
it**, not a model problem.

### 2.3 The four-corner trade-off

Any system can hold at most three of: **latency, quality, coverage
(languages/domains), cost & privacy**. Cloud vendors hold quality, coverage and
latency, and pay with privacy and marginal cost. "Strictly local" fixes cost to
the user's hardware, so one of latency / quality / coverage must give. For an
always-available tool the answer is: **coverage may be narrow, quality may be
second-best, latency must not collapse**.

The product promise is therefore not "the best translation", but **always
present, offline, zero marginal cost, customizable**.

### 2.4 Existence check

If the user is happy to use 豆包, OS-provided captions or a meeting platform's
built-in feature, this feature has no reason to exist. It earns its place only
by satisfying the constraints incumbents will not: strict offline operation,
Linux support, Chinese-first coverage, and domain customization. If those
constraints stop mattering, the correct answer is to not build it — not to
build a slightly better clone.


## 3. Functional decomposition

Each layer is defined by a first-principles question, not by a product feature.

| Layer | First-principles question | Derived answer |
| --- | --- | --- |
| Capture | Which sound does the user need to understand, with the least permission surface? | System output loopback is the common denominator; per-app capture is the precise answer later; microphone is a separate mode |
| Routing | When should the system not work at all? | VAD plus language ID: silence is not transcribed, source==target is not translated, compute is spent only where information changes |
| Segmentation | What is a translatable unit, and when is it frozen? | Clauses / stable prefixes; text has provisional and committed states, and the UI must show the difference |
| Recognition | Where do meeting errors actually come from? | Proper nouns and terminology first, accents second, model size third — a glossary beats a bigger model |
| Translation | How do spoken fragments hurt MT? | Short units lack context; constraints (glossary, style, context carry-over) affect readability more than model scale |
| Presentation | Is the source text kept? | A trade-off between recoverability and double reading load; bilingual vs translation-only is a per-scenario setting |
| Retention | Is a caption disposable or an asset? | Captions are disposable, meetings are searchable assets; retention policy drives the privacy architecture |


## 4. Market landscape (2026-10)

Recorded for context. Feature sets and pricing change quickly; entries marked
"not re-verified" come from prior knowledge rather than a fetched official
source.

### 4.1 Meeting platforms

| Product | Live translation | Notes |
| --- | --- | --- |
| 腾讯会议 | 17 languages (2024-08) + AI 同传 (2026: EN/ZH, self-listen only, voice cloning, free tier usable) | Strongest domestic option; bound to the platform |
| Microsoft Teams | 30+ translation languages (official docs) | Requires Teams Premium or Copilot license |
| Zoom | AI Companion translated captions, paid add-on (not re-verified) | Cloud |
| Google Meet | Translated captions, Workspace paid tiers (not re-verified) | Cloud |
| 飞书妙记 / 飞书会议 | Real-time captions, CN/EN/JP translation, speaker labels (search snapshot) | Bound to Feishu |

### 4.2 OS-level captions

| System | Capability | Limitation |
| --- | --- | --- |
| Windows 11 | `Win+Ctrl+L` captions any system audio; Copilot+ NPU translation 44 languages → English, Snapdragon also → Simplified Chinese (26100.3624, 2025-03) | Translation requires Copilot+ hardware; target languages limited |
| macOS | Live Captions on system audio and microphone, fully on-device (official docs) | Apple Silicon only; **no translation** |
| iOS 26 + AirPods | Live Translation in Messages/FaceTime/Phone, on-device | One-to-one calls only; not arbitrary app audio |
| Android / Pixel | Live Caption + Live Translate, on-device | Device-media oriented, language/device limited |
| Chrome | Live Caption, on-device | Captions only, **no translation** |

### 4.3 Standalone tools and hardware

- **豆包电脑版**: real-time bilingual subtitles, AI video viewing, ~60-language
  text translation, free; closed source, cloud, not extensible.
- **讯飞**: 讯飞听见 (real-time transcription + multilingual translation),
  讯飞同传 (9 languages, floating web subtitles), 讯飞译制 (14 languages,
  dubbing/voice cloning).
- **通义听悟 / 网易见外 / Otter / Fireflies / tl;dv / Fathom**: transcription
  and meeting-assistant products, mostly cloud, real-time translation varies.
- **Video consumption**: 沉浸式翻译, Trancy, Language Reactor — bilingual
  subtitles for YouTube/Netflix, not system audio.
- **Hardware**: Timekettle, Pixel Buds conversation mode, Samsung Galaxy Buds +
  Galaxy AI, AirPods Live Translation — 2–5 s latency, phone/cloud dependent.

### 4.4 Open source (GitHub API, 2026-10-04)

| Project | Stars | Stack | Position |
| --- | --- | --- | --- |
| WhisperLiveKit | 11.1k | Python | Local streaming ASR + diarization + translation + compatible API |
| LiveCaptions-Translator | 3.8k | C# | Windows Live Captions → translation overlay + history |
| kyutai-labs/hibiki | 1.5k | Rust | Streaming (simultaneous) speech translation model |
| sokuji | 1.4k | TS/Electron + extension | Two-way meeting translation for Zoom/Meet/Teams, cloud or offline |
| phuc-nt/my-translator | 1.3k | Python/Rust/Tauri | macOS/Windows real-time speech translation |
| hayamimi (早耳) | 347 | Python/sherpa-onnx | CPU-only multilingual captions + translation |

Building blocks: whisper.cpp, faster-whisper, sherpa-onnx (SenseVoice /
Paraformer / Zipformer), Silero VAD, RealtimeSTT.

### 4.5 The gap

1. Cross-application, system-level subtitle translation: Windows needs
   Copilot+ hardware and offers few target languages; macOS has no translation;
   Linux is empty.
2. Strictly local, offline, private: incumbents are cloud; open-source projects
   are developer tools with rough UX.
3. Chinese-first meeting interpretation outside a specific meeting platform.
4. Terminology and domain customization: platform products do not offer it.

These four are the product's reason to exist and should be treated as
requirements, not nice-to-haves.


## 5. Options considered

### 5.1 Cascade vs end-to-end

| | Cascade (ASR → MT) | End-to-end streaming ST |
| --- | --- | --- |
| Controllability | High, each stage replaceable | Low |
| Typical latency | 1.5–3 s final, ~1 s to first translated token | 1–2 s research, 2–4 s product |
| Language pairs | Product of ASR and MT coverage | Fixed at training time |
| Customization | Glossary, prompt, style | None |
| Fit with this repo | High — MT layer already exists | Would need a second inference stack |

Decision: **cascade for v1.** Revisit end-to-end only for a future
low-latency mode.

### 5.2 ASR

| Option | Verdict | Reason |
| --- | --- | --- |
| sherpa-onnx + SenseVoice / Zipformer | Preferred | True streaming, fast on CPU, good zh/en, punctuation; onnxruntime is a simpler packaging dependency than a full GPU stack |
| whisper.cpp (`whisper-rs`) | Fallback | Large ecosystem, MIT, llama.cpp toolchain already present; streaming is chunked with more latency jitter on long sentences |
| Cloud ASR | Rejected | Contradicts "strictly local" |

M0 measured (2026-10-04, Linux dev machine, 16 cores, no GPU offload):
SenseVoice int8 decodes real speech at RTF ~0.022 (7.15 s clip in 161 ms,
20-run average; ~0.09 CPU-seconds per audio-second across 4 threads), and the
real-speech en/zh test clips decode accurately (the en clip heard "gold" as
"code", one word off). Model acquisition route: the sherpa-onnx mirror on
Hugging Face through `https://hf-mirror.com` with `HF_HUB_DISABLE_XET=1` (the
ModelScope API 404s for this mirror, a plain gh-proxy GET hung, and HEAD is not
enough to download). No streaming Zipformer was tested because final-only
latency already passes.

M1 implementation: the official `sherpa-onnx` Rust crate (1.13.8) with
**shared** libraries. The prebuilt *static* archive aborts with
`free(): invalid pointer` inside `onnxruntime::GetPciBusId` once the Tauri
binary links the other static runtimes in this process; the shared `.so` files
(`libsherpa-onnx-c-api.so`, `libsherpa-onnx-cxx-api.so`, `libonnxruntime.so`)
isolate that C++ runtime and fix it. `sherpa-onnx-sys` copies the libraries
next to the produced binary, the client links with `-Wl,-rpath,$ORIGIN`
(`build.rs`), and `packaging/linux/make-deb.sh` installs them into
`/usr/lib/open-translator/` next to the executable.

### 5.3 Capture

PipeWire monitor source on Linux (the default sink's `.monitor`; PipeWire's
Pulse server exposes it through the Pulse API). P0 uses a subprocess
producing 16 kHz mono s16 — consistent with the project's existing subprocess
style (`wl-paste`, `spd-say`). Verified on the dev machine (PipeWire 1.6.2,
no PulseAudio client tools installed): `pw-record --target
@DEFAULT_AUDIO_SINK@ --rate 16000 --channels 1 --format s16` links to the
sink's monitor ports and captures playback. Note: `pw-record -n <samples>`
exits with status 1 after writing the requested samples — treat rc=1 with a
complete file as success. P1 can move to `libpulse-binding` or `pipewire-rs`
for device enumeration and hot switching. Failure modes to handle from the
start: multiple output devices (USB headset vs speakers), muted sink, and
PipeWire setups without a monitor.

### 5.4 Overlay window

Reuse the Tauri window infrastructure. On GNOME Wayland, native windows cannot
reliably keep above or be placed by a token-less background app, so the
existing `prefer_x11_backend` policy applies to the subtitle window too. GTK
layer-shell would be the native Wayland fix but is out of scope for v1.

M1 found two implementation traps: a newly mapped XWayland window can still
sit below the active native window with always-on-top set, so every segment
toggles the flag to force a restack; and click-through must be applied only
after `show()`, because tao panics (`window.window().unwrap()` on `None`) when
the GTK widget is not realized yet.

### 5.5 Where the code lives (as implemented)

- MT is already available in-process: `desktop/translator-popup-tauri` depends
  on `translator-service` (Cargo.toml), so `EngineRef`, `LlamaCppEngine` and
  `TimeoutEngine` can be reused directly.
- ASR lives in `core/asr` (package `translator-asr`): a **synchronous**
  `SpeechEngine` trait (`fn transcribe(&self, samples: &[f32], sample_rate:
  i32) -> Result<Transcript, SpeechError>`) plus a `VoiceSegmenter` wrapper
  over Silero VAD; the caller runs both on a blocking worker. Final-segment
  decoding does not need the boxed-future shape of `TranslationEngine`, and
  the simpler trait is easier to test. A separate crate keeps the heavy
  sherpa/onnxruntime dependency out of `translator-core`, whose tests run on
  all three CI platforms. The desktop depends on it Linux-only
  (`[target.'cfg(target_os = "linux")'.dependencies]`) so Windows/macOS builds
  do not link onnxruntime.
- Captions live in the desktop client: `src/caption.rs` owns the `pw-record`
  child, the VAD/ASR workers and the `caption-segment` / `caption-status`
  events; `ui/caption.{html,css,js}` renders them. New window labels must be
  added to `capabilities/default.json` or their `listen()` calls are rejected
  by the Tauri ACL — this was the M1 bug that made events invisible.
  Caption translation runs in the same module: a single-flight worker streams
  `caption-translation` deltas from the embedded llama.cpp engine (newest text
  wins) and `caption-config` carries the layout; the glossary file
  (`caption_glossary` or `glossary.txt`) is read on every translation. Missing
  ASR/VAD files are downloaded before capture starts (progress through
  `caption-status` state `downloading`; `auto_download = false` disables it).
- Do not add audio to `translator-service` until a second consumer exists
  (the browser extension's tab audio is the natural candidate, through
  `src/server.rs`).


## 6. Chosen design

### 6.1 Pipeline

```
PipeWire monitor ──► VAD / segmentation ──► local streaming ASR
        │                                        │
        │                                  partial / stable
        │                                        ▼
        └──────────────────────────────► commitment policy
                                                 │
                                                 ▼
                              local MT (HY-MT 1.8B GGUF, streaming)
                                                 │
                                                 ▼
                              subtitle overlay: provisional → final
```

### 6.2 Event model

The pipeline is an event stream; every renderer (desktop overlay now, extension
or third parties later) consumes the same events:

| Event | Meaning | UI |
| --- | --- | --- |
| `partial` | Unstable ASR hypothesis | internal only (or source line when enabled) |
| `stable` | Prefix that will not change (local agreement) | may start MT early |
| `translated` | MT tokens for a committed segment, streamed | second line, provisional styling |
| `revised` | Replacement text for the same segment | in-place update with a visible cue |
| `final` | Segment closed, translation complete | normal styling |

### 6.3 Latency budget

| Stage | Estimated | M0 measured (2026-10-04) |
| --- | --- | --- |
| Capture buffer | 20–50 ms | 100 ms read granularity in the spike |
| VAD tail silence | 200–400 ms | endpointing p50 384 ms / p95 512 ms (31-minute run) |
| ASR (SenseVoice int8, 4 threads) | 100–500 ms per segment | 73–197 ms for 2.7–9 s segments (RTF ~0.022) |
| Commitment | 0–300 ms | not implemented in M0 |
| MT first token (HY-MT 1.5 1.8B q4, in-process llama.cpp) | 100–500 ms | 240–450 ms |
| MT segment done | — | 0.5–0.9 s short requests |
| **Total, end of speech → first translated token** | **~0.8–1.5 s** | **p50 0.82 s / p95 0.99 s / max 1.19 s** (31-minute run) |
| **Total, end of speech → full translation** | **1.5–3 s** | **p50 1.20 s / p95 1.66 s / max 2.17 s** (31-minute run) |

Local models will not match cloud latency, and the MT cost is CPU-bound: one
~100-character request measured ~0.9 s wall and ~3.3 CPU-seconds in the client
process, against ~0.09 CPU-seconds per audio-second for ASR. Thread count,
quantization and optional GPU offload are the main power levers, and this
premium is the price of the privacy/offline constraint.

**Perceived latency is time-to-first-translated-token, not
time-to-complete-sentence.** `core/inference` already exposes
`generate_streaming(..., on_delta)`, and `TranslationEngine::translate_streaming`
already carries a `DeltaCallback`; the overlay must stream MT tokens into the
second line instead of waiting for the full translation.

### 6.4 Segmentation and context

- VAD (Silero) establishes speech bounds; a force-cut limits the maximum unit
  (target ~6 s or an equivalent character budget) so long monologues cannot
  push latency to double digits.
- The previous one or two final segments are carried as context into the MT
  prompt to reduce pronoun and terminology drift.
- ASR output must carry punctuation; segmentation quality dominates
  translation quality for subtitles.

### 6.5 Revision semantics and trust

- Provisional text is rendered de-emphasized (lower contrast); committing
  switches it to normal styling. Never replace text silently.
- A personal glossary (proper nouns, product names, domain terms) is a trust
  feature, not an enhancement; it should land before any attempt at a bigger
  model.
- History and export reuse `translator-core::history`; retention stays local.

### 6.6 Integration seams (verified against the tree)

- `desktop/translator-popup-tauri/Cargo.toml` already depends on
  `translator-service` (line 20) and `translator-core`; embedded inference is
  an existing pattern in the client.
- The new `SpeechEngine` trait must stay dyn-compatible (boxed futures,
  `Send + Sync` supertraits) exactly like `TranslationEngine`
  (`core/translator-service/src/engine/mod.rs:24`) so it can live behind an
  `Arc<dyn ...>`.
- Model downloads reuse `translator-core::models` (ModelScope URL + SHA-256 +
  Range resume); ASR models are new entries, not new machinery. Model sizes:
  ASR roughly 100–500 MB, MT roughly 1.1 GB — download on first use, never
  bundle into the installer.
- The settings UI reuses the shared `--ot-*` tokens and `ui/segmented.js`; the
  subtitle window reuses the existing window/positioning code and the X11
  preference on Wayland.
- Logging must follow the existing rule: never log raw audio or raw text.

### 6.7 Implemented ASR surface

```rust
pub trait SpeechEngine: Send + Sync {
    fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: i32,
    ) -> Result<Transcript, SpeechError>;

    fn name(&self) -> &'static str;
}

pub struct SenseVoiceEngine { /* OfflineRecognizer */ }

impl SenseVoiceEngine {
    pub fn load(
        model: &Path,
        tokens: &Path,
        language: &str,
        num_threads: i32,
    ) -> Result<Self, SpeechError>;
}

pub struct VoiceSegmenter { /* Silero VAD */ }

impl VoiceSegmenter {
    pub fn new(
        model: &Path,
        sample_rate: i32,
        min_silence_duration: f32,
        max_speech_duration: f32,
    ) -> Result<Self, SpeechError>;

    /// Feed mono 16 kHz samples; returns every finished segment.
    pub fn accept(&self, samples: &[f32]) -> Vec<Vec<f32>>;
    pub fn flush(&self) -> Vec<Vec<f32>>;
}
```


## 7. Milestones and acceptance

| Milestone | Deliverable | Acceptance |
| --- | --- | --- |
| M0 smoke — **done 2026-10-04** | `pw-record` → energy VAD → SenseVoice → local MT → terminal output, no UI | 31-minute real video run: 494 segments, 0 dropped, 0 MT failures; end of speech → first translated token p50 0.82 s / p95 0.99 s / max 1.19 s; → full translation p50 1.20 s / p95 1.66 s / max 2.17 s; ASR RTF 0.022; CPU recorded |
| M1 captions — **done 2026-10-05** | Subtitle window showing source-language captions | Real-machine verified: captions render, click-through works, no focus steal, tray toggle works. Position is bottom-center (config override, no drag UI yet); the 30-minute soak remains part of normal daily use |
| M2 translation — **done 2026-10-05** | Second line via HY-MT streaming | Real-machine verified: translation line streams, bilingual/translation-only/source layouts switch from the tray, and `glossary.txt` terms reach the request. Measured 411–606 ms to first translated token and 1.16–2.49 s per segment (from ASR completion) |
| M3 trust — **done 2026-10-05** | Provisional/final states, user corrections, personal glossary, history export | Real-machine verified: `···` placeholder while waiting, dimmed streaming → solid final translation, tray 编辑术语表… (creates/opens `glossary.txt`, applied live), tray 打开字幕记录 (per-session transcript under `~/.local/share/open-translator/captions/`). The 30-minute experiential soak continues as daily use |

M0 result (2026-10-04, Linux dev machine, spike scripts in `/tmp/kilo`): a
real-speech clip produced two VAD segments with 384 ms endpointing, 415–446 ms
from segment close to the first translated token and 665–721 ms to a finished
segment; short standalone MT requests ran at 240–255 ms to first token. The
31-minute video run confirmed it at scale: 494 segments over 25 minutes of
speech, 0 dropped and 0 MT failures, no drift across quartiles (Q1 848 ms →
Q4 804 ms to first token). Spot checks showed good translation quality; the
observed failure modes were word-level MT glitches (an untranslated
"presidency") and proper-noun ASR errors, which supports the glossary-first
plan. Caveats: the run used clean lecture video audio rather than an
interactive meeting (crosstalk, compressed microphones), and MT, not ASR,
dominates CPU. M0 is deliberately UI-free: the pipeline numbers decide whether
the feature is viable before any interface work. The real product acceptance
is M3 — the technology is not the risk; sustained trust is.

M1 result (2026-10-05, Linux dev machine, GNOME Wayland/XWayland): the client
now links `translator-asr` (Linux only) and adds a tray 实时字幕 check item,
`src/caption.rs` (`pw-record` → Silero VAD → SenseVoice → events) and a
click-through, non-focusable `caption` overlay whose text fades after 6 seconds
of silence. Two implementation traps were found on real hardware and fixed:
tao panics when `set_ignore_cursor_events` runs before the window is realized
(apply it after `show()`), and `listen()` in a new window is rejected until the
window label is added to `capabilities/default.json`. Packaging ships the
shared sherpa/onnxruntime libraries next to the binary and relies on the
`$ORIGIN` runpath. Not done yet: the app does not download the ASR model
(models were placed manually for M1), and the overlay has no drag UI —
position comes from `caption_x`/`caption_y` or the bottom-center default.

M2 result (2026-10-05): finished segments now run through the embedded
llama.cpp engine and stream into the second line (`caption-translation`
events; `caption-config` carries the layout). The tray 实时字幕 submenu holds
开启实时字幕 + 双语字幕/仅译文/仅原文, persisted as `caption_layout`. A simple
glossary (`caption_glossary` or `glossary.txt` next to the config, `source=target`
per line, max 50 terms) is appended to the prompt of every style; the runtime
log confirmed terms reach the request. Measured from ASR completion: first
translated token 411–606 ms, full segment 1.16–2.49 s (English → Korean with a
glossary term, `auto` source). Real-machine checks passed: translation line,
layout switching, no focus/click-through regressions.

M2 follow-up (2026-10-05): missing SenseVoice/VAD files are now downloaded on
first caption start (`translator-core::models::ASR_MODEL_FILES`; SHA-256
verified, resume and skip-if-valid supported, progress shown in the overlay).
Measured 240 MB at ~7.7 MB/s through `hf-mirror.com`; `auto_download = false`
keeps the "place the files yourself" behaviour. Real-machine check: deleted the
model directory, enabled captions, and download → capture → translation came up
without any manual step.

M3 result (2026-10-05): trust features landed. The overlay distinguishes
provisional from final text — a `···` placeholder while a segment waits for the
first translated token, a dimmed translation while tokens stream, and full
opacity once `done` arrives. Corrections flow through the personal glossary:
the tray 编辑术语表… item creates `glossary.txt` from a template on first use
and opens it, and edits apply to the next segment because the file is re-read
on every translation. Every finalized pair is appended to a per-session
transcript (`~/.local/share/open-translator/captions/captions-<epoch>.txt`,
relative `HH:MM:SS` stamps, source and translation lines) and the tray
打开字幕记录 item opens the folder. Real-machine checks passed on all four
points; the experiential acceptance (a 30-minute session used to the end)
remains a daily-use check rather than an automated one.


## 8. Risks

| Risk | Mitigation |
| --- | --- |
| Local model terminology errors are structural | Personal glossary + correction flow before larger models |
| Wayland-native keep-above/placement limits | Keep the X11-preferred policy; document the limitation |
| ASR + MT together on CPU (heat, fan, battery). M0 measured MT at ~3.3 CPU-seconds per ~100-character request vs ~0.09 CPU-seconds per audio-second for ASR | VAD gating (both models idle on silence), thread-count tuning, quantization, optional GPU offload |
| PipeWire monitor discoverability (headset vs speakers) | Device selector + health check; microphone mode as a separate future feature |
| Model download size | First-use download through the existing downloader |
| Existence risk (OS vendors ship the same thing) | The four constraints in §4.5 are the product; if they stop mattering, stop building |


## 9. Open questions

1. ASR model distribution — **done 2026-10-05**: `translator-core::models`
   carries `ASR_MODEL_FILES` (SenseVoice int8 + `tokens.txt` from
   `hf-mirror.com`, Silero VAD from the sherpa-onnx GitHub release with a
   gh-proxy fallback) with SHA-256 verification, resume and skip-if-valid; the
   caption start downloads missing files and streams progress to the overlay.
2. Overlay placement: a click-through window cannot be dragged — decide
   between drag support (temporarily re-enabling input) and a settings control
   for `caption_x`/`caption_y`.
3. Source language: `caption_language` defaults to `auto`; evaluate `auto` vs a
   fixed language for accuracy and latency now that the pipeline runs in-app.
4. Glossary: `source=target` files landed in M2 and the edit-from-tray flow in
   M3; per-language termbases, priorities and an in-overlay correction UI
   remain open (the overlay is click-through by design).
5. Per-app capture timing (capture only the meeting app's stream) — later than
   default-sink capture, but more precise.
6. Layouts landed as bilingual/translation/source; the default stays bilingual
   — revisit after real usage by someone who cannot read the source.
7. Model licensing and redistribution checks for SenseVoice/Silero before
   packaging.
8. Whether the browser extension should capture tab audio directly (second
   consumer) instead of relying on system loopback.

## Sources and confidence

- Verified from official sources on 2026-10-04: Microsoft Teams live captions
  (30+ translation languages, Premium/Copilot), Apple macOS Live Captions
  (on-device, Apple Silicon, no translation), iOS 26 Live Translation scope,
  讯飞听见 product pages, Windows Live Captions translation reporting
  (26100.3624 / 2025-03), GitHub repository data.
- Chinese product details (豆包 desktop, 腾讯会议 17 languages and AI 同传,
  飞书妙记) come from 2024–2026 third-party snapshots with dates and should be
  re-checked before any positioning decision.
- Zoom, Google Meet and Webex details are not re-verified and are marked as
  such above.
