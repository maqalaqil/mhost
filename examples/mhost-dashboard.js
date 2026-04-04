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
    return { ok: true, output: execSync(`${MHOST} ${args}`, { encoding: 'utf8', timeout: 8000 }) };
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

// ── Router ────────────────────────────────────────────────────────────────

function handleRequest(req, res) {
  const { method, url } = req;
  const p = url.split('?')[0].split('/').filter(Boolean);
  if (method === 'OPTIONS') {
    res.writeHead(204, { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Methods': 'GET,POST,DELETE,OPTIONS' });
    return res.end();
  }
  if (method === 'GET' && url === '/') {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    return res.end(HTML);
  }
  if (p[0] !== 'api') return errRes(res, 404, 'Not found');
  if (method === 'GET'    && p[1] === 'health')                                          return handleHealth(res);
  if (method === 'GET'    && p[1] === 'processes')                                       return handleProcesses(res);
  if (method === 'GET'    && p[1] === 'logs' && p[2] && !p[3])                          return handleLogs(res, decodeURIComponent(p[2]));
  if (method === 'GET'    && p[1] === 'logs' && p[2] && p[3] === 'stream')              return handleLogStream(res, decodeURIComponent(p[2]));
  if (method === 'POST'   && p[1] === 'process' && p[2] && p[3])                        return handleAction(res, decodeURIComponent(p[2]), p[3]);
  if (method === 'POST'   && p[1] === 'all' && p[2])                                    return handleActionAll(res, p[2]);
  if (method === 'DELETE' && p[1] === 'process' && p[2])                                return handleDelete(res, decodeURIComponent(p[2]));
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
  --bg:#0a0a12;--bg2:#0f0f1a;--bg3:#151525;--bg4:#1a1a2e;
  --text:#e2e8f0;--text2:#94a3b8;--text3:#64748b;
  --accent:#6366f1;--accent2:#818cf8;--accent3:#a5b4fc;
  --green:#22c55e;--green2:#4ade80;--red:#ef4444;--yellow:#eab308;
  --border:#1e293b;
  --font:-apple-system,BlinkMacSystemFont,'Inter','Segoe UI',sans-serif;
  --mono:'JetBrains Mono','Fira Code','SF Mono',monospace;
  --radius:12px;
}
html,body{height:100%;background:var(--bg);color:var(--text);font-family:var(--font);font-size:14px;line-height:1.5}
.app{display:flex;flex-direction:column;min-height:100vh}

/* ── Header ── */
header{position:sticky;top:0;z-index:50;background:var(--bg2);border-bottom:1px solid var(--border);padding:0 24px;height:60px;display:flex;align-items:center;justify-content:space-between;gap:12px}
.logo{font-size:1.15rem;font-weight:800;color:var(--text);letter-spacing:-.5px;display:flex;align-items:center;gap:8px}
.logo-icon{width:28px;height:28px;border-radius:8px;background:linear-gradient(135deg,var(--accent),var(--accent2));display:flex;align-items:center;justify-content:center;font-size:.85rem}
.logo span{color:var(--accent2)}
.hdr-center{display:flex;align-items:center;gap:16px;flex:1;justify-content:center}
.conn-badge{display:flex;align-items:center;gap:6px;background:var(--bg3);border:1px solid var(--border);border-radius:20px;padding:4px 12px;font-size:.75rem;color:var(--text2)}
.dot{width:7px;height:7px;border-radius:50%;background:var(--text3);transition:background .4s;flex-shrink:0}
.dot.ok{background:var(--green);box-shadow:0 0 6px var(--green)}
.dot.err{background:var(--red);box-shadow:0 0 6px var(--red)}
.refresh-ts{font-size:.72rem;color:var(--text3)}
.hdr-r{display:flex;align-items:center;gap:8px}
.hbtn{background:transparent;border:1px solid var(--border);color:var(--text2);padding:5px 12px;border-radius:8px;cursor:pointer;font-size:.76rem;font-weight:500;transition:all .15s;white-space:nowrap}
.hbtn:hover{border-color:var(--accent2);color:var(--accent2)}
.hbtn.danger:hover{border-color:var(--red);color:var(--red)}

/* ── Main layout ── */
main{flex:1;padding:24px;max-width:1280px;width:100%;margin:0 auto}

/* ── Analytics strip ── */
.analytics{display:grid;grid-template-columns:repeat(5,1fr);gap:12px;margin-bottom:24px}
.stat-card{background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius);padding:16px 18px;transition:border-color .2s}
.stat-card:hover{border-color:rgba(99,102,241,.3)}
.stat-num{font-size:1.9rem;font-weight:800;letter-spacing:-1px;line-height:1.1}
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
.proc-card{background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius);padding:18px 20px;cursor:pointer;transition:all .2s;position:relative;overflow:hidden}
.proc-card:hover{border-color:rgba(99,102,241,.3);transform:translateY(-2px);box-shadow:0 8px 30px rgba(0,0,0,.3)}
.proc-card.active{border-color:var(--accent);background:var(--bg3)}
.proc-card::before{content:'';position:absolute;top:0;left:0;right:0;height:2px;opacity:0;transition:opacity .2s}
.proc-card:hover::before,.proc-card.active::before{opacity:1}
.proc-card.s-online::before{background:linear-gradient(90deg,transparent,var(--green),transparent)}
.proc-card.s-errored::before{background:linear-gradient(90deg,transparent,var(--red),transparent)}
.proc-card.s-stopped::before{background:linear-gradient(90deg,transparent,var(--text3),transparent)}
.proc-card.s-starting::before,.proc-card.s-stopping::before{background:linear-gradient(90deg,transparent,var(--yellow),transparent)}

