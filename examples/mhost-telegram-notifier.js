#!/usr/bin/env node
/**
 * mhost Telegram Notifier
 *
 * Monitors mhost processes and sends Telegram alerts for:
 * - Process crashes (status changed to errored/stopped unexpectedly)
 * - Health check failures (HTTP 5xx from health endpoints)
 * - High restart counts
 * - Process recovery (came back online)
 *
 * Usage: node mhost-telegram-notifier.js
 * Or add to mhost: mhost start "node examples/mhost-telegram-notifier.js" --name notifier
 */

const http = require("http");
const https = require("https");
const { execSync } = require("child_process");
const path = require("path");

// ─── Config ─────────────────────────────────────────────────────
const BOT_TOKEN = process.env.MHOST_TELEGRAM_TOKEN;
const CHAT_ID = process.env.MHOST_TELEGRAM_CHAT;

if (!BOT_TOKEN || !CHAT_ID) {
  console.error("Error: MHOST_TELEGRAM_TOKEN and MHOST_TELEGRAM_CHAT environment variables are required.");
  console.error("Run: mhost notify setup");
  process.exit(1);
}
const POLL_INTERVAL = parseInt(process.env.POLL_INTERVAL || "10") * 1000; // 10s default
const MHOST_BIN = process.env.MHOST_BIN || path.join(__dirname, "..", "target", "release", "mhost");

// Health endpoints to monitor for 5xx
const HEALTH_ENDPOINTS = [
  { name: "react-app", url: "http://localhost:5173/health" },
  { name: "express-api", url: "http://localhost:4000/health" },
  { name: "node-api", url: "http://localhost:3000/health" },
];

// ─── State ──────────────────────────────────────────────────────
const previousState = new Map(); // process key -> { status, restarts }
const alertCooldown = new Map(); // alert key -> timestamp (throttle)
const COOLDOWN_MS = 60 * 1000; // 1 minute between same alerts
let startTime = Date.now();

