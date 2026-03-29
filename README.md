<div align="center">

<br>

```
                 ██╗                    ██╗
 ██████╗██████╗ ██████╗  ██████╗ ███████╗████████╗
██╔████╗██╔══██╗██╔══██╗██╔═══██╗██╔════╝╚══██╔══╝
██║██╔██║██████╔╝██║  ██║██║   ██║███████╗   ██║
██║╚████║██╔══██╗██║  ██║██║   ██║╚════██║   ██║
╚██╗╚██╗██║  ██║██████╔╝╚██████╔╝███████║   ██║
 ╚══╝╚═╝╚═╝  ╚═╝╚═════╝  ╚═════╝ ╚══════╝   ╚═╝
```

**The process manager that PM2 should have been.**

Built in Rust. Single binary. Zero runtime dependencies.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.82%2B-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)](#)

[Installation](#installation) | [Quick Start](#quick-start) | [Config](#ecosystem-config) | [AI](#ai-intelligence) | [Notifications](#notifications) | [Commands](#all-commands)

</div>

---

## Why mhost?

| | PM2 | mhost |
|---|---|---|
| **Runtime** | Requires Node.js | Single 14MB binary |
| **Health checks** | Alive/dead only | HTTP, TCP, script probes |
| **Log search** | Grep files manually | Built-in FTS5 full-text search |
| **Notifications** | Plugin required | Telegram, Slack, Discord, Email, PagerDuty, Teams, Ntfy, Webhook |
| **Metrics** | Basic | Prometheus export, time-series, alerts, auto-remediation |
| **Proxy** | None | Built-in reverse proxy with auto-TLS |
| **Deploy** | Basic | Git pull + hooks + rollback with history |
| **Dashboard** | Web only | Terminal TUI with sparklines |
| **Groups** | None | Dependency ordering with topological sort |
| **Restart** | Basic | Exponential backoff + circuit breaker |
| **Config** | JS only | TOML, YAML, JSON |
| **AI** | None | Built-in LLM intelligence (OpenAI/Claude) — diagnose, optimize, ask |

---

## Installation

<table>
<tr><td><b>Homebrew</b></td><td>

```bash
brew install maheralaqil/tap/mhost
```

</td></tr>
<tr><td><b>npm</b></td><td>

```bash
npm install -g mhost
```

</td></tr>
<tr><td><b>Cargo</b></td><td>

```bash
cargo install mhost
```

</td></tr>
<tr><td><b>curl</b></td><td>

```bash
curl -fsSL https://mhost.dev/install.sh | sh
```

</td></tr>
<tr><td><b>PowerShell</b></td><td>

```powershell
irm https://mhost.dev/install.ps1 | iex
```

</td></tr>
<tr><td><b>Scoop</b></td><td>

```powershell
scoop install mhost
```

</td></tr>
<tr><td><b>From source</b></td><td>

```bash
git clone https://github.com/maheralaqil/mhost && cd mhost && cargo build --release
```

</td></tr>
</table>

---

## Quick Start

### Start any process

```bash
# Node.js
mhost start "node server.js" --name api

# Python
mhost start "python3 worker.py" --name worker

# Any binary
mhost start "./my-service --port 8080" --name service

# From ecosystem config
mhost start mhost.toml
```

### See what's running

```
$ mhost list

id   name          status    pid     inst  uptime      restarts  memory
────────────────────────────────────────────────────────────────────────
a1b2 api-server    online    12345   0     2d 14h 22m  0         128MB
c3d4 worker        online    12346   0     2d 14h 22m  0         64MB
e5f6 worker        online    12347   1     2d 14h 22m  0         62MB
g7h8 static-site   online    12348   0     1d 8h 15m   1         32MB
```

### Manage processes

```bash
mhost stop api-server        # Graceful SIGTERM -> wait -> SIGKILL
mhost restart worker         # Zero-downtime restart
mhost scale worker 4         # Scale to 4 instances
mhost delete api-server      # Remove from registry
mhost stop all               # Stop everything
```

### Persist across reboots

```bash
mhost save                   # Save current process list
mhost resurrect              # Restore after daemon restart
mhost startup                # Auto-start mhost on boot (launchd/systemd)
```

---

## Ecosystem Config

Define your entire stack in one file. Supports **TOML**, **YAML**, and **JSON**.

```toml
# mhost.toml

# ─── API Server ──────────────────────────────────────────────
[process.api-server]
command = "node server.js"
cwd = "/app/api"
env = { NODE_ENV = "production", PORT = "3000" }
instances = 4                         # Run 4 instances (cluster mode)
max_memory = "512MB"                  # Kill & restart if exceeds
max_restarts = 15                     # Circuit breaker threshold
min_uptime = "2s"                     # Crash = exit before this
restart_delay = "100ms"               # Base delay (exponential backoff)
grace_period = "5s"                   # SIGTERM wait before SIGKILL

[process.api-server.health.http]      # Health probe
url = "http://localhost:3000/health"
interval = "10s"
timeout = "3s"
retries = 3

# ─── Background Worker ──────────────────────────────────────
[process.worker]
command = "python3 worker.py"
cwd = "/app/worker"
instances = 2
max_restarts = 10

# ─── Scheduled Cleanup ──────────────────────────────────────
[process.cleanup]
command = "sh cleanup.sh"
cwd = "/app/scripts"
cron_restart = "0 3 * * *"           # Restart daily at 3am

# ─── Process Groups (dependency ordering) ────────────────────
[groups.backend]
depends_on = []
processes = ["api-server", "worker"]

[groups.frontend]
depends_on = ["backend"]              # Starts AFTER backend group
processes = ["static-site"]

# ─── Notifications ───────────────────────────────────────────
[notifications.telegram]
type = "telegram"
bot_token = "${MHOST_TELEGRAM_TOKEN}"
chat_id = "${MHOST_TELEGRAM_CHAT}"
events = ["crash", "errored", "health_fail", "5xx_error"]
throttle = "60s"

[notifications.slack]
type = "slack"
webhook = "${SLACK_WEBHOOK_URL}"
events = ["crash", "deploy_success", "deploy_fail"]

# ─── Log Sinks ───────────────────────────────────────────────
[logs.sinks.graylog]
type = "gelf"
host = "graylog.local"
port = 12201
processes = "api-*"

[logs.sinks.loki]
type = "loki"
url = "http://loki.local:3100/loki/api/v1/push"
labels = { env = "production" }

# ─── Alerts & Auto-Remediation ───────────────────────────────
[alerts.high-memory]
process = "api-server"
condition = "memory > 450MB for 5m"
notify = ["telegram", "slack"]
action = "restart"

[remediation.zombie-detection]
condition = "cpu < 1% AND health_fail for 5m"
action = "restart"
cooldown = "10m"
notify = ["slack"]

# ─── Deploy ──────────────────────────────────────────────────
[deploy.production]
repo = "git@github.com:user/app.git"
branch = "main"
path = "/var/www/app"
pre_deploy = ["npm install", "npm run build"]
post_deploy = ["mhost restart api-server"]

# ─── Reverse Proxy ───────────────────────────────────────────
[proxy]
listen = "0.0.0.0:80"
tls_listen = "0.0.0.0:443"
acme = true
acme_email = "admin@example.com"

[proxy.routes]
"api.example.com" = { target = "api-server", port = 3000 }
"app.example.com" = { target = "static-site", port = 8080 }
```

```bash
mhost start mhost.toml
```

---

## Health Probes

Processes only transition to `online` after health checks pass. Failures trigger restarts.

```toml
# HTTP probe — check status code
[process.api.health.http]
url = "http://localhost:3000/health"
interval = "10s"
timeout = "3s"
retries = 3

# TCP probe — check port is open
[process.db.health.tcp]
host = "127.0.0.1"
port = 5432
interval = "5s"

# Script probe — check exit code
[process.app.health.script]
command = "./check-health.sh"
interval = "15s"
```

```bash
mhost health api-server       # Show health status per instance
```

---

## Process Groups & Dependencies

Start services in dependency order. Stop in reverse.

```toml
[groups.database]
processes = ["postgres", "redis"]

[groups.backend]
depends_on = ["database"]              # postgres & redis start first
processes = ["api-server", "worker"]

[groups.frontend]
depends_on = ["backend"]               # api-server starts before nginx
processes = ["nginx"]
```

```bash
mhost start --group backend            # Starts database group first, then backend
mhost stop --group frontend            # Stops in reverse dependency order
```

---

## Auto-Restart & Circuit Breaker

```
Process crashes
    |
    v
Restart attempt #1 (delay: 100ms)
    |
    v
Restart attempt #2 (delay: 200ms)     # Exponential backoff
    |
    v
Restart attempt #3 (delay: 400ms)     # 100ms * 2^attempt
    |                                   # Capped at 30 seconds
    ...
    v
Attempt #15 within min_uptime
    |
    v
ERRORED (circuit breaker open)         # Stops retrying
                                        # Sends notification
```

---

## Notifications

### Quick Setup (CLI)

```bash
mhost notify setup                    # Interactive wizard
```

```
  mhost Notification Setup

  Select channel type:
    1) Telegram
    2) Slack
    3) Discord
    4) Generic Webhook

  Channel type (1-4): 1

  Telegram Setup
  ─────────────────────────────
  1. Message @BotFather on Telegram
  2. Send /newbot and follow the instructions
  3. Copy the bot token below
  4. Message your bot, then get your chat ID from @userinfobot

  Bot token: 123456:ABC-DEF...
  Chat ID: 987654321
  Channel name [telegram]: telegram

  Available alert events:
    1   crash
    2   restart
    3   errored
    4   stopped
    5   recovered
    6   health_fail
    7   high_restarts
    8   5xx_error
    9   oom_kill
    10  deploy_success
    11  deploy_fail
    *   All events

  Select events (comma-separated numbers, or * for all): *

  ✓ Channel 'telegram' configured and saved
```

### Manage Channels

```bash
mhost notify list                     # Show all configured channels
mhost notify test telegram            # Send a test message
mhost notify enable telegram          # Enable a channel
mhost notify disable telegram         # Disable without removing
mhost notify remove telegram          # Delete a channel
mhost notify events                   # Show all event types
mhost notify events telegram          # Show channel subscriptions
mhost notify start                    # Launch notifier as managed process
```

### Supported Channels

| Channel | Transport | Features |
|---|---|---|
| **Telegram** | Bot API | Rich markdown, inline buttons |
| **Slack** | Webhook | Block Kit, color-coded |
| **Discord** | Webhook | Embeds, severity colors |
| **Webhook** | HTTP POST | Custom headers, HMAC-SHA256 signing |
| **Email** | SMTP/TLS | HTML templates, digest mode |
| **PagerDuty** | Events API v2 | Severity mapping, auto-resolve |
| **Microsoft Teams** | Webhook | Adaptive cards |
| **Ntfy** | HTTP | Self-hosted push notifications |

### Alert Events

| Event | Trigger |
|---|---|
| `crash` | Process exited with non-zero code |
| `restart` | Process auto-restarted by mhost |
| `errored` | Max restarts exceeded (circuit breaker tripped) |
| `stopped` | Process was stopped |
| `recovered` | Process came back online after failure |
| `health_fail` | Health check probe failed |
| `high_restarts` | Process restarted 5+ times |
| `5xx_error` | Health endpoint returned HTTP 5xx |
| `oom_kill` | Process killed for exceeding memory limit |
| `deploy_success` | Deploy completed successfully |
| `deploy_fail` | Deploy failed |

### Throttling & Escalation

```toml
[notifications.slack]
throttle = "60s"                       # Suppress duplicate alerts for 60s

[notifications.escalation]
chain = ["slack", "telegram", "pagerduty"]
escalate_after = "5m"                  # If no ack in 5min, notify next channel
```

---

## Log Engine

### Built-in full-text search (SQLite FTS5)

```bash
# Search across all logs
mhost logs api --search "connection refused" --since 1h

# Structured queries
mhost logs api --where "level=error AND status>=500"

# Regex filtering
mhost logs api --grep "status=[45]\d\d"

# Aggregation
mhost logs --all --where "level=error" --since 1h --count-by process

# Export
mhost logs api --since 7d --format jsonl > export.jsonl
```

### JSON auto-detection

If your process outputs JSON to stdout, mhost automatically parses and indexes every field:

```json
{"level":"error","message":"Connection timeout","status":503,"latency_ms":5032}
```

All fields become searchable: `--where "status>=500"`, `--search "timeout"`.

### External Log Sinks

Push logs to your existing infrastructure:

```toml
[logs.sinks.graylog]
type = "gelf"
host = "graylog.local"
port = 12201
transport = "udp"
processes = "api-*"                    # Glob pattern matching

[logs.sinks.loki]
type = "loki"
url = "http://loki.local:3100/loki/api/v1/push"

[logs.sinks.elasticsearch]
type = "elasticsearch"
url = "http://es.local:9200"
index = "mhost-logs-{date}"           # Date-templated index names

[logs.sinks.syslog]
type = "syslog"
host = "syslog.local"
port = 514
```

### Retention Policies

```
info  logs  ->  7 days
warn  logs  ->  30 days
error logs  ->  30 days
fatal logs  ->  90 days
```

---

## Metrics & Prometheus

### CLI

```bash
mhost metrics show api-server         # Current CPU, memory, uptime
mhost metrics history api --metric cpu --since 24h
mhost metrics start --listen 0.0.0.0:9090  # Start Prometheus exporter
```

### Prometheus endpoint

```
GET http://localhost:9090/metrics

# HELP mhost_process_cpu_percent CPU usage percentage
# TYPE mhost_process_cpu_percent gauge
mhost_process_cpu_percent{name="api",instance="0"} 42.5
mhost_process_memory_bytes{name="api",instance="0"} 134217728
mhost_process_uptime_seconds{name="api",instance="0"} 86400
mhost_process_restart_total{name="api",instance="0"} 2
```

### Alert Rules & Auto-Remediation

```toml
[alerts.high-memory]
process = "api-server"
condition = "memory > 450MB for 5m"
notify = ["telegram", "slack"]
action = "restart"                     # Auto-restart on breach

[remediation.zombie-detection]
condition = "cpu < 1% AND health_fail for 5m"
action = "restart"
cooldown = "10m"                       # Don't re-trigger for 10 min
```

---

## Reverse Proxy

Built-in HTTP/HTTPS reverse proxy with virtual host routing.

```toml
[proxy]
listen = "0.0.0.0:80"
tls_listen = "0.0.0.0:443"
acme = true                            # Auto-TLS via Let's Encrypt
acme_email = "admin@example.com"

[proxy.routes]
"api.example.com" = { target = "api-server", port = 3000, strategy = "least_connections" }
"app.example.com" = { target = "frontend", port = 8080, sticky = true }
```

**Features:** Load balancing (round-robin, least-connections, IP-hash), sticky sessions, WebSocket passthrough, self-signed TLS for local dev, ACME auto-cert for production.

```bash
mhost proxy                            # Show current routes
```

---

## Deploy Engine

```bash
mhost deploy production                # git pull + hooks + graceful reload
mhost rollback production              # Revert to previous successful deploy
```

```toml
[deploy.production]
repo = "git@github.com:user/app.git"
branch = "main"
path = "/var/www/app"
pre_deploy = ["npm install", "npm run build"]
post_deploy = ["mhost restart api-server"]
```

Deploy history is tracked in SQLite with commit hashes, timestamps, and status.

---

## AI Intelligence

The first process manager with built-in LLM capabilities. Supports **OpenAI** (GPT-4o) and **Claude** (Sonnet/Opus).

### Setup

```bash
mhost ai setup
```

```
  mhost AI Setup

  Select LLM provider:
    1) OpenAI (GPT-4o, GPT-4o-mini)
    2) Claude (Sonnet, Haiku, Opus)

  Provider (1-2): 1
  API key: sk-...
  Model [gpt-4o]: gpt-4o

  ✓ AI configured with openai (gpt-4o)
```

Or configure via file (`~/.mhost/ai.json`) or environment variables.

### Crash Diagnosis

```bash
mhost ai diagnose api-server
```

```
  Analyzing crash for 'api-server'...

  ## Root Cause
  The process crashed due to an unhandled promise rejection in database.js:42.
  The connection pool was exhausted after 15 concurrent requests exceeded the
  pool limit of 10.

  ## Impact
  Severity: HIGH — All API requests failed for 12 seconds until auto-restart.

  ## Fix Steps
  1. Increase pool size: `max_connections: 25` in database config
  2. Add connection timeout: `idle_timeout: 30000`

  ## Prevention
  - Add health check for DB connection pool utilization
  - Set up alert: `condition = "memory > 256MB for 5m"`

  ## Config Suggestions
  max_memory = "512MB"    # Currently 256MB — too tight
  max_restarts = 20       # Currently 15 — increase headroom
```

### Natural Language Log Queries

```bash
mhost ai logs api "show me all timeout errors in the last hour"
mhost ai logs worker "what errors happened during the deploy?"
mhost ai logs api "count errors by type today"
```

### Generate Config from Description

```bash
mhost ai config "I have a Node.js API on port 3000 with 2 Python celery workers
                  and a React frontend. Add health checks and process groups."
```

Outputs a complete, valid `mhost.toml` ready to use.

### Performance Optimization

```bash
mhost ai optimize api-server
```

Analyzes CPU/memory trends and suggests instance count, memory limits, restart thresholds.

### Incident Post-Mortem

```bash
mhost ai postmortem api-server
```

Generates a full Markdown incident report with timeline, root cause, impact, and action items.

### Ask Anything

```bash
mhost ai ask "which process is using the most memory?"
mhost ai ask "why has my worker restarted 12 times today?"
mhost ai ask "should I scale up my API?"
```

### More AI Commands

```bash
mhost ai watch                        # Scan all processes for anomalies
mhost ai explain mhost.toml           # Explain config in plain English
mhost ai suggest                      # Get proactive improvement suggestions
```

### AI Config

```toml
# In mhost.toml
[ai]
provider = "openai"                    # or "claude"
api_key = "${OPENAI_API_KEY}"          # env var expansion
model = "gpt-4o"                       # any supported model
max_tokens = 4096
```

---

## TUI Dashboard

```bash
mhost monit
```

```
┌─ mhost ─────────────────────────────────────────────────────┐
│ [Processes]  Logs   Metrics   Proxy                         │
├─────────────────────────────────────────────────────────────┤
│ #  Name          Status   PID    CPU%   Memory   Uptime    │
│ ─────────────────────────────────────────────────────────── │
│ 0  api-server    online   12345  12.3%  128MB    2d 14h    │
│ 1  worker        online   12346   3.1%   64MB    2d 14h    │
│ 2  worker        online   12347   2.8%   62MB    2d 14h    │
│ 3  cleanup       online   12348   0.1%   16MB    8h 22m    │
│                                                             │
│ CPU  ▁▂▃▅▇▅▃▂▁▂▃▄▅▃▂▁  12.3%                              │
│ MEM  ▃▃▃▃▃▃▃▄▄▄▃▃▃▃▃▃  128MB                              │
└─────────────────────────────────────────────────────────────┘
  j/k: navigate  Tab: switch view  q: quit  /: search
```

**Keyboard:** `j`/`k` navigate, `g`/`G` top/bottom, `Tab` switch tabs, `/` search, `q` quit, `r` restart, `s` stop selected process.

---

## All Commands

### Process Management

| Command | Description |
|---|---|
| `mhost start <app\|config>` | Start a process or ecosystem config |
| `mhost stop <app\|all>` | Graceful stop (SIGTERM -> grace period -> SIGKILL) |
| `mhost restart <app\|all>` | Restart with auto-recovery |
| `mhost delete <app\|all>` | Remove from process registry |
| `mhost list` | List all processes with status, CPU, memory, uptime |
| `mhost info <app>` | Detailed process info |
| `mhost env <app>` | Show environment variables |
| `mhost scale <app> <N>` | Scale to N instances |
| `mhost cluster <app> <N>` | Alias for scale |
| `mhost health <app>` | Show health check status per instance |
| `mhost config <app>` | Show process config as JSON |
| `mhost history <app>` | Show process event history |

### Groups

| Command | Description |
|---|---|
| `mhost start --group <name>` | Start group in dependency order |
| `mhost stop --group <name>` | Stop group in reverse order |

### Logs

| Command | Description |
|---|---|
| `mhost logs <app>` | Tail last 50 lines |
| `mhost logs <app> -n 200` | Tail last 200 lines |
| `mhost logs <app> --err` | Show stderr |
| `mhost logs <app> --grep "pattern"` | Filter by substring |
| `mhost logs <app> --search "query"` | FTS5 full-text search |
| `mhost logs <app> --where "level=error"` | Structured query |
| `mhost logs <app> --since 1h` | Time-range filter |
| `mhost logs <app> --format jsonl` | Export as JSON Lines |
| `mhost logs <app> --count-by level` | Aggregate by field |

### Notifications

| Command | Description |
|---|---|
| `mhost notify setup` | Interactive channel setup wizard |
| `mhost notify list` | Show configured channels |
| `mhost notify test <channel>` | Send test notification |
| `mhost notify enable <channel>` | Enable a channel |
| `mhost notify disable <channel>` | Disable a channel |
| `mhost notify remove <channel>` | Remove a channel |
| `mhost notify events [channel]` | Show event types and subscriptions |
| `mhost notify start` | Start notifier as managed process |

### Metrics

| Command | Description |
|---|---|
| `mhost metrics show <app>` | Current CPU, memory, uptime |
| `mhost metrics history <app>` | Time-series query |
| `mhost metrics start` | Start Prometheus /metrics exporter |

### Deploy

| Command | Description |
|---|---|
| `mhost deploy <env>` | Deploy via git pull + hooks |
| `mhost rollback <env>` | Revert to previous successful deploy |

### AI

| Command | Description |
|---|---|
| `mhost ai setup` | Interactive LLM provider setup (OpenAI/Claude) |
| `mhost ai diagnose <app>` | Analyze crash with root cause, fix steps, prevention |
| `mhost ai logs <app> "<question>"` | Natural language log search |
| `mhost ai optimize <app>` | Performance recommendations with config diff |
| `mhost ai config "<description>"` | Generate mhost.toml from plain English |
| `mhost ai postmortem <app>` | Generate incident report (Markdown) |
| `mhost ai watch` | Scan all processes for anomalies |
| `mhost ai ask "<question>"` | Ask anything about your processes |
| `mhost ai explain [config]` | Explain config in plain English |
| `mhost ai suggest` | Proactive improvement suggestions |

### Infrastructure

| Command | Description |
|---|---|
| `mhost monit` | Launch TUI dashboard |
| `mhost proxy` | Show reverse proxy routes |
| `mhost ping` | Check if daemon is alive |
| `mhost kill` | Kill the daemon |
| `mhost save` | Save process list for resurrection |
| `mhost resurrect` | Restore saved processes |
| `mhost startup` | Generate OS boot script (launchd/systemd) |
| `mhost unstartup` | Remove boot script |
| `mhost self-update` | Update to latest version |
| `mhost completion <shell>` | Generate shell completions (bash/zsh/fish/powershell) |

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                        mhost CLI                              │
│  start | stop | list | logs | monit | notify | ai | deploy   │
└──────────────┬───────────────────────────────────────────────┘
               │  JSON-RPC 2.0 over Unix Socket
┌──────────────▼───────────────────────────────────────────────┐
│                        mhostd (Daemon)                        │
│                                                               │
│  Process Supervisor     Health Checker     Reverse Proxy      │
│  - Spawn / signal       - HTTP / TCP       - Host routing     │
│  - Auto-restart          - Script probes   - Load balancing   │
│  - Backoff / circuit     - Status gate     - Auto-TLS         │
│  - Groups / deps                           - Sticky sessions  │
│                                                               │
│  Log Engine             Metrics            Notifications      │
│  - FTS5 search          - CPU / memory     - 8 channels       │
│  - JSON auto-parse      - Prometheus       - Throttle         │
│  - 4 external sinks     - Alerts           - Escalation       │
│  - Retention            - Auto-remediate   - Auto-resolve     │
│                                                               │
│  Deploy Engine          Scheduler          State Store        │
│  - Git pull / hooks     - Cron restarts    - SQLite           │
│  - Rollback             - Memory monitor   - Event history    │
│                                                               │
│  AI Intelligence                                              │
│  - OpenAI / Claude      - Crash diagnosis  - Log queries      │
│  - Config generation    - Optimization     - Anomaly watch    │
│  - Post-mortems         - Ask anything     - Suggestions      │
└──────────────────────────────────────────────────────────────┘
```

### Crate Structure (13 crates)

```
mhost/
├── mhost-core       Core types, state machine, protocol
├── mhost-config     TOML/YAML/JSON config parsing
├── mhost-ipc        JSON-RPC over Unix socket / named pipe
├── mhost-logs       Log capture, FTS5, rotation, sinks
├── mhost-health     HTTP/TCP/script health probes
├── mhost-notify     8 notification channels + throttle + escalation
├── mhost-metrics    Collector, time-series, Prometheus, alerts
├── mhost-proxy      Reverse proxy, TLS, ACME, load balancing
├── mhost-deploy     Git deploy, hooks, rollback, history
├── mhost-ai         LLM intelligence (OpenAI/Claude) — diagnose, optimize, ask
├── mhost-tui        Terminal dashboard (ratatui)
├── mhost-daemon     Supervisor, handler, state store (mhostd binary)
└── mhost-cli        CLI interface (mhost binary)
```

---

## Environment Variable Expansion

Use `${VAR}` syntax anywhere in config files:

```toml
[process.api]
command = "${API_BINARY}"
env = { DATABASE_URL = "${DB_URL}", PORT = "${API_PORT}" }

[notifications.telegram]
bot_token = "${MHOST_TELEGRAM_TOKEN}"
chat_id = "${MHOST_TELEGRAM_CHAT}"
```

---

## Cross-Platform

| Feature | macOS | Linux | Windows |
|---|---|---|---|
| IPC | Unix socket | Unix socket | Named pipe |
| Signals | SIGTERM/SIGKILL | SIGTERM/SIGKILL | TerminateProcess |
| Startup | launchd | systemd | Task Scheduler |
| Memory | `ps` | `/proc` | WMI |

---

## Examples

The `examples/` directory contains ready-to-run demo projects:

```bash
# Simple 3-process ecosystem
mhost start examples/mhost.toml

# Full-stack with health checks, groups, memory limits, cron
mhost start examples/full-stack.toml
```

| Example | What it demonstrates |
|---|---|
| `node-api/` | HTTP server with `/health` endpoint |
| `express-api/` | REST API with CRUD, stats, health checks |
| `react-app/` | SPA with live dashboard UI |
| `python-worker/` | Background task processor with JSON logging |
| `bash-monitor/` | System metrics collector (CPU, memory, disk) |
| `cron-job/` | Periodic cleanup with report generation |
| `crasher/` | Unstable process for testing auto-restart |
| `flaky-api/` | API that degrades and recovers (5xx testing) |

---

## License

MIT - [Maher Al-Aqil](https://github.com/maheralaqil)