.card-top{display:flex;align-items:flex-start;justify-content:space-between;margin-bottom:12px}
.card-name{font-weight:700;font-size:1rem;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;max-width:180px}
.chip{display:inline-flex;align-items:center;gap:5px;font-size:.72rem;font-weight:600;padding:2px 8px;border-radius:10px}
.s-online .chip{background:rgba(34,197,94,.12);color:var(--green2)}
.s-stopped .chip{background:rgba(100,116,139,.12);color:var(--text3)}
.s-errored .chip{background:rgba(239,68,68,.12);color:var(--red)}
.s-starting .chip,.s-stopping .chip{background:rgba(234,179,8,.12);color:var(--yellow)}
.cdot{width:6px;height:6px;border-radius:50%;flex-shrink:0}
.s-online .cdot{background:var(--green);animation:glow-g 2s ease-in-out infinite}
.s-errored .cdot{background:var(--red);animation:glow-r 1.5s infinite}
.s-starting .cdot,.s-stopping .cdot{background:var(--yellow);animation:py 1s infinite}
.s-stopped .cdot{background:var(--text3)}
@keyframes glow-g{0%,100%{box-shadow:0 0 3px var(--green)}50%{box-shadow:0 0 8px var(--green)}}
@keyframes glow-r{0%,100%{box-shadow:0 0 3px var(--red)}50%{box-shadow:0 0 8px var(--red)}}
@keyframes py{0%,100%{opacity:1}50%{opacity:.3}}

.card-meta{display:flex;gap:16px;margin-bottom:12px}
.meta-item{display:flex;flex-direction:column;gap:1px}
.meta-val{font-size:.82rem;font-family:var(--mono);color:var(--text);font-weight:500}
.meta-lbl{font-size:.67rem;color:var(--text3);text-transform:uppercase;letter-spacing:.4px}
.restarts-warn{color:var(--yellow)}

.card-actions{display:flex;gap:6px;opacity:0;transition:opacity .15s}
.proc-card:hover .card-actions,.proc-card.active .card-actions{opacity:1}
.abt{padding:4px 10px;border-radius:6px;cursor:pointer;font-size:.72rem;font-weight:600;border:1px solid transparent;transition:all .15s}
.ar{background:rgba(99,102,241,.1);color:var(--accent3);border-color:rgba(99,102,241,.3)}.ar:hover{background:rgba(99,102,241,.22)}
.as{background:rgba(234,179,8,.08);color:var(--yellow);border-color:rgba(234,179,8,.25)}.as:hover{background:rgba(234,179,8,.18)}
.ast{background:rgba(34,197,94,.08);color:var(--green2);border-color:rgba(34,197,94,.25)}.ast:hover{background:rgba(34,197,94,.18)}
.ad{background:rgba(239,68,68,.08);color:var(--red);border-color:rgba(239,68,68,.2)}.ad:hover{background:rgba(239,68,68,.18)}
.off{opacity:.35;pointer-events:none}