// ─── Telegram API ───────────────────────────────────────────────
function sendTelegram(text) {
  return new Promise((resolve, reject) => {
    // Escape MarkdownV2 special chars
    const escaped = text
      .replace(/([_\[\]()~`>#+=|{}.!-])/g, "\\$1");

    const data = JSON.stringify({
      chat_id: CHAT_ID,
      text: escaped,
      parse_mode: "MarkdownV2",
    });

    const req = https.request(
      `https://api.telegram.org/bot${BOT_TOKEN}/sendMessage`,
      { method: "POST", headers: { "Content-Type": "application/json", "Content-Length": data.length } },
      (res) => {
        let body = "";
        res.on("data", (c) => (body += c));
        res.on("end", () => {
          if (res.statusCode === 200) {
            resolve(JSON.parse(body));
          } else {
            console.error(`Telegram API error: ${res.statusCode} ${body}`);
            reject(new Error(`Telegram ${res.statusCode}`));
          }
        });
      }
    );
    req.on("error", reject);
    req.write(data);
    req.end();
  });
}

function shouldAlert(key) {
  const last = alertCooldown.get(key);
  if (last && Date.now() - last < COOLDOWN_MS) return false;
  alertCooldown.set(key, Date.now());
  return true;
}

// ─── Process Monitoring ─────────────────────────────────────────
function getProcessList() {
  try {
    // Use mhost IPC via CLI — parse the list output
    // Since mhost list doesn't output JSON, we'll use the ping + info approach
    // Actually, let's just parse the daemon's dump file or use a direct approach
    const result = execSync(`${MHOST_BIN} list 2>&1`, { encoding: "utf-8", timeout: 5000 });
    return parseProcessList(result);
  } catch (e) {
    return null; // daemon not running
  }
}

function parseProcessList(output) {
  const lines = output.split("\n").filter((l) => l.trim() && !l.startsWith("id") && !l.startsWith("─"));
  return lines.map((line) => {
    const parts = line.trim().split(/\s{2,}/);
    if (parts.length < 6) return null;
    return {
      id: parts[0],
      name: parts[1],
      status: parts[2],
      pid: parts[3],
      instance: parts[4],
      uptime: parts[5],
      restarts: parseInt(parts[6]) || 0,
    };
  }).filter(Boolean);
}

async function checkProcesses() {
  const processes = getProcessList();
  if (!processes) {
    if (shouldAlert("daemon-down")) {
      await sendTelegram(
        "🔴 CRITICAL: mhost daemon is not responding!\n\nThe process manager daemon appears to be down. All managed processes may be affected."
      );
    }
    return;
  }

  for (const proc of processes) {
    const key = `${proc.name}:${proc.instance}`;
    const prev = previousState.get(key);

    // ── Crash Detection ──
    if (prev && prev.status === "online" && (proc.status === "errored" || proc.status === "stopped")) {
      if (shouldAlert(`crash:${key}`)) {
        await sendTelegram(
          `🔴 CRASH: Process "${proc.name}" (instance ${proc.instance}) has ${proc.status}!\n\nPrevious status: ${prev.status}\nPID: ${proc.pid || "N/A"}\nRestarts: ${proc.restarts}`
        );
      }
    }

    // ── Errored State ──
    if (proc.status === "errored" && (!prev || prev.status !== "errored")) {
      if (shouldAlert(`errored:${key}`)) {
        await sendTelegram(
          `🔴 ERRORED: Process "${proc.name}" (instance ${proc.instance}) has entered errored state!\n\nThis means max restarts have been exceeded. Manual intervention required.\nRestarts: ${proc.restarts}`
        );
      }
    }

    // ── Recovery Detection ──
    if (prev && (prev.status === "errored" || prev.status === "stopped") && proc.status === "online") {
      if (shouldAlert(`recovery:${key}`)) {
        await sendTelegram(
          `🟢 RECOVERED: Process "${proc.name}" (instance ${proc.instance}) is back online!\n\nPID: ${proc.pid}\nUptime: ${proc.uptime}`
        );
      }
    }

    // ── High Restart Count ──
    if (proc.restarts >= 5 && (!prev || prev.restarts < 5)) {
      if (shouldAlert(`high-restarts:${key}`)) {
        await sendTelegram(
          `🟡 WARNING: Process "${proc.name}" (instance ${proc.instance}) has restarted ${proc.restarts} times!\n\nThis may indicate an unstable process.`
        );
      }
    }

    // ── Restart Spike ──
    if (prev && proc.restarts > prev.restarts + 2) {
      if (shouldAlert(`restart-spike:${key}`)) {
        await sendTelegram(
          `🟡 RESTART SPIKE: Process "${proc.name}" (instance ${proc.instance}) restarted ${proc.restarts - prev.restarts} times since last check!\n\nCurrent restarts: ${proc.restarts}`
        );
      }
    }

    previousState.set(key, { status: proc.status, restarts: proc.restarts });
  }
}

// ─── Health Endpoint Monitoring (5xx detection) ─────────────────
function checkHealth(endpoint) {
  return new Promise((resolve) => {
    const req = http.get(endpoint.url, { timeout: 5000 }, (res) => {
      let body = "";
      res.on("data", (c) => (body += c));
      res.on("end", () => {
        resolve({ name: endpoint.name, status: res.statusCode, ok: res.statusCode < 400, body });
      });
    });
    req.on("error", (e) => {
      resolve({ name: endpoint.name, status: 0, ok: false, error: e.message });
    });
    req.on("timeout", () => {
      req.destroy();
      resolve({ name: endpoint.name, status: 0, ok: false, error: "timeout" });
    });
  });
}

async function checkHealthEndpoints() {
  for (const endpoint of HEALTH_ENDPOINTS) {
    const result = await checkHealth(endpoint);

    if (result.status >= 500) {
      if (shouldAlert(`5xx:${endpoint.name}`)) {
        await sendTelegram(
          `🔴 5XX ERROR: Health check for "${endpoint.name}" returned ${result.status}!\n\nURL: ${endpoint.url}\nResponse: ${result.body?.substring(0, 200) || "N/A"}`
        );
      }
    } else if (result.status === 0 && result.error) {
      if (shouldAlert(`unreachable:${endpoint.name}`)) {
        await sendTelegram(
          `🟡 UNREACHABLE: Health endpoint for "${endpoint.name}" is not responding!\n\nURL: ${endpoint.url}\nError: ${result.error}`
        );
      }
    }
  }
}

// ─── Main Loop ──────────────────────────────────────────────────
async function main() {
  console.log(JSON.stringify({
    level: "info",
    message: "mhost Telegram notifier started",
    pid: process.pid,
    poll_interval_s: POLL_INTERVAL / 1000,
    health_endpoints: HEALTH_ENDPOINTS.map((e) => e.url),
    timestamp: new Date().toISOString(),
  }));

  // Send startup notification
  await sendTelegram(
    `🟢 mhost Notifier Started\n\nMonitoring ${HEALTH_ENDPOINTS.length} health endpoints\nPoll interval: ${POLL_INTERVAL / 1000}s\nHost: ${require("os").hostname()}`
  );

  // Main monitoring loop
  while (true) {
    try {
      await checkProcesses();
      await checkHealthEndpoints();
    } catch (e) {
      console.error(JSON.stringify({
        level: "error",
        message: `Monitor cycle failed: ${e.message}`,
        timestamp: new Date().toISOString(),
      }));
    }

    await new Promise((r) => setTimeout(r, POLL_INTERVAL));
  }
}

process.on("SIGTERM", async () => {
  console.log(JSON.stringify({ level: "info", message: "Notifier shutting down" }));
  try {
    await sendTelegram("🔴 mhost Notifier stopped. Process monitoring is offline.");
  } catch {}
  process.exit(0);
});

main().catch(console.error);
