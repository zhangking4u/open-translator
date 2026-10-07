# Browser Screenshot Translation (扩展截图翻译) — Design Intent

Status: decided 2026-10-06, revised 2026-10-07. This is **D1** of the
multimodal roadmap. The first form (whole-page screenshot overlay in the
browser) was removed after the first manual check (§5.6); the first image
consumer is now 翻译此图片 (right-click image → panel), the desktop
region-screenshot → card flow is next (B), and system-level UI translation is
deferred. The OCR stack (§5.1–§5.4) is implemented and validated; web reading
translation is evaluated separately in `docs/READING_TRANSLATION.md` (C).

## 1. Goal

Translate what is on the page without selecting text. The user triggers the
feature (command / context menu), the extension captures the visible tab, OCR
and the existing MT engine run locally, and the translations render in the
page as an overlay (per-block labels) or a panel. The page's own text is not
modified; the screenshot never leaves the machine; capture is always
user-triggered.

## 2. Why the extension first

| | Extension screenshot | Desktop region capture | System UI overlay |
| --- | --- | --- | --- |
| Overlay positioning | DOM, native | not needed (card) | hardest; Wayland cannot place it |
| Platform risk | ~none (in-browser) | medium (3 capture APIs) | high (macOS permission, games) |
| Reuses | port/streaming/history/UI | card/ball/history | almost everything new |
| Forces in the core | image endpoint + OCR provider + packaging | same | overlay layer |
| Real-machine check | Linux dev machine | Linux possible | macOS impossible here |
| Order | **1** | **2** | **3 (deferred)** |

The extension path walks the riskiest groundwork (image endpoint, OCR
provider, runtime packaging) through the smallest platform surface, and the
product itself (private, local web-page screenshot translation) stands on its
own.

Deferred consumers:

- **Desktop region screenshot → card**: global hotkey → system capture
  (Wayland: `org.freedesktop.portal.Screenshot` interactive selection) → OCR →
  existing MT → the existing card/history. Second image consumer; reuses
  everything after body 1 lands.
