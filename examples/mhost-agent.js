#!/usr/bin/env node
/**
 * mhost Agent — Autonomous AI-powered infrastructure manager
 *
 * Continuously monitors mhost processes, uses LLM to decide actions,
 * executes via mhost CLI, and communicates through Telegram.
 *
 * Usage:
 *   mhost agent start                       # Start via CLI
 *   OPENAI_API_KEY=... node mhost-agent.js  # Direct run
 */

"use strict";

const { execSync } = require("child_process");
const https = require("https");
const path = require("path");
const fs = require("fs");

// ─── Config ──────────────────────────────────────────────────────────────────

const CONFIG_PATH = path.join(
  process.env.HOME || "~",
  ".mhost",
  "agent.json"
);

const DEFAULT_CONFIG = {
  provider: "openai",
  api_key: "${OPENAI_API_KEY}",
  model: "gpt-4o",
  telegram_token: "${MHOST_TELEGRAM_TOKEN}",
  telegram_chat_id: "${MHOST_TELEGRAM_CHAT}",
  autonomy: "supervised",
  allowed_actions: ["restart", "scale", "logs", "info", "list", "save", "start"],
  blocked_actions: ["delete", "kill"],
  confirm_destructive: true,
  max_actions_per_hour: 20,
  observe_interval_seconds: 30,
  conversation_history_limit: 20,
};

function loadConfig() {
  try {
    const content = fs.readFileSync(CONFIG_PATH, "utf-8");
    return Object.assign({}, DEFAULT_CONFIG, JSON.parse(content));
  } catch {
    return Object.assign({}, DEFAULT_CONFIG);
  }
}

const config = loadConfig();

// ─── Brain (Self-healing Intelligence) ───────────────────────────────────────

let brain = null;
try {
  const { Brain } = require("./mhost-brain.js");
  brain = new Brain();
  console.log(
    JSON.stringify({
      level: "info",
      message: "Brain loaded",
      incidents: brain.incidents.length,
      playbooks: brain.playbooks.length,
    })
  );
} catch (e) {
  console.warn(
    JSON.stringify({ level: "warn", message: `Brain not available: ${e.message}` })
  );
}

const MHOST_BIN = process.env.MHOST_BIN || "mhost";
const OBSERVE_INTERVAL_MS = (config.observe_interval_seconds || 30) * 1000;
const POLL_INTERVAL_MS = 5000;
const MAX_TOOL_ITERATIONS = 10;

// ─── State ───────────────────────────────────────────────────────────────────

/** @type {Array<{role: string, content: string}>} */
let conversationHistory = [];

/** @type {Array<{timestamp: number, action: string, result: string}>} */
let actionLog = [];

/** @type {{action: string, args: object} | null} */
let pendingApproval = null;

let lastObservation = "";
let telegramOffset = 0;
let tickCount = 0;

// ─── OpenAI Tool Definitions ─────────────────────────────────────────────────

const TOOLS = [
  {
    type: "function",
    function: {
      name: "list_processes",
      description:
        "List all managed processes with their status, PID, CPU, memory, uptime, and restart count.",
      parameters: { type: "object", properties: {}, required: [] },
    },
  },
  {
    type: "function",
    function: {
      name: "get_logs",
      description: "Get recent stdout log lines for a process.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Process name" },
          lines: {
            type: "number",
            description: "Number of lines to return (default 20)",
          },
        },
        required: ["name"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "get_error_logs",
      description: "Get recent stderr/error log lines for a process.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Process name" },
          lines: {
            type: "number",
            description: "Number of lines to return (default 20)",
          },
        },
        required: ["name"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "restart_process",
      description:
        "Restart a process. Use when a process has crashed or is unhealthy.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Process name" },
        },
        required: ["name"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "stop_process",
      description: "Stop a running process gracefully.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Process name" },
        },
        required: ["name"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "start_process",
      description: "Start a previously stopped process.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Process name" },
        },
        required: ["name"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "scale_process",
      description: "Scale a process to N instances.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Process name" },
          instances: {
            type: "number",
            description: "Target instance count",
          },
        },
        required: ["name", "instances"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "get_info",
      description: "Get detailed information about a specific process.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Process name" },
        },
        required: ["name"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "save_processes",
      description:
        "Save the current process list for resurrection on next startup.",
      parameters: { type: "object", properties: {}, required: [] },
    },
  },
  {
    type: "function",
    function: {
      name: "send_telegram",
      description:
        "Send a message to the user via Telegram. Use this to report observations, actions taken, or ask for approval.",
      parameters: {
        type: "object",
        properties: {
          message: {
            type: "string",
            description: "Message to send (supports HTML formatting)",
          },
        },
        required: ["message"],
      },
    },
  },
];

