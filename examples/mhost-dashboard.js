#!/usr/bin/env node
'use strict';
// mhost Web Dashboard — single-file Node.js server with embedded SPA.
// Usage: node examples/mhost-dashboard.js
//        PORT=8080 node examples/mhost-dashboard.js

const http   = require('http');
const { execSync, spawn } = require('child_process');
const fs     = require('fs');
const path   = require('path');
const os     = require('os');

const PORT     = parseInt(process.env.PORT || '9400', 10);
const MHOST    = process.env.MHOST_BIN || 'mhost';
const LOGS_DIR = process.env.MHOST_LOGS_DIR || path.join(os.homedir(), '.mhost', 'logs');

// ── SSE client registry ───────────────────────────────────────────────────
const sseClients = new Map(); // Map<name, Set<res>>

// ── Helpers ───────────────────────────────────────────────────────────────

function runMhost(args) {
  try {
    return { ok: true, output: execSync(`${MHOST} ${args}`, { encoding: 'utf8', timeout: 15000 }) };
  } catch (err) {
    return { ok: false, output: (err.stderr || err.message || '').trim() };
  }
}

// Parse tabular `mhost list` output (strips ANSI, splits on 2+ spaces).
function parseProcessList(raw) {
  const clean = raw.replace(/\x1B\[[0-9;]*m/g, '');
  const STATUS_RE = /\b(online|stopped|starting|stopping|errored)\b/i;
  const out = [];
  for (const line of clean.split('\n')) {
    if (!STATUS_RE.test(line)) continue;
    const cols = line.trim().split(/\s{2,}/);
    if (cols.length < 3) continue;
    const si = cols.findIndex(c => STATUS_RE.test(c));
    if (si === -1) continue;
    const status   = cols[si].match(STATUS_RE)[1].toLowerCase();
    const namePre  = si > 0 ? cols[si - 1] : '';
    const name     = namePre.replace(/^\d+\s+/, '').trim() || `proc-${out.length}`;
    const after    = cols.slice(si + 1);
    const pid      = after[0] && /^\d+$/.test(after[0].trim()) ? parseInt(after[0], 10) : null;
    const uptime   = after[1]?.trim() !== '–' ? after[1]?.trim() || null : null;
    const restarts = after[2] && /^\d+$/.test(after[2].trim()) ? parseInt(after[2], 10) : 0;
    const memory   = after[3]?.trim() !== '–' ? after[3]?.trim() || null : null;
    out.push({ name, status, pid, uptime, restarts, memory });
  }
  return out;
}

function jsonRes(res, status, body) {
  res.writeHead(status, { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' });
  res.end(JSON.stringify(body));
}
function errRes(res, code, msg) { jsonRes(res, code, { error: msg }); }

// ── Route handlers ────────────────────────────────────────────────────────

function handleProcesses(res) {
  const r = runMhost('list');
  const processes = parseProcessList(r.output);
  jsonRes(res, 200, { processes, daemonUp: r.ok || processes.length > 0 });
}

function handleLogs(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`logs ${name} -n 100`);
  jsonRes(res, 200, { name, lines: r.output.split('\n').filter(l => l.trim()), ok: r.ok });
}

function handleAction(res, name, action) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  if (!['restart', 'stop', 'start'].includes(action)) return errRes(res, 400, 'Unknown action');
  const r = runMhost(`${action} ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleActionAll(res, action) {
  if (!['restart', 'stop'].includes(action)) return errRes(res, 400, 'Unknown action');
  const r = runMhost(`${action} all`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleDelete(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`delete ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleHealth(res) {
  const r = runMhost('ping');
  jsonRes(res, 200, { ok: r.ok, output: r.output.trim() });
}

function handleLogStream(res, name) {
  if (!isValidName(name)) { res.writeHead(400); return res.end(); }
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    'Connection': 'keep-alive',
    'Access-Control-Allow-Origin': '*',
  });
  res.write('retry: 2000\n\n');
  if (!sseClients.has(name)) sseClients.set(name, new Set());
  sseClients.get(name).add(res);
  const send = (l) => { if (!res.writableEnded) res.write(`data: ${JSON.stringify(l)}\n\n`); };
  const tryTail = (file, label) => {
    if (!fs.existsSync(file)) return null;
    const p = spawn('tail', ['-F', '-n', '50', file]);
    p.stdout.on('data', buf => buf.toString().split('\n').filter(l => l.trim()).forEach(l => send(`[${label}] ${l}`)));
    p.stderr.on('data', () => {});
    p.on('error', () => {});
    return p;
  };
  const tOut = tryTail(path.join(LOGS_DIR, `${name}-0-out.log`), 'out');
  const tErr = tryTail(path.join(LOGS_DIR, `${name}-0-err.log`), 'err');
  if (!tOut && !tErr) send(`Waiting for ${name} logs…`);
  const cleanup = () => { sseClients.get(name)?.delete(res); tOut?.kill(); tErr?.kill(); };
  res.on('close', cleanup);
  res.on('error', cleanup);
}

function isValidName(n) { return n && /^[a-zA-Z0-9_\-.]+$/.test(n); }

// ── New route handlers ───────────────────────────────────────────────────

function handleMetrics(res, name) {
  const args = name ? `metrics show ${name}` : 'metrics show';
  const r = runMhost(args);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleHealthName(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`health ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleConfig(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`config ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleEnv(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`env ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleHistory(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`history ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleBrainStatus(res) {
  const r = runMhost('brain status');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleBrainHistory(res) {
  const r = runMhost('brain history');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleBrainPlaybooks(res) {
  const r = runMhost('brain playbooks');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleAiDiagnose(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`ai diagnose ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleAiOptimize(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`ai optimize ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleAiAsk(res, question) {
  if (!question || typeof question !== 'string' || question.trim().length === 0) {
    return errRes(res, 400, 'Question is required');
  }
  const sanitized = question.replace(/"/g, '\\"').replace(/\$/g, '\\$').replace(/`/g, '\\`');
  const r = runMhost(`ai ask "${sanitized}"`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleNotifyList(res) {
  const r = runMhost('notify list');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleNotifyTest(res, channel) {
  if (!isValidName(channel)) return errRes(res, 400, 'Invalid channel');
  const r = runMhost(`notify test ${channel}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleSnapshotCreate(res, name) {
  if (!name || typeof name !== 'string') return errRes(res, 400, 'Snapshot name is required');
  if (!isValidName(name)) return errRes(res, 400, 'Invalid snapshot name');
  const r = runMhost(`snapshot create ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleSnapshotList(res) {
  const r = runMhost('snapshot list');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleSnapshotRestore(res, name) {
  if (!name || typeof name !== 'string') return errRes(res, 400, 'Snapshot name is required');
  if (!isValidName(name)) return errRes(res, 400, 'Invalid snapshot name');
  const r = runMhost(`snapshot restore ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleCost(res) {
  const r = runMhost('cost');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleSla(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`sla ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleLink(res) {
  const r = runMhost('link');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleCloudServices(res) {
  const r = runMhost('cloud services');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleCloudCost(res) {
  const r = runMhost('cloud cost');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleCloudDrift(res) {
  const r = runMhost('cloud drift');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleScale(res, name, instances) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const n = parseInt(instances, 10);
  if (isNaN(n) || n < 1 || n > 10) return errRes(res, 400, 'Instances must be 1-10');
  const r = runMhost(`scale ${name} ${n}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleReload(res, name) {
  if (!isValidName(name)) return errRes(res, 400, 'Invalid name');
  const r = runMhost(`reload ${name}`);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleSave(res) {
  const r = runMhost('save');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleResurrect(res) {
  const r = runMhost('resurrect');
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleNotifySave(res, config) {
  // Write channel config to notify.json via mhost notify setup is interactive,
  // so we write directly to the config file
  const os = require('os');
  const path = require('path');
  const notifyPath = path.join(os.homedir(), '.mhost', 'notify.json');
  try {
    let existing = {};
    if (fs.existsSync(notifyPath)) {
      existing = JSON.parse(fs.readFileSync(notifyPath, 'utf8'));
    }
    if (!existing.channels) existing.channels = {};
    existing.channels[config.name || config.type] = {
      type: config.type,
      token: config.token,
      chat_id: config.chat_id || undefined,
      events: config.events || ['crash', 'restart', 'errored'],
      enabled: true,
    };
    fs.writeFileSync(notifyPath, JSON.stringify(existing, null, 2));
    jsonRes(res, 200, { ok: true, output: 'Channel saved' });
  } catch (e) {
    jsonRes(res, 500, { ok: false, output: e.message });
  }
}

function handleDockerList(res) { const r = runMhost('docker list'); jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output }); }
function handleTemplateList(res) { const r = runMhost('template list'); jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output }); }
function handleAudit(res) { const r = runMhost('audit'); jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output }); }
function handlePluginList(res) { const r = runMhost('plugin list'); jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output }); }
function handleCron(res) { const r = runMhost('cron'); jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output }); }
function handleWorkspaceList(res) { const r = runMhost('workspace list'); jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output }); }
function handleHooksList(res) { const r = runMhost('hooks list'); jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output }); }

function handleCloudProvision(res, body) {
  const args = ['cloud', 'provision',
    '--provider', body.provider || 'railway',
    '--name', body.name || 'app',
    '--type', 'container',
  ];
  if (body.image) args.push('--image', body.image);
  if (body.port) args.push('--port', String(body.port));
  if (body.region) args.push('--region', body.region);
  if (body.instances) args.push('--instances', String(body.instances));
  const r = runMhost(args.join(' '));
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

function handleCloudExport(res, format) {
  if (!format) format = 'terraform';
  const r = runMhost('cloud export ' + format);
  jsonRes(res, r.ok ? 200 : 500, { ok: r.ok, output: r.output });
}

// ── Body parser helper ───────────────────────────────────────────────────

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', c => chunks.push(c));
    req.on('end', () => {
      try { resolve(JSON.parse(Buffer.concat(chunks).toString())); }
      catch { resolve({}); }
    });
    req.on('error', reject);
  });
}

// ── Router ────────────────────────────────────────────────────────────────

async function handleRequest(req, res) {
  const { method, url } = req;
  const p = url.split('?')[0].split('/').filter(Boolean);
  if (method === 'OPTIONS') {
    res.writeHead(204, {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET,POST,DELETE,OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type',
    });
    return res.end();
  }
  if (method === 'GET' && url === '/') {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    return res.end(HTML);
  }
  if (p[0] !== 'api') return errRes(res, 404, 'Not found');

  // Existing routes
  if (method === 'GET'    && p[1] === 'health' && !p[2])                                 return handleHealth(res);
  if (method === 'GET'    && p[1] === 'processes')                                       return handleProcesses(res);
  if (method === 'GET'    && p[1] === 'logs' && p[2] && !p[3])                          return handleLogs(res, decodeURIComponent(p[2]));
  if (method === 'GET'    && p[1] === 'logs' && p[2] && p[3] === 'stream')              return handleLogStream(res, decodeURIComponent(p[2]));
  if (method === 'POST'   && p[1] === 'process' && p[2] && p[3] === 'scale')            { const b = await readBody(req); return handleScale(res, decodeURIComponent(p[2]), b.instances); }
  if (method === 'POST'   && p[1] === 'process' && p[2] && p[3] === 'reload')           return handleReload(res, decodeURIComponent(p[2]));
  if (method === 'POST'   && p[1] === 'process' && p[2] && p[3])                        return handleAction(res, decodeURIComponent(p[2]), p[3]);
  if (method === 'POST'   && p[1] === 'all' && p[2])                                    return handleActionAll(res, p[2]);
  if (method === 'DELETE' && p[1] === 'process' && p[2])                                return handleDelete(res, decodeURIComponent(p[2]));

  // Metrics
  if (method === 'GET'    && p[1] === 'metrics' && p[2])                                return handleMetrics(res, decodeURIComponent(p[2]));
  if (method === 'GET'    && p[1] === 'metrics' && !p[2])                               return handleMetrics(res, null);

  // Health (per-process)
  if (method === 'GET'    && p[1] === 'health' && p[2])                                 return handleHealthName(res, decodeURIComponent(p[2]));

  // Config & Info
  if (method === 'GET'    && p[1] === 'config' && p[2])                                 return handleConfig(res, decodeURIComponent(p[2]));
  if (method === 'GET'    && p[1] === 'env' && p[2])                                    return handleEnv(res, decodeURIComponent(p[2]));
  if (method === 'GET'    && p[1] === 'history' && p[2])                                return handleHistory(res, decodeURIComponent(p[2]));

  // Brain
  if (method === 'GET'    && p[1] === 'brain' && p[2] === 'status')                     return handleBrainStatus(res);
  if (method === 'GET'    && p[1] === 'brain' && p[2] === 'history')                    return handleBrainHistory(res);
  if (method === 'GET'    && p[1] === 'brain' && p[2] === 'playbooks')                  return handleBrainPlaybooks(res);

  // AI
  if (method === 'POST'   && p[1] === 'ai' && p[2] === 'diagnose' && p[3])             return handleAiDiagnose(res, decodeURIComponent(p[3]));
  if (method === 'POST'   && p[1] === 'ai' && p[2] === 'optimize' && p[3])             return handleAiOptimize(res, decodeURIComponent(p[3]));
  if (method === 'POST'   && p[1] === 'ai' && p[2] === 'ask')                          { const b = await readBody(req); return handleAiAsk(res, b.question); }

  // Notifications
  if (method === 'GET'    && p[1] === 'notify' && p[2] === 'list')                      return handleNotifyList(res);
  if (method === 'POST'   && p[1] === 'notify' && p[2] === 'test' && p[3])             return handleNotifyTest(res, decodeURIComponent(p[3]));

  // Snapshots
  if (method === 'POST'   && p[1] === 'snapshots' && p[2] === 'create')                { const b = await readBody(req); return handleSnapshotCreate(res, b.name); }
  if (method === 'GET'    && p[1] === 'snapshots' && !p[2])                             return handleSnapshotList(res);
  if (method === 'POST'   && p[1] === 'snapshots' && p[2] === 'restore')               { const b = await readBody(req); return handleSnapshotRestore(res, b.name); }

  // Cost & SLA
  if (method === 'GET'    && p[1] === 'cost' && !p[2])                                  return handleCost(res);
  if (method === 'GET'    && p[1] === 'sla' && p[2])                                    return handleSla(res, decodeURIComponent(p[2]));

  // Dependencies
  if (method === 'GET'    && p[1] === 'link' && !p[2])                                  return handleLink(res);

  // Notify save (from dashboard setup wizard)
  if (method === 'POST'   && p[1] === 'notify' && p[2] === 'save')                     { const b = await readBody(req); return handleNotifySave(res, b); }

  // Cloud Native
  if (method === 'GET'    && p[1] === 'cloud' && p[2] === 'services')                   return handleCloudServices(res);
  if (method === 'GET'    && p[1] === 'cloud' && p[2] === 'cost')                       return handleCloudCost(res);
  if (method === 'GET'    && p[1] === 'cloud' && p[2] === 'drift')                      return handleCloudDrift(res);
  if (method === 'POST'   && p[1] === 'cloud' && p[2] === 'provision')                 { const b = await readBody(req); return handleCloudProvision(res, b); }
  if (method === 'GET'    && p[1] === 'cloud' && p[2] === 'export')                     { const u = new URL(req.url, 'http://x'); return handleCloudExport(res, u.searchParams.get('format')); }

  // Save & Resurrect
  if (method === 'POST'   && p[1] === 'save')                                           return handleSave(res);
  if (method === 'POST'   && p[1] === 'resurrect')                                      return handleResurrect(res);

  // Docker
  if (method === 'GET' && p[1] === 'docker' && p[2] === 'list')                         return handleDockerList(res);
  // Templates
  if (method === 'GET' && p[1] === 'templates')                                         return handleTemplateList(res);
  // Audit
  if (method === 'GET' && p[1] === 'audit')                                             return handleAudit(res);
  // Plugins
  if (method === 'GET' && p[1] === 'plugins')                                           return handlePluginList(res);
  // Cron
  if (method === 'GET' && p[1] === 'cron')                                              return handleCron(res);
  // Workspaces
  if (method === 'GET' && p[1] === 'workspaces')                                        return handleWorkspaceList(res);
  // Hooks
  if (method === 'GET' && p[1] === 'hooks')                                             return handleHooksList(res);

  errRes(res, 404, 'Not found');
}

// ── HTML / SPA ────────────────────────────────────────────────────────────
const HTML = `<!DOCTYPE html>
<html lang="en"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>mhost Dashboard</title>
<style>
*,*::before,*::after{margin:0;padding:0;box-sizing:border-box}
:root{
  --bg:#0d1117;--bg2:#161b22;--bg3:#1c2333;--bg4:#21283b;
  --text:#e2e8f0;--text2:#94a3b8;--text3:#64748b;
  --accent:#8b5cf6;--accent2:#a78bfa;--accent3:#c4b5fd;
  --green:#22c55e;--green2:#4ade80;--red:#ef4444;--yellow:#fbbf24;
  --border:#30363d;
  --font:system-ui,-apple-system,'Segoe UI',sans-serif;
  --mono:'JetBrains Mono','Fira Code','SF Mono',monospace;
  --radius:8px;
}
html,body{height:100%;background:var(--bg);color:var(--text);font-family:var(--font);font-size:14px;line-height:1.5}
.app{display:flex;flex-direction:column;min-height:100vh}

/* ── Header ── */
header{position:sticky;top:0;z-index:50;background:var(--bg2);border-bottom:1px solid var(--border);padding:0 24px;height:56px;display:flex;align-items:center;justify-content:space-between;gap:12px}
.logo{font-size:1.1rem;font-weight:800;color:var(--text);letter-spacing:-.5px;display:flex;align-items:center;gap:8px}
.logo-icon{width:26px;height:26px;border-radius:6px;background:linear-gradient(135deg,var(--accent),var(--accent2));display:flex;align-items:center;justify-content:center;font-size:.8rem}
.logo span{color:var(--accent2)}
.hdr-center{display:flex;align-items:center;gap:16px;flex:1;justify-content:center}
.conn-badge{display:flex;align-items:center;gap:6px;background:var(--bg3);border:1px solid var(--border);border-radius:20px;padding:4px 12px;font-size:.75rem;color:var(--text2)}
.dot{width:7px;height:7px;border-radius:50%;background:var(--text3);transition:background .4s;flex-shrink:0}
.dot.ok{background:var(--green);box-shadow:0 0 6px var(--green)}
.dot.err{background:var(--red);box-shadow:0 0 6px var(--red)}
.refresh-ts{font-size:.72rem;color:var(--text3)}
.hdr-r{display:flex;align-items:center;gap:8px}
.hbtn{background:transparent;border:1px solid var(--border);color:var(--text2);padding:5px 12px;border-radius:var(--radius);cursor:pointer;font-size:.76rem;font-weight:500;transition:all .15s;white-space:nowrap}
.hbtn:hover{border-color:var(--accent2);color:var(--accent2)}
.hbtn.danger:hover{border-color:var(--red);color:var(--red)}
.hbtn.primary{background:var(--accent);color:#fff;border-color:var(--accent)}.hbtn.primary:hover{background:var(--accent2)}

/* ── Tab bar ── */
.tab-bar{background:var(--bg2);border-bottom:1px solid var(--border);padding:0 24px;display:flex;gap:0;overflow-x:auto}
.tab-btn{padding:10px 18px;font-size:.8rem;font-weight:600;color:var(--text3);cursor:pointer;border:none;background:none;border-bottom:2px solid transparent;transition:all .15s;white-space:nowrap}
.tab-btn:hover{color:var(--text2)}
.tab-btn.active{color:var(--accent2);border-bottom-color:var(--accent)}

/* ── Main layout ── */
main{flex:1;padding:24px;max-width:1400px;width:100%;margin:0 auto}
.tab-panel{display:none}.tab-panel.active{display:block}

/* ── Analytics strip ── */
.analytics{display:grid;grid-template-columns:repeat(5,1fr);gap:12px;margin-bottom:24px}
.stat-card{background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius);padding:14px 16px;transition:border-color .2s}
.stat-card:hover{border-color:rgba(139,92,246,.3)}
.stat-num{font-size:1.8rem;font-weight:800;letter-spacing:-1px;line-height:1.1}
.stat-label{font-size:.72rem;color:var(--text3);margin-top:4px;text-transform:uppercase;letter-spacing:.5px;font-weight:600}
.stat-num.green{color:var(--green2)}
.stat-num.red{color:var(--red)}
.stat-num.yellow{color:var(--yellow)}
.stat-num.accent{color:var(--accent3)}
.fleet-bar-wrap{margin-top:8px;height:4px;background:var(--bg4);border-radius:2px;overflow:hidden}
.fleet-bar{height:100%;background:linear-gradient(90deg,var(--green),var(--green2));border-radius:2px;transition:width .6s ease}

/* ── Section header ── */
.sec-hdr{display:flex;align-items:center;justify-content:space-between;margin-bottom:14px}
.sec-title{font-size:.75rem;font-weight:700;text-transform:uppercase;letter-spacing:.8px;color:var(--text3)}

/* ── Process cards grid ── */
.cards-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:14px;margin-bottom:24px}
.proc-card{background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius);padding:16px 18px;cursor:pointer;transition:all .2s;position:relative;overflow:hidden}
.proc-card:hover{border-color:rgba(139,92,246,.3);transform:translateY(-2px);box-shadow:0 8px 30px rgba(0,0,0,.3)}
.proc-card.active{border-color:var(--accent);background:var(--bg3)}
.proc-card::before{content:'';position:absolute;top:0;left:0;right:0;height:2px;opacity:0;transition:opacity .2s}
.proc-card:hover::before,.proc-card.active::before{opacity:1}
.proc-card.s-online::before{background:linear-gradient(90deg,transparent,var(--green),transparent)}
.proc-card.s-errored::before{background:linear-gradient(90deg,transparent,var(--red),transparent)}
.proc-card.s-stopped::before{background:linear-gradient(90deg,transparent,var(--text3),transparent)}
.proc-card.s-starting::before,.proc-card.s-stopping::before{background:linear-gradient(90deg,transparent,var(--yellow),transparent)}

.card-top{display:flex;align-items:flex-start;justify-content:space-between;margin-bottom:10px}
.card-name{font-weight:700;font-size:.95rem;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;max-width:180px}
.chip{display:inline-flex;align-items:center;gap:5px;font-size:.72rem;font-weight:600;padding:2px 8px;border-radius:10px}
.s-online .chip{background:rgba(34,197,94,.12);color:var(--green2)}
.s-stopped .chip{background:rgba(100,116,139,.12);color:var(--text3)}
.s-errored .chip{background:rgba(239,68,68,.12);color:var(--red)}
.s-starting .chip,.s-stopping .chip{background:rgba(251,191,36,.12);color:var(--yellow)}
.cdot{width:6px;height:6px;border-radius:50%;flex-shrink:0}
.s-online .cdot{background:var(--green);animation:glow-g 2s ease-in-out infinite}
.s-errored .cdot{background:var(--red);animation:glow-r 1.5s infinite}
.s-starting .cdot,.s-stopping .cdot{background:var(--yellow);animation:py 1s infinite}
.s-stopped .cdot{background:var(--text3)}
@keyframes glow-g{0%,100%{box-shadow:0 0 3px var(--green)}50%{box-shadow:0 0 8px var(--green)}}
@keyframes glow-r{0%,100%{box-shadow:0 0 3px var(--red)}50%{box-shadow:0 0 8px var(--red)}}
@keyframes py{0%,100%{opacity:1}50%{opacity:.3}}

.card-meta{display:flex;gap:16px;margin-bottom:10px}
.meta-item{display:flex;flex-direction:column;gap:1px}
.meta-val{font-size:.82rem;font-family:var(--mono);color:var(--text);font-weight:500}
.meta-lbl{font-size:.67rem;color:var(--text3);text-transform:uppercase;letter-spacing:.4px}
.restarts-warn{color:var(--yellow)}

.card-actions{display:flex;gap:6px;flex-wrap:wrap;opacity:0;transition:opacity .15s}
.proc-card:hover .card-actions,.proc-card.active .card-actions{opacity:1}
.abt{padding:4px 10px;border-radius:6px;cursor:pointer;font-size:.72rem;font-weight:600;border:1px solid transparent;transition:all .15s}
.ar{background:rgba(139,92,246,.1);color:var(--accent3);border-color:rgba(139,92,246,.3)}.ar:hover{background:rgba(139,92,246,.22)}
.as{background:rgba(251,191,36,.08);color:var(--yellow);border-color:rgba(251,191,36,.25)}.as:hover{background:rgba(251,191,36,.18)}
.ast{background:rgba(34,197,94,.08);color:var(--green2);border-color:rgba(34,197,94,.25)}.ast:hover{background:rgba(34,197,94,.18)}
.ad{background:rgba(239,68,68,.08);color:var(--red);border-color:rgba(239,68,68,.2)}.ad:hover{background:rgba(239,68,68,.18)}
.arl{background:rgba(59,130,246,.08);color:#60a5fa;border-color:rgba(59,130,246,.2)}.arl:hover{background:rgba(59,130,246,.18)}
.off{opacity:.35;pointer-events:none}

/* ── Scale select ── */
.scale-wrap{display:inline-flex;align-items:center;gap:4px}
.scale-sel{background:var(--bg3);color:var(--text2);border:1px solid var(--border);border-radius:4px;padding:2px 4px;font-size:.72rem;font-family:var(--mono);cursor:pointer}

/* ── Env expandable ── */
.env-toggle{font-size:.7rem;color:var(--accent2);cursor:pointer;margin-top:6px;display:inline-block}
.env-box{margin-top:6px;padding:8px 10px;background:var(--bg);border:1px solid var(--border);border-radius:6px;font-family:var(--mono);font-size:.7rem;color:var(--text2);max-height:150px;overflow-y:auto;white-space:pre-wrap;word-break:break-all;display:none}
.env-box.open{display:block}

/* ── Log panel ── */
.log-panel{background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius);margin-bottom:24px;overflow:hidden}
.log-header{padding:12px 16px;background:var(--bg3);border-bottom:1px solid var(--border);display:flex;align-items:center;gap:10px;flex-wrap:wrap}
.log-proc-name{font-weight:700;color:var(--accent3);font-size:.9rem}
.log-line-count{font-size:.72rem;color:var(--text3);background:var(--bg4);padding:2px 8px;border-radius:10px;font-family:var(--mono)}
.log-search{background:var(--bg);border:1px solid var(--border);border-radius:6px;padding:4px 10px;font-size:.75rem;color:var(--text);font-family:var(--mono);outline:none;width:180px}
.log-search:focus{border-color:var(--accent)}
.log-follow-label{display:flex;align-items:center;gap:5px;font-size:.72rem;color:var(--text3);cursor:pointer;user-select:none;margin-left:auto}
.log-follow-label input{accent-color:var(--accent)}
.log-box{height:280px;overflow-y:auto;padding:14px 16px;font-family:var(--mono);font-size:.73rem;line-height:1.7;color:var(--text2)}
.log-box::-webkit-scrollbar{width:4px}
.log-box::-webkit-scrollbar-thumb{background:var(--border);border-radius:2px}
.ll{white-space:pre-wrap;word-break:break-all}
.ll.err{color:rgba(239,68,68,.9)}
.ll.warn{color:rgba(251,191,36,.85)}
.ll.hidden{display:none}
.ll.highlight{background:rgba(139,92,246,.15);border-radius:3px}
.ll.new{animation:fadein .25s ease}
@keyframes fadein{from{opacity:0;transform:translateY(2px)}to{opacity:1;transform:none}}

/* ── Generic panel card ── */
.panel-card{background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius);padding:18px 20px;margin-bottom:16px}
.panel-card h3{font-size:.85rem;font-weight:700;margin-bottom:12px;color:var(--text)}
.panel-card .sub{font-size:.72rem;color:var(--text3);margin-bottom:8px;text-transform:uppercase;letter-spacing:.5px}

/* ── Output block (CLI output) ── */
.output-block{background:var(--bg);border:1px solid var(--border);border-radius:6px;padding:12px 14px;font-family:var(--mono);font-size:.73rem;line-height:1.7;color:var(--text2);max-height:400px;overflow-y:auto;white-space:pre-wrap;word-break:break-all}
.output-block:empty::after{content:'No data';color:var(--text3)}

/* ── Metrics cards ── */
.metrics-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));gap:14px}
.metric-card{background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius);padding:16px 18px}
.metric-card h4{font-size:.85rem;font-weight:700;margin-bottom:10px;display:flex;align-items:center;gap:8px}
.metric-bar{height:6px;background:var(--bg4);border-radius:3px;margin:6px 0;overflow:hidden}
.metric-bar-fill{height:100%;border-radius:3px;transition:width .4s ease}
.metric-row{display:flex;justify-content:space-between;align-items:center;padding:4px 0;font-size:.78rem}
.metric-row .lbl{color:var(--text3)}
.metric-row .val{font-family:var(--mono);font-weight:600}

/* ── Brain health bars ── */
.health-bar-wrap{margin-bottom:10px}
.health-bar-label{display:flex;justify-content:space-between;font-size:.78rem;margin-bottom:4px}
.health-bar-label .name{color:var(--text)}
.health-bar-label .score{font-family:var(--mono);font-weight:700}
.health-bar-bg{height:8px;background:var(--bg4);border-radius:4px;overflow:hidden}
.health-bar-fg{height:100%;border-radius:4px;transition:width .5s ease}

/* ── AI panel ── */
.ai-input-row{display:flex;gap:8px;margin-bottom:16px}
.ai-input{flex:1;background:var(--bg);border:1px solid var(--border);border-radius:var(--radius);padding:10px 14px;color:var(--text);font-size:.85rem;font-family:var(--font);outline:none}
.ai-input:focus{border-color:var(--accent)}
.ai-input::placeholder{color:var(--text3)}
.ai-result{margin-top:12px}

/* ── System grid ── */
.sys-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(340px,1fr));gap:16px}
.sys-actions{display:flex;gap:8px;flex-wrap:wrap;margin-bottom:12px}

/* ── Loading spinner ── */
.loading{display:inline-flex;align-items:center;gap:8px;color:var(--text3);font-size:.8rem}
.spinner{width:16px;height:16px;border:2px solid var(--border);border-top-color:var(--accent);border-radius:50%;animation:spin .6s linear infinite}
@keyframes spin{to{transform:rotate(360deg)}}

/* ── Empty state ── */
.empty{padding:60px 24px;text-align:center;color:var(--text3)}
.empty-icon{font-size:2.5rem;margin-bottom:12px;opacity:.3}

/* ── Skeleton loader ── */
.skeleton{background:linear-gradient(90deg,var(--bg3) 25%,var(--bg4) 50%,var(--bg3) 75%);background-size:200% 100%;animation:shimmer 1.5s infinite;border-radius:var(--radius)}
@keyframes shimmer{0%{background-position:200% 0}100%{background-position:-200% 0}}
.skel-card{height:130px;border-radius:var(--radius)}

/* ── Toast ── */
.toast{position:fixed;bottom:24px;right:24px;background:var(--bg3);border:1px solid var(--border);border-radius:10px;padding:12px 20px;font-size:.82rem;color:var(--text);box-shadow:0 8px 32px rgba(0,0,0,.5);opacity:0;transform:translateY(10px);transition:all .25s;pointer-events:none;z-index:200;max-width:360px}
.toast.show{opacity:1;transform:none}
.toast.ok{border-color:rgba(34,197,94,.4);color:var(--green2)}
.toast.err{border-color:rgba(239,68,68,.4);color:var(--red)}

/* ── Footer ── */
footer{background:var(--bg2);border-top:1px solid var(--border);padding:10px 24px;display:flex;align-items:center;gap:16px;font-size:.73rem;color:var(--text3);flex-wrap:wrap}
footer span{color:var(--text2)}

@media(max-width:768px){
  .analytics{grid-template-columns:repeat(2,1fr)}
  .cards-grid{grid-template-columns:1fr}
  .metrics-grid{grid-template-columns:1fr}
  .sys-grid{grid-template-columns:1fr}
  .hdr-center{display:none}
  main{padding:12px}
  header{padding:0 12px}
  .tab-bar{padding:0 12px}
  .tab-btn{padding:8px 12px;font-size:.75rem}
}
</style>
</head>
<body>
<div class="app">

<header>
  <div class="logo">
    <div class="logo-icon">M</div>
    m<span>host</span>
  </div>
  <div class="hdr-center">
    <div class="conn-badge">
      <div id="dot" class="dot"></div>
      <span id="lbl">Connecting...</span>
    </div>
    <span class="refresh-ts" id="ts"></span>
  </div>
  <div class="hdr-r">
    <button class="hbtn" onclick="refresh()">Refresh</button>
    <button class="hbtn" onclick="actAll('restart')">Restart All</button>
    <button class="hbtn danger" onclick="actAll('stop')">Stop All</button>
  </div>
</header>

<div class="tab-bar">
  <button class="tab-btn active" data-tab="processes" onclick="switchTab('processes')">Processes</button>
  <button class="tab-btn" data-tab="metrics" onclick="switchTab('metrics')">Metrics</button>
  <button class="tab-btn" data-tab="logs" onclick="switchTab('logs')">Logs</button>
  <button class="tab-btn" data-tab="brain" onclick="switchTab('brain')">Brain</button>
  <button class="tab-btn" data-tab="ai" onclick="switchTab('ai')">AI</button>
  <button class="tab-btn" data-tab="cloud" onclick="switchTab('cloud')">Cloud</button>
  <button class="tab-btn" data-tab="system" onclick="switchTab('system')">System</button>
</div>

<main>
  <!-- ═══ PROCESSES TAB ═══ -->
  <div class="tab-panel active" id="panel-processes">
    <div class="analytics" id="analytics">
      <div class="stat-card"><div class="stat-num accent" id="s-total">--</div><div class="stat-label">Total Processes</div></div>
      <div class="stat-card"><div class="stat-num green" id="s-online">--</div><div class="stat-label">Online</div></div>
      <div class="stat-card"><div class="stat-num red" id="s-offline">--</div><div class="stat-label">Offline / Errored</div></div>
      <div class="stat-card"><div class="stat-num yellow" id="s-restarts">--</div><div class="stat-label">Total Restarts</div></div>
      <div class="stat-card">
        <div class="stat-num accent" id="s-health">--%</div>
        <div class="stat-label">Fleet Health</div>
        <div class="fleet-bar-wrap"><div class="fleet-bar" id="fleet-bar" style="width:0%"></div></div>
      </div>
    </div>
    <div class="sec-hdr"><span class="sec-title">Processes</span></div>
    <div class="cards-grid" id="cards">
      <div class="skeleton skel-card"></div>
      <div class="skeleton skel-card"></div>
      <div class="skeleton skel-card"></div>
    </div>
    <div id="proc-log-panel" style="display:none"></div>
  </div>

  <!-- ═══ METRICS TAB ═══ -->
  <div class="tab-panel" id="panel-metrics">
    <div class="sec-hdr">
      <span class="sec-title">Process Metrics</span>
      <span class="refresh-ts" id="metrics-ts"></span>
    </div>
    <div class="metrics-grid" id="metrics-grid">
      <div class="loading"><div class="spinner"></div> Loading metrics...</div>
    </div>
  </div>

  <!-- ═══ LOGS TAB ═══ -->
  <div class="tab-panel" id="panel-logs">
    <div class="sec-hdr"><span class="sec-title">Log Viewer</span></div>
    <div style="margin-bottom:14px">
      <label style="font-size:.78rem;color:var(--text3)">Select process:</label>
      <select id="log-proc-select" onchange="loadLogsTab(this.value)" style="background:var(--bg3);color:var(--text);border:1px solid var(--border);border-radius:6px;padding:6px 10px;font-size:.8rem;margin-left:8px"></select>
    </div>
    <div id="logs-tab-panel"></div>
  </div>

  <!-- ═══ BRAIN TAB ═══ -->
  <div class="tab-panel" id="panel-brain">
    <div class="sec-hdr"><span class="sec-title">Brain Intelligence</span></div>
    <div class="sys-grid">
      <div class="panel-card">
        <h3>Health Status</h3>
        <div id="brain-status">
          <div class="loading"><div class="spinner"></div> Loading brain status...</div>
        </div>
      </div>
      <div class="panel-card">
        <h3>Incident History</h3>
        <div id="brain-history">
          <div class="loading"><div class="spinner"></div> Loading incident history...</div>
        </div>
      </div>
      <div class="panel-card">
        <h3>Playbook Rules</h3>
        <div id="brain-playbooks">
          <div class="loading"><div class="spinner"></div> Loading playbooks...</div>
        </div>
      </div>
    </div>
  </div>

  <!-- ═══ AI TAB ═══ -->
  <div class="tab-panel" id="panel-ai">
    <div class="sec-hdr"><span class="sec-title">AI Assistant</span></div>
    <div class="panel-card">
      <h3>Ask AI</h3>
      <div class="ai-input-row">
        <input class="ai-input" id="ai-question" type="text" placeholder="Ask mhost AI a question..." onkeydown="if(event.key==='Enter')askAI()">
        <button class="hbtn primary" onclick="askAI()">Ask</button>
      </div>
      <div id="ai-ask-result"></div>
    </div>
    <div class="sec-hdr" style="margin-top:20px"><span class="sec-title">Per-Process AI Actions</span></div>
    <div class="cards-grid" id="ai-process-cards">
      <div class="loading"><div class="spinner"></div> Loading processes...</div>
    </div>
    <div id="ai-result-panel"></div>
  </div>

  <!-- ═══ CLOUD TAB ═══ -->
  <div class="tab-panel" id="panel-cloud">
    <div class="sec-hdr">
      <span class="sec-title">Cloud Native</span>
      <button class="hbtn primary" onclick="toggleCloudProvision()">+ Provision Service</button>
    </div>
    <div id="cloud-provision-form" style="display:none;margin-bottom:20px;padding:16px;background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius)">
      <div style="font-weight:600;margin-bottom:12px">Provision New Cloud Service</div>
      <div style="display:grid;grid-template-columns:1fr 1fr;gap:10px;max-width:600px">
        <div>
          <label style="font-size:.72rem;color:var(--text3)">Provider</label>
          <select id="cp-provider" style="width:100%;background:var(--bg3);color:var(--text);border:1px solid var(--border);border-radius:6px;padding:6px 10px;font-size:.8rem">
            <option value="railway">Railway</option><option value="fly">Fly.io</option>
            <option value="aws">AWS</option><option value="gcp">GCP</option><option value="azure">Azure</option>
            <option value="vercel">Vercel</option><option value="digitalocean">DigitalOcean</option>
            <option value="cloudflare">Cloudflare</option><option value="netlify">Netlify</option><option value="supabase">Supabase</option>
          </select>
        </div>
        <div>
          <label style="font-size:.72rem;color:var(--text3)">Service Name</label>
          <input id="cp-name" class="ai-input" type="text" placeholder="my-api" style="width:100%;padding:6px 10px;font-size:.8rem">
        </div>
        <div>
          <label style="font-size:.72rem;color:var(--text3)">Image</label>
          <input id="cp-image" class="ai-input" type="text" placeholder="node:20" style="width:100%;padding:6px 10px;font-size:.8rem">
        </div>
        <div>
          <label style="font-size:.72rem;color:var(--text3)">Port</label>
          <input id="cp-port" class="ai-input" type="number" placeholder="3000" style="width:100%;padding:6px 10px;font-size:.8rem">
        </div>
        <div>
          <label style="font-size:.72rem;color:var(--text3)">Region</label>
          <input id="cp-region" class="ai-input" type="text" placeholder="us-east-1" style="width:100%;padding:6px 10px;font-size:.8rem">
        </div>
        <div>
          <label style="font-size:.72rem;color:var(--text3)">Instances</label>
          <input id="cp-instances" class="ai-input" type="number" value="1" style="width:100%;padding:6px 10px;font-size:.8rem">
        </div>
      </div>
      <div class="sys-actions" style="margin-top:12px">
        <button class="hbtn primary" onclick="doCloudProvision()">Provision</button>
        <button class="hbtn" onclick="toggleCloudProvision()">Cancel</button>
      </div>
      <div id="cp-result" style="margin-top:8px"></div>
    </div>
    <div class="sys-grid">
      <div class="panel-card">
        <h3>Cloud Services</h3>
        <button class="hbtn" onclick="loadCloudServices()" style="margin-bottom:10px">Refresh</button>
        <div id="cloud-services">
          <div class="loading"><div class="spinner"></div> Loading cloud services...</div>
        </div>
      </div>
      <div class="panel-card">
        <h3>Cost Breakdown</h3>
        <button class="hbtn" onclick="loadCloudCost()" style="margin-bottom:10px">Refresh</button>
        <div id="cloud-cost">
          <div class="loading"><div class="spinner"></div> Loading cost data...</div>
        </div>
      </div>
      <div class="panel-card">
        <h3>Drift Detection</h3>
        <button class="hbtn" onclick="loadCloudDrift()" style="margin-bottom:10px">Refresh</button>
        <div id="cloud-drift">
          <div class="loading"><div class="spinner"></div> Loading drift status...</div>
        </div>
      </div>
    </div>
  </div>

  <!-- ═══ SYSTEM TAB ═══ -->
  <div class="tab-panel" id="panel-system">
    <div class="sec-hdr"><span class="sec-title">System Management</span></div>
    <div class="sys-grid">
      <div class="panel-card">
        <h3>Snapshots</h3>
        <div class="sys-actions">
          <input id="snap-name" class="ai-input" type="text" placeholder="Snapshot name" style="max-width:200px;padding:6px 10px;font-size:.8rem">
          <button class="hbtn primary" onclick="createSnapshot()">Create</button>
          <button class="hbtn" onclick="loadSnapshots()">Refresh List</button>
        </div>
        <div id="snapshot-list">
          <div class="loading"><div class="spinner"></div> Loading snapshots...</div>
        </div>
        <div id="snapshot-restore-ui" style="margin-top:10px"></div>
      </div>
      <div class="panel-card">
        <h3>Notifications</h3>
        <div class="sys-actions" style="margin-bottom:10px">
          <button class="hbtn" onclick="loadNotifications()">Refresh</button>
          <button class="hbtn primary" onclick="toggleNotifySetup()">+ Add Channel</button>
        </div>
        <div id="notify-setup" style="display:none;margin-bottom:14px;padding:14px;background:var(--bg3);border-radius:var(--radius);border:1px solid var(--border)">
          <div style="font-weight:600;margin-bottom:10px">Add Notification Channel</div>
          <label style="font-size:.78rem;color:var(--text3)">Type:</label>
          <select id="notify-type" style="background:var(--bg);color:var(--text);border:1px solid var(--border);border-radius:6px;padding:6px 10px;font-size:.8rem;margin-left:8px;margin-bottom:8px">
            <option value="telegram">Telegram</option>
            <option value="slack">Slack</option>
            <option value="discord">Discord</option>
            <option value="webhook">Webhook</option>
            <option value="email">Email</option>
            <option value="pagerduty">PagerDuty</option>
            <option value="teams">Microsoft Teams</option>
            <option value="ntfy">Ntfy</option>
          </select><br>
          <label style="font-size:.78rem;color:var(--text3)">Channel name:</label>
          <input id="notify-name" class="ai-input" type="text" placeholder="e.g. my-telegram" style="max-width:200px;padding:6px 10px;font-size:.8rem;margin-bottom:8px"><br>
          <label style="font-size:.78rem;color:var(--text3)">Token/Webhook URL:</label>
          <input id="notify-token" class="ai-input" type="text" placeholder="Bot token or webhook URL" style="max-width:360px;padding:6px 10px;font-size:.8rem;margin-bottom:8px"><br>
          <label style="font-size:.78rem;color:var(--text3)">Chat ID (Telegram only):</label>
          <input id="notify-chatid" class="ai-input" type="text" placeholder="e.g. 987654321" style="max-width:200px;padding:6px 10px;font-size:.8rem;margin-bottom:12px"><br>
          <label style="font-size:.78rem;color:var(--text3)">Events:</label>
          <div style="display:flex;flex-wrap:wrap;gap:6px;margin:6px 0 12px">
            <label style="font-size:.72rem"><input type="checkbox" class="notify-event" value="crash" checked> crash</label>
            <label style="font-size:.72rem"><input type="checkbox" class="notify-event" value="restart" checked> restart</label>
            <label style="font-size:.72rem"><input type="checkbox" class="notify-event" value="errored" checked> errored</label>
            <label style="font-size:.72rem"><input type="checkbox" class="notify-event" value="stopped"> stopped</label>
            <label style="font-size:.72rem"><input type="checkbox" class="notify-event" value="recovered"> recovered</label>
            <label style="font-size:.72rem"><input type="checkbox" class="notify-event" value="health_fail" checked> health_fail</label>
            <label style="font-size:.72rem"><input type="checkbox" class="notify-event" value="deploy_success"> deploy</label>
            <label style="font-size:.72rem"><input type="checkbox" class="notify-event" value="oom_kill"> oom</label>
          </div>
          <div class="sys-actions">
            <button class="hbtn primary" onclick="addNotifyChannel()">Save Channel</button>
            <button class="hbtn" onclick="toggleNotifySetup()">Cancel</button>
          </div>
          <div id="notify-setup-result" style="margin-top:8px"></div>
        </div>
        <div id="notify-list">
          <div class="loading"><div class="spinner"></div> Loading notifications...</div>
        </div>
      </div>
      <div class="panel-card">
        <h3>Save & Resurrect</h3>
        <p style="font-size:.78rem;color:var(--text3);margin-bottom:12px">Save current process list and resurrect previously saved processes.</p>
        <div class="sys-actions">
          <button class="hbtn primary" onclick="doSave()">Save</button>
          <button class="hbtn" onclick="doResurrect()">Resurrect</button>
        </div>
        <div id="save-result"></div>
      </div>
      <div class="panel-card">
        <h3>Cost Overview</h3>
        <button class="hbtn" onclick="loadCost()" style="margin-bottom:10px">Refresh</button>
        <div id="cost-output">
          <div class="loading"><div class="spinner"></div> Loading cost data...</div>
        </div>
      </div>
      <div class="panel-card">
        <h3>SLA Report</h3>
        <div style="margin-bottom:10px">
          <label style="font-size:.78rem;color:var(--text3)">Process:</label>
          <select id="sla-proc-select" onchange="loadSla(this.value)" style="background:var(--bg3);color:var(--text);border:1px solid var(--border);border-radius:6px;padding:6px 10px;font-size:.8rem;margin-left:8px"></select>
        </div>
        <div id="sla-output"></div>
      </div>
      <div class="panel-card">
        <h3>Dependency Graph</h3>
        <button class="hbtn" onclick="loadLink()" style="margin-bottom:10px">Refresh</button>
        <div id="link-output">
          <div class="loading"><div class="spinner"></div> Loading dependencies...</div>
        </div>
      </div>
      <div class="panel-card">
        <h3>Export Infrastructure as Code</h3>
        <p style="font-size:.78rem;color:var(--text3);margin-bottom:12px">Generate IaC files from your current cloud services configuration.</p>
        <div class="sys-actions">
          <button class="hbtn" onclick="exportIaC('terraform')">Terraform</button>
          <button class="hbtn" onclick="exportIaC('docker-compose')">Docker Compose</button>
          <button class="hbtn" onclick="exportIaC('kubernetes')">Kubernetes</button>
        </div>
        <pre id="export-output" style="display:none;margin-top:12px;background:var(--bg);border:1px solid var(--border);border-radius:var(--radius);padding:12px;font-size:.75rem;font-family:var(--mono);color:var(--text2);max-height:300px;overflow:auto;white-space:pre-wrap"></pre>
        <div id="export-actions" style="display:none;margin-top:8px">
          <button class="hbtn primary" onclick="copyExport()">Copy to Clipboard</button>
        </div>
      </div>
      <div class="panel-card">
        <h3>Docker Containers</h3>
        <button class="hbtn" onclick="loadDocker()" style="margin-bottom:10px">Refresh</button>
        <div id="docker-output"><div class="loading"><div class="spinner"></div> Loading...</div></div>
      </div>
      <div class="panel-card">
        <h3>Plugins</h3>
        <button class="hbtn" onclick="loadPlugins()" style="margin-bottom:10px">Refresh</button>
        <div id="plugins-output"><div class="loading"><div class="spinner"></div> Loading...</div></div>
      </div>
      <div class="panel-card">
        <h3>Cron Schedules</h3>
        <button class="hbtn" onclick="loadCronDash()" style="margin-bottom:10px">Refresh</button>
        <div id="cron-output"><div class="loading"><div class="spinner"></div> Loading...</div></div>
      </div>
      <div class="panel-card">
        <h3>Audit Trail</h3>
        <button class="hbtn" onclick="loadAuditDash()" style="margin-bottom:10px">Refresh</button>
        <div id="audit-output"><div class="loading"><div class="spinner"></div> Loading...</div></div>
      </div>
      <div class="panel-card">
        <h3>Workspaces</h3>
        <button class="hbtn" onclick="loadWorkspaces()" style="margin-bottom:10px">Refresh</button>
        <div id="workspace-output"><div class="loading"><div class="spinner"></div> Loading...</div></div>
      </div>
      <div class="panel-card">
        <h3>Incoming Webhooks</h3>
        <button class="hbtn" onclick="loadHooks()" style="margin-bottom:10px">Refresh</button>
        <div id="hooks-output"><div class="loading"><div class="spinner"></div> Loading...</div></div>
      </div>
    </div>
  </div>
</main>

<footer>
  <div>Processes: <span id="fc">--</span></div>
  <div>Daemon: <span id="fd">--</span></div>
  <div>Updated: <span id="ft">--</span></div>
</footer>
</div>

<div class="toast" id="toast"></div>

<script>
const S = {
  processes: [],
  expanded: null,
  sse: null,
  follow: true,
  logs: {},
  daemonUp: false,
  activeTab: 'processes',
  metricsInterval: null,
  envCache: {},
};

// ── Utilities ──

function toast(m, t='ok') {
  const e = document.getElementById('toast');
  e.textContent = m; e.className = 'toast show ' + t;
  clearTimeout(e._t); e._t = setTimeout(() => { e.className = 'toast'; }, 3200);
}
function san(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
function fmt(v, f='--') { return (v != null && v !== '') ? v : f; }
async function api(url, o={}) {
  const r = await fetch(url, o);
  if (!r.ok) {
    let msg;
    try { const j = await r.json(); msg = j.error || JSON.stringify(j); } catch { msg = await r.text(); }
    throw new Error(msg);
  }
  return r.json();
}
function loadingHtml(msg) { return '<div class="loading"><div class="spinner"></div> ' + san(msg) + '</div>'; }
function outputBlock(text) { return '<div class="output-block">' + san(text || '') + '</div>'; }

// ── Tab switching ──

function switchTab(tab) {
  S.activeTab = tab;
  document.querySelectorAll('.tab-btn').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
  document.querySelectorAll('.tab-panel').forEach(p => p.classList.toggle('active', p.id === 'panel-' + tab));

  if (tab === 'metrics') loadMetrics();
  if (tab === 'logs') populateLogSelect();
  if (tab === 'brain') loadBrainTab();
  if (tab === 'ai') renderAiProcessCards();
  if (tab === 'cloud') loadCloudTab();
  if (tab === 'system') loadSystemTab();

  // Start/stop metrics auto-refresh
  if (tab === 'metrics') {
    if (!S.metricsInterval) S.metricsInterval = setInterval(loadMetrics, 5000);
  } else {
    if (S.metricsInterval) { clearInterval(S.metricsInterval); S.metricsInterval = null; }
  }
}

// ── Process tab (enhanced) ──

function chipHtml(s) {
  const k = (s || 'unknown').toLowerCase();
  return '<span class="chip"><span class="cdot"></span>' + k + '</span>';
}

function cardBtns(p) {
  const n = p.name, s = p.status;
  const canR  = s === 'online' || s === 'errored' || s === 'stopped';
  const canSt = s === 'online' || s === 'starting';
  const canGo = s === 'stopped' || s === 'errored';
  const stop  = canSt
    ? '<button class="abt as" onclick="event.stopPropagation();act(\\'' + n + '\\',\\'stop\\')">Stop</button>'
    : '<button class="abt ast ' + (canGo?'':'off') + '" onclick="event.stopPropagation();act(\\'' + n + '\\',\\'start\\')">Start</button>';
  return '<div class="card-actions">'
    + '<button class="abt ar ' + (canR?'':'off') + '" onclick="event.stopPropagation();act(\\'' + n + '\\',\\'restart\\')">Restart</button>'
    + stop
    + '<button class="abt arl" onclick="event.stopPropagation();reloadProc(\\'' + n + '\\')">Reload</button>'
    + '<div class="scale-wrap"><select class="scale-sel" onclick="event.stopPropagation()" onchange="event.stopPropagation();scaleProc(\\'' + n + '\\',this.value)">'
    + [1,2,3,4,5,6,7,8,9,10].map(i => '<option value="' + i + '">' + i + '</option>').join('')
    + '</select><span style="font-size:.65rem;color:var(--text3)">scale</span></div>'
    + '<button class="abt ad" onclick="event.stopPropagation();del(\\'' + n + '\\')">Delete</button>'
    + '</div>';
}

function envToggleHtml(name) {
  return '<span class="env-toggle" onclick="event.stopPropagation();toggleEnv(\\'' + name + '\\')">Show env vars</span>'
    + '<div class="env-box" id="env-' + name + '"></div>';
}

function renderCards(ps) {
  if (!ps.length) return '<div class="empty"><div class="empty-icon">--</div><p>No processes. Run <code>mhost start &lt;app&gt;</code></p></div>';
  return ps.map(p => {
    const k = (p.status || 'unknown').toLowerCase();
    const exp = S.expanded === p.name;
    return '<div class="proc-card s-' + k + (exp ? ' active' : '') + '" onclick="toggle(\\'' + p.name + '\\')">'
      + '<div class="card-top">'
      + '<div class="card-name">' + san(p.name) + '</div>'
      + chipHtml(p.status)
      + '</div>'
      + '<div class="card-meta">'
      + '<div class="meta-item"><div class="meta-val">' + fmt(p.pid) + '</div><div class="meta-lbl">PID</div></div>'
      + '<div class="meta-item"><div class="meta-val">' + fmt(p.uptime) + '</div><div class="meta-lbl">Uptime</div></div>'
      + '<div class="meta-item"><div class="meta-val ' + (p.restarts > 0 ? 'restarts-warn' : '') + '">' + p.restarts + '</div><div class="meta-lbl">Restarts</div></div>'
      + '<div class="meta-item"><div class="meta-val">' + fmt(p.memory) + '</div><div class="meta-lbl">Memory</div></div>'
      + '</div>'
      + cardBtns(p)
      + envToggleHtml(p.name)
      + '</div>';
  }).join('');
}

async function toggleEnv(name) {
  const el = document.getElementById('env-' + name);
  if (!el) return;
  if (el.classList.contains('open')) {
    el.classList.remove('open');
    return;
  }
  el.innerHTML = 'Loading...';
  el.classList.add('open');
  try {
    const d = await api('/api/env/' + encodeURIComponent(name));
    el.textContent = d.output || 'No env vars found';
  } catch (e) { el.textContent = 'Error: ' + e.message; }
}

async function scaleProc(name, instances) {
  try {
    const r = await api('/api/process/' + encodeURIComponent(name) + '/scale', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ instances: parseInt(instances, 10) }),
    });
    toast(r.ok ? name + ' scaled to ' + instances : (r.output || 'Scale failed'), r.ok ? 'ok' : 'err');
    await refresh();
  } catch (e) { toast(String(e), 'err'); }
}

async function reloadProc(name) {
  try {
    const r = await api('/api/process/' + encodeURIComponent(name) + '/reload', { method: 'POST' });
    toast(r.ok ? name + ' reloaded (zero-downtime)' : (r.output || 'Reload failed'), r.ok ? 'ok' : 'err');
    await refresh();
  } catch (e) { toast(String(e), 'err'); }
}

function logLineClass(line) {
  const l = line.toLowerCase();
  if (l.includes('[err]') || /\\berror\\b/.test(l) || l.includes('exception')) return 'err';
  if (l.includes('warn') || l.includes('[warn]')) return 'warn';
  return '';
}

function renderLogPanel(p) {
  const lines = S.logs[p.name] || [];
  const lineCount = lines.length;
  const html = lines.length
    ? lines.map(l => '<div class="ll ' + l.cls + '">' + san(l.t) + '</div>').join('')
    : '<div class="ll" style="color:var(--text3)">No logs yet...</div>';
  return '<div class="log-panel">'
    + '<div class="log-header">'
    + '<span class="log-proc-name">' + san(p.name) + '</span>'
    + '<span class="log-line-count">' + lineCount + ' lines</span>'
    + '<input class="log-search" placeholder="Search logs..." oninput="filterLogs(this.value)">'
    + '<label class="log-follow-label">'
    + '<input type="checkbox" id="fchk" ' + (S.follow ? 'checked' : '') + ' onchange="S.follow=this.checked">'
    + 'Auto-scroll</label>'
    + '</div>'
    + '<div class="log-box" id="lbox">' + html + '</div>'
    + '</div>';
}

function filterLogs(query) {
  const box = document.getElementById('lbox');
  if (!box) return;
  const q = query.toLowerCase();
  box.querySelectorAll('.ll').forEach(el => {
    if (!q) {
      el.classList.remove('hidden', 'highlight');
      return;
    }
    const match = el.textContent.toLowerCase().includes(q);
    el.classList.toggle('hidden', !match);
    el.classList.toggle('highlight', match);
  });
}

function updateAnalytics(ps) {
  const total   = ps.length;
  const online  = ps.filter(p => p.status === 'online').length;
  const offline = ps.filter(p => p.status === 'stopped' || p.status === 'errored').length;
  const restarts = ps.reduce((a, p) => a + (p.restarts || 0), 0);
  const health  = total ? Math.round((online / total) * 100) : 0;
  document.getElementById('s-total').textContent    = total;
  document.getElementById('s-online').textContent   = online;
  document.getElementById('s-offline').textContent  = offline;
  document.getElementById('s-restarts').textContent = restarts;
  document.getElementById('s-health').textContent   = health + '%';
  document.getElementById('fleet-bar').style.width  = health + '%';
}

function render(d) {
  S.processes = d.processes || []; S.daemonUp = d.daemonUp !== false;
  const online = S.processes.filter(p => p.status === 'online').length;
  document.getElementById('dot').className = 'dot ' + (S.daemonUp ? 'ok' : 'err');
  document.getElementById('lbl').textContent = S.daemonUp ? 'Connected' : 'Daemon offline';
  document.getElementById('ts').textContent  = 'Updated ' + new Date().toLocaleTimeString();
  document.getElementById('fc').textContent  = S.processes.length + ' (' + online + ' online)';
  document.getElementById('fd').textContent  = S.daemonUp ? 'Running' : 'Offline';
  document.getElementById('ft').textContent  = new Date().toLocaleTimeString();
  updateAnalytics(S.processes);

  const savedScroll = document.getElementById('lbox')?.scrollTop;
  document.getElementById('cards').innerHTML = renderCards(S.processes);
  const panelEl = document.getElementById('proc-log-panel');
  if (S.expanded) {
    const p = S.processes.find(x => x.name === S.expanded);
    if (p) { panelEl.style.display = ''; panelEl.innerHTML = renderLogPanel(p); }
    else   { panelEl.style.display = 'none'; }
  } else { panelEl.style.display = 'none'; }
  const lb = document.getElementById('lbox');
  if (lb) lb.scrollTop = S.follow ? lb.scrollHeight : (savedScroll ?? lb.scrollHeight);
}

function startSSE(name) {
  if (S.sse) { S.sse.close(); S.sse = null; }
  const es = new EventSource('/api/logs/' + encodeURIComponent(name) + '/stream');
  S.sse = es;
  es.onmessage = e => {
    let l; try { l = JSON.parse(e.data); } catch { l = e.data; }
    const cls = logLineClass(String(l));
    if (!S.logs[name]) S.logs[name] = [];
    S.logs[name].push({ t: String(l), cls: cls });
    if (S.logs[name].length > 500) S.logs[name].shift();
    if (S.expanded === name) {
      const box = document.getElementById('lbox');
      if (!box) return;
      const el = document.createElement('div');
      el.className = 'll new ' + cls;
      el.textContent = String(l);
      box.appendChild(el);
      const cnt = document.querySelector('.log-line-count');
      if (cnt) cnt.textContent = S.logs[name].length + ' lines';
      if (S.follow) box.scrollTop = box.scrollHeight;
    }
  };
  es.onerror = () => {};
}

async function toggle(name) {
  if (S.expanded === name) {
    S.expanded = null;
    if (S.sse) { S.sse.close(); S.sse = null; }
  } else {
    S.expanded = name;
    try {
      const d = await api('/api/logs/' + encodeURIComponent(name));
      S.logs[name] = (d.lines || []).map(t => ({ t: t, cls: logLineClass(t) }));
    } catch { S.logs[name] = []; }
    startSSE(name);
  }
  const p = S.processes.find(x => x.name === S.expanded);
  const panelEl = document.getElementById('proc-log-panel');
  if (S.expanded && p) { panelEl.style.display = ''; panelEl.innerHTML = renderLogPanel(p); }
  else                 { panelEl.style.display = 'none'; }
  const lb = document.getElementById('lbox');
  if (lb && S.follow) lb.scrollTop = lb.scrollHeight;
}

async function act(name, action) {
  try {
    const r = await api('/api/process/' + encodeURIComponent(name) + '/' + action, { method: 'POST' });
    toast(r.ok ? name + ' ' + action + 'd' : (r.output || 'Failed'), r.ok ? 'ok' : 'err');
    await refresh();
  } catch(e) { toast(String(e), 'err'); }
}

async function actAll(action) {
  if (!confirm(action + ' ALL processes?')) return;
  try {
    const r = await api('/api/all/' + action, { method: 'POST' });
    toast(r.ok ? 'All processes ' + action + 'ped' : (r.output || 'Failed'), r.ok ? 'ok' : 'err');
    await refresh();
  } catch(e) { toast(String(e), 'err'); }
}

async function del(name) {
  if (!confirm('Delete "' + name + '"? This cannot be undone.')) return;
  try {
    const r = await api('/api/process/' + encodeURIComponent(name), { method: 'DELETE' });
    toast(r.ok ? name + ' deleted' : (r.output || 'Failed'), r.ok ? 'ok' : 'err');
    if (S.expanded === name) { S.expanded = null; if (S.sse) { S.sse.close(); S.sse = null; } }
    await refresh();
  } catch(e) { toast(String(e), 'err'); }
}

async function refresh() {
  try { render(await api('/api/processes')); }
  catch {
    document.getElementById('dot').className = 'dot err';
    document.getElementById('lbl').textContent = 'Connection error';
    document.getElementById('fd').textContent  = 'Offline';
  }
}

// ── Metrics tab ──

async function loadMetrics() {
  const grid = document.getElementById('metrics-grid');
  if (!S.processes.length) {
    grid.innerHTML = '<div class="empty"><p>No processes running</p></div>';
    return;
  }
  try {
    const d = await api('/api/metrics');
    const ts = document.getElementById('metrics-ts');
    if (ts) ts.textContent = 'Updated ' + new Date().toLocaleTimeString();

    // Parse the output into per-process metric cards
    const output = (d.output || '').replace(/\\x1B\\[[0-9;]*m/g, '');
    let cards = '';
    for (const p of S.processes) {
      cards += '<div class="metric-card">'
        + '<h4><span class="cdot" style="background:' + (p.status === 'online' ? 'var(--green)' : 'var(--text3)') + ';width:8px;height:8px;border-radius:50%;display:inline-block"></span> ' + san(p.name) + '</h4>'
        + '<div class="metric-row"><span class="lbl">Status</span><span class="val">' + p.status + '</span></div>'
        + '<div class="metric-row"><span class="lbl">Memory</span><span class="val">' + fmt(p.memory) + '</span></div>'
        + '<div class="metric-row"><span class="lbl">Uptime</span><span class="val">' + fmt(p.uptime) + '</span></div>'
        + '<div class="metric-row"><span class="lbl">Restarts</span><span class="val">' + p.restarts + '</span></div>'
        + '<div class="metric-row"><span class="lbl">PID</span><span class="val">' + fmt(p.pid) + '</span></div>'
        + '</div>';
    }
    if (d.ok && output.trim()) {
      cards += '<div class="panel-card" style="grid-column:1/-1"><h3>Raw Metrics Output</h3>' + outputBlock(output) + '</div>';
    }
    grid.innerHTML = cards || '<div class="empty"><p>No metrics data</p></div>';
  } catch (e) {
    grid.innerHTML = '<div class="empty"><p>Error loading metrics: ' + san(e.message) + '</p></div>';
  }
}

// ── Logs tab (standalone) ──

function populateLogSelect() {
  const sel = document.getElementById('log-proc-select');
  if (!sel) return;
  const opts = S.processes.map(p => '<option value="' + p.name + '">' + p.name + ' (' + p.status + ')</option>');
  sel.innerHTML = '<option value="">-- Select --</option>' + opts.join('');
}

async function loadLogsTab(name) {
  const panel = document.getElementById('logs-tab-panel');
  if (!name) { panel.innerHTML = ''; return; }
  panel.innerHTML = loadingHtml('Loading logs for ' + name + '...');
  try {
    const d = await api('/api/logs/' + encodeURIComponent(name));
    const lines = (d.lines || []);
    const html = lines.length
      ? lines.map(l => '<div class="ll ' + logLineClass(l) + '">' + san(l) + '</div>').join('')
      : '<div class="ll" style="color:var(--text3)">No logs available</div>';
    panel.innerHTML = '<div class="log-panel">'
      + '<div class="log-header">'
      + '<span class="log-proc-name">' + san(name) + '</span>'
      + '<span class="log-line-count">' + lines.length + ' lines</span>'
      + '<input class="log-search" placeholder="Search logs..." oninput="filterLogsTab(this.value)">'
      + '</div>'
      + '<div class="log-box" id="lbox-tab">' + html + '</div>'
      + '</div>';
    const lb = document.getElementById('lbox-tab');
    if (lb) lb.scrollTop = lb.scrollHeight;
  } catch (e) { panel.innerHTML = '<div class="empty"><p>Error: ' + san(e.message) + '</p></div>'; }
}

function filterLogsTab(query) {
  const box = document.getElementById('lbox-tab');
  if (!box) return;
  const q = query.toLowerCase();
  box.querySelectorAll('.ll').forEach(el => {
    if (!q) { el.classList.remove('hidden', 'highlight'); return; }
    const match = el.textContent.toLowerCase().includes(q);
    el.classList.toggle('hidden', !match);
    el.classList.toggle('highlight', match);
  });
}

// ── Brain tab ──

async function loadBrainTab() {
  loadBrainStatus();
  loadBrainHistory();
  loadBrainPlaybooks();
}

async function loadBrainStatus() {
  const el = document.getElementById('brain-status');
  el.innerHTML = loadingHtml('Loading brain status...');
  try {
    const d = await api('/api/brain/status');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function loadBrainHistory() {
  const el = document.getElementById('brain-history');
  el.innerHTML = loadingHtml('Loading incident history...');
  try {
    const d = await api('/api/brain/history');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function loadBrainPlaybooks() {
  const el = document.getElementById('brain-playbooks');
  el.innerHTML = loadingHtml('Loading playbooks...');
  try {
    const d = await api('/api/brain/playbooks');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

// ── AI tab ──

function renderAiProcessCards() {
  const grid = document.getElementById('ai-process-cards');
  if (!S.processes.length) {
    grid.innerHTML = '<div class="empty"><p>No processes running</p></div>';
    return;
  }
  grid.innerHTML = S.processes.map(p => {
    const k = (p.status || 'unknown').toLowerCase();
    return '<div class="proc-card s-' + k + '" style="cursor:default">'
      + '<div class="card-top">'
      + '<div class="card-name">' + san(p.name) + '</div>'
      + chipHtml(p.status)
      + '</div>'
      + '<div class="card-actions" style="opacity:1">'
      + '<button class="abt ar" onclick="aiDiagnose(\\'' + p.name + '\\')">Diagnose</button>'
      + '<button class="abt ast" onclick="aiOptimize(\\'' + p.name + '\\')">Optimize</button>'
      + '</div></div>';
  }).join('');
}

async function aiDiagnose(name) {
  const panel = document.getElementById('ai-result-panel');
  panel.innerHTML = '<div class="panel-card"><h3>Diagnosing ' + san(name) + '...</h3>' + loadingHtml('AI is analyzing...') + '</div>';
  try {
    const d = await api('/api/ai/diagnose/' + encodeURIComponent(name), { method: 'POST' });
    panel.innerHTML = '<div class="panel-card"><h3>Diagnosis: ' + san(name) + '</h3>' + outputBlock(d.output) + '</div>';
  } catch (e) {
    panel.innerHTML = '<div class="panel-card"><h3>Diagnosis Failed</h3><p style="color:var(--red)">' + san(e.message) + '</p></div>';
  }
}

async function aiOptimize(name) {
  const panel = document.getElementById('ai-result-panel');
  panel.innerHTML = '<div class="panel-card"><h3>Optimizing ' + san(name) + '...</h3>' + loadingHtml('AI is analyzing...') + '</div>';
  try {
    const d = await api('/api/ai/optimize/' + encodeURIComponent(name), { method: 'POST' });
    panel.innerHTML = '<div class="panel-card"><h3>Optimization: ' + san(name) + '</h3>' + outputBlock(d.output) + '</div>';
  } catch (e) {
    panel.innerHTML = '<div class="panel-card"><h3>Optimization Failed</h3><p style="color:var(--red)">' + san(e.message) + '</p></div>';
  }
}

async function askAI() {
  const input = document.getElementById('ai-question');
  const question = input.value.trim();
  if (!question) return;
  const el = document.getElementById('ai-ask-result');
  el.innerHTML = '<div class="ai-result">' + loadingHtml('Thinking...') + '</div>';
  try {
    const d = await api('/api/ai/ask', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ question: question }),
    });
    el.innerHTML = '<div class="ai-result">' + outputBlock(d.output) + '</div>';
    input.value = '';
  } catch (e) {
    el.innerHTML = '<div class="ai-result"><p style="color:var(--red)">' + san(e.message) + '</p></div>';
  }
}

// ── Cloud tab ──

function loadCloudTab() {
  loadCloudServices();
  loadCloudCost();
  loadCloudDrift();
}

async function loadCloudServices() {
  const el = document.getElementById('cloud-services');
  el.innerHTML = loadingHtml('Loading cloud services...');
  try {
    const d = await api('/api/cloud/services');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function loadCloudCost() {
  const el = document.getElementById('cloud-cost');
  el.innerHTML = loadingHtml('Loading cost data...');
  try {
    const d = await api('/api/cloud/cost');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function loadCloudDrift() {
  const el = document.getElementById('cloud-drift');
  el.innerHTML = loadingHtml('Loading drift status...');
  try {
    const d = await api('/api/cloud/drift');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

// ── System tab ──

function loadSystemTab() {
  loadSnapshots();
  loadNotifications();
  loadCost();
  loadLink();
  populateSlaSelect();
}

async function createSnapshot() {
  const input = document.getElementById('snap-name');
  const name = input.value.trim();
  if (!name) { toast('Snapshot name is required', 'err'); return; }
  try {
    const d = await api('/api/snapshots/create', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: name }),
    });
    toast(d.ok ? 'Snapshot "' + name + '" created' : (d.output || 'Failed'), d.ok ? 'ok' : 'err');
    input.value = '';
    loadSnapshots();
  } catch (e) { toast(String(e), 'err'); }
}

async function loadSnapshots() {
  const el = document.getElementById('snapshot-list');
  el.innerHTML = loadingHtml('Loading snapshots...');
  try {
    const d = await api('/api/snapshots');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function restoreSnapshot(name) {
  if (!confirm('Restore snapshot "' + name + '"?')) return;
  try {
    const d = await api('/api/snapshots/restore', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: name }),
    });
    toast(d.ok ? 'Snapshot restored' : (d.output || 'Failed'), d.ok ? 'ok' : 'err');
    await refresh();
  } catch (e) { toast(String(e), 'err'); }
}

async function loadNotifications() {
  const el = document.getElementById('notify-list');
  el.innerHTML = loadingHtml('Loading notification channels...');
  try {
    const d = await api('/api/notify/list');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function testNotifyChannel(channel) {
  try {
    const d = await api('/api/notify/test/' + encodeURIComponent(channel), { method: 'POST' });
    toast(d.ok ? 'Test notification sent to ' + channel : (d.output || 'Failed'), d.ok ? 'ok' : 'err');
  } catch (e) { toast(String(e), 'err'); }
}

async function doSave() {
  const el = document.getElementById('save-result');
  el.innerHTML = loadingHtml('Saving...');
  try {
    const d = await api('/api/save', { method: 'POST' });
    toast(d.ok ? 'Process list saved' : (d.output || 'Save failed'), d.ok ? 'ok' : 'err');
    el.innerHTML = d.ok ? '<p style="color:var(--green2);font-size:.82rem">Saved successfully</p>' : outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function doResurrect() {
  const el = document.getElementById('save-result');
  el.innerHTML = loadingHtml('Resurrecting...');
  try {
    const d = await api('/api/resurrect', { method: 'POST' });
    toast(d.ok ? 'Processes resurrected' : (d.output || 'Resurrect failed'), d.ok ? 'ok' : 'err');
    el.innerHTML = outputBlock(d.output);
    await refresh();
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function loadCost() {
  const el = document.getElementById('cost-output');
  el.innerHTML = loadingHtml('Loading cost data...');
  try {
    const d = await api('/api/cost');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

function populateSlaSelect() {
  const sel = document.getElementById('sla-proc-select');
  if (!sel) return;
  const opts = S.processes.map(p => '<option value="' + p.name + '">' + p.name + '</option>');
  sel.innerHTML = '<option value="">-- Select --</option>' + opts.join('');
}

async function loadSla(name) {
  const el = document.getElementById('sla-output');
  if (!name) { el.innerHTML = ''; return; }
  el.innerHTML = loadingHtml('Loading SLA report...');
  try {
    const d = await api('/api/sla/' + encodeURIComponent(name));
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

async function loadLink() {
  const el = document.getElementById('link-output');
  el.innerHTML = loadingHtml('Loading dependency graph...');
  try {
    const d = await api('/api/link');
    el.innerHTML = outputBlock(d.output);
  } catch (e) { el.innerHTML = '<p style="color:var(--red);font-size:.82rem">' + san(e.message) + '</p>'; }
}

// ── Notify Setup ──

function toggleNotifySetup() {
  const el = document.getElementById('notify-setup');
  el.style.display = el.style.display === 'none' ? 'block' : 'none';
}

async function addNotifyChannel() {
  const type = document.getElementById('notify-type').value;
  const name = document.getElementById('notify-name').value || type;
  const token = document.getElementById('notify-token').value;
  const chatId = document.getElementById('notify-chatid').value;
  const events = [...document.querySelectorAll('.notify-event:checked')].map(c => c.value);

  if (!token) { toast('Token/URL is required', 'err'); return; }

  // Save via CLI — build notify config
  const config = { type, name, token, events };
  if (chatId) config.chat_id = chatId;

  const resultEl = document.getElementById('notify-setup-result');
  resultEl.innerHTML = '<span style="color:var(--yellow)">Saving channel...</span>';

  // Use mhost notify setup is interactive, so we write directly to notify.json
  try {
    const r = await api('/api/notify/save', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    if (r.ok) {
      toast('Channel "' + name + '" added', 'ok');
      toggleNotifySetup();
      loadNotifications();
    } else {
      resultEl.innerHTML = '<span style="color:var(--red)">' + (r.output || 'Failed to save') + '</span>';
    }
  } catch(e) {
    resultEl.innerHTML = '<span style="color:var(--red)">' + e.message + '</span>';
  }
}

// ── Cloud Provision ──

function toggleCloudProvision() {
  const el = document.getElementById('cloud-provision-form');
  el.style.display = el.style.display === 'none' ? 'block' : 'none';
}

async function doCloudProvision() {
  const provider = document.getElementById('cp-provider').value;
  const name = document.getElementById('cp-name').value;
  const image = document.getElementById('cp-image').value;
  const port = document.getElementById('cp-port').value;
  const region = document.getElementById('cp-region').value || 'us-east-1';
  const instances = document.getElementById('cp-instances').value || '1';

  if (!name) { toast('Service name required', 'err'); return; }

  const resultEl = document.getElementById('cp-result');
  resultEl.innerHTML = '<span style="color:var(--yellow)">Provisioning on ' + provider + '...</span>';

  const r = await api('/api/cloud/provision', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ provider, name, image, port: port ? parseInt(port) : null, region, instances: parseInt(instances) }),
  });

  if (r.ok) {
    toast('Service "' + name + '" provisioned on ' + provider, 'ok');
    toggleCloudProvision();
    loadCloudServices();
  } else {
    resultEl.innerHTML = '<span style="color:var(--red)">' + (r.output || 'Provision failed') + '</span>';
  }
}

// ── IaC Export ──

async function exportIaC(format) {
  const outputEl = document.getElementById('export-output');
  const actionsEl = document.getElementById('export-actions');
  outputEl.style.display = 'block';
  outputEl.textContent = 'Generating ' + format + '...';

  const r = await api('/api/cloud/export?format=' + format);
  if (r.ok && r.output) {
    outputEl.textContent = r.output;
    actionsEl.style.display = 'block';
  } else {
    outputEl.textContent = r.output || 'Export requires cloud services data. Configure cloud providers first.';
    actionsEl.style.display = 'none';
  }
}

function copyExport() {
  const text = document.getElementById('export-output').textContent;
  navigator.clipboard.writeText(text).then(() => toast('Copied to clipboard', 'ok'));
}

// ── New feature panels ──

async function loadDocker() { const el = document.getElementById('docker-output'); el.innerHTML = loadingHtml('Loading...'); try { const r = await api('/api/docker/list'); el.innerHTML = r.ok ? outputBlock(r.output) : '<span style="color:var(--text3)">No Docker containers</span>'; } catch(e) { el.innerHTML = '<span style="color:var(--text3)">No Docker containers</span>'; } }
async function loadPlugins() { const el = document.getElementById('plugins-output'); el.innerHTML = loadingHtml('Loading...'); try { const r = await api('/api/plugins'); el.innerHTML = r.ok ? outputBlock(r.output) : '<span style="color:var(--text3)">No plugins installed</span>'; } catch(e) { el.innerHTML = '<span style="color:var(--text3)">No plugins installed</span>'; } }
async function loadCronDash() { const el = document.getElementById('cron-output'); el.innerHTML = loadingHtml('Loading...'); try { const r = await api('/api/cron'); el.innerHTML = r.ok ? outputBlock(r.output) : '<span style="color:var(--text3)">No cron schedules</span>'; } catch(e) { el.innerHTML = '<span style="color:var(--text3)">No cron schedules</span>'; } }
async function loadAuditDash() { const el = document.getElementById('audit-output'); el.innerHTML = loadingHtml('Loading...'); try { const r = await api('/api/audit'); el.innerHTML = r.ok ? outputBlock(r.output) : '<span style="color:var(--text3)">No audit entries</span>'; } catch(e) { el.innerHTML = '<span style="color:var(--text3)">No audit entries</span>'; } }
async function loadWorkspaces() { const el = document.getElementById('workspace-output'); el.innerHTML = loadingHtml('Loading...'); try { const r = await api('/api/workspaces'); el.innerHTML = r.ok ? outputBlock(r.output) : '<span style="color:var(--text3)">No workspaces</span>'; } catch(e) { el.innerHTML = '<span style="color:var(--text3)">No workspaces</span>'; } }
async function loadHooks() { const el = document.getElementById('hooks-output'); el.innerHTML = loadingHtml('Loading...'); try { const r = await api('/api/hooks'); el.innerHTML = r.ok ? outputBlock(r.output) : '<span style="color:var(--text3)">No webhooks</span>'; } catch(e) { el.innerHTML = '<span style="color:var(--text3)">No webhooks</span>'; } }

// ── Init ──

refresh();
setInterval(refresh, 3000);
</script>
</body></html>`;

// ── Server ────────────────────────────────────────────────────────────────

const server = http.createServer(handleRequest);
server.listen(PORT, () => {
  console.log(JSON.stringify({ level: 'info', message: `mhost Dashboard at http://localhost:${PORT}`, pid: process.pid }));
});

// ── Graceful shutdown ─────────────────────────────────────────────────────

function shutdown(sig) {
  console.log(JSON.stringify({ level: 'info', message: `${sig} — shutting down` }));
  for (const clients of sseClients.values()) for (const r of clients) try { r.end(); } catch (_) {}
  server.close(() => { console.log(JSON.stringify({ level: 'info', message: 'closed' })); process.exit(0); });
  setTimeout(() => process.exit(0), 5000).unref();
}

process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGINT',  () => shutdown('SIGINT'));
