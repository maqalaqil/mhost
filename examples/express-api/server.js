const http = require("http");

const PORT = process.env.PORT || 4000;
const DB = [];  // in-memory "database"
let requestCount = 0;

function parseBody(req) {
  return new Promise((resolve) => {
    let body = "";
    req.on("data", (chunk) => body += chunk);
    req.on("end", () => {
      try { resolve(JSON.parse(body)); }
      catch { resolve(null); }
    });
  });
}

const server = http.createServer(async (req, res) => {
  requestCount++;
  const url = new URL(req.url, `http://localhost:${PORT}`);

  res.setHeader("Content-Type", "application/json");
  res.setHeader("X-Request-Id", `${Date.now()}-${requestCount}`);

  // Health check
  if (url.pathname === "/health") {
    res.writeHead(200);
    res.end(JSON.stringify({ status: "ok", db_size: DB.length, requests: requestCount }));
    return;
  }

  // GET /api/todos
  if (req.method === "GET" && url.pathname === "/api/todos") {
    console.log(JSON.stringify({ level: "info", message: "GET /api/todos", count: DB.length }));
    res.writeHead(200);
    res.end(JSON.stringify({ todos: DB, total: DB.length }));
    return;
  }

  // POST /api/todos
  if (req.method === "POST" && url.pathname === "/api/todos") {
    const body = await parseBody(req);
    if (!body || !body.title) {
      res.writeHead(400);
      res.end(JSON.stringify({ error: "title is required" }));
      return;
    }
    const todo = { id: DB.length + 1, title: body.title, done: false, created_at: new Date().toISOString() };
    DB.push(todo);
    console.log(JSON.stringify({ level: "info", message: "Created todo", todo_id: todo.id, title: todo.title }));
    res.writeHead(201);
    res.end(JSON.stringify(todo));
    return;
  }

  // DELETE /api/todos/:id
  if (req.method === "DELETE" && url.pathname.startsWith("/api/todos/")) {
    const id = parseInt(url.pathname.split("/").pop());
    const idx = DB.findIndex(t => t.id === id);
    if (idx === -1) {
      res.writeHead(404);
      res.end(JSON.stringify({ error: "not found" }));
      return;
    }
    DB.splice(idx, 1);
    console.log(JSON.stringify({ level: "info", message: "Deleted todo", todo_id: id }));
    res.writeHead(200);
    res.end(JSON.stringify({ deleted: id }));
    return;
  }

  // GET /api/stats
  if (url.pathname === "/api/stats") {
    res.writeHead(200);
    res.end(JSON.stringify({
      pid: process.pid,
      uptime: process.uptime(),
      memory_mb: Math.round(process.memoryUsage().rss / 1048576),
      requests: requestCount,
      todos: DB.length,
    }));
    return;
  }

  res.writeHead(404);
  res.end(JSON.stringify({ error: "not found" }));
});

server.listen(PORT, () => {
  console.log(JSON.stringify({
    level: "info",
    message: `Express-style API started on port ${PORT}`,
    pid: process.pid,
    endpoints: ["/health", "/api/todos", "/api/stats"],
  }));
});

process.on("SIGTERM", () => {
  console.log(JSON.stringify({ level: "info", message: "API shutting down gracefully" }));
  server.close(() => process.exit(0));
});
