const http = require("http");

const PORT = process.env.PORT || 4500;
let requestCount = 0;
let healthy = true;

// After 30 seconds, start returning 500s to simulate degradation
setTimeout(() => {
  healthy = false;
  console.log(JSON.stringify({ level: "error", message: "Service degraded — returning 500s", pid: process.pid }));
}, 30000);

// After 60 seconds, recover
setTimeout(() => {
  healthy = true;
  console.log(JSON.stringify({ level: "info", message: "Service recovered", pid: process.pid }));
}, 60000);

const server = http.createServer((req, res) => {
  requestCount++;

  if (req.url === "/health") {
    if (healthy) {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "ok", requests: requestCount }));
    } else {
      res.writeHead(503, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "degraded", error: "service unhealthy", requests: requestCount }));
    }
    return;
  }

  if (!healthy && Math.random() < 0.7) {
    res.writeHead(500, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: "Internal Server Error", request_id: requestCount }));
    console.log(JSON.stringify({ level: "error", message: "500 error", request_id: requestCount, path: req.url }));
    return;
  }

  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ message: "ok", pid: process.pid, requests: requestCount }));
});

server.listen(PORT, () => {
  console.log(JSON.stringify({
    level: "info",
    message: `Flaky API started on port ${PORT}`,
    pid: process.pid,
    note: "Will degrade at 30s, recover at 60s",
  }));
});

process.on("SIGTERM", () => {
  server.close(() => process.exit(0));
});