// ─── System Prompt ───────────────────────────────────────────────────────────

function buildSystemPrompt() {
  const autonomy = config.autonomy || "supervised";
  const blocked = (config.blocked_actions || []).join(", ") || "none";
  const maxActions = config.max_actions_per_hour || 20;

  const autonomyInstructions = {
    autonomous:
      "You can act freely without asking permission. Always notify the user AFTER you act.",
    supervised:
      "For any action that changes state (restart, stop, scale, delete), ask the user for approval via send_telegram first. Wait for their reply in the next cycle.",
    manual:
      "Only act when the user explicitly instructs you via Telegram. Otherwise just observe and report.",
  };

  return `You are mhost Agent — an autonomous DevOps AI that manages server processes.

You run inside the mhost process manager. Your responsibilities:
1. OBSERVE: Check process status and logs for crashes, errors, or performance issues.
2. THINK: Analyse what you see — high restart counts, OOM errors, degraded performance, anomalies.
3. ACT: Take corrective action — restart crashed processes, scale under load, alert on anomalies.
4. REPORT: Tell the user what you found and what you did via send_telegram.

BRAIN: You have access to a Brain with persistent memory of past incidents, health scores, and
auto-learned playbooks. Brain context for affected processes will be injected into observations.
Use past incident data to make better decisions. After fixing an issue, the brain will
automatically learn from it so next time it can self-heal without calling you.

AUTONOMY LEVEL: ${autonomy}
${autonomyInstructions[autonomy] || autonomyInstructions.supervised}

HARD RULES:
- Blocked actions (NEVER execute): ${blocked}
- Maximum ${maxActions} state-changing actions per hour (rate limit enforced)
- Always explain WHY before or after acting
- If everything is healthy, stay silent — do NOT send a Telegram message
- Keep Telegram messages concise and readable (plain text or minimal HTML)
- Reference specific log lines when diagnosing issues

Current time: ${new Date().toISOString()}`;
}

// ─── Tool Execution ──────────────────────────────────────────────────────────

/**
 * Check whether an action name is blocked by config.
 * @param {string} toolName
 * @returns {boolean}
 */
function isBlocked(toolName) {
  const blocked = config.blocked_actions || [];
  // Map tool names to their action keyword
  const actionKeyword = toolName.replace("_process", "");
  return blocked.includes(actionKeyword) || blocked.includes(toolName);
}

/**
 * Check whether the per-hour rate limit has been exceeded.
 * @returns {boolean}
 */
function isRateLimited() {
  const oneHourAgo = Date.now() - 3_600_000;
  const recentActions = actionLog.filter((a) => a.timestamp > oneHourAgo);
  return recentActions.length >= (config.max_actions_per_hour || 20);
}

/**
 * Execute a tool and return a JSON string result.
 * @param {string} name  Tool name
 * @param {object} args  Tool arguments
 * @returns {string}     JSON-encoded result
 */
