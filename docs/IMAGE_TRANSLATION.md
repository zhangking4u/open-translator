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

## 6. Risks

| Risk | Mitigation |
| --- | --- |
| DRM/protected video captures black | detect and say so; do not pretend |
| Coordinate mapping bugs (DPR, zoom, scroll, iframes) | unit-test the math; start with the top frame |
| Firefox capture API parity | spike before building the UX |
| OCR runtime packaging (second onnxruntime) | prefer pure Rust; exclude `ort` by default |
| Overlay interferes with the page | pointer-events off except a small toolbar; Esc/menu dismiss |
| OCR quality on stylized UI | VLM fallback later; report misses honestly |

## 7. Open questions

1. OCR model and license choice (spike output; README attribution entry).
2. Iframe overlay strategy: top-frame vs per-frame rendering.
3. PDF viewer (no content script): `result.html` panel fallback enough?
4. Whether the response should also carry a whole-image reading (VLM) for
   documents, or stay block-only in v1.
