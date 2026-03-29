const http = require("http");
const fs = require("fs");
const path = require("path");

const PORT = process.env.PORT || 5173;
const DIST = path.join(__dirname, "dist");

// Ensure dist directory exists
if (!fs.existsSync(DIST)) {
  fs.mkdirSync(DIST, { recursive: true });
}

// Create index.html if it doesn't exist
const indexPath = path.join(DIST, "index.html");
if (!fs.existsSync(indexPath)) {
  fs.writeFileSync(indexPath, `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>mhost React App</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #1a1a2e; color: #eee; }
    .app { max-width: 800px; margin: 0 auto; padding: 2rem; }
    h1 { font-size: 2.5rem; margin-bottom: 0.5rem; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
    .subtitle { color: #888; margin-bottom: 2rem; }
    .card { background: #16213e; border-radius: 12px; padding: 1.5rem; margin-bottom: 1rem; border: 1px solid #0f3460; }
    .card h3 { color: #667eea; margin-bottom: 0.5rem; }
    .status { display: inline-block; padding: 4px 12px; border-radius: 20px; font-size: 0.85rem; font-weight: 600; }
    .status.online { background: #0f5132; color: #75b798; }
    .counter { font-size: 3rem; font-weight: bold; color: #667eea; text-align: center; margin: 1rem 0; }
    button { background: #667eea; color: white; border: none; padding: 10px 24px; border-radius: 8px; cursor: pointer; font-size: 1rem; margin-right: 8px; }
    button:hover { background: #5a6fd6; }
    .footer { margin-top: 2rem; color: #555; font-size: 0.85rem; text-align: center; }
  </style>
</head>
<body>
  <div class="app" id="root">
    <h1>mhost Dashboard</h1>
    <p class="subtitle">React-style SPA served by mhost process manager</p>
    <div class="card">
      <h3>Server Status</h3>
      <span class="status online">Online</span>
      <p style="margin-top: 0.5rem; color: #aaa;">PID: <span id="pid">-</span> | Port: ${PORT}</p>
    </div>
    <div class="card">
      <h3>Live Counter</h3>
      <div class="counter" id="counter">0</div>
      <div style="text-align:center">
        <button onclick="document.getElementById('counter').textContent = parseInt(document.getElementById('counter').textContent) + 1">Increment</button>
        <button onclick="document.getElementById('counter').textContent = 0" style="background:#e74c3c">Reset</button>
      </div>
    </div>
    <div class="card">
      <h3>API Health</h3>
      <p id="health" style="color:#aaa">Checking...</p>
    </div>
    <p class="footer">Managed by mhost v0.1.0</p>
  </div>
  <script>
    fetch('/api/status').then(r => r.json()).then(d => {
      document.getElementById('pid').textContent = d.pid;
      document.getElementById('health').innerHTML = 'Uptime: ' + Math.floor(d.uptime) + 's | Requests: ' + d.requests;
    }).catch(() => {
      document.getElementById('health').textContent = 'API unreachable';
    });
  </script>
</body>
</html>`);
}

const MIME = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".png": "image/png",
  ".svg": "image/svg+xml",
};

let requestCount = 0;

const server = http.createServer((req, res) => {
  requestCount++;

  // API endpoint
  if (req.url === "/api/status") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({
      pid: process.pid,
      uptime: process.uptime(),
      requests: requestCount,
      memory: process.memoryUsage().rss,
    }));
    return;
  }

  // Health endpoint
  if (req.url === "/health") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "ok" }));
    return;
  }

  // Serve static files
  let filePath = path.join(DIST, req.url === "/" ? "index.html" : req.url);
  const ext = path.extname(filePath);

  // SPA fallback
  if (!ext || !fs.existsSync(filePath)) {
    filePath = indexPath;
  }

  try {
    const content = fs.readFileSync(filePath);
    const mime = MIME[path.extname(filePath)] || "application/octet-stream";
    res.writeHead(200, { "Content-Type": mime });
    res.end(content);
  } catch {
    res.writeHead(404);
    res.end("Not Found");
  }
});

server.listen(PORT, () => {
  console.log(JSON.stringify({
    level: "info",
    message: `React app server started on http://localhost:${PORT}`,
    pid: process.pid,
    timestamp: new Date().toISOString(),
  }));
});

process.on("SIGTERM", () => {
  console.log(JSON.stringify({ level: "info", message: "React server shutting down" }));
  server.close(() => process.exit(0));
});