function executeTool(name, args) {
  if (isBlocked(name)) {
    return JSON.stringify({
      error: `Action '${name}' is blocked by configuration`,
    });
  }

  // Rate-limit only state-changing actions
  const stateChangingTools = new Set([
    "restart_process",
    "stop_process",
    "start_process",
    "scale_process",
    "save_processes",
  ]);
  if (stateChangingTools.has(name) && isRateLimited()) {
    return JSON.stringify({
      error: `Rate limit reached: maximum ${config.max_actions_per_hour || 20} actions per hour exceeded`,
    });
  }

  try {
    let output;

    switch (name) {
      case "list_processes":
        output = execSync(`${MHOST_BIN} list 2>&1`, {
          encoding: "utf-8",
          timeout: 10_000,
        });
        break;

      case "get_logs":
        output = execSync(
          `${MHOST_BIN} logs ${shellEscape(args.name)} -n ${Number(args.lines) || 20} 2>&1`,
          { encoding: "utf-8", timeout: 10_000 }
        );
        break;

      case "get_error_logs":
        output = execSync(
          `${MHOST_BIN} logs ${shellEscape(args.name)} --err -n ${Number(args.lines) || 20} 2>&1`,
          { encoding: "utf-8", timeout: 10_000 }
        );
        break;

      case "restart_process":
        output = execSync(
          `${MHOST_BIN} restart ${shellEscape(args.name)} 2>&1`,
          { encoding: "utf-8", timeout: 15_000 }
        );
        actionLog.push({
          timestamp: Date.now(),
          action: `restart ${args.name}`,
          result: output.trim(),
        });
        break;

      case "stop_process":
        output = execSync(
          `${MHOST_BIN} stop ${shellEscape(args.name)} 2>&1`,
          { encoding: "utf-8", timeout: 15_000 }
        );
        actionLog.push({
          timestamp: Date.now(),
          action: `stop ${args.name}`,
          result: output.trim(),
        });
        break;

      case "start_process":
        output = execSync(
          `${MHOST_BIN} start ${shellEscape(args.name)} 2>&1`,
          { encoding: "utf-8", timeout: 15_000 }
        );
        actionLog.push({
          timestamp: Date.now(),
          action: `start ${args.name}`,
          result: output.trim(),
        });
        break;

      case "scale_process":
        output = execSync(
          `${MHOST_BIN} scale ${shellEscape(args.name)} ${Number(args.instances)} 2>&1`,
          { encoding: "utf-8", timeout: 15_000 }
        );
        actionLog.push({
          timestamp: Date.now(),
          action: `scale ${args.name} ${args.instances}`,
          result: output.trim(),
        });
        break;

      case "get_info":
        output = execSync(
          `${MHOST_BIN} info ${shellEscape(args.name)} 2>&1`,
          { encoding: "utf-8", timeout: 10_000 }
        );
        break;

      case "save_processes":
        output = execSync(`${MHOST_BIN} save 2>&1`, {
          encoding: "utf-8",
          timeout: 10_000,
        });
        actionLog.push({
          timestamp: Date.now(),
          action: "save",
          result: output.trim(),
        });
        break;

      case "send_telegram":
        sendTelegramMessage(String(args.message));
        output = "Message sent to Telegram";
        break;

      default:
        return JSON.stringify({ error: `Unknown tool: ${name}` });
    }

    return JSON.stringify({ success: true, output: output.trim() });
  } catch (err) {
    logError(`Tool ${name} failed: ${err.message}`);
    return JSON.stringify({
      error: err.message,
      stderr: (err.stderr || "").toString().trim(),
    });
  }
}

// ─── LLM Interaction ─────────────────────────────────────────────────────────

/**
 * Send a message to the LLM and run the tool-calling loop until the model
 * produces a final text response (no more tool calls).
 *
 * @param {string | null} userMessage  Optional new user message to append
 * @returns {Promise<string | null>}   Final text response or null
 */
async function callLLM(userMessage) {
  if (userMessage) {
    conversationHistory.push({ role: "user", content: userMessage });
  }

  // Trim history to configured limit
  const limit = config.conversation_history_limit || 20;
  while (conversationHistory.length > limit) {
    conversationHistory.shift();
  }

  const messages = [
    { role: "system", content: buildSystemPrompt() },
    ...conversationHistory,
  ];

  for (let i = 0; i < MAX_TOOL_ITERATIONS; i++) {
    const response = await chatCompletion(messages);
    if (!response) break;

    // LLM wants to call tools
    if (response.tool_calls && response.tool_calls.length > 0) {
      messages.push({
        role: "assistant",
        content: response.content || "",
        tool_calls: response.tool_calls,
      });

      for (const toolCall of response.tool_calls) {
        const fnName = toolCall.function.name;
        const fnArgs = safeParseJson(toolCall.function.arguments || "{}");
        logInfo(`[tool] ${fnName}(${JSON.stringify(fnArgs)})`);
        const result = executeTool(fnName, fnArgs);
        messages.push({
          role: "tool",
          tool_call_id: toolCall.id,
          content: result,
        });
      }
      continue;
    }

    // Final text response — add to history and return
    if (response.content) {
      conversationHistory.push({ role: "assistant", content: response.content });
      return response.content;
    }
    break;
  }

  return null;
}

