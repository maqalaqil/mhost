const http = require("http");

const PORT = process.env.PORT || 3000;
let requestCount = 0;

const server = http.createServer((req, res) => {
  requestCount++;

  if (req.url === "/health") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "ok", requests: requestCount, pid: process.pid }));
    return;
  }

  if (req.url === "/") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({
      message: "Hello from mhost Node API!",
      pid: process.pid,
      uptime: process.uptime(),
      requests: requestCount,
    }));
    return;
  }

  res.writeHead(404);
  res.end("Not Found");
});

server.listen(PORT, () => {
  console.log(JSON.stringify({
    level: "info",
    message: `API server started on port ${PORT}`,
    pid: process.pid,
    timestamp: new Date().toISOString(),
  }));
});

// Graceful shutdown
process.on("SIGTERM", () => {
  console.log(JSON.stringify({ level: "info", message: "Received SIGTERM, shutting down gracefully" }));
  server.close(() => process.exit(0));
});
