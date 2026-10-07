#!/usr/bin/env node
// S1 spike driver: serves the fixture article plus a same-origin /translate
// proxy to the local desktop client, launches headless Edge, and reports the
// reading-translation metrics from window.__OT_SPIKE__.
//
// Usage:
//   node s1.mjs [--batch block|join] [--max-chars 1200] [--service http://127.0.0.1:17890]
//               [--browser-path <msedge.exe>] [--out-dir <dir>] [--timeout-ms 300000]

import http from "node:http";
import { readFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    args[argv[i].replace(/^--/, "")] = argv[i + 1];
  }
  return args;
}

const args = parseArgs(process.argv.slice(2));
const here = dirname(fileURLToPath(import.meta.url));
const BATCH = args.batch === "join" ? "join" : "block";
const MAX_CHARS = String(args["max-chars"] || 1200);
const SERVICE = args.service || "http://127.0.0.1:17890";
const BROWSER_PATH =
  args["browser-path"] || "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";
const OUT_DIR = args["out-dir"] || join(tmpdir(), "ot-spike");
const TIMEOUT_MS = Number(args["timeout-ms"] || 540000);
const CDP_PORT = 9223;

mkdirSync(OUT_DIR, { recursive: true });

const article = await readFile(join(here, "article.html"));
const spike = await readFile(join(here, "reading-spike.js"));

// Static page plus a same-origin /translate proxy to the desktop client.
const server = http.createServer((request, response) => {
  if (request.method === "GET" && request.url.startsWith("/article.html")) {
    response.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    response.end(article);
    return;
  }
  if (request.method === "GET" && request.url === "/reading-spike.js") {
    response.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8" });
    response.end(spike);
    return;
  }
  if (request.method === "POST" && request.url === "/translate") {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const upstream = new URL("/translate", SERVICE);
      const forwarded = http.request(
        upstream,
        { method: "POST", headers: { "Content-Type": "application/json" } },
        (upstreamResponse) => {
          response.writeHead(upstreamResponse.statusCode || 502, {
            "Content-Type": "application/json",
          });
          upstreamResponse.pipe(response);
        }
      );
      forwarded.on("error", (error) => {
        response.writeHead(502, { "Content-Type": "application/json" });
        response.end(JSON.stringify({ error: { kind: "proxy", message: String(error) } }));
      });
      forwarded.end(body);
    });
    return;
  }

  response.writeHead(404);
  response.end();
});

await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const pageUrl = `http://127.0.0.1:${server.address().port}/article.html?batch=${BATCH}&maxChars=${MAX_CHARS}&delayMs=1500`;

// --- headless Edge ---
const profileDir = join(OUT_DIR, `profile-${Date.now()}`);
mkdirSync(profileDir, { recursive: true });
const browser = spawn(
  BROWSER_PATH,
  [
    "--headless=new",
    "--disable-gpu",
    "--no-first-run",
    "--no-default-browser-check",
    `--remote-debugging-port=${CDP_PORT}`,
    `--user-data-dir=${profileDir}`,
    "about:blank",
  ],
  { stdio: "ignore" }
);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function waitForCdp() {
  for (let attempt = 0; attempt < 60; attempt++) {
    try {
      const response = await fetch(`http://127.0.0.1:${CDP_PORT}/json/version`);
      if (response.ok) return;
    } catch {
      // not up yet
    }
    await sleep(500);
  }
  throw new Error("CDP did not come up");
}

function shutdown(code) {
  try {
    spawn("taskkill", ["/F", "/T", "/PID", String(browser.pid)], { stdio: "ignore" });
  } catch {
    // best effort
  }
  server.close();
  process.exit(code);
}