// ─── OpenAI API ──────────────────────────────────────────────────────────────

/**
 * Call the configured LLM chat completion endpoint.
 * @param {Array} messages
 * @returns {Promise<object | null>}  OpenAI-style message object
 */
async function chatCompletion(messages) {
  const apiKey = resolveEnv(config.api_key || "${OPENAI_API_KEY}");
  if (!apiKey) {
    throw new Error(
      "No LLM API key configured. Set OPENAI_API_KEY or configure ~/.mhost/agent.json"
    );
  }

  const provider = (config.provider || "openai").toLowerCase();
  const isAnthropic = provider === "claude" || provider === "anthropic";

  const body = JSON.stringify(
    isAnthropic
      ? buildAnthropicRequest(messages)
      : buildOpenAiRequest(messages)
  );

  const hostname = isAnthropic ? "api.anthropic.com" : "api.openai.com";
  const reqPath = isAnthropic ? "/v1/messages" : "/v1/chat/completions";
  const headers = isAnthropic
    ? {
        "x-api-key": apiKey,
        "anthropic-version": "2023-06-01",
        "Content-Type": "application/json",
        "Content-Length": Buffer.byteLength(body),
      }
    : {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
        "Content-Length": Buffer.byteLength(body),
      };

  const rawResponse = await httpsPost(hostname, reqPath, headers, body);
  const parsed = safeParseJson(rawResponse);

  if (isAnthropic) {
    return normalizeAnthropicResponse(parsed);
  }
  return parsed.choices?.[0]?.message || null;
}

function buildOpenAiRequest(messages) {
  return {
    model: config.model || "gpt-4o",
    messages,
    tools: TOOLS,
    tool_choice: "auto",
    max_tokens: 2048,
    temperature: 0.3,
  };
}

function buildAnthropicRequest(messages) {
  // Anthropic uses system as a top-level field, not a message role
  const system = messages.find((m) => m.role === "system")?.content || "";
  const nonSystem = messages.filter((m) => m.role !== "system");

  return {
    model: config.model || "claude-sonnet-4-20250514",
    system,
    messages: nonSystem,
    max_tokens: 2048,
    temperature: 0.3,
  };
}

/**
 * Convert an Anthropic response to OpenAI message format.
 * Tool calling from Anthropic is not wired here — only text is returned.
 * @param {object} parsed
 * @returns {object}
 */
function normalizeAnthropicResponse(parsed) {
  const text =
    parsed.content?.find((b) => b.type === "text")?.text ||
    parsed.content?.[0]?.text ||
    "";
  return { content: text, tool_calls: null };
}

// ─── Telegram ────────────────────────────────────────────────────────────────

function resolveTelegramToken() {
  return resolveEnv(config.telegram_token || "${MHOST_TELEGRAM_TOKEN}");
}

function resolveTelegramChatId() {
  return resolveEnv(config.telegram_chat_id || "${MHOST_TELEGRAM_CHAT}");
}

/**
 * Send a Telegram message.  Silently no-ops if credentials are missing.
 * @param {string} text
 */
function sendTelegramMessage(text) {
  const token = resolveTelegramToken();
  const chatId = resolveTelegramChatId();
  if (!token || !chatId) {
    logWarn("Telegram credentials not configured — message not sent");
    return;
  }

  try {
    const body = JSON.stringify({
      chat_id: chatId,
      text,
      parse_mode: "HTML",
    });
    // Use curl so we don't need a third-party HTTP library
    execSync(
      `curl -s -X POST "https://api.telegram.org/bot${token}/sendMessage" ` +
        `-H "Content-Type: application/json" ` +
        `-d ${shellEscapeSingleQuoted(body)}`,
      { timeout: 10_000 }
    );
  } catch (err) {
    logError(`Telegram send failed: ${err.message}`);
  }
}

