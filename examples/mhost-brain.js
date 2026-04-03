#!/usr/bin/env node
/**
 * mhost Brain — Self-healing intelligence with persistent memory
 *
 * Provides incident memory, healing playbooks, health scores, and trend
 * detection so the agent can fix known issues without consulting the LLM.
 */

"use strict";

const fs = require("fs");
const path = require("path");

// ─── Paths ───────────────────────────────────────────────────────────────────

const BRAIN_DIR = path.join(process.env.HOME || "~", ".mhost", "brain");
const INCIDENTS_FILE = path.join(BRAIN_DIR, "incidents.json");
const PLAYBOOKS_FILE = path.join(BRAIN_DIR, "playbooks.json");
const HEALTH_FILE = path.join(BRAIN_DIR, "health.json");
const TRENDS_FILE = path.join(BRAIN_DIR, "trends.json");

// ─── Brain class ─────────────────────────────────────────────────────────────

class Brain {
  constructor() {
    fs.mkdirSync(BRAIN_DIR, { recursive: true });
    this.incidents = this._load(INCIDENTS_FILE, []);
    this.playbooks = this._load(PLAYBOOKS_FILE, this._defaultPlaybooks());
    this.healthScores = this._load(HEALTH_FILE, {});
    this.trends = this._load(TRENDS_FILE, {});
  }

  // ─── Persistence ───────────────────────────────────────────────────────────

  _load(file, fallback) {
    try {
      return JSON.parse(fs.readFileSync(file, "utf-8"));
    } catch {
      return fallback;
    }
  }

  _save(file, data) {
    fs.writeFileSync(file, JSON.stringify(data, null, 2));
  }

  // ─── Incidents (Memory) ────────────────────────────────────────────────────

  /**
   * Record a new incident and optionally auto-learn a playbook.
   * @param {{ process: string, error: string, status: string, action: string, result: string, timestamp?: string }} incident
   * @returns {object} The stored incident with id/timestamp assigned.
   */
  recordIncident(incident) {
    const stored = Object.assign({}, incident, {
      id: this.incidents.length + 1,
      timestamp: incident.timestamp || new Date().toISOString(),
    });

    const updated = [...this.incidents, stored];
    this.incidents = updated.length > 500 ? updated.slice(-500) : updated;
    this._save(INCIDENTS_FILE, this.incidents);

    // Auto-learn from successful fixes
    if (stored.result === "success" && stored.action && stored.error) {
      this._learnPlaybook(stored);
    }

    return stored;
  }

  /**
   * Find past incidents with a similar error pattern for a process.
   * @param {string} processName
   * @param {string} error
   * @param {number} [limit=5]
   * @returns {object[]}
   */
  findSimilarIncidents(processName, error, limit = 5) {
    const errorWords = (error || "")
      .toLowerCase()
      .split(/\s+/)
      .filter((w) => w.length > 3);

    return this.incidents
      .filter(
        (i) =>
          i.process === processName ||
          errorWords.some((w) => (i.error || "").toLowerCase().includes(w))
      )
      .slice(-limit)
      .reverse();
  }

  /**
   * Return the N most recent incidents for a process.
   * @param {string} processName
   * @param {number} [limit=10]
   * @returns {object[]}
   */
  getProcessHistory(processName, limit = 10) {
    return this.incidents
      .filter((i) => i.process === processName)
      .slice(-limit)
      .reverse();
  }

  // ─── Playbooks (Auto-learned healing rules) ────────────────────────────────

  _defaultPlaybooks() {
    return [
      {
        id: 1,
        name: "port-conflict",
        trigger: "EADDRINUSE",
        action: "wait-restart",
        wait_ms: 5000,
        description: "Port in use — wait 5s for port release then restart",
      },
      {
        id: 2,
        name: "oom-kill",
        trigger: "out of memory",
        action: "restart",
        description: "Out of memory — restart immediately",
      },
      {
        id: 3,
        name: "module-not-found",
        trigger: "Cannot find module",
        action: "notify",
        description: "Missing module — notify (likely bad deploy)",
      },
      {
        id: 4,
        name: "connection-refused",
        trigger: "ECONNREFUSED",
        action: "wait-restart",
        wait_ms: 10000,
        description: "Dependency down — wait 10s then restart",
      },
      {
        id: 5,
        name: "permission-denied",
        trigger: "EACCES",
        action: "notify",
        description: "Permission error — notify (needs manual fix)",
      },
      {
        id: 6,
        name: "disk-full",
        trigger: "ENOSPC",
        action: "notify",
        description: "Disk full — notify immediately",
      },
      {
        id: 7,
        name: "crash-loop",
        trigger: "__crash_loop__",
        action: "stop-escalate",
        description: "3+ crashes in 10min — stop and escalate to human",
      },
      {
        id: 8,
        name: "memory-leak",
        trigger: "__memory_trend__",
        action: "restart",
        description: "Memory growing steadily — preemptive restart",
      },
    ];
  }