try {
  await waitForCdp();

  const { webSocketDebuggerUrl } = await (
    await fetch(`http://127.0.0.1:${CDP_PORT}/json/version`)
  ).json();

  const ws = new WebSocket(webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
  });

  let nextId = 0;
  const pending = new Map();
  ws.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id !== undefined && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
    }
  };

  function send(method, params = {}, sessionId) {
    return new Promise((resolve) => {
      const id = ++nextId;
      pending.set(id, resolve);
      ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  }

  async function evaluate(sessionId, expression) {
    const result = await send(
      "Runtime.evaluate",
      { expression, awaitPromise: true, returnByValue: true },
      sessionId
    );
    if (result.error) throw new Error(JSON.stringify(result.error));
    if (result.result.exceptionDetails) {
      throw new Error(JSON.stringify(result.result.exceptionDetails));
    }
    return result.result.result.value;
  }

  const created = await send("Target.createTarget", { url: pageUrl });
  const sessionId = (
    await send("Target.attachToTarget", { targetId: created.result.targetId, flatten: true })
  ).result.sessionId;
  await send("Runtime.enable", {}, sessionId);
  await send("Page.enable", {}, sessionId);

  // "Before" screenshot: the spike waits delayMs before collecting.
  await sleep(900);
  const before = await send(
    "Page.captureScreenshot",
    { format: "png", captureBeyondViewport: true },
    sessionId
  );
  writeFileSync(join(OUT_DIR, "layout-before.png"), Buffer.from(before.result.data, "base64"));

  const deadline = Date.now() + TIMEOUT_MS;
  let done = false;
  while (Date.now() < deadline) {
    done = await evaluate(
      sessionId,
      "Boolean(window.__OT_SPIKE__ && window.__OT_SPIKE__.done_flag)"
    );
    if (done) break;
    await sleep(1000);
  }

  if (done) {
    const after = await send(
      "Page.captureScreenshot",
      { format: "png", captureBeyondViewport: true },
      sessionId
    );
    writeFileSync(join(OUT_DIR, "layout-after.png"), Buffer.from(after.result.data, "base64"));
  }

  const report = await evaluate(
    sessionId,
    "JSON.stringify(window.__OT_SPIKE__, (key, value) => (key === 'element' ? undefined : value))"
  );

  const parsed = JSON.parse(report);
  parsed.timedOut = !done;
  writeFileSync(join(OUT_DIR, `s1-${BATCH}.json`), JSON.stringify(parsed, null, 2));

  const statuses = {};
  for (const block of parsed.blocks) {
    statuses[block.status] = (statuses[block.status] || 0) + 1;
  }
  const latencies = parsed.blocks
    .filter((block) => block.status !== "cache")
    .map((block) => block.latencyMs)
    .sort((a, b) => a - b);
  const percentile = (p) =>
    latencies.length === 0 ? null : latencies[Math.min(latencies.length - 1, Math.floor((latencies.length * p) / 100))];

  console.log(
    JSON.stringify(
      {
        batch: parsed.batch,
        queued: parsed.queued,
        done: parsed.done,
        failed: parsed.failed,
        timedOut: parsed.timedOut,
        cacheHits: parsed.cacheHits,
        skipped: parsed.skipped,
        statuses,
        requests: parsed.requests.length,
        joinedRequests: parsed.requests.filter((r) => r.blocks > 1).length,
        firstBlockMs: parsed.firstBlockMs,
        firstViewportMs: parsed.firstViewportMs,
        fullPageMs: parsed.fullPageMs,
        latencyMs: {
          p50: percentile(50),
          p90: percentile(90),
          max: latencies.at(-1) ?? null,
        },
        layout: parsed.layout,
        glossaryHits: parsed.blocks
          .filter((block) => block.glossaryHits.length > 0)
          .map((block) => block.glossaryHits),
        errors: parsed.errors,
        screenshots: {
          before: join(OUT_DIR, "layout-before.png"),
          after: join(OUT_DIR, "layout-after.png"),
        },
        samples: parsed.blocks.slice(0, 3).map((block) => ({
          original: block.original,
          translation: block.translation,
        })),
      },
      null,
      2
    )
  );

  shutdown(0);
} catch (error) {
  console.error(String(error));
  shutdown(1);
}