/**
 * Fetch pending Telegram updates.
 * @returns {Promise<Array>}
 */
async function getTelegramUpdates() {
  const token = resolveTelegramToken();
  if (!token) return [];

  try {
    const raw = execSync(
      `curl -s "https://api.telegram.org/bot${token}/getUpdates?offset=${telegramOffset}&timeout=1"`,
      { encoding: "utf-8", timeout: 8_000 }
    );
    const parsed = safeParseJson(raw);
    return parsed.result || [];
  } catch {
    return [];
  }
}

// ─── Observation ─────────────────────────────────────────────────────────────

/**
 * Run `mhost list` and return the raw output.
 * @returns {string}
 */
function observeProcesses() {
  try {
    return execSync(`${MHOST_BIN} list 2>&1`, {
      encoding: "utf-8",
      timeout: 10_000,
    }).trim();
  } catch {
    return "mhost daemon not responding";
  }
}

// ─── Main Loop ───────────────────────────────────────────────────────────────

async function main() {
  logInfo(
    JSON.stringify({
      level: "info",
      message: "mhost Agent started",
      autonomy: config.autonomy,
      model: config.model,
      observe_interval_seconds: OBSERVE_INTERVAL_MS / 1000,
      pid: process.pid,
    })
  );

  sendTelegramMessage(
    `<b>mhost Agent started</b>\n\n` +
      `Autonomy: ${config.autonomy || "supervised"}\n` +
      `Model: ${config.model || "gpt-4o"}\n` +
      `Observe interval: ${OBSERVE_INTERVAL_MS / 1000}s`
  );

  // How many poll cycles fit in one observe interval?
  const ticksPerObserve = Math.max(1, Math.floor(OBSERVE_INTERVAL_MS / POLL_INTERVAL_MS));

  while (true) {
    try {
      await processTelegramMessages();
      await maybeTriggerObservation(ticksPerObserve);
    } catch (err) {
      logError(`Agent loop error: ${err.message}`);
    }

    tickCount++;
    await sleep(POLL_INTERVAL_MS);
  }
}

/**
 * Fetch and handle any pending Telegram messages.
 */
async function processTelegramMessages() {
  const updates = await getTelegramUpdates();
  const expectedChatId = resolveTelegramChatId();

  for (const update of updates) {
    telegramOffset = (update.update_id || 0) + 1;
    const text = update.message?.text?.trim();
    const incomingChatId = update.message?.chat?.id?.toString();

    if (!text || incomingChatId !== expectedChatId) continue;

    logInfo(`[user] ${text}`);

    if (pendingApproval && isApprovalMessage(text)) {
      await executeApprovedAction();
    } else if (pendingApproval && isDenialMessage(text)) {
      sendTelegramMessage("Cancelled.");
      pendingApproval = null;
    } else {
      const response = await callLLM(text);
      if (response) {
        sendTelegramMessage(response);
      }
    }
  }
}

/**
 * Check if it is time to run an observation cycle and, if so, run it.
 * @param {number} ticksPerObserve
 */
