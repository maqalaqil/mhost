#!/usr/bin/env node
'use strict';

// ─── mhost Web Dashboard ───────────────────────────────────────────────────
// Self-contained Node.js HTTP server with embedded SPA.
// Usage:  node examples/mhost-dashboard.js
//         PORT=8080 node examples/mhost-dashboard.js

const http    = require('http');
const { execSync, spawn } = require('child_process');
const fs      = require('fs');
const path    = require('path');
const os      = require('os');

const PORT     = parseInt(process.env.PORT || '9400', 10);
const MHOST    = process.env.MHOST_BIN || 'mhost';
const LOGS_DIR = process.env.MHOST_LOGS_DIR
               || path.join(os.homedir(), '.mhost', 'logs');

// ─── SSE client registry ──────────────────────────────────────────────────
// Map<name, Set<res>> — keeps track of active streaming connections.
const sseClients = new Map();

// ─── Helpers ──────────────────────────────────────────────────────────────

function runMhost(args) {
  try {
    const raw = execSync(`${MHOST} ${args}`, { encoding: 'utf8', timeout: 8000 });
    return { ok: true, output: raw };
  } catch (err) {
    const msg = (err.stderr || err.message || '').trim();
    return { ok: false, output: msg };
  }
}

/**
 * Parse the tabular output of `mhost list` into an array of process objects.
 *
 * The table format (from output.rs) looks like:
 *   <index>  <name>  <status_icon> <status_label>  <pid>  <uptime>  <restarts>  <mem>
 *
 * Strategy: strip ANSI, split on 2+ spaces, match known status keywords.
 */
function parseProcessList(raw) {
  // Strip ANSI escape codes.
  const clean = raw.replace(/\x1B\[[0-9;]*m/g, '');
  const processes = [];
  const STATUS_RE = /\b(online|stopped|starting|stopping|errored)\b/i;

  for (const line of clean.split('\n')) {
    if (!STATUS_RE.test(line)) continue;
    // Split on 2+ whitespace — the table uses double-space as column separator.
    const cols = line.trim().split(/\s{2,}/);
    if (cols.length < 3) continue;

    // Find which column holds the status keyword.
    let statusIdx = -1;
    for (let i = 0; i < cols.length; i++) {
      if (STATUS_RE.test(cols[i])) { statusIdx = i; break; }
    }
    if (statusIdx === -1) continue;

    // Extract status (strip leading icon character like ●, ○, ✖, ◐, ◑).
    const statusMatch = cols[statusIdx].match(STATUS_RE);
    const status = statusMatch ? statusMatch[1].toLowerCase() : 'unknown';

    // The name is the column immediately before the status, falling back to cols[1].
    const namePre = statusIdx > 0 ? cols[statusIdx - 1] : '';
    // Strip leading row-index if present (numeric prefix from the table).
    const name = namePre.replace(/^\d+\s+/, '').trim() || `process-${processes.length}`;

    // Remaining columns after status: PID, uptime, restarts, mem.
    const after = cols.slice(statusIdx + 1);
    const pid      = after[0] && /^\d+$/.test(after[0].trim()) ? parseInt(after[0], 10) : null;
    const uptime   = after[1] && after[1].trim() !== '–' ? after[1].trim() : null;
    const restarts = after[2] && /^\d+$/.test(after[2].trim()) ? parseInt(after[2], 10) : 0;
    const memory   = after[3] && after[3].trim() !== '–' ? after[3].trim() : null;

    processes.push({ name, status, pid, uptime, restarts, memory });
  }

  return processes;
}

function jsonRes(res, status, body) {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Access-Control-Allow-Origin': '*',
  });
  res.end(payload);
}

function errorRes(res, status, message) {
  jsonRes(res, status, { error: message });
}

// ─── Route handlers ───────────────────────────────────────────────────────

function handleGetProcesses(res) {
  const result = runMhost('list');
  if (!result.ok && result.output.toLowerCase().includes('no process')) {
    return jsonRes(res, 200, { processes: [] });
  }
  const processes = parseProcessList(result.output);
  const daemonUp  = result.ok || processes.length > 0;
  jsonRes(res, 200, { processes, daemonUp });
}

