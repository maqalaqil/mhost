# CLAUDE.md — Project Context for AI Assistants

This file gives any Claude session (or other AI) full context to understand and work on the mhost project.

## What is mhost?

mhost is an **AI-powered process manager** written in Rust — a PM2 replacement with built-in health probes, notifications, metrics, reverse proxy, deploy engine, AI diagnostics, cloud fleet management, chat bot control, autonomous agent, and self-healing brain.

**Single binary. Zero runtime dependencies. Cross-platform (macOS, Linux, Windows).**

## Quick Reference

| Stat | Value |
|---|---|
| Language | Rust |
| Crates | 15 |
| Tests | 797 |
| CLI Commands | 52+ (with subcommands: 95+) |
| Source files | 160 .rs files |
| Website | `website/index.html` (single-file landing page) |
| Repo | `github.com/maqalaqil/mhost` |
| Domain | mhostai.com |

## Architecture

```
mhost (CLI binary)  ◄──── JSON-RPC over Unix socket ────►  mhostd (daemon binary)
     │                                                           │
     ├── commands/start.rs    (auto-detect interpreter)          ├── supervisor.rs   (spawn, kill, restart)
     ├── commands/logs.rs     (colored log viewer)               ├── handler.rs      (RPC dispatch)
     ├── commands/dev.rs      (file watcher, auto-restart)       ├── state.rs        (SQLite persistence)
     ├── commands/dashboard.rs (web UI launcher)                 ├── watcher.rs      (exit watcher, backoff)
     ├── commands/ai.rs       (LLM commands)                     ├── cron_scheduler.rs
     ├── commands/agent.rs    (autonomous agent)                 ├── memory_monitor.rs
     ├── commands/brain.rs    (self-healing)                     └── remote.rs       (mTLS stub)
     ├── commands/notify.rs   (notification setup)
     ├── commands/bot.rs      (Telegram/Discord bot)
     ├── commands/cloud.rs    (remote fleet SSH)
     ├── commands/reload.rs   (zero-downtime reload)
     ├── commands/bench.rs    (HTTP load testing)
     ├── commands/canary.rs   (canary deployments)
     ├── commands/snapshot.rs  (state snapshots)
     ├── commands/replay.rs   (incident replay)
     ├── commands/link.rs     (dependency graph)
     ├── commands/cost.rs     (cloud cost estimation)
     ├── commands/certs.rs    (SSL cert monitoring)
     ├── commands/sla.rs      (uptime reports)
     ├── commands/diff.rs     (environment comparison)
     ├── commands/share.rs    (tunnel exposure)
     ├── commands/recipe.rs   (command recipes)
     ├── commands/migrate.rs  (PM2 migration)
     └── output.rs            (table formatting)
```

## Crate Dependency Graph

```
mhost-core          ← shared types: ProcessConfig, ProcessInfo, ProcessStatus, RPC protocol
  ↑
mhost-config        ← TOML/YAML/JSON parsing, env var expansion
  ↑
mhost-ipc           ← JSON-RPC over Unix socket (client + server)
  ↑
mhost-logs          ← log capture, FTS5 search, rotation, sinks (GELF/Loki/ES/Syslog)
mhost-health        ← HTTP/TCP/script health probes
mhost-notify        ← 8 notification channels + throttle + escalation
mhost-metrics       ← CPU/memory polling, Prometheus, alerts, auto-remediation
mhost-ai            ← OpenAI/Claude provider, diagnose, optimize, config gen, etc.
mhost-proxy         ← reverse proxy, TLS, ACME, load balancing
mhost-deploy        ← git deploy, hooks, rollback, history
mhost-cloud         ← SSH fleet management, cloud provider import
mhost-bot           ← Telegram/Discord bot, permissions, audit
mhost-tui           ← ratatui terminal dashboard
  ↑
mhost-daemon        ← supervisor, handler, state store (mhostd binary)
  ↑
mhost-cli           ← CLI interface, all commands (mhost binary)
```

## Key Design Decisions

1. **Immutability** — `ProcessInfo::transition_to()` returns a new struct, never mutates
2. **IPC via JSON-RPC 2.0** — CLI talks to daemon over Unix socket (Windows: TCP fallback)
3. **Handler wraps responses** — daemon returns `{"processes": [...]}` wrapper. CLI must extract.
4. **CWD resolution** — `mhost start server.js` resolves script path from caller's CWD, not daemon's
5. **Log format** — daemon writes `TIMESTAMP [process-name] content` to `~/.mhost/logs/`
6. **Orphan cleanup** — when child.stdout is taken for log capture, `stop_process` kills by PID directly via `nix::sys::signal::kill`
7. **Auto-detect interpreter** — `.js` → node, `.py` → python3, `.ts` → npx tsx, `.sh` → sh
8. **Agent scripts** — copied to `~/.mhost/agent-scripts/` on first run so they work from any directory

## File Locations

