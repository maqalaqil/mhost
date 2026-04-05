# mhost API Layer — Design Spec

## Overview

Add a public API layer to mhost: REST API with bearer token auth, WebSocket real-time streaming, and outbound webhook system. Built as a Rust-native axum server inside the daemon, gated behind a Cargo feature flag (`--features api`) so the default binary stays lean.

## Architecture

```
                          ┌─────────────────────────────────────────────┐
                          │              mhostd (daemon)                │
                          │                                             │
  mhost CLI ──────────────┤  IPC Server (Unix socket)  ← existing      │
                          │       │                                     │
                          │       ▼                                     │
  Browser/Mobile ─────────┤  API Server (HTTP/WS)      ← NEW           │
  CI/CD pipelines ────────┤    ├── REST endpoints      (axum)          │
  Grafana/Datadog ────────┤    ├── WebSocket streams                   │
  Custom scripts ─────────┤    └── Token auth middleware               │
                          │       │                                     │
                          │       ▼                                     │
                          │  ┌─────────┐  ┌──────────┐  ┌───────────┐ │
                          │  │Supervisor│  │Event Bus │  │Token Store│ │
                          │  └─────────┘  └──────────┘  └───────────┘ │
                          │       │              │                      │
                          │       │              ▼                      │
                          │       │       Webhook Dispatcher ← NEW     │
                          │       │         ├── HTTP POST + HMAC       │
                          │       │         └── Retry + dead letter     │
                          └───────┼────────────────────────────────────┘
                                  │
                          Process spawning, health checks, logs
```

### Key decisions

- API server runs **inside `mhostd`** — same process, direct supervisor/state access, zero IPC overhead.
- Gated behind `--features api` Cargo feature flag. Default binary has no API code compiled. Zero overhead when disabled.
- Default port: **19516** (one above Windows TCP IPC fallback).
- Three subsystems: REST API, WebSocket streaming, outbound webhooks.
- New crate: **`mhost-api`**.
- Existing Node.js dashboard can be rewired to hit this API instead of shelling out to CLI.
- **Event Bus** is a `tokio::sync::broadcast` channel inside the daemon. The supervisor publishes process events (crash, restart, stop, etc.) to it. The API server subscribes to forward events to WebSocket clients and outbound webhooks.

## Authentication & Token System

### Token storage

File: `~/.mhost/api-tokens.json`

```json
[
  {
    "id": "tok_a1b2c3",
    "name": "ci-pipeline",
    "secret_hash": "<argon2 hash>",
    "role": "operator",
    "created_at": "2026-04-04T12:00:00Z",
    "last_used": "2026-04-04T14:30:00Z",
    "expires_at": null
  }
]
```

### Roles

Three roles, consistent with the existing bot permission system:

| Role | Permissions |
|---|---|
| **viewer** | GET endpoints only — list, info, logs, health, metrics |
| **operator** | viewer + POST/DELETE — start, stop, restart, scale, deploy |
| **admin** | operator + token management, webhook config, kill daemon |

### Auth flow

1. Client sends `Authorization: Bearer mhost_tok_...` header.
2. Server hashes the token, looks up the hash in the token store.
3. Checks role against endpoint requirement.
4. Updates `last_used` timestamp.
5. Returns 401 (invalid/expired token) or 403 (insufficient role).

### Token creation

```bash
$ mhost api token create --name ci-pipeline --role operator
```

- Token shown **once** at creation. Only the argon2 hash is stored.
- Prefix: `mhost_tok_` for identification.
- Optional: `--expires 30d` for time-limited tokens.
- Rate limiting: 100 req/min per token (configurable).

## REST API

### Base URL

`http://localhost:19516/api/v1`

### Response envelope

All responses use a consistent format:

```json
{
  "ok": true,
  "data": { ... },
  "error": null
}
```

Error responses:

```json
{
  "ok": false,
  "data": null,
  "error": "Process 'api-server' not found"
}
```

### Endpoints (25 total)

#### Process Management

| Method | Endpoint | Role | Description |
|---|---|---|---|
| `GET` | `/processes` | viewer | List all processes |
| `GET` | `/processes/:name` | viewer | Process detail (info + config) |
| `POST` | `/processes` | operator | Start a new process |
| `POST` | `/processes/:name/restart` | operator | Restart |
| `POST` | `/processes/:name/stop` | operator | Stop |
| `POST` | `/processes/:name/reload` | operator | Zero-downtime reload |
| `POST` | `/processes/:name/scale` | operator | Scale `{ "instances": N }` |
| `DELETE` | `/processes/:name` | operator | Delete from registry |
| `POST` | `/processes/stop-all` | operator | Stop everything |
| `POST` | `/processes/restart-all` | operator | Restart everything |

