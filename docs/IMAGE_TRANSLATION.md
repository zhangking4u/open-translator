# Browser Screenshot Translation (扩展截图翻译) — Design Intent

Status: decided 2026-10-06. This is **D1** of the multimodal roadmap: browser
screenshot translation is the **first image consumer**; the desktop
region-screenshot → card flow is second; system-level UI translation is
deferred. Nothing is implemented yet — the OCR selection spike (§5, M0) is the
next step. Phase 1a (the shared latest-wins scheduler) landed 2026-10-06 as
the prerequisite (`translator-core::latest_wins`).

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

Still pending: the extension capture/overlay UX (M2). The B2 runtime bundle
landed 2026-10-07: the Windows zip and macOS dmg carry a pinned ONNX Runtime
1.28.2 (SHA-256 verified in the release workflow) that the engine auto-loads
next to the executable; the deb keeps reusing the sherpa-shipped
`libonnxruntime.so`, and the local install scripts fetch the runtime on first
run.

## 6. Risks

| Risk | Mitigation |
| --- | --- |
| DRM/protected video captures black | detect and say so; do not pretend |
| Coordinate mapping bugs (DPR, zoom, scroll, iframes) | unit-test the math; start with the top frame |
| Firefox capture API parity | spike before building the UX |
| OCR runtime packaging | Linux: reuse the sherpa-shipped ONNX Runtime via `ort` `load-dynamic` (spike-verified); Windows/macOS: bundle a pinned ORT dylib (B2 decided 2026-10-07) |
| Overlay interferes with the page | pointer-events off except a small toolbar; Esc/menu dismiss |
| OCR quality on stylized UI | VLM fallback later; report misses honestly |

## 7. Open questions

1. OCR model set and license entry: `ppocrv6-tiny` is the provisional choice
   (README attribution entry pending; see §5.1).
2. Iframe overlay strategy: top-frame vs per-frame rendering.
3. PDF viewer (no content script): `result.html` panel fallback enough?
4. Whether the response should also carry a whole-image reading (VLM) for
   documents, or stay block-only in v1.
