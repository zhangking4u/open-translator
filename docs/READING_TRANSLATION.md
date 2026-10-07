# Web Reading Translation (网页阅读翻译) — Evaluation

Status: evaluation started 2026-10-07. This is **C** of the post-screenshot
pivot (see `docs/IMAGE_TRANSLATION.md` §5.6): the whole-page OCR overlay was
removed because web pages are selectable and translatable in the DOM. Reading
foreign pages is the real web need; this document evaluates building a
DOM-based bilingual reading mode and proposes a spike before any commitment.
Spike S1–S3 completed on 2026-10-07 (results in §9, including the glossary
prompt fix it surfaced); the MVP landed the same day (§10).

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
- Per-block latency p50 3.5 s, p90 5.1 s, max 5.7 s (200–300-char
  paragraphs) on the post-fix run; the viewport blocks settle first and the
  full page took 123 s (pre-fix runs measured p50 4.9 s and 175–249 s for the
  same page; engine state and prompt size dominate the spread).
- Layout: `scrollWidth == clientWidth` before and after (no horizontal
  overflow); body height 3176 → 5232 px. Translations render as muted
  left-bordered siblings; lists/tables keep their structure.
- Quality: fluent and accurate on the sampled paragraphs, numbers and
  product names preserved, and the glossary terms are applied.

### S3 — batching comparison

| strategy | requests | p50 | full page | outcome |
| --- | --- | --- | --- | --- |
| per-block | 40 | 3.5 s | 123 s | 40/40 ok |
| join ≤ 1200 chars | — | — | — | every request 504: joined generation exceeds the service's 30 s non-streaming timeout on this machine |
| join ≤ 600 chars (pre-fix) | 9 joined + 41 single | 8.4 s | 523 s | every joined group split mismatched (glossary echo) → full per-block fallback |
| join ≤ 600 chars (post-fix) | 24 (9 joined) | 10.7 s | 158 s | 26 blocks joined, 14 fell back from 3 mismatched groups; no throughput win over per-block |

Joined requests fit 3–7 blocks in 18–29 s, better throughput than the
per-block sum, but the model's line discipline is not reliable enough yet
(3/24 requests came back with the wrong line count) and the 30 s timeout
caps the budget.

### S2 — dynamic feed (MutationObserver)

Fixture `browser/spike/feed.html`: append-on-scroll feed (capped at 40 items),
repeated texts (cache), hidden and short items, and two re-render stresses —
`替换正文` replaces six paragraphs with fresh nodes in place (orphan/stale
stress) and `重新渲染` wipes and rebuilds the list (SPA navigation). The
driver runs `--page feed --scroll-seconds 120`, clicks the stresses at 40% and
70%, waits until the queue has been drained for 3 s and then audits the DOM.

Results (post-fix engine, per-block):

- 50 blocks translated / 0 failed; 13 model requests and 37 cache hits — the
  repeated feed texts never re-entered the engine, and re-collected blocks
  (fresh nodes carrying the same text) were served from the cache.
- Audit: **0 duplicates, 0 missing translations, 0 orphans, 0 stale
  siblings** after the two re-renders; observer saw 37 mutation batches / 102
  added nodes with 0 budget skips; no horizontal overflow (756 = 756). The
  full drain took 125 s for the 120 s scroll plus a second wave of items.
- Residual quality note: one unique text of 13 came back with a trailing
  glossary-commentary sentence (`…必须使用以下术语来表述：…`) at temperature
  0.7 — the same class as the heading case above (≈2/53 unique texts across
  both fixtures). The MVP should either post-filter a trailing meta sentence
  or retry/lower the temperature when the prompt carries a glossary.



Finding: with a non-empty `glossary`, HY-MT continued the standalone hint
line (`术语表（必须使用以下译法）：…`) instead of translating — every response
started with it, short inputs returned only it, and joined requests gained an
extra line. Isolation: the same sentence without a glossary was clean.

