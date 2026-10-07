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