async function maybeTriggerObservation(ticksPerObserve) {
  if (tickCount % ticksPerObserve !== 0) return;

  const currentState = observeProcesses();
  const stateChanged = currentState !== lastObservation;
  const periodicCheck = tickCount % (ticksPerObserve * 10) === 0; // every ~5 min

  if (!stateChanged && !periodicCheck) return;

  lastObservation = currentState;

  // ── Brain self-healing pass ─────────────────────────────────────────────
  // For every troubled process, attempt a brain-driven fix before falling
  // through to the LLM. Processes that are handled here are skipped in the
  // LLM prompt so we avoid wasting tokens on already-resolved issues.
  const brainHandledProcesses = new Set();

  if (brain) {
    const processes = parseAgentProcessList(currentState);

    for (const proc of processes) {
      // Update health score on every cycle for all processes
      brain.updateHealth(proc.name, {
        status: proc.status,
        restarts: proc.restarts,
        error_count: proc.status === "errored" ? 1 : 0,
        memory_trend: brain.detectTrend(proc.name, "memory"),
      });

      // Only investigate troubled processes
      if (proc.status !== "errored" && proc.restarts <= 5) continue;

      // Fetch recent error logs to feed into the decision
      let errorText = "";
      try {
        errorText = execSync(
          `${MHOST_BIN} logs ${shellEscape(proc.name)} --err -n 5 2>&1`,
          { encoding: "utf-8", timeout: 5_000 }
        );
      } catch {
        // Ignore — proceed with empty error text
      }

      const decision = brain.decide(proc.name, errorText, proc.status);
      logInfo(
        `[brain] ${proc.name}: ${decision.reason} (needsLLM=${decision.needsLLM})`
      );

      if (decision.needsLLM) {
        // Inject brain memory context into the prompt for this process
        // (handled below when we build the LLM prompt)
        continue;
      }

      // Self-heal without LLM call
      let result = "success";
      try {
        if (
          decision.action === "restart" ||
          decision.action === "wait-restart"
        ) {
          if (decision.playbook?.wait_ms) {
            await sleep(decision.playbook.wait_ms);
          }
          execSync(`${MHOST_BIN} restart ${shellEscape(proc.name)} 2>&1`, {
            encoding: "utf-8",
            timeout: 15_000,
          });
          sendTelegramMessage(
            `<b>Brain auto-healed</b>\n\nProcess: ${proc.name}\nReason: ${decision.reason}\nAction: restart`
          );
        } else if (decision.action === "stop-escalate") {
          execSync(`${MHOST_BIN} stop ${shellEscape(proc.name)} 2>&1`, {
            encoding: "utf-8",
            timeout: 15_000,
          });
          sendTelegramMessage(
            `<b>Brain escalation</b>\n\nProcess: ${proc.name}\n${decision.reason}\n\nProcess stopped. Manual intervention needed.`
          );
        } else if (decision.action === "notify") {
          sendTelegramMessage(
            `<b>Brain alert</b>\n\nProcess: ${proc.name}\n${decision.reason}`
          );
        }
      } catch (e) {
        result = "failed";
        logError(`Brain action failed for ${proc.name}: ${e.message}`);
      }

      brain.recordIncident({
        process: proc.name,
        error: errorText.substring(0, 200),
        status: proc.status,
        action: decision.action,
        result,
      });

      brainHandledProcesses.add(proc.name);
    }
  }

  // ── Build LLM prompt, injecting brain memory for unresolved issues ──────
  let brainContext = "";
  if (brain) {
    const processes = parseAgentProcessList(currentState);
    for (const proc of processes) {
      if (!brainHandledProcesses.has(proc.name) && proc.status === "errored") {
        brainContext += brain.getMemoryContext(proc.name);
      }
    }
  }

  const prompt =
    `[PERIODIC CHECK] Current process state:\n\n${currentState}\n` +
    (brainContext ? `\n${brainContext}\n` : "") +
    `\nAnalyze this. If everything is healthy, stay silent (do not call send_telegram). ` +
    `If you find issues (crashes, high restarts, errors), investigate with get_logs / ` +
    `get_error_logs and take appropriate action. ` +
    `If autonomy is "supervised" and you need to take a state-changing action, ` +
    `ask for user approval via send_telegram first.`;

  await callLLM(prompt);
}

/**
 * Execute the action that the user just approved.
 */
async function executeApprovedAction() {
  if (!pendingApproval) return;
  const { action, args } = pendingApproval;
  pendingApproval = null;

  const result = executeTool(action, args);
  const parsed = safeParseJson(result);
  const detail = parsed.output || parsed.error || "";
  sendTelegramMessage(
    `Done: ${action}(${JSON.stringify(args)})\n\n<pre>${escapeHtml(detail)}</pre>`
  );
}

function isApprovalMessage(text) {
  const t = text.toLowerCase();
  return t === "yes" || t === "👍" || t === "approve" || t === "do it" || t === "y";
}

function isDenialMessage(text) {
  const t = text.toLowerCase();
  return t === "no" || t === "cancel" || t === "nope" || t === "n";
}