- **System-level UI translation** (ChatGPT's Level 2/3): UIA/AX/OCR text +
  boxes → translated overlay over the source UI. Windows/macOS first; on
  GNOME Wayland a token-less background app cannot position overlays, so the
  project's platform-honesty rule applies. Parked until Windows real-machine
  availability.

## 3. Pipeline

```
user gesture (command / context menu)
        │  activeTab
        ▼
tabs.captureVisibleTab → PNG data URL
        │  existing background port
        ▼
local service  POST /translate/image
        │
        ▼
OCR: text + box ──► existing MT (batch, glossary)
        │
        ▼
content script: overlay in the page DOM
```

OCR first, not VLM: OCR returns the boxes the overlay needs and is small/fast
on CPU. A VLM is a later enhancement for OCR-failure fallback and whole-image
understanding (the `llama-cpp-2` crate already ships `mtmd`), not on this
consumer's critical path.

## 4. Technical decisions

### 4.1 Capture

- `tabs.captureVisibleTab` from a user gesture (command, context menu). The
  temporary `activeTab` permission is enough (Chrome MV3 and Firefox MV2);
  the content script already matches `<all_urls>`, so the incremental trust
  surface is small. Chrome caps capture at 2 calls/second — irrelevant for
  user-triggered use.
- Firefox API/permission parity is verified in the spike, not assumed.

### 4.2 Coordinates

- Blocks are viewport-relative; the renderer accounts for `devicePixelRatio`
  and page zoom, scroll and fixed elements.
- Cross-origin iframes are in the bitmap but each frame runs its own content
  script: decide top-frame overlay vs per-frame rendering in M2 (the mapping
  math is unit-testable either way).

### 4.3 Service endpoint

`POST /translate/image` (sketch; final shape in M1):

```json
// request
{ "image": "<base64 png>", "mime": "image/png", "dpr": 2.0, "viewport": {"w": 1280, "h": 720}, "target": "zh" }
// response
{ "blocks": [ { "text": "START GAME", "translation": "开始游戏", "box": {"x": 0, "y": 0, "w": 120, "h": 24} } ] }
```

Errors keep the existing `{"error":{"kind","message"}}` shape. Logging keeps
the existing rule and extends it: **never log image data**.

### 4.4 Request model

- `ImageInput` enters the domain `TranslationInput` in the same change as the
  endpoint, with the OCR provider and capability gating — an unsupported
  modality returns an explicit 4xx/501, never a lying 502. Text remains the
  default path; `Video` is not added (not an input modality, and no consumer).
- Image requests get their own budgets (pixels, bytes, timeout); the text
  limits (`TRANSLATOR_MAX_CHARS`, the 30 s timeout) do not apply.

### 4.5 OCR provider

| Candidate | Pros | Risks |
| --- | --- | --- |
| Pure Rust OCR (`ocrs`/rten family) | no onnxruntime conflict, three platforms, core stays platform-independent | mixed zh/en small-text quality must be measured |
| Reuse the sherpa `libonnxruntime.so` | no new runtime | ABI/version check; Linux packaging path only |
| `ort` crate | mature ecosystem | brings a second onnxruntime → symbol/size conflicts with sherpa; **excluded by default** |

The model is downloaded on first use through `translator-core::models`
(never bundled); its license goes into the README third-party table. The
spike decides with measured numbers, not preference.

## 5. Milestones

| Milestone | Deliverable | Acceptance |
| --- | --- | --- |
| M0 spike — next | OCR selection + measured matrix | fixed mixed zh/en page (20+ blocks): char accuracy, block recall, per-image latency, binary/model size delta, packaging story on all three OSes |
| M1 | service image endpoint + OCR provider + desktop wiring + model download | endpoint unit/integration tests; OCR quality/latency from M0; explicit capability error without a provider |
| M2 | extension UX: command/menu, overlay (click-through, small toolbar), error states, PDF-viewer fallback | Chrome e2e; end-to-end latency, CPU peak, offline; Firefox manual pass; capture→overlay visible time and OCR numbers re-measured on the final path |

### 5.1 Spike results (2026-10-06)

Runtime selection (verified on the Linux dev machine):

- **Selected: `rapidocr-core` 0.2.2 + `ort` with `load-dynamic`, reusing the
  sherpa-shipped `libonnxruntime.so` 1.28.2** (vendored crate with
  `ort { default-features = false, features = ["std", "ndarray",
  "load-dynamic", "api-28"] }`). The build needs no OpenSSL and downloads no
  ONNX Runtime, and the process loads the shared library the deb already
  ships — no second runtime on Linux.
- Eliminated: `ocrs` (the recognition alphabet is Latin-only —
  `DEFAULT_ALPHABET` has no CJK — and no Chinese recognition model exists);
  `ort` default features (add a second ONNX Runtime and need OpenSSL headers
  at build time).
- Packaging (B2, decided and implemented 2026-10-07): the Windows zip and
  macOS dmg bundle a pinned ONNX Runtime 1.28.2 dylib
  (`onnxruntime.dll` / `libonnxruntime.dylib`, SHA-256 verified in the
  release workflow from the official GitHub asset) next to the binary, which
  `core/ocr` picks up automatically; Linux keeps reusing the sherpa-shipped
  `libonnxruntime.so`. The dependency side is the vendored patched
  `rapidocr-core` (A2, `vendor/rapidocr-core`) until upstream exposes the ort
  features.

Fixture: a Chrome-rendered 1280×800 page with ~30 mixed zh/en blocks on dark
and light panels (`/tmp/kilo/ocr-spike/fixture.html`), captured at DPR 1 and
DPR 2; detection found 41 lines on both.

| Model set | 1 thread | 4 threads | det / rec at 4t | Models | Notes |
| --- | --- | --- | --- | --- | --- |
| `ppocrv6-tiny` | 515 ms | **240 ms** (8t: 274 ms) | 108 / 132 ms | ~6.2 MB | provisional default |
| `ppocrv5-ch-mobile` | 2308 ms | 1392 ms | 254 / 1133 ms | ~20.7 MB | more case/punctuation misses |
| `ppocrv6-small` | 2952 ms | 2069 ms | 1066 / 994 ms | ~30.5 MB | no quality gain over tiny, high variance |

Scoring against the fixture's ground truth (41 blocks, NFKC + whitespace
normalization): **41/41 blocks matched, mean similarity 1.0000,
recall@0.95 = 1.000** at both DPR 1 and DPR 2. The only differences were
spacing/punctuation presentation (e.g. `100 / 100` → `100/100`), which
normalization removes. DPR 2 median at 4 threads: 596 ms (det 307 / rec 137).

Models come from ModelScope (`RapidAI/RapidOCR`, Apache-2.0), downloaded on
first use; `rapidocr-core` is Apache-2.0 (README attribution entry pending).
OCR sessions should default to 4 intra-op threads (ORT default is 1; 8
oversubscribes the 41 recognition crops).

Remaining M0 item: a real-webpage fixture — the current one is synthetic but
covers both scripts, dark/light and DPR 1/2; clean synthetic text does not
exercise stylized-UI failure modes. Final validation stays on the M2 path per
the milestone table.