Fix: fold the terms into the instruction paragraph (`…不要额外解释。必须使用
以下术语译法：context window=上下文窗口；量化=量化`) in `hunyuan_mt_prompt`
(`core/translator-service/src/domain/prompt.rs`); the empty-glossary shape is
unchanged. A 12-text corpus matrix picked the layout: standalone terms before
the instruction 7/12 commentary, after the text 1/12, inline 0/12. The
40-block rerun shows no glossary echoes; one residual remains — a heading
*about* glossaries ("Glossaries beat fine-tuning") still returns the term list
(1/40). Generic/translategemma keep their existing hint placement (their
trailing instruction line already prevents the continuation).

### Decision proposal for the MVP

- Default to per-block requests with viewport-first ordering (p50 ~3.5 s on
  this machine; the visible screen fills progressively).
- Keep joining out of v1: line discipline needs a tolerant splitter, and the
  join budget must respect the non-streaming timeout — or batching moves to
  the streaming path. Re-measure on the Linux dev machine before revisiting.
- Reuse the fixed glossary prompt for the reading mode (terms apply) and add
  a small output guard for the residual glossary commentary (strip a trailing
  meta sentence, or retry at a lower temperature).
- The MutationObserver pass is sound: markers + cache + idempotent injection
  + orphan pruning survived both re-render styles with a clean audit, so the
  MVP can build on the same lifecycle.

Next: the spike is complete (S1/S2/S3); the MVP scope decision can proceed,
with the output guard and the Linux-side measurement as the remaining
prerequisites.

## 10. MVP landed (2026-10-07)

The bilingual reading mode shipped as the extension's `reading.js` (a content
script on all pages, inert until enabled):

- Popup toggle 网页翻译 (`readingEnabled`) plus the existing per-site opt-out
  (`disabledSites`); top frame only in v1.
- Block set and skip heuristics as specced (paragraphs/headings/lists/quotes/
  cells; hidden, code, editable, target-language and short blocks skipped).
- Viewport-first single-flight queue with an IntersectionObserver for the
  off-screen backlog (a long article never queues everything at once), a
  MutationObserver for dynamic pages, a per-page text cache, idempotent
  injection and orphan pruning — the lifecycle verified in S2.
- Per-block requests (the spike's default), hover controls 复制 / 重译, no
  history writes (`record: false`), no original replacement (仅译文 deferred
  per §7 open question 3).
- Streaming landed the same day: the serialized queue translates over the
  existing `translate` port and renders deltas into the pending block (the
  one-shot message remains the fallback when the background cannot stream);
  stopping the mode cancels the in-flight stream.
- Coverage revision after the first real-page pass: a post-order leaf walk
  claims the innermost block first and falls back to containers, so div-based
  text (tag-soup pages) is translated without double injections; container
  blocks with link density > 0.6 are skipped as UI; class/style/hidden
  attribute changes trigger a debounced rescan (tabs/accordions become
  translatable once visible); the MutationObserver watches
  `document.documentElement` (SPA frameworks replace `document.body`); the
  `lang`-based target skip now applies to non-CJK targets only — for zh/ja/ko
  the script-ratio heuristic is enough, so English blocks on a `lang="zh"`
  page are no longer skipped. A follow-up pass lowered the thresholds:
  headings (h1–h6, th, summary, dt, figcaption) from 3 chars, `nav`/`header`
  labels from 4 chars (compact injection style), while footer/aside stay
  skipped and paragraphs keep 12.
- Not in v1: joined batches (S3), extension-side glossary
  (the extension has no glossary storage yet; the shared glossary stays a
  desktop-side feature until the output guard lands), iframes, shadow DOM.
- Verified: a targeted headless-Edge run against the running client (inject →
  state hook → cleanup on disable, plus a div-based block, all pass) and the
  CI harness gained a reading-mode scenario that waits for the streamed block
  to settle.