#### Logs

| Method | Endpoint | Role | Description |
|---|---|---|---|
| `GET` | `/logs/:name` | viewer | Last N lines `?lines=50&err=false` |
| `GET` | `/logs/:name/search` | viewer | FTS5 search `?q=timeout&since=1h` |

#### Health & Metrics

| Method | Endpoint | Role | Description |
|---|---|---|---|
| `GET` | `/health` | viewer | Daemon health + version |
| `GET` | `/health/:name` | viewer | Process health probe status |
| `GET` | `/metrics/:name` | viewer | CPU, memory, uptime |
| `GET` | `/metrics` | viewer | All process metrics (Prometheus-compatible with `Accept: text/plain`) |

#### System

| Method | Endpoint | Role | Description |
|---|---|---|---|
| `POST` | `/save` | operator | Save process list |
| `POST` | `/resurrect` | operator | Restore saved processes |
| `POST` | `/kill` | admin | Kill daemon |
| `GET` | `/version` | viewer | Version + platform info |

#### Token Management

| Method | Endpoint | Role | Description |
|---|---|---|---|
| `GET` | `/tokens` | admin | List tokens (no secrets) |
| `POST` | `/tokens` | admin | Create token |
| `DELETE` | `/tokens/:id` | admin | Revoke a token |

#### Webhook Management

| Method | Endpoint | Role | Description |
|---|---|---|---|
| `GET` | `/webhooks` | admin | List configured webhooks |
| `POST` | `/webhooks` | admin | Register webhook |
| `DELETE` | `/webhooks/:id` | admin | Remove webhook |
| `POST` | `/webhooks/:id/test` | admin | Send test event |

## WebSocket Streaming

### Endpoint

`ws://localhost:19516/api/v1/ws?token=mhost_tok_...`

Auth via query param since WebSocket handshake cannot set custom headers.

### Subscription model

Client sends JSON messages to subscribe/unsubscribe to channels:

```json
{ "type": "subscribe", "channel": "events" }
{ "type": "subscribe", "channel": "logs", "process": "api-server" }
{ "type": "subscribe", "channel": "metrics", "process": "api-server" }
{ "type": "subscribe", "channel": "all" }
{ "type": "unsubscribe", "channel": "logs", "process": "api-server" }
```

### Server push messages

**Process event:**
```json
{
  "channel": "events",
  "event": "crash",
  "process": "api-server",
  "data": { "exit_code": 1, "pid": 12345, "restarts": 3 },
  "timestamp": "2026-04-04T14:30:00Z"
}
```

**Log line:**
```json
{
  "channel": "logs",
  "process": "api-server",
  "line": "Error: Connection refused",
  "stream": "stderr",
  "timestamp": "2026-04-04T14:30:00Z"
}
```

**Metrics snapshot (pushed every 5s):**
```json
{
  "channel": "metrics",
  "process": "api-server",
  "data": { "cpu": 12.3, "memory_mb": 128, "uptime_secs": 86400 },
  "timestamp": "2026-04-04T14:30:01Z"
}
```

### Commands over WebSocket

Operator/admin tokens can send commands:

```json
{ "type": "command", "action": "restart", "process": "api-server" }
```

Response:
```json
{ "type": "command_result", "ok": true, "action": "restart", "process": "api-server" }
```

### Connection management

- Heartbeat ping every 30s, pong required within 10s.
- Max 50 concurrent WebSocket connections.
- Role enforcement on command messages (viewer can subscribe, not send commands).

## Outbound Webhooks

### Registration

```bash
$ mhost api webhook add \
    --url https://myapp.com/hooks/mhost \
    --events crash,restart,health_fail \
    --secret my-signing-key
```

### Delivery format

```
POST https://myapp.com/hooks/mhost
Content-Type: application/json
X-Mhost-Event: crash
X-Mhost-Delivery: del_f8a2b3c4
X-Mhost-Signature: sha256=7d38cdd689735b008326...
X-Mhost-Timestamp: 1743782400

{
  "id": "del_f8a2b3c4",
  "event": "crash",
  "process": "api-server",
  "timestamp": "2026-04-04T14:30:00Z",
  "data": {
    "exit_code": 1,
    "pid": 12345,
    "restarts": 3,
    "last_log_lines": ["Error: ECONNREFUSED", "at connect (net.js:42)"]
  }
}
```

