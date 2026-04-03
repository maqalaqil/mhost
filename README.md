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
| **Autonomous agent** | None | AI agent with LLM tool calling — monitors, acts, reports to Telegram |
| **Cloud fleet** | None | SSH fleet management — AWS, Azure, DO, Railway auto-import |

---

## Installation

<table>
<tr><td><b>Homebrew</b></td><td>

```bash
brew install maqalaqil/tap/mhost
```

</td></tr>
<tr><td><b>npm</b></td><td>

```bash
npm install -g @maqalaqil93/mhost
```

</td></tr>
<tr><td><b>Cargo</b></td><td>

```bash
cargo install mhost
```

</td></tr>
<tr><td><b>curl</b></td><td>

```bash
curl -fsSL https://mhostai.com/install.sh | sh
```

</td></tr>
<tr><td><b>PowerShell</b></td><td>

```powershell
irm https://mhostai.com/install.ps1 | iex
```

</td></tr>
<tr><td><b>Scoop</b></td><td>

```powershell
scoop install mhost
```

</td></tr>
<tr><td><b>From source</b></td><td>

```bash
git clone https://github.com/maqalaqil/mhost && cd mhost && cargo build --release
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

## Cloud Fleet Management

Manage processes across any number of remote servers from your local terminal via SSH.

### Add Servers

```bash
mhost cloud add prod-api --host 54.123.45.67 --key ~/.ssh/id_rsa
mhost cloud add staging --host staging.myapp.com --user deploy
```

### Auto-Import from Cloud Providers

```bash
mhost cloud import aws --region us-east-1 --tag env=production
mhost cloud import digitalocean --tag production
mhost cloud import azure --resource-group myapp
mhost cloud import railway --project myapp
```

### Remote Operations

```bash
mhost cloud list                          # Show all servers
mhost cloud status                        # Health check entire fleet
mhost cloud deploy prod-api mhost.toml    # Deploy config to remote
mhost cloud exec prod-api "mhost list"    # Run command on remote
mhost cloud logs prod-api api-server      # Stream remote logs
mhost cloud restart all api-server        # Restart across ALL servers
mhost cloud scale prod-api worker 4       # Scale on specific server
mhost cloud sync mhost.toml              # Push config to all servers
mhost cloud ssh prod-api                  # Open SSH shell
mhost cloud install prod-api              # Install mhost on remote
mhost cloud update all                    # Update mhost on all servers
```

### AI-Powered Cloud Operations

```bash
mhost cloud ai-setup "I need a Node API on EC2 with 2 workers"
mhost cloud ai-diagnose prod-api          # AI analyzes remote server
mhost cloud ai-migrate staging prod-api   # AI plans migration
```

### Fleet Config (`~/.mhost/fleet.json`)

```json
{
  "servers": {
    "prod-api": {
      "host": "54.123.45.67",
      "port": 22,
      "user": "deploy",
      "auth": "key",
      "key_path": "~/.ssh/id_rsa",
      "tags": ["production", "api"],
      "provider": "aws",
      "region": "us-east-1"
    }
  },
  "groups": {
    "production": ["prod-api", "prod-worker"]
  }
}
```

Supports: **AWS EC2**, **Azure VMs**, **DigitalOcean**, **Railway**, any SSH-accessible server.

---

## Chat Bot Control

Control your processes from anywhere using a Telegram or Discord bot. Send `/status`, `/start`, `/stop`, and more — directly from your phone.

### Setup

```bash
mhost bot setup
```

```
  mhost Bot Setup

  Select platform:
    1) Telegram
    2) Discord

  Platform (1-2): 1
  Bot token: 123456:ABC-DEF...
  Your user/chat ID (admin): 987654321

  ✓ Bot configured for telegram
  Config: ~/.mhost/bot.json
  Start:  mhost bot enable
