# mhost Feature Brainstorm — What Else?

> Raw idea dump. Not a plan. Ideas here are novel relative to what's already shipped or already scoped in `2026-04-05-mega-features.md`, `2026-04-04-cloud-native-design.md`, and the cloud-platform specs. Prune ruthlessly before any of these become plans.

Today: 2026-04-19. Branch: `claude/brainstorm-features-3QfGl`.

---

## 1. Observability & Debugging

### 1.1 OpenTelemetry-native export
- First-class OTLP exporter for metrics, logs, and traces — not just Prometheus + Loki shims.
- Auto-inject `OTEL_SERVICE_NAME` and `OTEL_RESOURCE_ATTRIBUTES` env vars from process config so traces correlate without code changes.
- `mhost trace <process>` — tail spans live alongside logs.

### 1.2 eBPF-based process visibility (Linux)
- Attach uprobes/USDT and kprobes to managed processes for syscall counts, file/network activity, and on-CPU flamegraphs, without code changes.
- `mhost flamegraph <process> --duration 30s` — capture and render SVG.
- Gated behind capability check; graceful fallback on non-Linux.

### 1.3 Log anomaly detection
- Local clustering (log templates → Drain algorithm) to learn "normal" log patterns per process.
- Alert when rare templates appear or frequency spikes. Complements `log-alert` (which is rule-based today).
- Zero LLM cost; pure statistics. AI used only for human-readable summaries.

### 1.4 Distributed tracing correlation in `logs`
- When logs contain W3C `traceparent` or OTel `trace_id`, `mhost logs --trace <id>` pivots across all processes.
- Dashboard: click a log line → see all logs sharing the same trace.

### 1.5 Time-travel debugging
- Beyond `snapshot`/`replay`: record syscalls + env + logs into a replay bundle, then `mhost replay --step` single-steps config + env state at crash time.
- Pairs with `postmortem` but deterministic, not LLM-narrated.

---

## 2. Safety, Security, Supply Chain

### 2.1 Process sandboxing profiles
- Generate seccomp (Linux), Landlock (Linux 5.13+), or AppArmor profiles from observed syscalls.
- `mhost sandbox learn <process> --duration 1h` → writes a profile, then `mhost sandbox enforce <process>`.
- macOS: sandbox-exec; Windows: AppContainer (stretch).

### 2.2 Process DNA / drift detection
- On `start`, hash binary + shared libs + interpreter version. Store fingerprint.
- On restart, compare; alert on drift ("node upgraded from 20.11 → 20.12 — this may be why it's crashing").
- Zero-config defense against accidental environment changes.

### 2.3 SBOM + CVE scanning for managed processes
- Auto-generate CycloneDX SBOM from `package-lock.json`, `Cargo.lock`, `requirements.txt`, `go.sum` of each process's workdir.
- Scan against local OSV database; alert on new CVEs affecting a running process without restarting.
- `mhost certs` already monitors TLS — this is the software-supply-chain analogue.

### 2.4 Secrets auto-rotation hooks
- Integrations: Vault, AWS Secrets Manager, GCP Secret Manager, Doppler, Infisical.
- Config: `rotate_secrets = ["DATABASE_URL", "API_KEY"]` with a rotation policy. mhost pulls new values, performs zero-downtime reload.
- Complements existing `secrets set/list/remove`.

### 2.5 Audit log streaming to SIEM
- Stream the existing audit trail (Phase A4) to Splunk HEC, Datadog, Elasticsearch, or generic syslog-over-TLS.
- Required for enterprise/regulated deployments.

---

## 3. Scheduling & Resource Management

### 3.1 Fair-share / quota scheduler
- Per-team or per-tag CPU/memory/IO quotas enforced via cgroups v2 (Linux) or job objects (Windows).
- When a team exceeds quota, their newest processes throttle first.
- Complements `limits <process>` (per-process) with cross-process fairness.

### 3.2 Predictive autoscaling
- Train a simple model (Holt-Winters or Prophet-lite in Rust) on historical CPU/request metrics.
- `mhost scale api --predictive` — pre-scale before the daily 9am spike instead of reacting to it.
- Transparent: `mhost forecast api` shows the predicted curve and chosen instance count.

### 3.3 GPU / accelerator awareness
- Track NVIDIA (NVML), AMD (ROCm SMI), Apple Metal, and Intel GPUs.
- `mhost list --gpu` shows per-process VRAM/SM utilization.
- Health probes: "VRAM > 90% for 5min → restart".
- Huge for ML/LLM inference workloads — an underserved segment for PM2-style tools.

### 3.4 Chaos engineering mode
- `mhost chaos inject <process> --cpu 80% --duration 2m`
- `mhost chaos inject <process> --network-latency 200ms`
- `mhost chaos inject <process> --kill-random --probability 0.01`
- Scheduled chaos via cron: weekly game-day automation.
- Ties to `brain`: self-healing gets tested continuously.

### 3.5 Disaster recovery drills
- `mhost dr-drill` — scheduled rehearsal that restores from a snapshot on a secondary host, runs smoke tests, tears down.
- Generates a compliance-ready report. Uses existing `snapshot` + `bench`.

---

## 4. Declarative & GitOps

