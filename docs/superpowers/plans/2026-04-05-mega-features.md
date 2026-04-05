# mhost Mega Feature Plan — 16 Features

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development

**Goal:** Add 16 new features to mhost covering Docker, plugins, templates, cron dashboard, multi-tenancy, resource limits, log alerts, process tags, rollback, hot config reload, auto-detection, audit trail, webhooks v2, desktop app, VS Code extension, GitHub Action, and public status page.

**Architecture:** Each feature is a self-contained module added to existing crates. No new crates unless absolutely needed. All features follow existing patterns (CLI commands, daemon handlers, dashboard tabs, agent tools).

---

## Phase A: Core Infrastructure (6 features)

### A1. Docker Integration
- New module `crates/mhost-cli/src/commands/docker.rs`
- `mhost docker run nginx:latest --name web --port 8080` — pull + run container
- `mhost docker list` — list mhost-managed containers
- `mhost docker stop/restart/logs/rm <name>`
- Uses Docker Engine API via Unix socket (`/var/run/docker.sock`) with reqwest
- Containers tracked in mhost's process list alongside native processes
- Dashboard: Docker tab showing containers
- Agent tools: docker_run, docker_list, docker_stop, docker_logs

### A2. Plugin System
- New module `crates/mhost-cli/src/commands/plugin.rs`
- Plugin directory: `~/.mhost/plugins/`
- `mhost plugin install <name>` — download from registry or local path
- `mhost plugin list` — show installed plugins
- `mhost plugin remove <name>`
- Plugins are JS scripts with lifecycle hooks: `onStart`, `onStop`, `onCrash`, `onRestart`, `onHealthFail`
- Plugin manifest: `plugin.json` with name, version, hooks, description
- Daemon loads plugins and calls hooks at appropriate lifecycle points

### A3. Process Tags
- Add `tags: Vec<String>` to ProcessConfig
- `mhost start server.js --name api --tag env=prod --tag team=backend`
- `mhost list --tag env=prod` — filter by tag
- `mhost stop --tag team=backend` — bulk operations by tag
- `mhost restart --tag env=staging`
- Tags stored in process config, persisted in state store

### A4. Audit Trail
- New file `crates/mhost-daemon/src/audit.rs`
- Every action logged: start, stop, restart, delete, scale, deploy, config change
- Stored in `~/.mhost/audit.jsonl` — append-only log
- Fields: timestamp, action, process, user, source (cli/api/bot/agent), details
- `mhost audit` — show recent audit entries
- `mhost audit --process api` — filter by process
- `mhost audit --since 24h` — time filter
- Dashboard: Audit tab in System
- Agent tool: get_audit

### A5. Hot Config Reload
- File watcher on `mhost.toml` (or whatever config file was used to start)
- On change: diff old vs new config, apply changes without full restart
- New processes added, removed processes stopped, changed processes restarted
- `mhost watch mhost.toml` — explicit watch mode
- Notification on config change applied

### A6. Rollback per Process
- `mhost rollback <process>` — revert to previous config version
- Config history stored in SQLite: each start/deploy saves a versioned config
- `mhost history <process> --configs` — show config versions
- `mhost rollback <process> --version 3` — rollback to specific version

## Phase B: Developer Experience (4 features)

### B1. Process Templates
- `mhost template list` — show available templates
- `mhost template init nextjs` — generate mhost.toml for Next.js
- `mhost template init django` — Django template
- Templates: nextjs, react, vue, express, fastapi, django, rails, go, rust, python-worker, static-site
- Each template includes: command, health checks, env vars, memory limits, groups
- Stored as embedded TOML strings in the binary

### B2. Dependency Auto-Detection
- `mhost init` — scan current directory and auto-generate mhost.toml
- Detects: package.json (Node), requirements.txt/pyproject.toml (Python), Cargo.toml (Rust), go.mod (Go), Gemfile (Ruby), composer.json (PHP)
- Reads start scripts from package.json, Procfile, etc.
- Suggests health checks, memory limits, instance count
- Interactive confirmation before writing

### B3. Resource Limits (Linux cgroups)
- `mhost start --cpu-limit 50% --memory-limit 512MB`
- On Linux: creates cgroup for the process with enforced limits
- On macOS: advisory only (no cgroup support, uses monitoring + restart on breach)
- ProcessConfig gains: `cpu_limit`, `memory_limit_mb`
- `mhost info <process>` shows resource limit status

### B4. Log Alerts
- `mhost logs --alert "error|exception" --notify telegram`
- Pattern matching on log lines in real-time
- Configurable in mhost.toml:
  ```toml
  [process.api.log_alerts]
  patterns = ["error", "FATAL", "OOM"]
  notify = ["telegram"]
  cooldown = "60s"
  ```
- Uses existing notification channels
- Brain learns which log patterns precede crashes

## Phase C: Monitoring & Ops (3 features)

### C1. Cron Dashboard
- `mhost cron` — show all cron-scheduled processes with next run times
- Visual timeline in TUI showing when each cron fires
- Dashboard: Cron tab with schedule visualization
- Next run calculation from cron expressions

### C2. Multi-Tenancy / Workspaces
- `mhost workspace create myproject`
- `mhost workspace switch myproject`
- `mhost workspace list`
- Each workspace: separate process list, configs, logs, state
- Stored in `~/.mhost/workspaces/<name>/`
- Default workspace: "default" (current behavior)
- Env var override: `MHOST_WORKSPACE=myproject`

### C3. Public Status Page
- `mhost status-page start --port 8080`
- Generates a public-facing HTML page showing process uptime
- Green/yellow/red indicators per process
- Uptime percentage over last 24h, 7d, 30d
- Incident history from brain data
- Custom branding: title, logo URL, description
- Config in mhost.toml: `[status_page]`

## Phase D: External Tools (3 features)

### D1. Incoming Webhooks V2
- `mhost webhooks create --action restart --process api`
- Generates unique URL: `http://localhost:19516/hooks/<token>`
- POST to the URL triggers the action
- Supports: restart, stop, start, scale, deploy
- HMAC signature verification
- Rate limited per webhook
- Dashboard: manage incoming webhooks in System tab

### D2. VS Code Extension
- `website/vscode-extension/` directory
- Extension shows process list in sidebar (TreeView)
- Start/stop/restart/scale from VS Code
- Log viewer panel
- Status bar showing online/offline count
- Uses mhost CLI or API

### D3. GitHub Action
- `website/github-action/` directory
- `action.yml` for marketplace
- Inputs: command, config-file, server (for remote deploy)
- Usage: `uses: maqalaqil/mhost-action@v1` with `command: deploy production`
- Installs mhost binary, runs the command