### 5.2 M1 progress (2026-10-06)

The service side of M1 landed:

- `core/ocr` (crate `translator-ocr`): the engine-agnostic `OcrEngine` surface
  (`recognize(pixels, width, height) -> Vec<OcrBlock>` with `OcrBlock { text,
  score, quad }` and a `Quad::bounding_box` helper). The adapter followed the
  next day (§5.3).
- `translator-service`: `POST /translate/image` — base64 or data-URL
  PNG/JPEG input, OCR through the injected provider (`AppState::with_ocr`),
  one joined translation request per page with a per-block fallback when the
  model changes the line count, explicit `ocr_unavailable` (501) when no
  provider is configured, 32 MiB / 16 MP / `max_chars` budgets, and image
  bytes never logged. 10 integration tests cover translation, data URLs, the
  fallback, and the 400/501 paths.

Still pending: model download through `translator-core::models`, the desktop
wiring (`server.rs` + extension API), and the B2 runtime bundle for
Windows/macOS installers.

### 5.3 Adapter landed (2026-10-07)

Per the A2+B2 decision:

- `vendor/rapidocr-core` (0.2.2, Apache-2.0) with one manifest change — `ort`
  builds with `load-dynamic` (std/ndarray/api-28), so nothing is downloaded or
  linked at build time and OpenSSL is not required.
- `core/ocr::RapidOcrEngine` (`load` downloads missing ModelScope assets,
  `load_offline` fails instead) maps RapidOCR quads/scores onto `OcrBlock`
  and runs on the host-provided ONNX Runtime; on Linux that is the
  sherpa-shipped `libonnxruntime.so`.
- Re-verified end-to-end on the M0 fixture through the new adapter:
  41 blocks, 273 ms at 4 threads (PP-OCRv6-tiny, 1280×800).

### 5.4 Desktop wiring landed (2026-10-07)

The client now serves the image endpoint: when the extension interface starts
(`serve_extension`), it loads a `RapidOcrEngine` from `ocr_model_dir` (config
key, default `models/ocr/`), downloading PP-OCRv6-tiny from ModelScope on
first use, and injects it into the service state. A load failure only logs —
the server still starts and `/translate/image` answers 501, so a missing model
or runtime never blocks the normal text path. On Linux the `libonnxruntime.so`
shipped next to the binary is picked up automatically; the engine also looks
for `onnxruntime.dll`/`libonnxruntime.dylib` there, which is the hook the B2
installer bundle will use. `translator-core` gained
`paths::default_ocr_model_dir()` and the `ocr_model_dir` setting.

The extension capture/overlay UX landed the same day (§5.5). The B2 runtime
bundle landed 2026-10-07: the Windows zip and macOS dmg carry a pinned ONNX
Runtime 1.28.2 (SHA-256 verified in the release workflow) that the engine
auto-loads next to the executable; the deb keeps reusing the sherpa-shipped
`libonnxruntime.so`, and the local install scripts fetch the runtime on first
run.

### 5.5 First extension form (2026-10-07, superseded)

The first form shipped a whole-page screenshot overlay: the 截图翻译页面
context menu item and the `screenshot-translate` command (`Alt+Shift+S`)
captured the viewport, POSTed `/translate/image`, and painted each block's
translation on top of the page at its original position. It was replaced the
same day after the first manual check — see §5.6.

### 5.6 Revision: the whole-page overlay was removed (2026-10-07)

The first manual check in Chrome showed the obvious failure mode: the
translations stacked over the page's own (selectable) text, occluding the
original and adding no value where a DOM translation belongs. The screenshot
overlay premise — "translate whatever is on screen in place" — is wrong for
web pages; screenshot/OCR translation is only for content that cannot be
selected at all.

What replaced it (option A of the pivot):

- Trigger: the 翻译此图片（OpenTranslator）context menu (image context) only.
  The page-level command and menu item were removed.
- Flow: the background asks the content script for the image's visible
  rectangle, captures the tab (`activeTab` + `tabs.captureVisibleTab`), and
  the content script crops the capture on a canvas to that rectangle (data
  URLs stay untainted; the crop follows bitmap width / innerWidth, covering
  DPR and page zoom before POSTing the smaller image).
- Presentation (second revision, same day): a modal viewer
  (`data-opentranslator="image-viewer"`) draws each translation over its
  original position — camera-translation (Lens) style. The first fix used a
  flat panel next to the image, but a text list loses the spatial mapping
  that makes image translation intuitive; covering the source is right here
  precisely because the source is a static snapshot, unlike a live page.
  Hovering a box shows the original in the footer, clicking copies that
  block, the toolbar toggles 原文/译文 and 列表 (bilingual pairs) and copies
  all translations, backdrop/Esc close; errors show a toast; pages without a
  content script fall back to `result.html`.