// ─── HTTPS Helper ────────────────────────────────────────────────────────────

/**
 * Make an HTTPS POST request and return the response body as a string.
 * @param {string} hostname
 * @param {string} urlPath
 * @param {object} headers
 * @param {string} body
 * @returns {Promise<string>}
 */
function httpsPost(hostname, urlPath, headers, body) {
  return new Promise((resolve, reject) => {
    const req = https.request(
      { hostname, path: urlPath, method: "POST", headers },
      (res) => {
        let data = "";
        res.on("data", (chunk) => { data += chunk; });
        res.on("end", () => resolve(data));
      }
    );
    req.on("error", reject);
    req.write(body);
    req.end();
  });
}

// ─── Utility Functions ───────────────────────────────────────────────────────

/**
 * Resolve a value that may be an environment variable reference like ${VAR}.
 * @param {string} value
 * @returns {string}
 */
function resolveEnv(value) {
  if (!value) return "";
  const match = value.match(/^\$\{([^}]+)\}$/);
  if (match) return process.env[match[1]] || "";
  return value;
}

/**
 * Safely parse JSON without throwing.
 * @param {string} text
 * @returns {object}
 */
function safeParseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return {};
  }
}

/**
 * Escape a string for use as a shell argument (single quotes).
 * @param {string} value
 * @returns {string}
 */
function shellEscape(value) {
  return `'${String(value).replace(/'/g, "'\\''")}'`;
}

/**
 * Escape a JSON body string so it can be passed inside single quotes to curl.
 * @param {string} value
 * @returns {string}
 */
function shellEscapeSingleQuoted(value) {
  return `'${value.replace(/'/g, "'\\''")}'`;
}

/**
 * Escape HTML special characters for Telegram HTML parse mode.
 * @param {string} text
 * @returns {string}
 */
function escapeHtml(text) {
  return String(text)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function logInfo(msg) {
  console.log(JSON.stringify({ level: "info", ts: new Date().toISOString(), msg }));
}

function logWarn(msg) {
  console.warn(JSON.stringify({ level: "warn", ts: new Date().toISOString(), msg }));
}

function logError(msg) {
  console.error(JSON.stringify({ level: "error", ts: new Date().toISOString(), msg }));
}

// ─── Brain Helpers ───────────────────────────────────────────────────────────

/**
 * Parse the text output of `mhost list` into a compact process array.
 * Each entry: { name: string, status: string, restarts: number }
 *
 * The parser is intentionally lenient — it only extracts lines that contain
 * a recognisable status word and have at least three whitespace-separated
 * tokens after stripping Unicode status indicators.
 *
 * @param {string} output  Raw stdout from `mhost list`
 * @returns {{ name: string, status: string, restarts: number }[]}
 */
function parseAgentProcessList(output) {
  const lines = output.split("\n");
  const results = [];

  for (const line of lines) {
    let status = null;
    if (line.includes("online")) status = "online";
    else if (line.includes("errored")) status = "errored";
    else if (line.includes("stopped")) status = "stopped";
    else if (line.includes("starting")) status = "starting";
    else continue;

    // Strip common Unicode status bullets before tokenising
    const stripped = line.replace(/[●◐◑○✖]/g, "").trim();
    const tokens = stripped.split(/\s+/).filter(Boolean);
    if (tokens.length < 3) continue;

    const name = tokens[1];
    const restarts = parseInt(tokens[tokens.length - 2], 10) || 0;
    results.push({ name, status, restarts });
  }

  return results;
}

// ─── Shutdown ────────────────────────────────────────────────────────────────

process.on("SIGTERM", () => {
  logInfo("Agent shutting down (SIGTERM)");
  sendTelegramMessage("mhost Agent stopped.");
  process.exit(0);
});

process.on("SIGINT", () => {
  logInfo("Agent shutting down (SIGINT)");
  sendTelegramMessage("mhost Agent stopped.");
  process.exit(0);
});

// ─── Entry Point ─────────────────────────────────────────────────────────────

main().catch((err) => {
  logError(`Agent fatal error: ${err.message}`);
  process.exit(1);
});