| Path | Purpose |
|---|---|
| `~/.mhost/mhostd.sock` | Daemon IPC socket |
| `~/.mhost/mhostd.pid` | Daemon PID file |
| `~/.mhost/mhost.db` | SQLite state store |
| `~/.mhost/logs/` | Process stdout/stderr log files |
| `~/.mhost/pids/` | Per-process PID files |
| `~/.mhost/notify.json` | Notification channel config |
| `~/.mhost/ai.json` | AI provider config |
| `~/.mhost/bot.json` | Telegram/Discord bot config |
| `~/.mhost/agent.json` | Autonomous agent config |
| `~/.mhost/fleet.json` | Cloud fleet server config |
| `~/.mhost/brain/` | Brain memory (incidents, playbooks, health, trends) |
| `~/.mhost/agent-scripts/` | Cached agent + brain JS files |
| `~/.mhost/dashboard/` | Cached dashboard JS file |

## Common Patterns

### Adding a new CLI command

1. Create `crates/mhost-cli/src/commands/mycommand.rs`
2. Add `pub mod mycommand;` to `commands/mod.rs`
3. Add variant to `Commands` enum in `cli.rs`
4. Wire dispatch in `main.rs` (daemon vs non-daemon path)

### Adding an RPC method

1. Add `pub const MY_METHOD: &str = "my.method";` to `crates/mhost-core/src/protocol.rs`
2. Add match arm in `crates/mhost-daemon/src/handler.rs`
3. Call from CLI via `client.call(methods::MY_METHOD, params).await`

### Process list response format

The daemon wraps process lists: `{"processes": [...]}`. CLI must extract:
```rust
let result = resp.result.unwrap_or_default();
let list = if let Some(arr) = result.get("processes") { arr.clone() } else { result };
let processes: Vec<ProcessInfo> = serde_json::from_value(list)?;
```

### ANSI-safe table formatting

Use `format!("{:<WIDTH$}", raw_text)` BEFORE applying `.green()` / `.red()` color. Color after padding, never pad colored strings.

## Build & Test

```bash
cargo build --workspace                    # Build all
cargo build --release -p mhost-cli -p mhost-daemon  # Release binaries
cargo test --workspace                     # Run all 793 tests
cargo clippy --workspace -- -D warnings    # Lint
cargo fmt --all --check                    # Format check
```

## CI/CD

- Push to `main` → auto version bump → build 5 platforms → GitHub Release → npm + crates.io + Homebrew + Docker + GitHub Pages
- PR → lint + test on 3 platforms

## Known Issues / Gotchas

1. **`cargo clippy` strict** — CI runs with `-D warnings`. Format strings must use inline syntax (`format!("{var}")` not `format!("{}", var)`)
2. **macOS runners** — `macos-13` deprecated, use `macos-15` for both x86 and ARM builds
3. **OpenSSL on musl** — uses vendored OpenSSL (`features = ["vendored"]`) for Linux static builds
4. **Windows IPC** — no Unix sockets, falls back to TCP `127.0.0.1:19515`
5. **Platform-gated code** — `startup.rs` imports gated with `#[cfg(target_os = "macos")]` etc.
6. **Test environment** — some tests assume no local `~/.mhost/` state. `agent_status_no_config` handles both cases.
7. **Node.js scripts** — `examples/mhost-agent.js`, `mhost-brain.js`, `mhost-dashboard.js`, `mhost-telegram-notifier.js` are Node.js, not Rust

## CLI Command Map

### Core
`start`, `stop`, `restart`, `delete`, `list`, `info`, `env`, `scale`, `cluster`, `health`, `config`, `history`, `reload`, `save`, `resurrect`, `startup`, `unstartup`, `ping`, `kill`

### Logs
`logs [name] [--follow] [--err] [-n N] [--grep] [--search] [--where] [--since] [--format] [--count-by]`

### Dev
`dev <script> [--watch dir] [--ext js,ts] [--env .env.local]`

### Monitoring
`monit` (TUI), `dashboard [--port]` (web UI), `metrics show|history|start`

### Notifications
`notify setup|list|test|enable|disable|remove|events|start`

### AI
`ai setup|diagnose|logs|optimize|config|postmortem|watch|ask|explain|suggest`

### Agent & Brain
`agent setup|start|stop|status`
`brain status|history|playbooks|explain`

### Bot
`bot setup|enable|disable|status|permissions|add-admin|add-operator|add-viewer|remove-user|logs`

### Cloud
`cloud add|remove|list|status|deploy|exec|logs|restart|scale|sync|ssh|install|update|import|ai-setup|ai-diagnose|ai-migrate`

### Production & Operations
`reload <app>`, `dev <script>`, `dashboard [--port]`, `bench <url>`, `canary <app>`, `snapshot create|list|restore`, `replay <process>`, `link`, `cost`, `certs [--url]`, `sla <app>`, `diff <env_a> <env_b>`, `share <app>`, `run <file>`, `migrate --from <pm>`, `team`, `playground`

### Other
`proxy`, `deploy <env>`, `rollback <env>`, `self-update`, `completion <shell>`