  /**
   * Find the first playbook whose trigger appears in the error text.
   * Internal triggers (prefixed with __) are excluded from text matching.
   * @param {string} errorText
   * @returns {object|null}
   */
  findPlaybook(errorText) {
    if (!errorText) return null;
    const lower = errorText.toLowerCase();
    return (
      this.playbooks.find(
        (p) =>
          !p.trigger.startsWith("__") &&
          lower.includes(p.trigger.toLowerCase())
      ) || null
    );
  }

  _learnPlaybook(incident) {
    const errorKey = _extractErrorKey(incident.error);
    if (!errorKey) return;

    const existing = this.playbooks.find((p) => p.trigger === errorKey);
    if (existing) {
      const updated = Object.assign({}, existing, {
        success_count: (existing.success_count || 0) + 1,
      });
      this.playbooks = this.playbooks.map((p) =>
        p.trigger === errorKey ? updated : p
      );
    } else {
      const newPlaybook = {
        id: this.playbooks.length + 1,
        name: `auto-${incident.process}-${Date.now()}`,
        trigger: errorKey,
        action: incident.action,
        description: `Auto-learned from incident #${incident.id}: ${(incident.error || "").substring(0, 80)}`,
        learned: true,
        success_count: 1,
      };
      this.playbooks = [...this.playbooks, newPlaybook];
    }
    this._save(PLAYBOOKS_FILE, this.playbooks);
  }

  // ─── Health Scores ─────────────────────────────────────────────────────────

  /**
   * Compute and persist a health score for a process.
   * @param {string} processName
   * @param {{ status: string, restarts: number, uptime_seconds?: number, error_count?: number, memory_trend?: string }} metrics
   * @returns {number} 0–100
   */
  updateHealth(processName, metrics) {
    const prev = this.healthScores[processName] || { score: 100, history: [] };
    let score = 100;

    if (metrics.status === "errored") score -= 50;
    else if (metrics.status === "stopped") score -= 30;
    else if (metrics.status === "starting") score -= 10;

    if (metrics.restarts > 0) score -= Math.min(metrics.restarts * 5, 30);
    if ((metrics.error_count || 0) > 0)
      score -= Math.min((metrics.error_count || 0) * 3, 20);
    if (metrics.memory_trend === "rising") score -= 10;

    score = Math.max(0, Math.min(100, score));

    const now = new Date().toISOString();
    const newHistory = [...prev.history, { score, timestamp: now }];

    const updated = {
      score,
      last_updated: now,
      history: newHistory.length > 100 ? newHistory.slice(-100) : newHistory,
    };

    this.healthScores = Object.assign({}, this.healthScores, {
      [processName]: updated,
    });
    this._save(HEALTH_FILE, this.healthScores);
    return score;
  }

  /**
   * @param {string} processName
   * @returns {number} 0–100 (defaults to 100 when unknown)
   */
  getHealthScore(processName) {
    return this.healthScores[processName]?.score ?? 100;
  }

  /**
   * @returns {{ name: string, score: number, last_updated: string }[]}
   */
  getAllHealthScores() {
    return Object.entries(this.healthScores).map(([name, data]) => ({
      name,
      score: data.score,
      last_updated: data.last_updated,
    }));
  }

  // ─── Trend Detection ───────────────────────────────────────────────────────

  /**
   * Record a numeric metric data point.
   * @param {string} processName
   * @param {string} metric  e.g. "memory", "cpu"
   * @param {number} value
   */
  recordMetric(processName, metric, value) {
    const processTrends = this.trends[processName] || {};
    const existing = processTrends[metric] || [];
    const updated = [...existing, { value, timestamp: Date.now() }];
    const trimmed = updated.length > 200 ? updated.slice(-200) : updated;

    this.trends = Object.assign({}, this.trends, {
      [processName]: Object.assign({}, processTrends, { [metric]: trimmed }),
    });
    this._save(TRENDS_FILE, this.trends);
  }

  /**
   * Detect whether a metric is rising, falling, or stable using linear regression.
   * @param {string} processName
   * @param {string} metric
   * @returns {"rising"|"falling"|"stable"}
   */
  detectTrend(processName, metric) {
    const data = this.trends[processName]?.[metric];
    if (!data || data.length < 10) return "stable";

    const recent = data.slice(-20);
    const n = recent.length;
    const sumX = recent.reduce((s, _, i) => s + i, 0);
    const sumY = recent.reduce((s, d) => s + d.value, 0);
    const sumXY = recent.reduce((s, d, i) => s + i * d.value, 0);
    const sumX2 = recent.reduce((s, _, i) => s + i * i, 0);

    const slope = (n * sumXY - sumX * sumY) / (n * sumX2 - sumX * sumX);
    const avgY = sumY / n;
    const relativeSlope = slope / (avgY || 1);

    if (relativeSlope > 0.02) return "rising";
    if (relativeSlope < -0.02) return "falling";
    return "stable";
  }