/* ── Log panel ── */
.log-panel{background:var(--bg2);border:1px solid var(--border);border-radius:var(--radius);margin-bottom:24px;overflow:hidden}
.log-header{padding:12px 16px;background:var(--bg3);border-bottom:1px solid var(--border);display:flex;align-items:center;gap:10px;flex-wrap:wrap}
.log-proc-name{font-weight:700;color:var(--accent3);font-size:.9rem}
.log-line-count{font-size:.72rem;color:var(--text3);background:var(--bg4);padding:2px 8px;border-radius:10px;font-family:var(--mono)}
.log-follow-label{display:flex;align-items:center;gap:5px;font-size:.72rem;color:var(--text3);cursor:pointer;user-select:none;margin-left:auto}
.log-follow-label input{accent-color:var(--accent)}
.log-box{height:260px;overflow-y:auto;padding:14px 16px;font-family:var(--mono);font-size:.73rem;line-height:1.7;color:var(--text2)}
.log-box::-webkit-scrollbar{width:4px}
.log-box::-webkit-scrollbar-thumb{background:var(--border);border-radius:2px}
.ll{white-space:pre-wrap;word-break:break-all}
.ll.err{color:rgba(239,68,68,.9)}
.ll.warn{color:rgba(234,179,8,.85)}
.ll.new{animation:fadein .25s ease}
@keyframes fadein{from{opacity:0;transform:translateY(2px)}to{opacity:1;transform:none}}

/* ── Empty state ── */
.empty{padding:60px 24px;text-align:center;color:var(--text3)}
.empty-icon{font-size:2.5rem;margin-bottom:12px;opacity:.3}

/* ── Skeleton loader ── */
.skeleton{background:linear-gradient(90deg,var(--bg3) 25%,var(--bg4) 50%,var(--bg3) 75%);background-size:200% 100%;animation:shimmer 1.5s infinite;border-radius:8px}
@keyframes shimmer{0%{background-position:200% 0}100%{background-position:-200% 0}}
.skel-card{height:130px;border-radius:var(--radius)}

/* ── Toast ── */
.toast{position:fixed;bottom:24px;right:24px;background:var(--bg3);border:1px solid var(--border);border-radius:10px;padding:12px 20px;font-size:.82rem;color:var(--text);box-shadow:0 8px 32px rgba(0,0,0,.5);opacity:0;transform:translateY(10px);transition:all .25s;pointer-events:none;z-index:200;max-width:320px}
.toast.show{opacity:1;transform:none}
.toast.ok{border-color:rgba(34,197,94,.4);color:var(--green2)}
.toast.err{border-color:rgba(239,68,68,.4);color:var(--red)}

/* ── Footer ── */
footer{background:var(--bg2);border-top:1px solid var(--border);padding:10px 24px;display:flex;align-items:center;gap:16px;font-size:.73rem;color:var(--text3);flex-wrap:wrap}
footer span{color:var(--text2)}

@media(max-width:720px){
  .analytics{grid-template-columns:repeat(2,1fr)}
  .cards-grid{grid-template-columns:1fr}
  .hdr-center{display:none}
  main{padding:12px}
  header{padding:0 12px}
}
</style>
</head>
<body>
<div class="app">

<header>
  <div class="logo">
    <div class="logo-icon">⚙</div>
    m<span>host</span> Dashboard
  </div>
  <div class="hdr-center">
    <div class="conn-badge">
      <div id="dot" class="dot"></div>
      <span id="lbl">Connecting…</span>
    </div>
    <span class="refresh-ts" id="ts"></span>
  </div>
  <div class="hdr-r">
    <button class="hbtn" onclick="refresh()">⟳ Refresh</button>
    <button class="hbtn" onclick="actAll('restart')">↺ Restart All</button>
    <button class="hbtn danger" onclick="actAll('stop')">■ Stop All</button>
  </div>
</header>

<main>
  <!-- Analytics Strip -->
  <div class="analytics" id="analytics">
    <div class="stat-card"><div class="stat-num accent" id="s-total">—</div><div class="stat-label">Total Processes</div></div>
    <div class="stat-card"><div class="stat-num green" id="s-online">—</div><div class="stat-label">Online</div></div>
    <div class="stat-card"><div class="stat-num red" id="s-offline">—</div><div class="stat-label">Offline / Errored</div></div>
    <div class="stat-card"><div class="stat-num yellow" id="s-restarts">—</div><div class="stat-label">Total Restarts</div></div>
    <div class="stat-card">
      <div class="stat-num accent" id="s-health">—%</div>
      <div class="stat-label">Fleet Health</div>
      <div class="fleet-bar-wrap"><div class="fleet-bar" id="fleet-bar" style="width:0%"></div></div>
    </div>
  </div>

  <!-- Process Cards -->
  <div class="sec-hdr">
    <span class="sec-title">Processes</span>
  </div>
  <div class="cards-grid" id="cards">
    <div class="skeleton skel-card"></div>
    <div class="skeleton skel-card"></div>
    <div class="skeleton skel-card"></div>
  </div>

  <!-- Log Panel (shown when a process is expanded) -->
  <div id="log-panel" style="display:none"></div>
