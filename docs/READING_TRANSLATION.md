# Web Reading Translation (网页阅读翻译) — Evaluation

Status: evaluation started 2026-10-07. This is **C** of the post-screenshot
pivot (see `docs/IMAGE_TRANSLATION.md` §5.6): the whole-page OCR overlay was
removed because web pages are selectable and translatable in the DOM. Reading
foreign pages is the real web need; this document evaluates building a
DOM-based bilingual reading mode and proposes a spike before any commitment.

## 1. Why this exists

- The first manual check of 截图翻译 showed OCR-over-page is the wrong tool for
  selectable content (occlusion, duplication with the page's own text).
- The proven UX on the web is inline bilingual translation: each paragraph
  keeps its original and gains a translation below it (沉浸式翻译-style
  extensions). The original stays readable, nothing is covered.
- OpenTranslator's differentiator is fully local processing (embedded
  llama.cpp + HY-MT, warm paragraph latency ~0.2–0.5 s), shared glossary and
  history with the desktop client, and no page text leaving the machine.

## 2. Reference UX patterns worth matching

- Paragraph-level injection: translate `p`, `h1`–`h6`, `li`, `blockquote`,
  `td` and similar blocks; insert the translation as a sibling below/inline,
  never replace the original by default.
- Per-block state: subtle loading indicator → streaming text → final; hover
  controls (显示原文/复制/重译).
- Modes: 双语（默认）/ 仅译文; per-site rules; an explicit on/off toggle in the
  popup next to the existing auto-translate/site switches.
- Dynamic content: MutationObserver so SPA navigations and infinite scroll
  keep getting translated; virtualization must not thrash.
- Progressive order: viewport first (IntersectionObserver), cancel or skip
  off-screen work; the MT engine is serialized, so scheduling matters.

## 3. Fit with the existing stack

- No model or runtime work: the text path (`POST /translate/stream`, glossary,
  `latest_wins` scheduling patterns) already exists and is shared with the
  desktop.
- Extension UI language/tokens (`OT_TOKENS_CSS`, `OTSelect`) carry over.
- Work items are all in the extension: DOM walker, scheduler, injection CSS,
  popup toggle, e2e hooks.

## 4. Hard problems and risks

| Problem | Notes / mitigation |
| --- | --- |
| Layout breakage | Injecting into flex/grid/inline contexts can reflow badly; keep inserted nodes `display: block` siblings of block elements, never modify existing nodes |
| Dynamic pages | React/Vue re-renders replace or move injected nodes; re-inject via MutationObserver with a per-node marker; cap work per frame |
| Volume and latency | A long article can be 100+ blocks on a serialized engine; viewport-first queue, skip hidden/short (`< N chars`) nodes, merge adjacent short blocks into one request |
| Quality and token budget | Paragraph-level requests lose cross-paragraph context; glossary covers UI terms; numbers/codes are already prompt-preserved; consider joining paragraphs up to the `max_chars` budget and splitting on line count (same fallback as the image endpoint) |
| Language detection | Skip blocks already in the target language using the `lang` attribute plus a light script heuristic; no extra model |
| Coexistence with selection/typing bubbles | Reading mode is passive; never auto-open bubbles; the injected controls must not capture the page's own clicks (pointer-events only on the small control row) |
| Firefox MV2 vs Chrome MV3 | Content-script logic is identical; only the background port lifecycle differs (already handled) |

## 5. Proposed MVP scope

- Static or lightly dynamic article pages; top frame only in v1 (same-origin
  iframes later).
- Bilingual insert for the block set above; 双语/仅译文 toggle; per-site
  opt-out reused from `disabledSites`; popup toggle 网页翻译.
- Viewport-first, single-flight queue with cancel-on-scroll; in-memory cache
  `text → translation` per page; no history pollution (reading mode should not
  write the translation history).
- Hover controls: 复制, 重译. No editing, no replace-original in v1.

## 6. Spike plan (1–2 days, before committing to the MVP)

| Step | What | Acceptance |
| --- | --- | --- |
| S1 | Static mixed zh/en article, 40+ paragraphs: inject bilingual blocks, measure per-block and time-to-full-page latency with the dev machine's engine | numbers recorded; layout screenshots before/after; spot-check quality incl. glossary |
| S2 | Dynamic page (feed/SPA) with MutationObserver re-injection | no duplicate/stale blocks after 2 minutes of scrolling; bounded re-translation |
| S3 | Batching: per-block vs joined-up-to-`max_chars` requests | quality/latency comparison table; decide the default strategy |

## 7. Open questions (decide after the spike)

1. Batching strategy and block merging threshold.
2. Skip heuristics: minimum length, hidden nodes, code blocks, already-target
   language detection quality.
3. Whether 仅译文 mode is needed in v1 (bilingual is the differentiator; 仅译文
   risks layout damage).