  // ─── Crash Loop Detection ──────────────────────────────────────────────────

  /**
   * Return true if a process has crashed >= threshold times within windowMinutes.
   * @param {string} processName
   * @param {number} [windowMinutes=10]
   * @param {number} [threshold=3]
   * @returns {boolean}
   */
  isCrashLooping(processName, windowMinutes = 10, threshold = 3) {
    const cutoff = new Date(Date.now() - windowMinutes * 60_000).toISOString();
    const recentCrashes = this.incidents.filter(
      (i) =>
        i.process === processName &&
        i.status === "errored" &&
        i.timestamp > cutoff
    );
    return recentCrashes.length >= threshold;
  }

  // ─── Memory Context for LLM ────────────────────────────────────────────────

  /**
   * Build a compact text block summarising past incidents for injection into
   * the LLM system prompt.
   * @param {string} processName
   * @returns {string}
   */
  getMemoryContext(processName) {
    const history = this.getProcessHistory(processName, 5);
    const health = this.getHealthScore(processName);
    const memTrend = this.detectTrend(processName, "memory");
    const crashLooping = this.isCrashLooping(processName);

    let context = `\n[BRAIN MEMORY for ${processName}]\n`;
    context += `Health score: ${health}/100\n`;
    context += `Memory trend: ${memTrend}\n`;
    context += `Crash looping: ${crashLooping}\n`;

    if (history.length > 0) {
      context += `Past incidents:\n`;
      for (const inc of history) {
        context += `  - ${inc.timestamp}: ${(inc.error || "").substring(0, 100)} → ${inc.action} (${inc.result})\n`;
      }
    }

    return context;
  }

  // ─── Self-Heal Decision ────────────────────────────────────────────────────

  /**
   * Determine the best action for a process issue.
   *
   * Decision priority:
   *   1. Crash loop detection
   *   2. Rising memory trend + low health score
   *   3. Known error-pattern playbook
   *   4. Similar past incident that was fixed successfully
   *   5. Unknown → delegate to LLM
   *
   * @param {string} processName
   * @param {string} error
   * @param {string} status
   * @returns {{ action: string|null, playbook: object|null, needsLLM: boolean, reason: string }}
   */
  decide(processName, error, status) {
    // 1. Crash loop
    if (this.isCrashLooping(processName)) {
      return {
        action: "stop-escalate",
        playbook:
          this.playbooks.find((p) => p.trigger === "__crash_loop__") || null,
        needsLLM: false,
        reason: `Crash loop detected for ${processName} (3+ crashes in 10min). Stopping and escalating.`,
      };
    }

    // 2. Memory leak + degraded health
    const memTrend = this.detectTrend(processName, "memory");
    if (memTrend === "rising") {
      const health = this.getHealthScore(processName);
      if (health < 50) {
        return {
          action: "restart",
          playbook:
            this.playbooks.find((p) => p.trigger === "__memory_trend__") ||
            null,
          needsLLM: false,
          reason: `Memory leak detected for ${processName} (health: ${health}/100, trend: rising). Preemptive restart.`,
        };
      }
    }

    // 3. Known playbook
    const playbook = this.findPlaybook(error);
    if (playbook) {
      return {
        action: playbook.action,
        playbook,
        needsLLM: false,
        reason: `Known pattern: ${playbook.description}`,
      };
    }

    // 4. Past successful fix
    const similar = this.findSimilarIncidents(processName, error, 3);
    if (
      similar.length > 0 &&
      similar[0].action &&
      similar[0].result === "success"
    ) {
      return {
        action: similar[0].action,
        playbook: null,
        needsLLM: false,
        reason: `Similar to past incident #${similar[0].id}: "${(similar[0].error || "").substring(0, 60)}". Last fix: ${similar[0].action}`,
      };
    }

    // 5. Unknown
    return {
      action: null,
      playbook: null,
      needsLLM: true,
      reason: "Unknown issue — consulting LLM with memory context",
    };
  }
}

// ─── Utility ─────────────────────────────────────────────────────────────────

/**
 * Extract a short, stable error identifier from a raw error string.
 * @param {string|null|undefined} error
 * @returns {string|null}
 */
function _extractErrorKey(error) {
  if (!error) return null;

  const patterns = [
    /E[A-Z]+/,
    /Cannot find module/,
    /out of memory/i,
    /SIGKILL/,
    /SIGTERM/,
    /timeout/i,
    /connection refused/i,
  ];

  for (const p of patterns) {
    const match = error.match(p);
    if (match) return match[0];
  }

  return null;
}

// ─── Exports ─────────────────────────────────────────────────────────────────

module.exports = { Brain };