```

### Chat Commands

| Command | Permission | Description |
|---|---|---|
| `/status` | Viewer+ | List all processes with status |
| `/start <app>` | Operator+ | Start a process |
| `/stop <app>` | Operator+ | Stop a process |
| `/restart <app>` | Operator+ | Restart a process |
| `/logs <app>` | Viewer+ | Show recent log lines |
| `/health <app>` | Viewer+ | Show health check status |
| `/scale <app> <N>` | Operator+ | Scale to N instances |
| `/deploy <env>` | Operator+ | Trigger a deploy |
| `/ai <question>` | Admin | Ask the AI assistant |
| `/help` | All | Show available commands |

### Permission System

Three roles control who can do what:

| Role | Commands | Assigned by |
|---|---|---|
| **Admin** | All commands including AI and permission management | First user during setup |
| **Operator** | start, stop, restart, scale, logs, health, deploy | Admin |
| **Viewer** | status, logs, health, help (read-only) | Admin |

Users not in any list are denied all commands. Users can be explicitly blocked.

```bash
mhost bot add-admin  987654321
mhost bot add-operator 111222333
mhost bot add-viewer  444555666
mhost bot remove-user 444555666
```

### CLI Commands

| Command | Description |
|---|---|
| `mhost bot setup` | Interactive setup wizard (Telegram/Discord) |
| `mhost bot enable` | Start the bot (long-polling) |
| `mhost bot disable` | Stop the bot |
| `mhost bot status` | Show platform, enabled state, user lists |
| `mhost bot permissions` | Show role → user mapping |
| `mhost bot add-admin <id>` | Grant admin access |
| `mhost bot add-operator <id>` | Grant operator access |
| `mhost bot add-viewer <id>` | Grant viewer access |
| `mhost bot remove-user <id>` | Remove user from all roles |
| `mhost bot logs` | Show recent bot audit log (last 20 entries) |

### Security Features

- **Role-based access control** — three tiers with explicit deny lists
- **Confirmation prompts** — destructive commands (stop, restart) require `/confirm` before executing
- **Rate limiting** — configurable commands-per-minute cap per user (default: 30/min)
- **Audit logging** — every command is logged to `~/.mhost/bot-audit.jsonl` with user ID, timestamp, and result
- **Auto-alerts** — daemon crash/recover events forwarded to admin users automatically

### Bot Config (`~/.mhost/bot.json`)

```json
{
  "enabled": true,
  "platform": "telegram",
  "token": "123456:ABC-DEF...",
  "permissions": {
    "admins": [987654321],
    "operators": [111222333],
    "viewers": [444555666],
    "blocked": []
  },
  "confirm_destructive": true,
  "auto_alerts": true,
  "rate_limit": 30
}
```

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

## Autonomous Agent

An AI agent that continuously monitors your processes, diagnoses issues, and takes action — communicating through Telegram.

### Setup

```bash
mhost agent setup     # Configure LLM, Telegram, autonomy level
mhost agent start     # Start the agent
mhost agent stop      # Stop
mhost agent status    # Show config
```

### How It Works

The agent runs as a managed mhost process. Every 30 seconds it:

1. **Observes** — checks all process statuses and logs
2. **Thinks** — sends observations to the LLM (GPT-4o/Claude) with tool definitions
3. **Acts** — executes mhost commands (restart, scale, etc.) via tool calling
4. **Reports** — sends findings and actions to your Telegram

### Conversation

Chat naturally with the agent via Telegram:

```
You: "my api keeps crashing, can you check?"
Agent: "Checked api-server logs — EADDRINUSE on port 3000.
        Previous instance didn't release the port.
        I've restarted it. Now online (PID 45123)."

You: "scale workers to 4"
Agent: "Done. Workers scaled to 4 instances. All online."

You: "what's using the most memory?"
Agent: "api-server: 128MB. Workers: 64MB each. Total: 320MB."
```

### Autonomy Levels

| Level | Behavior |
|---|---|
| `autonomous` | Acts on its own, notifies you after |
| `supervised` | Proposes actions, waits for your approval |
| `manual` | Only acts when you send a message |

### Safety

- Blocked actions configurable (delete, kill disabled by default)
- Rate limit: 20 actions/hour
- Conversation history capped
- Full audit trail

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

### Agent

| Command | Description |
|---|---|
| `mhost agent setup` | Configure LLM provider, Telegram, autonomy |
| `mhost agent start` | Start the autonomous agent |
| `mhost agent stop` | Stop the agent |
| `mhost agent status` | Show agent config |

### Cloud Fleet

| Command | Description |
|---|---|
| `mhost cloud add <name> --host <ip>` | Add a remote server |
| `mhost cloud remove <name>` | Remove a server |
| `mhost cloud list` | List all fleet servers |
| `mhost cloud status` | Health check all servers |
| `mhost cloud deploy <server> <config>` | Deploy config to remote |
| `mhost cloud exec <server> "<cmd>"` | Run command on remote |
| `mhost cloud logs <server> <app>` | Stream remote logs |
| `mhost cloud restart <server\|all> <app>` | Restart across servers |
| `mhost cloud scale <server> <app> <N>` | Scale on remote |
| `mhost cloud sync <config>` | Push config to all servers |
| `mhost cloud ssh <server>` | Open SSH shell |
| `mhost cloud install <server>` | Install mhost on remote |
| `mhost cloud update <server\|all>` | Update mhost on remotes |
| `mhost cloud import <provider>` | Import from AWS/Azure/DO/Railway |
| `mhost cloud ai-setup "<desc>"` | AI infrastructure planning |
| `mhost cloud ai-diagnose <server>` | AI remote diagnosis |
| `mhost cloud ai-migrate <from> <to>` | AI migration planning |

### Bot

| Command | Description |
|---|---|
| `mhost bot setup` | Interactive setup wizard (Telegram/Discord) |
| `mhost bot enable` | Start the bot (long-polling) |
| `mhost bot disable` | Stop the bot |
| `mhost bot status` | Show platform, enabled state, user lists |
| `mhost bot permissions` | Show role → user mapping |
| `mhost bot add-admin <id>` | Grant admin access |
| `mhost bot add-operator <id>` | Grant operator access |
| `mhost bot add-viewer <id>` | Grant viewer access |
| `mhost bot remove-user <id>` | Remove user from all roles |
| `mhost bot logs` | Show recent bot audit log |

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
│  AI Intelligence        Cloud Fleet        Autonomous Agent   │
│  - OpenAI / Claude      - SSH fleet mgmt   - AWS / Azure      │
│  - Crash diagnosis      - Auto-import      - DO / Railway     │
│  - Config generation    - AI provisioning  - Remote deploy    │
│  - Optimization         - AI migration     - Fleet sync       │
│  Autonomous Agent                                             │
│  - Observe/Think/Act    - Telegram I/O     - Tool calling     │
│  - 3 autonomy levels    - Rate limiting    - Audit trail      │
└──────────────────────────────────────────────────────────────┘
```

### Crate Structure (15 crates)

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
├── mhost-cloud      Remote fleet management — SSH, cloud providers, AI ops
├── mhost-bot        Telegram/Discord bot — role-based control, audit logging
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

MIT - [Maher Al-Aqil](https://github.com/maqalaqil)