4. Popup information architecture: the popup already carries language,
   auto-translate and site toggles; add 网页翻译 without crowding.
5. Later: same-origin iframes, PDF.js-based pages, and whether the desktop
   client should host the same reading mode for text it cannot reach.

## 8. Relationship to other work

- Does not touch OCR; it is pure text-path work.
- Shares scheduler ideas with `latest_wins` but needs a queue (many blocks),
  not just a newest-wins slot — likely a small extension-side queue module
  first, promoted to shared code only if the desktop needs it too.
- Not the same as the deferred system-level UI translation (that is for
  unselectable native UI).

## 9. Spike results — S1 and S3 (2026-10-07)

Setup: `browser/spike/article.html` (40 translatable blocks plus skip cases:
9 short, 2 target-language, 1 hidden, 1 inside the footer), the prototype
`browser/spike/reading-spike.js` (DOM walk → skip heuristics → viewport-first
single-flight queue → sibling injection) and the driver `browser/spike/s1.mjs`
(headless Edge over CDP; the page server proxies `POST /translate` to the
running desktop client on `127.0.0.1:17890`). Engine: embedded llama.cpp with
HY-MT 1.5-1.8B Q4_K_M, warm, Windows dev machine. Layout screenshots land in
`%TEMP%\ot-spike\layout-{before,after}.png` (saved for eyeballing; the numeric
layout check below is the actual acceptance).

### S1 — per-block requests

- 40/40 blocks translated, 0 failures; the skip rules caught exactly the
  intended set (short, hidden, target-language, footer).
- Per-block latency p50 4.9 s, p90 6.4 s, max 6.6 s (200–300-char
  paragraphs); first block 4.7 s; the 5 viewport blocks finished at 26.6 s;
  the full page took 175 s.
- Layout: `scrollWidth == clientWidth` before and after (no horizontal
  overflow); body height 3176 → 5679 px. Translations render as muted
  left-bordered siblings; lists/tables keep their structure.
- Quality (glossary aside, see below): fluent and accurate on the sampled
  paragraphs, numbers and product names preserved.

### S3 — batching comparison

| strategy | requests | p50 | full page | outcome |
| --- | --- | --- | --- | --- |
| per-block | 40 | 4.9 s | 175 s | 40/40 ok |
| join ≤ 1200 chars | — | — | — | every request 504: joined generation exceeds the service's 30 s non-streaming timeout on this machine |
| join ≤ 600 chars | 9 joined + 41 single | 8.4 s | 523 s | every joined group came back one line too many → line-count mismatch → full per-block fallback (slower than plain per-block) |

Joined requests themselves are promising when they fit (3–7 blocks in
18–29 s, i.e. better throughput than the per-block sum), but the 30 s
timeout caps the join budget on this machine, and the only reason the line
contract failed is finding 1 below.

### Quality findings (blocking and non-blocking)

1. **Glossary echo — a product bug, not a spike bug.** With any non-empty
   `glossary` the HY-MT output starts with the raw hint line
   (`术语表（必须使用以下译法）：上下文窗口=上下文窗口…`); with two terms and a
   short sentence the response is *only* that line, with no translation.
   Isolation: the same sentence without a glossary translates correctly.
   Cause: the hymt prompt places the terms between the instruction and the
   text (`将以下文本翻译为…：\n术语表（必须使用以下译法）：…\n\n{text}`), and the
   base-style model continues that fragment instead of translating. This
   affects every glossary consumer (captions, extension, a future reading
   mode) and must be fixed before reading mode can share the glossary.
   Candidate fixes: end the prompt with the text and put terms after it, or
   on their own instruction line before the main instruction, with a clear
   cue before the text; needs prompt experiments and prompt unit tests.
2. The joined line contract breaks on the same preamble; a tolerant splitter
   (strip a leading glossary line) or streaming-based batching would be
   needed on top of finding 1.
3. Non-streaming joins are bounded by `TRANSLATOR_TIMEOUT_MS` (30 s in the
   shipped client); batching must budget by expected generation time, not
   only by `max_chars` — or run the joined request over the streaming path,
   whose idle timeout restarts per delta.

### Decision proposal for the MVP

- Default to per-block requests with viewport-first ordering: on this
  machine a paragraph settles in ~5 s and the visible screen fills
  progressively; that matches the current desktop/extension latency budget.
- Keep a small join (2–3 short blocks, well under 600 chars) only after the
  glossary prompt fix and with a per-request budget below the service
  timeout; re-measure on the Linux dev machine (whose paragraph latency was
  measured much lower) before fixing the default.
- The glossary prompt fix is a prerequisite for any glossary-carrying mode
  and is worth landing as a standalone fix (it affects captions today).

Next: S2 (dynamic feed + MutationObserver) has not been exercised yet; the
glossary prompt fix should precede the MVP scope decision.