### Available events

| Event | Trigger |
|---|---|
| `process.crash` | Non-zero exit |
| `process.restart` | Auto-restarted |
| `process.start` | Process started |
| `process.stop` | Process stopped |
| `process.errored` | Circuit breaker tripped |
| `process.recovered` | Came back online |
| `health.fail` | Health probe failed |
| `health.pass` | Health probe recovered |
| `deploy.start` | Deploy began |
| `deploy.success` | Deploy completed |
| `deploy.fail` | Deploy failed |
| `metrics.alert` | Alert threshold crossed |
| `*` | All events |

### Reliability

- **Retry:** 3 attempts with exponential backoff (5s, 30s, 120s).
- **Timeout:** 10s per delivery attempt.
- **Dead letter:** Failed deliveries logged to `~/.mhost/webhook-failures.jsonl`.
- **Signing:** HMAC-SHA256 via shared secret. Reuses existing `compute_hmac` from `mhost-notify`.
- **Replay protection:** `X-Mhost-Timestamp` lets receivers reject deliveries older than 5 minutes.

### Storage

File: `~/.mhost/webhooks.json`

```json
{
  "webhooks": [
    {
      "id": "wh_x9k2m1",
      "url": "https://myapp.com/hooks/mhost",
      "events": ["crash", "restart", "health_fail"],
      "secret": "my-signing-key",
      "enabled": true,
      "created_at": "2026-04-04T12:00:00Z",
      "failure_count": 0
    }
  ]
}
```

## CLI Commands

```bash
# Server lifecycle
mhost api start [--port 19516] [--bind 0.0.0.0]
mhost api stop
mhost api status

# Token management
mhost api token create --name <name> --role <role> [--expires 30d]
mhost api token list
mhost api token revoke <id>

# Webhook management
mhost api webhook add --url <url> --events <events> [--secret <key>]
mhost api webhook list
mhost api webhook remove <id>
mhost api webhook test <id>
mhost api webhook failures
```

## Configuration

```toml
[api]
enabled = true
port = 19516
bind = "0.0.0.0"
rate_limit = 100            # requests/min per token
max_ws_connections = 50
cors_origins = ["*"]
```

## Feature Flag

Cargo feature: `api` on `mhost-daemon`.

- `cargo install mhost` — no API code compiled, zero overhead.
- `cargo install mhost --features api` — includes API server.
- Binary size difference: ~2-3MB.

When compiled without the feature, `mhost api *` commands print an error with reinstall instructions.

## Crate Structure

```
crates/mhost-api/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Feature-gated public API
│   ├── server.rs           # Axum HTTP/WS server setup
│   ├── auth.rs             # Token validation middleware
│   ├── tokens.rs           # Token CRUD + storage
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── processes.rs    # /processes/* handlers
│   │   ├── logs.rs         # /logs/* handlers
│   │   ├── health.rs       # /health/* handlers
│   │   ├── metrics.rs      # /metrics/* handlers
│   │   ├── system.rs       # /save, /resurrect, /kill, /version
│   │   ├── tokens.rs       # /tokens/* handlers
│   │   └── webhooks.rs     # /webhooks/* handlers
│   ├── websocket.rs        # WebSocket upgrade + channel subscriptions
│   ├── webhook_dispatch.rs # Outbound webhook delivery + retry
│   └── rate_limit.rs       # Per-token rate limiting
```

## File Locations

| Path | Purpose |
|---|---|
| `~/.mhost/api-tokens.json` | Token store (hashed secrets) |
| `~/.mhost/webhooks.json` | Webhook registrations |
| `~/.mhost/webhook-failures.jsonl` | Dead letter log |

## Dependencies

New crate dependencies (only compiled with `--features api`):

- `axum` — HTTP server + WebSocket support
- `axum-extra` — typed headers
- `tokio-tungstenite` — WebSocket protocol (via axum)
- `argon2` — token secret hashing
- `tower` — middleware (rate limiting, CORS)
- `tower-http` — CORS layer
- `uuid` — delivery IDs

Most of these are already in the workspace (axum used by metrics Prometheus exporter).
