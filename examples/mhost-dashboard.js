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
    let si = cols.findIndex(c => STATUS_RE.test(c));
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
  res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', 'Connection': 'keep-alive', 'Access-Control-Allow-Origin': '*' });
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
  if (method === 'OPTIONS') { res.writeHead(204, { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Methods': 'GET,POST,DELETE,OPTIONS' }); return res.end(); }
  if (method === 'GET' && url === '/') { res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); return res.end(HTML); }
  if (p[0] !== 'api') return errRes(res, 404, 'Not found');
  if (method === 'GET'    && p[1] === 'health')                           return handleHealth(res);
  if (method === 'GET'    && p[1] === 'processes')                        return handleProcesses(res);
  if (method === 'GET'    && p[1] === 'logs' && p[2] && !p[3])           return handleLogs(res, decodeURIComponent(p[2]));
  if (method === 'GET'    && p[1] === 'logs' && p[2] && p[3]==='stream') return handleLogStream(res, decodeURIComponent(p[2]));
  if (method === 'POST'   && p[1] === 'process' && p[2] && p[3])         return handleAction(res, decodeURIComponent(p[2]), p[3]);
  if (method === 'DELETE' && p[1] === 'process' && p[2])                 return handleDelete(res, decodeURIComponent(p[2]));
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
}
html,body{height:100%;background:var(--bg);color:var(--text);font-family:var(--font);font-size:14px;line-height:1.5}
.app{display:flex;flex-direction:column;min-height:100vh}
header{position:sticky;top:0;z-index:50;background:var(--bg2);border-bottom:1px solid var(--border);padding:0 24px;height:56px;display:flex;align-items:center;justify-content:space-between}
.logo{font-size:1.1rem;font-weight:700;color:var(--text);letter-spacing:-.5px}
.logo span{color:var(--accent2)}
.hdr-r{display:flex;align-items:center;gap:14px}
.dot{width:8px;height:8px;border-radius:50%;background:var(--text3);transition:background .4s;flex-shrink:0}
.dot.ok{background:var(--green);box-shadow:0 0 6px var(--green)}
.dot.err{background:var(--red);box-shadow:0 0 6px var(--red)}
.lbl{font-size:.75rem;color:var(--text2)}
.rbtn{background:transparent;border:1px solid var(--border);color:var(--text2);padding:4px 10px;border-radius:6px;cursor:pointer;font-size:.75rem;transition:all .15s}
.rbtn:hover{border-color:var(--accent2);color:var(--accent2)}
main{flex:1;padding:24px;max-width:1200px;width:100%;margin:0 auto}
.tbl{background:var(--bg2);border:1px solid var(--border);border-radius:10px;overflow:hidden}
.th,.tr{display:grid;grid-template-columns:2fr 1fr 80px 120px 50px 80px 165px;padding:10px 16px;align-items:center}
.th{background:var(--bg3);border-bottom:1px solid var(--border);font-size:.7rem;font-weight:600;text-transform:uppercase;letter-spacing:.5px;color:var(--text3)}
.tr{border-bottom:1px solid var(--border);cursor:pointer;transition:background .15s}
.tr:last-of-type{border-bottom:none}
.tr:hover{background:var(--bg3)}
.tr.active{background:var(--bg4);border-left:3px solid var(--accent)}
.name{font-weight:500;overflow:hidden;white-space:nowrap;text-overflow:ellipsis}
.chip{display:inline-flex;align-items:center;gap:5px;font-size:.75rem;font-weight:500}
.cdot{width:7px;height:7px;border-radius:50%;flex-shrink:0}
.s-online .cdot{background:var(--green);box-shadow:0 0 5px var(--green)}.s-online .clbl{color:var(--green2)}
.s-stopped .cdot{background:var(--text3)}.s-stopped .clbl{color:var(--text3)}
.s-errored .cdot{background:var(--red);animation:pr 1.5s infinite}.s-errored .clbl{color:var(--red)}
.s-starting .cdot,.s-stopping .cdot{background:var(--yellow)}.s-starting .clbl,.s-stopping .clbl{color:var(--yellow)}
.s-starting .cdot{animation:py 1s infinite}
@keyframes pr{0%,100%{box-shadow:0 0 4px var(--red)}50%{box-shadow:0 0 10px var(--red)}}
@keyframes py{0%,100%{opacity:1}50%{opacity:.3}}
.dim{color:var(--text3);font-size:.8rem}.mono{font-family:var(--mono);font-size:.75rem}
.rwarn{color:var(--yellow)}
.abt{padding:4px 10px;border-radius:6px;cursor:pointer;font-size:.72rem;font-weight:600;border:1px solid transparent;transition:all .15s;margin-right:4px}
.ar{background:rgba(99,102,241,.1);color:var(--accent3);border-color:rgba(99,102,241,.3)}.ar:hover{background:rgba(99,102,241,.22)}
.as{background:rgba(234,179,8,.08);color:var(--yellow);border-color:rgba(234,179,8,.25)}.as:hover{background:rgba(234,179,8,.18)}
.ast{background:rgba(34,197,94,.08);color:var(--green2);border-color:rgba(34,197,94,.25)}.ast:hover{background:rgba(34,197,94,.18)}
.ad{background:rgba(239,68,68,.08);color:var(--red);border-color:rgba(239,68,68,.2)}.ad:hover{background:rgba(239,68,68,.18)}
.off{opacity:.35;pointer-events:none}
.panel{border-top:1px solid var(--border);background:var(--bg);padding:16px 20px}
.phr{display:flex;align-items:center;gap:10px;margin-bottom:14px;flex-wrap:wrap}
.ptl{font-weight:600;font-size:.95rem;color:var(--accent3)}
.llh{display:flex;align-items:center;justify-content:space-between;margin-bottom:8px}
.llbl{font-size:.7rem;font-weight:600;text-transform:uppercase;letter-spacing:.6px;color:var(--text3)}
.fol{display:flex;align-items:center;gap:5px;font-size:.72rem;color:var(--text3);cursor:pointer;user-select:none}
.fol input{accent-color:var(--accent)}
.lbox{background:var(--bg2);border:1px solid var(--border);border-radius:8px;height:240px;overflow-y:auto;padding:12px;font-family:var(--mono);font-size:.72rem;line-height:1.6;color:var(--text2)}
.ll{white-space:pre-wrap;word-break:break-all}
.ll.e{color:rgba(239,68,68,.85)}
.ll.n{animation:fi .3s ease}
@keyframes fi{from{opacity:0;transform:translateY(2px)}to{opacity:1;transform:none}}
.empty{padding:48px 24px;text-align:center;color:var(--text3)}
.empty .ico{font-size:2.5rem;margin-bottom:12px;opacity:.35}
.toast{position:fixed;bottom:24px;right:24px;background:var(--bg3);border:1px solid var(--border);border-radius:8px;padding:10px 18px;font-size:.8rem;color:var(--text);box-shadow:0 4px 20px rgba(0,0,0,.4);opacity:0;transform:translateY(8px);transition:all .25s;pointer-events:none;z-index:100}
.toast.show{opacity:1;transform:none}
.toast.ok{border-color:rgba(34,197,94,.4);color:var(--green2)}
.toast.err{border-color:rgba(239,68,68,.4);color:var(--red)}
footer{background:var(--bg2);border-top:1px solid var(--border);padding:11px 24px;display:flex;align-items:center;justify-content:space-between;font-size:.75rem;color:var(--text3);flex-wrap:wrap;gap:8px}
footer span{color:var(--text2);font-weight:500}
@media(max-width:720px){.th,.tr{grid-template-columns:2fr 1fr 70px 90px}.col-r,.col-m,.col-a2{display:none}main{padding:12px}}
</style>
</head>
<body>
<div class="app">
<header>
  <div class="logo">m<span>host</span> Dashboard</div>
  <div class="hdr-r">
    <div id="dot" class="dot"></div>
    <span id="lbl" class="lbl">Connecting…</span>
    <button class="rbtn" onclick="refresh()">⟳ Refresh</button>
  </div>
</header>
<main>
  <div class="tbl" id="tbl">
    <div class="th"><div>Name</div><div>Status</div><div>PID</div><div>Uptime</div><div class="col-r">↺</div><div class="col-m">Memory</div><div>Actions</div></div>
    <div id="body"><div class="empty"><div class="ico">○</div><p>Loading…</p></div></div>
  </div>
</main>
<footer>
  <div>Processes: <span id="fc">—</span></div>
  <div>Daemon: <span id="fd">—</span></div>
  <div>Updated: <span id="ft">—</span></div>
</footer>
</div>
<div class="toast" id="toast"></div>
<script>
const S={processes:[],expanded:null,sse:null,follow:true,logs:{},daemonUp:false};

function toast(m,t='ok'){const e=document.getElementById('toast');e.textContent=m;e.className='toast show '+t;clearTimeout(e._t);e._t=setTimeout(()=>{e.className='toast'},3000)}
function san(s){return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;')}
function fmt(v,f='—'){return v!=null&&v!==''?v:f}

async function api(url,o={}){const r=await fetch(url,o);if(!r.ok)throw new Error(await r.text());return r.json()}

function chip(s){const k=(s||'unknown').toLowerCase();return \`<span class="chip s-\${k}"><span class="cdot"></span><span class="clbl">\${k}</span></span>\`}

function btns(p){
  const n=p.name,s=p.status;
  const canR=s==='online'||s==='errored'||s==='stopped';
  const canSt=s==='online'||s==='starting';
  const canGo=s==='stopped'||s==='errored';
  return \`<button class="abt ar \${canR?'':'off'}" onclick="event.stopPropagation();act('\${n}','restart')">Restart</button>\`
    +(canSt?\`<button class="abt as" onclick="event.stopPropagation();act('\${n}','stop')">Stop</button>\`
           :\`<button class="abt ast \${canGo?'':'off'}" onclick="event.stopPropagation();act('\${n}','start')">Start</button>\`)
    +\`<button class="abt ad" onclick="event.stopPropagation();del('\${n}')">Delete</button>\`;
}

function panel(p){
  const lines=S.logs[p.name]||[];
  const html=lines.length?lines.map(l=>\`<div class="ll\${l.e?' e':''}">\${san(l.t)}</div>\`).join(''):'<div class="ll" style="color:var(--text3)">No logs yet…</div>';
  return \`<div class="panel" id="pnl">
    <div class="phr"><span class="ptl">\${san(p.name)}</span>\${btns(p)}</div>
    <div class="llh"><span class="llbl">Logs</span><label class="fol"><input type="checkbox" id="fchk" \${S.follow?'checked':''} onchange="S.follow=this.checked">Auto-follow</label></div>
    <div class="lbox" id="lbox">\${html}</div>
  </div>\`;
}

function rows(ps){
  if(!ps.length)return '<div class="empty"><div class="ico">○</div><p>No processes — run <code>mhost start &lt;app&gt;</code></p></div>';
  return ps.map(p=>{
    const exp=S.expanded===p.name;
    const row=\`<div class="tr\${exp?' active':''}" onclick="toggle('\${p.name}')">
      <div class="name">\${san(p.name)}</div>
      <div>\${chip(p.status)}</div>
      <div class="mono dim">\${fmt(p.pid)}</div>
      <div class="dim">\${fmt(p.uptime)}</div>
      <div class="dim \${p.restarts>0?'rwarn':''} col-r">\${p.restarts}</div>
      <div class="mono dim col-m">\${fmt(p.memory)}</div>
      <div>\${btns(p)}</div>
    </div>\`;
    return exp?row+panel(p):row;
  }).join('');
}

function render(d){
  S.processes=d.processes||[];S.daemonUp=d.daemonUp!==false;
  const online=S.processes.filter(p=>p.status==='online').length;
  document.getElementById('dot').className='dot '+(S.daemonUp?'ok':'err');
  document.getElementById('lbl').textContent=S.daemonUp?'Connected':'Daemon offline';
  document.getElementById('fc').textContent=S.processes.length+' ('+online+' online)';
  document.getElementById('fd').textContent=S.daemonUp?'Running':'Offline';
  document.getElementById('ft').textContent=new Date().toLocaleTimeString();
  const sc=document.getElementById('lbox')?.scrollTop;
  document.getElementById('body').innerHTML=rows(S.processes);
  const lb=document.getElementById('lbox');
  if(lb) lb.scrollTop=S.follow?lb.scrollHeight:(sc??lb.scrollHeight);
}

function startSSE(name){
  if(S.sse){S.sse.close();S.sse=null}
  const es=new EventSource('/api/logs/'+encodeURIComponent(name)+'/stream');
  S.sse=es;
  es.onmessage=e=>{
    let l;try{l=JSON.parse(e.data)}catch{l=e.data}
    const isE=String(l).includes('[err]');
    if(!S.logs[name])S.logs[name]=[];
    S.logs[name].push({t:String(l),e:isE});
    if(S.logs[name].length>500)S.logs[name].shift();
    if(S.expanded===name){
      const box=document.getElementById('lbox');
      if(!box)return;
      const el=document.createElement('div');el.className='ll n'+(isE?' e':'');el.textContent=String(l);
      box.appendChild(el);if(S.follow)box.scrollTop=box.scrollHeight;
    }
  };
  es.onerror=()=>{};
}

async function toggle(name){
  if(S.expanded===name){S.expanded=null;if(S.sse){S.sse.close();S.sse=null}}
  else{
    S.expanded=name;
    try{const d=await api('/api/logs/'+encodeURIComponent(name));S.logs[name]=(d.lines||[]).map(t=>({t,e:t.includes('[err]')}))}catch{S.logs[name]=[]}
    startSSE(name);
  }
  document.getElementById('body').innerHTML=rows(S.processes);
  const lb=document.getElementById('lbox');if(lb&&S.follow)lb.scrollTop=lb.scrollHeight;
}

async function act(name,action){
  try{const r=await api('/api/process/'+encodeURIComponent(name)+'/'+action,{method:'POST'});toast(r.ok?name+' '+action+'d':(r.output||'Failed'),r.ok?'ok':'err');await refresh()}
  catch(e){toast(String(e),'err')}
}

async function del(name){
  if(!confirm('Delete "'+name+'"? This cannot be undone.'))return;
  try{const r=await api('/api/process/'+encodeURIComponent(name),{method:'DELETE'});toast(r.ok?name+' deleted':(r.output||'Failed'),r.ok?'ok':'err');if(S.expanded===name){S.expanded=null;if(S.sse){S.sse.close();S.sse=null}}await refresh()}
  catch(e){toast(String(e),'err')}
}

async function refresh(){
  try{render(await api('/api/processes'))}
  catch{document.getElementById('dot').className='dot err';document.getElementById('lbl').textContent='Error';document.getElementById('fd').textContent='Offline'}
}

refresh();setInterval(refresh,2000);
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
