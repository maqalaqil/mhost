# Changelog

All notable changes to mhost will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-03-29

### Added

#### Core Process Management
- Start, stop, restart, delete, list, scale, cluster processes
- Ecosystem config files (TOML, YAML, JSON) with environment variable expansion
- Auto-restart with exponential backoff (100ms-30s) and circuit breaker
- Graceful shutdown (SIGTERM -> grace period -> SIGKILL)
- Process groups with dependency ordering (topological sort)
- Cron-scheduled restarts
- Memory limit monitoring and enforcement
- Save/resurrect for process persistence across reboots
- Startup scripts (launchd on macOS, systemd on Linux)

#### Health Probes
- HTTP health checks with status code validation
- TCP port health checks
- Script-based health checks (exit code)
- Configurable interval, timeout, and retry count
- Process only transitions to "online" after health check passes

#### Log Engine
- Log capture with file rotation (10MB default, 5 files)
- In-memory ring buffer (1000 lines) for live streaming
- JSON auto-detection and field indexing
- SQLite FTS5 full-text search
- Structured query parser (`level=error AND status>=500`)
- Time-range and aggregation queries
- Retention policies (7d info, 30d error, 90d fatal)
- External sinks: Graylog (GELF), Loki, Elasticsearch, Syslog (RFC 5424)

#### Notifications
- 8 notification channels: Telegram, Slack, Discord, Webhook, Email (SMTP), PagerDuty, Microsoft Teams, Ntfy
- Interactive CLI setup wizard (`mhost notify setup`)
- Per-channel event filtering (crash, restart, errored, health_fail, 5xx, OOM, deploy)
- Throttling per channel (configurable window)
- Escalation chains with auto-escalate timer
- Auto-resolve notifications on recovery
- HMAC-SHA256 webhook signing

#### Metrics & Observability
- Per-process CPU and memory polling (sysinfo)
- Time-series storage in SQLite
- Prometheus `/metrics` endpoint (axum)
- Alert condition parser (`memory > 450MB for 5m`)
- Auto-remediation engine with cooldown

#### Reverse Proxy
- HTTP/HTTPS reverse proxy with Host header routing
- Load balancing: round-robin, least-connections, IP-hash
- WebSocket upgrade passthrough
- Self-signed TLS with rcgen/rustls
- ACME (Let's Encrypt) auto-certificate provisioning
- Cookie-based sticky sessions

#### Deploy Engine
- Git clone/pull via git2
- Pre/post deploy hook execution with timeout
- Deploy history tracked in SQLite
- One-command rollback to previous successful deploy

#### AI Intelligence
- OpenAI and Claude (Anthropic) provider support
- `mhost ai diagnose` — crash root cause analysis
- `mhost ai logs` — natural language log queries
- `mhost ai optimize` — performance recommendations
- `mhost ai config` — generate mhost.toml from description
- `mhost ai postmortem` — incident report generation
- `mhost ai watch` — anomaly detection
- `mhost ai ask` — general Q&A about processes
- `mhost ai explain` — config explanation in plain English
- `mhost ai suggest` — proactive improvement suggestions

#### Cloud Fleet Management
- SSH-based remote server management
- Auto-import from AWS EC2, Azure VMs, DigitalOcean, Railway
- Remote deploy, logs, restart, scale operations
- Fleet-wide sync and status
- AI-powered infrastructure provisioning and migration planning

#### Chat Bot
- Telegram and Discord bot support
- Role-based permissions (admin, operator, viewer)
- Destructive action confirmation (30s window)
- Rate limiting (30 commands/min)
- Full audit log (JSONL)

#### TUI Dashboard
- Split-pane terminal UI with ratatui
- Process table with color-coded status dots
- CPU and memory sparkline graphs (60 data points)
- Live log tail for selected process
- Vim-style keyboard navigation (j/k/g/G/Tab)
- Process actions (restart/stop/delete with confirmation)
- Search and sort by column

#### Distribution
- Single binary (14MB), zero runtime dependencies
- Homebrew, npm, Cargo, curl, PowerShell, Scoop installation
- Docker image (Alpine-based)
- GitHub Actions CI/CD: auto-release on push to main
- Cross-platform: macOS (x64/ARM), Linux (x64/ARM), Windows (x64/ARM)
- Shell completions (bash, zsh, fish, powershell)
- Self-update command

### Technical
- 15 Rust crates in workspace
- 774 unit tests
- SQLite for state persistence
- JSON-RPC 2.0 over Unix socket / named pipe
- Async runtime: tokio

[Unreleased]: https://github.com/maheralaqil/mhost/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/maheralaqil/mhost/releases/tag/v0.1.0