### 4.1 GitOps reconciliation
- Point mhost at a Git repo containing `mhost.toml` files; it reconciles every N seconds.
- Drift alerts when an operator manually changes state that contradicts Git.
- This is Flux/ArgoCD patterns applied to the PM2 layer.

### 4.2 Kubernetes operator
- CRDs: `Process`, `ProcessGroup`, `ProcessPolicy`.
- Lets teams who standardize on K8s manage long-running non-container workloads (edge devices, VMs, bare metal) through the same control plane.
- mhost-operator runs in-cluster; mhostd runs on each node.

### 4.3 Natural-language policies
- `mhost policy add "restart api if memory > 80% for 5 minutes and notify #alerts"`
- LLM compiles NL → structured policy YAML → committed to Git.
- User reviews the YAML before activation (no runtime LLM in the control loop).

---

## 5. Developer Experience

### 5.1 AI chat mode — conversational control
- `mhost chat` opens a REPL where "restart the api", "why did worker-3 crash yesterday", "scale to 4 instances during business hours" become structured commands after confirmation.
- Uses existing `ai ask` + `agent` primitives, but as a persistent session.

### 5.2 Simulation mode
- `mhost simulate --config new-mhost.toml --replay last-7-days` — runs a new config against recorded load/crash history and reports: "this config would have caused 2 cascading failures".
- Makes config changes far less scary in production.

### 5.3 Collaborative live sessions
- tmate-style ephemeral URL: `mhost share-session` gives a teammate read-only log tail + command-proposal UI (commands require owner approval).
- Better than screen-sharing for on-call handoffs.

### 5.4 WASM plugin runtime
- Current plugin system is JS. Add WASM (Component Model) as a safer, language-agnostic alternative.
- Plugins get capability-gated host imports (read-logs, post-notify, etc.) — no arbitrary filesystem/network by default.

### 5.5 Process lineage graph
- Track parent/child spawns (including short-lived workers).
- `mhost lineage api` renders a tree; useful for diagnosing orphans and fork-bombs.
- `link` today shows config dependencies; this adds runtime lineage.

---

## 6. Edge, Mobile, and Platform Reach

### 6.1 Edge / serverless adapters
- `mhost deploy --target cloudflare-workers|deno-deploy|fly-machines|modal` — translate a mhost process into the target platform's primitives.
- Unified control plane across long-running servers and ephemeral edge functions.

### 6.2 Mobile companion app
- iOS/Android app that subscribes to the existing cloud relay WebSocket.
- On-call engineer gets push notifications, one-tap restart/rollback, and a live dashboard on the phone.
- Reuses the existing `cloud-auth.json` device code flow.

### 6.3 Offline-capable PWA dashboard
- The current dashboard is a single-file HTML; make it a proper PWA with service worker so it works on flaky networks (planes, ops rooms).
- Queues commands when offline, flushes on reconnect.

### 6.4 Voice control (experimental)
- Browser SpeechRecognition in the dashboard → `mhost agent` tool call.
- Useful during incidents when hands are on a debugger, or for accessibility.

---

## 7. Cost & FinOps

### 7.1 Cost anomaly AI
- Existing `cost` gives point-in-time estimates. Add time-series anomaly detection: "fleet spend up 40% this week — driven by `worker-*` memory scale-outs".
- Weekly digest to Slack/email.

### 7.2 Carbon footprint reporting
- Map cloud region/instance type → gCO₂/hour using public intensity data.
- `mhost sustainability` shows carbon per process per day. Differentiator for ESG-conscious buyers.

### 7.3 Right-sizing recommender
- Observe actual CPU/mem p95 vs provisioned limits; recommend downsizing.
- Pairs with `cost` and cloud-native instance types.

---

## 8. Long Tail

- **Localization** — i18n for CLI messages and dashboard (at least es, pt-BR, zh, ja, de, fr).
- **Accessibility pass** on the dashboard (keyboard nav, ARIA, reduced-motion).
- **Terminal screen-reader mode** for the TUI monit.
- **`mhost doctor`** — single command that validates the whole environment (socket, permissions, disk space, cert expiry, orphan PIDs, DB integrity).
- **Process health SLOs**, not just SLAs — error-budget burn-rate alerts.
- **Runbook-as-code** — `.mhost/runbooks/*.md` loaded by `brain` for incident response templates.
- **Multi-region active/active** — two mhostd instances gossip state; either can take over.
- **Immutable deploys** — content-addressed artifact storage keyed by Git SHA, nothing mutable on the host.

---

## Prioritization heuristic (for later)

When turning these into real plans, score each on:

1. **Underserved?** — Does PM2/supervisord/systemd/nomad already do this well? If yes, skip.
2. **AI-leverage?** — Does mhost's LLM layer make this 10× better than existing tools?
3. **Single-binary friendly?** — Can it ship without new runtime deps?
4. **Cross-platform?** — Linux-only features are fine but should degrade gracefully on macOS/Windows.

Top candidates by that scoring: **GPU awareness (3.3)**, **log anomaly detection (1.3)**, **sandboxing profiles (2.1)**, **simulation mode (5.2)**, **predictive autoscaling (3.2)**, **process DNA (2.2)**, **natural-language policies (4.3)**.