function handleGetLogs(res, name) {
  if (!name || /[^a-zA-Z0-9_\-.]/.test(name)) {
    return errorRes(res, 400, 'Invalid process name');
  }
  const result = runMhost(`logs ${name} -n 100`);
  const lines  = result.output.split('\n').filter(l => l.trim());
  jsonRes(res, 200, { name, lines, ok: result.ok });
}

function handleAction(res, name, action) {
  if (!name || /[^a-zA-Z0-9_\-.]/.test(name)) {
    return errorRes(res, 400, 'Invalid process name');
  }
  const allowed = { restart: 'restart', stop: 'stop', start: 'start' };
  const cmd = allowed[action];
  if (!cmd) return errorRes(res, 400, 'Unknown action');

  const result = runMhost(`${cmd} ${name}`);
  jsonRes(res, result.ok ? 200 : 500, { ok: result.ok, output: result.output });
}

function handleDelete(res, name) {
  if (!name || /[^a-zA-Z0-9_\-.]/.test(name)) {
    return errorRes(res, 400, 'Invalid process name');
  }
  const result = runMhost(`delete ${name}`);
  jsonRes(res, result.ok ? 200 : 500, { ok: result.ok, output: result.output });
}

function handleHealth(res) {
  const result = runMhost('ping');
  jsonRes(res, 200, { ok: result.ok, output: result.output.trim() });
}

/**
 * SSE log streaming — tails ~/.mhost/logs/<name>-0-out.log (and -err.log).
 * Falls back to polling `mhost logs` if the log file cannot be tailed.
 */
function handleLogStream(res, name) {
  if (!name || /[^a-zA-Z0-9_\-.]/.test(name)) {
    res.writeHead(400); res.end(); return;
  }

  res.writeHead(200, {
    'Content-Type':  'text/event-stream',
    'Cache-Control': 'no-cache',
    'Connection':    'keep-alive',
    'Access-Control-Allow-Origin': '*',
  });
  res.write('retry: 2000\n\n');

  // Register client.
  if (!sseClients.has(name)) sseClients.set(name, new Set());
  sseClients.get(name).add(res);

  const send = (line) => {
    if (!res.writableEnded) {
      res.write(`data: ${JSON.stringify(line)}\n\n`);
    }
  };

  const outLog = path.join(LOGS_DIR, `${name}-0-out.log`);
  const errLog = path.join(LOGS_DIR, `${name}-0-err.log`);

  let tailOut = null;
  let tailErr = null;

  const tryTail = (logFile, label) => {
    if (!fs.existsSync(logFile)) return null;
    const proc = spawn('tail', ['-F', '-n', '50', logFile]);
    proc.stdout.on('data', (buf) => {
      for (const line of buf.toString().split('\n')) {
        if (line.trim()) send(`[${label}] ${line}`);
      }
    });
    proc.stderr.on('data', () => {});
    proc.on('error', () => {});
    return proc;
  };

  tailOut = tryTail(outLog, 'out');
  tailErr = tryTail(errLog, 'err');

  // If no log files exist yet, send a placeholder and poll.
  if (!tailOut && !tailErr) {
    send(`Waiting for ${name} logs…`);
  }

  const cleanup = () => {
    sseClients.get(name)?.delete(res);
    tailOut?.kill();
    tailErr?.kill();
  };

  res.on('close', cleanup);
  res.on('error', cleanup);
}

// ─── Router ───────────────────────────────────────────────────────────────

function handleRequest(req, res) {
  const { method, url } = req;
  const parts = url.split('?')[0].split('/').filter(Boolean); // ['api','processes']

  // CORS pre-flight.
  if (method === 'OPTIONS') {
    res.writeHead(204, {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, DELETE, OPTIONS',
    });
    return res.end();
  }

  // ── GET / → HTML dashboard
  if (method === 'GET' && url === '/') {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    return res.end(HTML);
  }

  // ── API routes
  if (parts[0] === 'api') {
    // GET /api/health
    if (method === 'GET' && parts[1] === 'health') {
      return handleHealth(res);
    }
    // GET /api/processes
    if (method === 'GET' && parts[1] === 'processes') {
      return handleGetProcesses(res);
    }
    // GET /api/logs/:name
    if (method === 'GET' && parts[1] === 'logs' && parts[2] && !parts[3]) {
      return handleGetLogs(res, decodeURIComponent(parts[2]));
    }
    // GET /api/logs/:name/stream
    if (method === 'GET' && parts[1] === 'logs' && parts[2] && parts[3] === 'stream') {
      return handleLogStream(res, decodeURIComponent(parts[2]));
    }
    // POST /api/process/:name/restart|stop|start
    if (method === 'POST' && parts[1] === 'process' && parts[2] && parts[3]) {
      return handleAction(res, decodeURIComponent(parts[2]), parts[3]);
    }
    // DELETE /api/process/:name
    if (method === 'DELETE' && parts[1] === 'process' && parts[2]) {
      return handleDelete(res, decodeURIComponent(parts[2]));
    }
  }

  errorRes(res, 404, 'Not found');
}