- E2E: a mock `/translate/image` drives `translateImage` + an `image-result`
  message; the crop helper is asserted in the page, and the viewer checks
  cover the box overlay, the 原文 toggle, list mode and close;
  `captureVisibleTab` needs a user gesture and stays a manual check.

The rest of the pivot: B — desktop region screenshot → card is the real home
for unselectable content; C — a DOM-based web reading translation mode is
evaluated separately in `docs/READING_TRANSLATION.md`.

### 5.7 Desktop region screenshot (B) landed (2026-10-07)

Revised the same day from the first cut (portal's native picker → card) after
a first-principles review: selecting a region is deixis ("this here"), so the
answer must stay attached to the region — the card, designed for selected
text, destroyed the spatial mapping.

- Selection: a transparent full-screen selector window covers the monitor and
  the screen **stays live** — only the dragged rectangle is highlighted (no
  freeze, no dim; the first cut dimmed a frozen capture and was unusable). It
  reports the rectangle in CSS pixels, hides, and after a 200 ms compositor
  settle the XDG portal (`ashpd`, non-interactive) captures the screen
  without a dialog. Coordinates are mapped through the selector window's
  actual screen position and device scale (the WM may constrain it to the
  work area — GNOME's dock/top bar — so a full-monitor assumption offsets
  every crop). The portal does not report region coordinates — that is
  why the selector is ours, and why the region can be replayed exactly.
- Viewer: a Lens-style floating window anchored at the region draws each
  translation over the captured pixels (hover shows the original in the
  footer, click copies the block; toolbar 原文/译文 · 列表 · 复制 · 刷新 ·
  关闭; blur/Esc/× dismiss). Long text switches to the bilingual list mode.
- Replay: the region is stored; pressing the hotkey while the viewer is
  visible (or the 刷新 button) re-captures and re-translates the same region
  without selecting again — the game loop. With the viewer hidden, the tray
  item and the hotkey start a new selection.
- Pipeline: unchanged — the shared `translate_image_bytes` runs in-process
  with the lazily loaded OCR provider; history records the joined pair.
- Platform: Linux (XDG portal) and Windows (Windows Graphics Capture through
  `xcap`'s `wgc` feature — the monitor holding the selector is captured in
  physical pixels, so a mixed-DPI multi-monitor desktop needs no stitching);
  the item is disabled on macOS. Windows drags the viewer from the page
  (`move_viewer_by`: the OS move loop does not engage for this always-on-top
  tool window), Linux keeps the compositor move; blur-dismiss is Linux-only
  (Windows users close with Esc/×). The viewer is a normal always-on-top
  window, not a click-through overlay; exclusive-fullscreen games may not show
  any window, borderless windowed is the reliable mode.

Geometry (monitor slice for a virtual-desktop capture, CSS→capture-pixel
mapping, clamping, minimum selection) is unit-tested in `screenshot.rs` and
passes on Windows. Manual verification on the Linux dev machine is pending
(drag, viewer, refresh); portal capture, URI parsing and file reading are
verified. The Windows backend (physical-pixel selector placement, WGC
capture, viewer, drag) passed its real-machine check on 2026-10-07.

## 6. Risks

| Risk | Mitigation |
| --- | --- |
| DRM/protected video captures black | detect and say so; do not pretend |
| Coordinate mapping bugs (DPR, zoom, scroll, iframes) | unit-test the math; start with the top frame |
| Firefox capture API parity | spike before building the UX |
| OCR runtime packaging | Linux: reuse the sherpa-shipped ONNX Runtime via `ort` `load-dynamic` (spike-verified); Windows/macOS: bundle a pinned ORT dylib (B2 decided 2026-10-07) |
| Viewer occludes the page while open | intended (modal over a static snapshot); backdrop/Esc close, no page interaction needed |
| OCR quality on stylized UI | VLM fallback later; report misses honestly |

## 7. Open questions

1. OCR model set and license entry: `ppocrv6-tiny` is the provisional choice
   (README attribution entry pending; see §5.1).
2. 翻译此图片 matches `document.images` in the top frame; images inside
   cross-origin iframes fall back to the full-viewport capture (or the result
   page) — acceptable, revisit only if it hurts.
3. PDF viewer (no content script): `result.html` panel fallback enough?
4. Whether the response should also carry a whole-image reading (VLM) for
   documents, or stay block-only in v1.
5. Desktop region screenshot (B): portal (Wayland) vs X11 selection, and
   whether the card shows原文/译文 pairs or only the translation.