</main>

<footer>
  <div>Processes: <span id="fc">—</span></div>
  <div>Daemon: <span id="fd">—</span></div>
  <div>Updated: <span id="ft">—</span></div>
</footer>
</div>

<div class="toast" id="toast"></div>

<script>
const S = { processes:[], expanded:null, sse:null, follow:true, logs:{}, daemonUp:false };

function toast(m, t='ok') {
  const e = document.getElementById('toast');
  e.textContent = m; e.className = 'toast show ' + t;
  clearTimeout(e._t); e._t = setTimeout(() => { e.className = 'toast'; }, 3200);
}
function san(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
function fmt(v, f='—') { return (v != null && v !== '') ? v : f; }
async function api(url, o={}) { const r = await fetch(url, o); if (!r.ok) throw new Error(await r.text()); return r.json(); }

function chipHtml(s) {
  const k = (s || 'unknown').toLowerCase();
  return \`<span class="chip"><span class="cdot"></span>\${k}</span>\`;
}

function cardBtns(p) {
  const n = p.name, s = p.status;
  const canR  = s === 'online' || s === 'errored' || s === 'stopped';
  const canSt = s === 'online' || s === 'starting';
  const canGo = s === 'stopped' || s === 'errored';
  const stop  = canSt
    ? \`<button class="abt as" onclick="event.stopPropagation();act('\${n}','stop')">Stop</button>\`
    : \`<button class="abt ast \${canGo?'':'off'}" onclick="event.stopPropagation();act('\${n}','start')">Start</button>\`;
  return \`<div class="card-actions">
    <button class="abt ar \${canR?'':'off'}" onclick="event.stopPropagation();act('\${n}','restart')">Restart</button>
    \${stop}
    <button class="abt ad" onclick="event.stopPropagation();del('\${n}')">Delete</button>
  </div>\`;
}

function renderCards(ps) {
  if (!ps.length) return '<div class="empty"><div class="empty-icon">○</div><p>No processes — run <code>mhost start &lt;app&gt;</code></p></div>';
  return ps.map(p => {
    const k = (p.status || 'unknown').toLowerCase();
    const exp = S.expanded === p.name;
    return \`<div class="proc-card s-\${k}\${exp ? ' active' : ''}" onclick="toggle('\${p.name}')">
      <div class="card-top">
        <div class="card-name">\${san(p.name)}</div>
        \${chipHtml(p.status)}
      </div>
      <div class="card-meta">
        <div class="meta-item"><div class="meta-val">\${fmt(p.pid)}</div><div class="meta-lbl">PID</div></div>
        <div class="meta-item"><div class="meta-val">\${fmt(p.uptime)}</div><div class="meta-lbl">Uptime</div></div>
        <div class="meta-item"><div class="meta-val \${p.restarts > 0 ? 'restarts-warn' : ''}">\${p.restarts}</div><div class="meta-lbl">Restarts</div></div>
        <div class="meta-item"><div class="meta-val">\${fmt(p.memory)}</div><div class="meta-lbl">Memory</div></div>
      </div>
      \${cardBtns(p)}
    </div>\`;
  }).join('');
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
    ? lines.map(l => \`<div class="ll \${l.cls}">\${san(l.t)}</div>\`).join('')
    : '<div class="ll" style="color:var(--text3)">No logs yet…</div>';
  return \`<div class="log-panel">
    <div class="log-header">
      <span class="log-proc-name">\${san(p.name)}</span>
      <span class="log-line-count">\${lineCount} lines</span>
      <label class="log-follow-label">
        <input type="checkbox" id="fchk" \${S.follow ? 'checked' : ''} onchange="S.follow=this.checked">
        Auto-scroll
      </label>
    </div>
    <div class="log-box" id="lbox">\${html}</div>
  </div>\`;
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
  const panelEl = document.getElementById('log-panel');
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
    S.logs[name].push({ t: String(l), cls });
    if (S.logs[name].length > 500) S.logs[name].shift();
    if (S.expanded === name) {
      const box = document.getElementById('lbox');
      if (!box) return;
      const el = document.createElement('div');
      el.className = 'll new ' + cls;
      el.textContent = String(l);
      box.appendChild(el);
      // update line count
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
      S.logs[name] = (d.lines || []).map(t => ({ t, cls: logLineClass(t) }));
    } catch { S.logs[name] = []; }
    startSSE(name);
  }
  const p = S.processes.find(x => x.name === S.expanded);
  const panelEl = document.getElementById('log-panel');
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