// ─── HTML / SPA ───────────────────────────────────────────────────────────

const HTML = /* html */`<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>mhost Dashboard</title>
<style>
*,*::before,*::after{margin:0;padding:0;box-sizing:border-box}
:root{
  --bg:#0a0a12;--bg2:#0f0f1a;--bg3:#151525;--bg4:#1a1a2e;
  --text:#e2e8f0;--text2:#94a3b8;--text3:#64748b;
  --accent:#6366f1;--accent2:#818cf8;--accent3:#a5b4fc;
  --green:#22c55e;--green2:#4ade80;
  --red:#ef4444;--yellow:#eab308;--cyan:#06b6d4;
  --border:#1e293b;
  --font:-apple-system,BlinkMacSystemFont,'Inter','Segoe UI',sans-serif;
  --mono:'JetBrains Mono','Fira Code','SF Mono',monospace;
}
html,body{height:100%;background:var(--bg);color:var(--text);font-family:var(--font);font-size:14px;line-height:1.5}

/* ── Layout ── */
.app{display:flex;flex-direction:column;min-height:100vh}
header{
  position:sticky;top:0;z-index:50;
  background:var(--bg2);border-bottom:1px solid var(--border);
  padding:0 24px;height:56px;display:flex;align-items:center;justify-content:space-between;
}
.logo{font-size:1.1rem;font-weight:700;color:var(--text);letter-spacing:-.5px}
.logo span{color:var(--accent2)}
.header-right{display:flex;align-items:center;gap:16px}
.conn-dot{width:8px;height:8px;border-radius:50%;background:var(--text3);transition:background .4s}
.conn-dot.ok{background:var(--green);box-shadow:0 0 6px var(--green)}
.conn-dot.err{background:var(--red);box-shadow:0 0 6px var(--red)}
.conn-label{font-size:.75rem;color:var(--text2)}
.refresh-btn{
  background:transparent;border:1px solid var(--border);color:var(--text2);
  padding:4px 10px;border-radius:6px;cursor:pointer;font-size:.75rem;
  transition:all .15s;
}
.refresh-btn:hover{border-color:var(--accent2);color:var(--accent2)}

main{flex:1;padding:24px;max-width:1200px;width:100%;margin:0 auto}

/* ── Process table ── */
.process-table{
  background:var(--bg2);border:1px solid var(--border);border-radius:10px;overflow:hidden;
}
.table-header{
  display:grid;grid-template-columns:2fr 1fr 80px 110px 60px 80px 90px;
  padding:10px 16px;background:var(--bg3);border-bottom:1px solid var(--border);
  font-size:.7rem;font-weight:600;text-transform:uppercase;letter-spacing:.6px;color:var(--text3);
}
.process-row{
  display:grid;grid-template-columns:2fr 1fr 80px 110px 60px 80px 90px;
  padding:12px 16px;border-bottom:1px solid var(--border);cursor:pointer;
  transition:background .15s;align-items:center;
}
.process-row:last-of-type{border-bottom:none}
.process-row:hover{background:var(--bg3)}
.process-row.active{background:var(--bg4);border-left:2px solid var(--accent)}
.proc-name{font-weight:500;color:var(--text);display:flex;align-items:center;gap:8px;overflow:hidden;white-space:nowrap;text-overflow:ellipsis}
.proc-name .inst-badge{font-size:.65rem;color:var(--text3);background:var(--bg3);border:1px solid var(--border);border-radius:4px;padding:1px 5px;flex-shrink:0}
.status-chip{display:inline-flex;align-items:center;gap:5px;font-size:.75rem;font-weight:500}
.status-dot{width:7px;height:7px;border-radius:50%;flex-shrink:0}
.s-online .status-dot{background:var(--green);box-shadow:0 0 5px var(--green)}
.s-online .status-label{color:var(--green2)}
.s-stopped .status-dot{background:var(--text3)}
.s-stopped .status-label{color:var(--text3)}
.s-errored .status-dot{background:var(--red);animation:pulse-red 1.5s infinite}
.s-errored .status-label{color:var(--red)}
.s-starting .status-dot{background:var(--yellow);animation:pulse-y 1s infinite}
.s-starting .status-label{color:var(--yellow)}
.s-stopping .status-dot{background:var(--yellow)}
.s-stopping .status-label{color:var(--yellow)}
@keyframes pulse-red{0%,100%{box-shadow:0 0 4px var(--red)}50%{box-shadow:0 0 10px var(--red)}}
@keyframes pulse-y{0%,100%{opacity:1}50%{opacity:.4}}
.cell-dim{color:var(--text3);font-size:.8rem}
.cell-mono{font-family:var(--mono);font-size:.75rem}
.restarts-warn{color:var(--yellow)}
.mem{font-family:var(--mono);font-size:.75rem;color:var(--text2)}

/* ── Empty state ── */
.empty-state{padding:48px 24px;text-align:center;color:var(--text3)}
.empty-state .icon{font-size:2.5rem;margin-bottom:12px;opacity:.4}
.empty-state p{font-size:.9rem}

/* ── Expanded detail panel ── */
.detail-panel{
  border-top:1px solid var(--border);background:var(--bg);
  padding:16px 20px;
}
.detail-header{display:flex;align-items:center;gap:12px;margin-bottom:14px;flex-wrap:wrap}
.detail-title{font-weight:600;font-size:.95rem;color:var(--accent3)}
.action-btn{
  padding:5px 14px;border-radius:6px;cursor:pointer;font-size:.75rem;font-weight:600;
  border:1px solid transparent;transition:all .15s;
}
.btn-restart{background:rgba(99,102,241,.12);color:var(--accent3);border-color:rgba(99,102,241,.3)}
.btn-restart:hover{background:rgba(99,102,241,.25)}
.btn-stop{background:rgba(234,179,8,.08);color:var(--yellow);border-color:rgba(234,179,8,.25)}
.btn-stop:hover{background:rgba(234,179,8,.18)}
.btn-start{background:rgba(34,197,94,.08);color:var(--green2);border-color:rgba(34,197,94,.25)}
.btn-start:hover{background:rgba(34,197,94,.18)}
.btn-delete{background:rgba(239,68,68,.08);color:var(--red);border-color:rgba(239,68,68,.2)}
.btn-delete:hover{background:rgba(239,68,68,.18)}
.btn-disabled{opacity:.4;pointer-events:none}

/* ── Log viewer ── */
.log-header{display:flex;align-items:center;justify-content:space-between;margin-bottom:8px}
.log-label{font-size:.7rem;font-weight:600;text-transform:uppercase;letter-spacing:.6px;color:var(--text3)}
.log-follow{display:flex;align-items:center;gap:6px;font-size:.72rem;color:var(--text3);cursor:pointer;user-select:none}
.log-follow input{accent-color:var(--accent)}
.log-box{
  background:var(--bg2);border:1px solid var(--border);border-radius:8px;
  height:240px;overflow-y:auto;padding:12px;
  font-family:var(--mono);font-size:.72rem;line-height:1.6;color:var(--text2);
}
.log-line{white-space:pre-wrap;word-break:break-all}
.log-line.stderr{color:rgba(239,68,68,.85)}
.log-line.fresh{animation:fade-in .3s ease}
@keyframes fade-in{from{opacity:0;transform:translateY(2px)}to{opacity:1;transform:none}}
.toast{
  position:fixed;bottom:24px;right:24px;
  background:var(--bg3);border:1px solid var(--border);border-radius:8px;
  padding:10px 18px;font-size:.8rem;color:var(--text);
  box-shadow:0 4px 20px rgba(0,0,0,.4);
  opacity:0;transform:translateY(8px);transition:all .25s;pointer-events:none;
  z-index:100;
}
.toast.show{opacity:1;transform:none}
.toast.ok{border-color:rgba(34,197,94,.4);color:var(--green2)}
.toast.err{border-color:rgba(239,68,68,.4);color:var(--red)}

/* ── Footer ── */
footer{
  background:var(--bg2);border-top:1px solid var(--border);
  padding:12px 24px;display:flex;align-items:center;justify-content:space-between;
  font-size:.75rem;color:var(--text3);flex-wrap:wrap;gap:8px;
}
.footer-stat{display:flex;align-items:center;gap:6px}
.footer-stat span{color:var(--text2);font-weight:500}

/* ── Responsive ── */
@media(max-width:720px){
  .table-header,.process-row{grid-template-columns:2fr 1fr 60px 80px}
  .col-restarts,.col-mem,.col-cpu{display:none}
  main{padding:12px}
}
</style>
</head>
<body>
<div class="app">

<header>
  <div class="logo">m<span>host</span> Dashboard</div>
  <div class="header-right">
    <div id="connDot" class="conn-dot"></div>
    <span id="connLabel" class="conn-label">Connecting…</span>
    <button class="refresh-btn" onclick="refresh()">⟳ Refresh</button>
  </div>
</header>

<main>
  <div class="process-table" id="processTable">
    <div class="table-header">
      <div>Name</div>
      <div>Status</div>
      <div>PID</div>
      <div>Uptime</div>
      <div class="col-restarts">↺</div>
      <div class="col-mem">Memory</div>
      <div>Actions</div>
    </div>
    <div id="tableBody"><div class="empty-state"><div class="icon">○</div><p>Loading…</p></div></div>
  </div>
</main>

<footer>
  <div class="footer-stat">Processes: <span id="footerCount">—</span></div>
  <div class="footer-stat">Daemon: <span id="footerDaemon">—</span></div>
  <div class="footer-stat">Last refresh: <span id="footerTime">—</span></div>
</footer>

</div><!-- .app -->

<div class="toast" id="toast"></div>

<script>
// ── State ─────────────────────────────────────────────────────────────────
let state = {
  processes:   [],
  expanded:    null,   // process name currently expanded
  sseSource:   null,   // active EventSource
  followLog:   true,
  logLines:    {},     // { [name]: string[] }
  daemonUp:    false,
  loading:     true,
};

// ── Utilities ─────────────────────────────────────────────────────────────

function toast(msg, type = 'ok') {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.className = 'toast show ' + type;
  clearTimeout(el._t);
  el._t = setTimeout(() => { el.className = 'toast'; }, 3000);
}

function fmt(val, fallback = '—') {
  return val != null && val !== '' ? val : fallback;
}

function sanitize(s) {
  return String(s)
    .replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

// ── API calls ─────────────────────────────────────────────────────────────

async function apiFetch(path, opts = {}) {
  const r = await fetch(path, opts);
  if (!r.ok) throw new Error(await r.text());
  return r.json();
}

async function fetchProcesses() {
  const data = await apiFetch('/api/processes');
  return data;
}

async function fetchLogs(name) {
  const data = await apiFetch('/api/logs/' + encodeURIComponent(name));
  return data.lines || [];
}

async function doAction(name, action) {
  return apiFetch('/api/process/' + encodeURIComponent(name) + '/' + action, { method: 'POST' });
}

async function doDelete(name) {
  return apiFetch('/api/process/' + encodeURIComponent(name), { method: 'DELETE' });
}

// ── SSE log streaming ─────────────────────────────────────────────────────

function startStream(name) {
  if (state.sseSource) { state.sseSource.close(); state.sseSource = null; }
  const es = new EventSource('/api/logs/' + encodeURIComponent(name) + '/stream');
  state.sseSource = es;
  es.onmessage = (e) => {
    let line;
    try { line = JSON.parse(e.data); } catch { line = e.data; }
    appendLogLine(name, String(line), line.includes('[err]'));
  };
  es.onerror = () => {};
}

function stopStream() {
  if (state.sseSource) { state.sseSource.close(); state.sseSource = null; }
}

function appendLogLine(name, line, isErr = false) {
  if (!state.logLines[name]) state.logLines[name] = [];
  state.logLines[name].push({ text: line, err: isErr });
  // Keep last 500 lines in memory.
  if (state.logLines[name].length > 500) state.logLines[name].shift();

  if (state.expanded === name) {
    const box = document.getElementById('logBox');
    if (!box) return;
    const el = document.createElement('div');
    el.className = 'log-line fresh' + (isErr ? ' stderr' : '');
    el.textContent = line;
    box.appendChild(el);
    if (state.followLog) box.scrollTop = box.scrollHeight;
  }
}

// ── Render ────────────────────────────────────────────────────────────────

function statusChip(status) {
  const s = (status || 'unknown').toLowerCase();
  return \`<span class="status-chip s-\${s}">
    <span class="status-dot"></span>
    <span class="status-label">\${s}</span>
  </span>\`;
}

function actionButtons(proc) {
  const s = proc.status;
  const n = proc.name;
  const canStart   = s === 'stopped' || s === 'errored';
  const canStop    = s === 'online'  || s === 'starting';
  const canRestart = s === 'online'  || s === 'errored' || s === 'stopped';

  return \`
    <button class="action-btn btn-restart \${canRestart ? '' : 'btn-disabled'}"
      onclick="event.stopPropagation();action('\${n}','restart')">Restart</button>
    \${canStop
      ? \`<button class="action-btn btn-stop" onclick="event.stopPropagation();action('\${n}','stop')">Stop</button>\`
      : \`<button class="action-btn btn-start \${canStart ? '' : 'btn-disabled'}" onclick="event.stopPropagation();action('\${n}','start')">Start</button>\`}
    <button class="action-btn btn-delete"
      onclick="event.stopPropagation();confirmDelete('\${n}')">Delete</button>
  \`;
}

function renderDetailPanel(proc) {
  const lines  = state.logLines[proc.name] || [];
  const logHtml = lines.length
    ? lines.map(l => \`<div class="log-line\${l.err ? ' stderr' : ''}">\${sanitize(l.text)}</div>\`).join('')
    : '<div class="log-line" style="color:var(--text3)">No log lines yet…</div>';

  return \`<div class="detail-panel" id="detailPanel">
    <div class="detail-header">
      <span class="detail-title">\${sanitize(proc.name)}</span>
      \${actionButtons(proc)}
    </div>
    <div class="log-header">
      <span class="log-label">Logs</span>
      <label class="log-follow">
        <input type="checkbox" id="followChk" \${state.followLog ? 'checked' : ''}
          onchange="state.followLog=this.checked">
        Auto-follow
      </label>
    </div>
    <div class="log-box" id="logBox">\${logHtml}</div>
  </div>\`;
}

function renderTable(processes) {
  if (!processes.length) {
    return \`<div class="empty-state"><div class="icon">○</div>
      <p>No processes registered — run <code>mhost start &lt;app&gt;</code></p></div>\`;
  }

  return processes.map(proc => {
    const isExpanded = state.expanded === proc.name;
    const rowClass = 'process-row' + (isExpanded ? ' active' : '');
    const restartCls = proc.restarts > 0 ? 'restarts-warn' : 'cell-dim';
    const row = \`<div class="\${rowClass}" onclick="toggleExpand('\${proc.name}')">
      <div class="proc-name">\${sanitize(proc.name)}</div>
      <div>\${statusChip(proc.status)}</div>
      <div class="cell-mono cell-dim">\${fmt(proc.pid)}</div>
      <div class="cell-dim">\${fmt(proc.uptime)}</div>
      <div class="cell-dim \${restartCls} col-restarts">\${proc.restarts}</div>
      <div class="mem col-mem">\${fmt(proc.memory)}</div>
      <div>\${actionButtons(proc)}</div>
    </div>\`;

    if (isExpanded) {
      return row + renderDetailPanel(proc);
    }
    return row;
  }).join('');
}

function applyRender(data) {
  state.processes = data.processes || [];
  state.daemonUp  = data.daemonUp !== false;
  state.loading   = false;

  // Connection indicator.
  const dot   = document.getElementById('connDot');
  const label = document.getElementById('connLabel');
  dot.className   = 'conn-dot ' + (state.daemonUp ? 'ok' : 'err');
  label.textContent = state.daemonUp ? 'Connected' : 'Daemon offline';

  // Footer.
  const online = state.processes.filter(p => p.status === 'online').length;
  document.getElementById('footerCount').textContent =
    state.processes.length + ' (' + online + ' online)';
  document.getElementById('footerDaemon').textContent =
    state.daemonUp ? 'Running' : 'Offline';
  document.getElementById('footerTime').textContent =
    new Date().toLocaleTimeString();

  // Table — preserve scroll position in log box.
  const logScroll = document.getElementById('logBox')?.scrollTop;
  document.getElementById('tableBody').innerHTML = renderTable(state.processes);
  if (logScroll != null) {
    const lb = document.getElementById('logBox');
    if (lb) lb.scrollTop = state.followLog ? lb.scrollHeight : logScroll;
  } else if (state.followLog) {
    const lb = document.getElementById('logBox');
    if (lb) lb.scrollTop = lb.scrollHeight;
  }
}

// ── Toggle expand ─────────────────────────────────────────────────────────

async function toggleExpand(name) {
  if (state.expanded === name) {
    state.expanded = null;
    stopStream();
  } else {
    state.expanded = name;
    // Load historical logs then start stream.
    try {
      const lines = await fetchLogs(name);
      state.logLines[name] = lines.map(l => ({ text: l, err: l.includes('[err]') }));
    } catch (_) {
      state.logLines[name] = [];
    }
    startStream(name);
  }

  // Re-render without a full network fetch.
  const proc = state.processes.find(p => p.name === name) || { name, status: 'unknown', pid: null, uptime: null, restarts: 0, memory: null };
  document.getElementById('tableBody').innerHTML = renderTable(state.processes);
  if (state.expanded) {
    const lb = document.getElementById('logBox');
    if (lb && state.followLog) lb.scrollTop = lb.scrollHeight;
  }
}

// ── Actions ───────────────────────────────────────────────────────────────

async function action(name, act) {
  try {
    const r = await doAction(name, act);
    toast(r.ok ? name + ' ' + act + 'ed' : (r.output || 'Failed'), r.ok ? 'ok' : 'err');
    await refresh();
  } catch (e) {
    toast(String(e), 'err');
  }
}

async function confirmDelete(name) {
  if (!confirm('Delete process "' + name + '"? This cannot be undone.')) return;
  try {
    const r = await doDelete(name);
    toast(r.ok ? name + ' deleted' : (r.output || 'Failed'), r.ok ? 'ok' : 'err');
    if (state.expanded === name) { state.expanded = null; stopStream(); }
    await refresh();
  } catch (e) {
    toast(String(e), 'err');
  }
}

// ── Poll / refresh ────────────────────────────────────────────────────────

async function refresh() {
  try {
    const data = await fetchProcesses();
    applyRender(data);
  } catch (e) {
    const dot = document.getElementById('connDot');
    dot.className = 'conn-dot err';
    document.getElementById('connLabel').textContent = 'Error';
    document.getElementById('footerDaemon').textContent = 'Offline';
  }
}

// Initial load + 2-second auto-refresh.
refresh();
setInterval(refresh, 2000);
</script>
</body>
</html>`;

// ─── Server ───────────────────────────────────────────────────────────────

const server = http.createServer(handleRequest);

server.listen(PORT, () => {
  console.log(JSON.stringify({
    level:   'info',
    message: `mhost Dashboard running at http://localhost:${PORT}`,
    pid:     process.pid,
  }));
});

// ─── Graceful shutdown ────────────────────────────────────────────────────

function shutdown(signal) {
  console.log(JSON.stringify({ level: 'info', message: `${signal} received — shutting down` }));
  // Close all SSE connections.
  for (const clients of sseClients.values()) {
    for (const res of clients) {
      try { res.end(); } catch (_) {}
    }
  }
  server.close(() => {
    console.log(JSON.stringify({ level: 'info', message: 'Server closed' }));
    process.exit(0);
  });
  // Force exit after 5 s if connections linger.
  setTimeout(() => process.exit(0), 5000).unref();
}

process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGINT',  () => shutdown('SIGINT'));
