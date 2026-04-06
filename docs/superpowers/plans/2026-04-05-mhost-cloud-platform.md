# mhost Cloud Platform — Full Implementation Plan

> This is a NEW private repository: `mhost-cloud`
> The open-source `mhost` CLI repo gets minimal additions (login/connect commands + daemon sync).

**Goal:** Build the full mhost Cloud managed platform — auth, dashboard, fleet map, AI cost optimizer, smart autoscaling, deployment pipelines, incident war rooms, secrets vault, billing, plugin marketplace, compliance reports.

**Architecture:** Rust backend (4 services) + SvelteKit frontend + MongoDB + Redis + ClickHouse + Cloudflare R2. Hosted on Railway. Deployed via mhost GitHub Action.

**Spec:** `docs/superpowers/specs/2026-04-05-mhost-cloud-platform-design.md`

---

## Phase 1: Foundation (Weeks 1-2)

### 1.1 Create Private Repo + Workspace

```
mhost-cloud/
├── Cargo.toml                    # Rust workspace
├── crates/
│   ├── cloud-shared/             # Shared types, DB, auth
│   ├── cloud-api/                # REST API (Axum)
│   ├── cloud-relay/              # WebSocket relay
│   ├── cloud-worker/             # Background jobs
│   └── cloud-scheduler/          # Cron tasks
├── frontend/                     # SvelteKit app
├── railway.toml                  # Railway config
├── .github/workflows/deploy.yml
└── .env.example
```

**Tasks:**
- Create GitHub private repo `maqalaqil/mhost-cloud`
- Initialize Rust workspace with 5 crates
- Initialize SvelteKit project in `frontend/`
- Set up Railway project with 5 services
- Configure Cloudflare DNS for api.mhostai.com, app.mhostai.com, ws.mhostai.com
- Set up MongoDB Atlas cluster
- Set up Redis (Upstash or Railway)
- Set up Cloudflare R2 bucket
- Create `.env.example` with all required env vars
- Create `railway.toml` with service definitions
- Create GitHub Action workflow for auto-deploy

### 1.2 cloud-shared Crate

```
crates/cloud-shared/src/
├── lib.rs
├── config.rs          # Env var config with defaults
├── db.rs              # MongoDB connection pool
├── redis.rs           # Redis connection
├── models/
│   ├── mod.rs
│   ├── user.rs        # User document type
│   ├── team.rs        # Team document type
│   ├── server.rs      # Server document type
│   └── billing.rs     # Billing/subscription type
├── auth.rs            # JWT create/validate, password hash
├── email.rs           # Resend email client
└── errors.rs          # Shared error types
```

**Tasks:**
- Config struct loading from env vars (MONGODB_URI, REDIS_URL, JWT_SECRET, STRIPE_KEY, RESEND_KEY, R2 creds)
- MongoDB client with connection pooling (mongodb crate)
- Redis client (redis crate)
- User model: email, name, password_hash (argon2), oauth_providers, plan, stripe_customer_id, settings, created_at
- Team model: name, slug, owner_id, plan, members (user_id + role), settings, usage
- Server model: team_id, name, registration_token, status, region, provider, processes, last_heartbeat, coordinates
- JWT helpers: create_access_token (15min), create_refresh_token (30d), validate_token
- Password helpers: hash_password, verify_password (argon2)
- Email helpers: send_welcome, send_invite, send_alert, send_invoice
- Shared error type with HTTP status mapping

### 1.3 Auth System (cloud-api)

```
crates/cloud-api/src/routes/auth.rs
```

**Endpoints:**
```
POST /auth/register          { email, password, name } → create user, send welcome email
POST /auth/login             { email, password } → { access_token, refresh_token }
POST /auth/refresh           { refresh_token } → { access_token }
POST /auth/logout            → invalidate refresh token
GET  /auth/oauth/github      → redirect to GitHub OAuth
GET  /auth/oauth/google      → redirect to Google OAuth
GET  /auth/oauth/callback    → handle OAuth, create/link user, return tokens
POST /auth/device/code       → generate device code for CLI login (code + device_id)
GET  /auth/device/poll       { device_id } → pending | { access_token } (CLI polls this)
POST /auth/device/approve    { code } → link device to user (browser calls this)
POST /auth/forgot-password   { email } → send reset email
POST /auth/reset-password    { token, password }
```

**Tasks:**
- Register endpoint: validate email uniqueness, hash password, create user doc, send welcome email
- Login endpoint: verify credentials, generate JWT pair
- OAuth flow: GitHub App + Google OAuth2 client, create or link user
- Device code flow (for `mhost login`): generate 8-char code, store in Redis with 10min TTL, CLI polls, browser approves
- JWT middleware: extract + validate token, inject user into request extensions
- Refresh token rotation: old refresh token invalidated on use
- Rate limiting: 5 attempts per minute per IP for login/register

### 1.4 User + Team Endpoints (cloud-api)

```
crates/cloud-api/src/routes/users.rs
crates/cloud-api/src/routes/teams.rs
```

**Tasks:**
- GET /users/me → return user profile
- PATCH /users/me → update name, settings
- POST /users/me/api-keys → generate API key (mhk_xxx), hash + store
- GET /users/me/api-keys → list (masked values)
- DELETE /users/me/api-keys/:id → revoke
- POST /teams → create team, set user as owner
- GET /teams → list user's teams
- GET /teams/:id → team detail
- POST /teams/:id/invite → generate invite code, send email
- GET /teams/:id/members → list with roles
- PATCH /teams/:id/members/:uid → change role (admin/developer/viewer)
- DELETE /teams/:id/members/:uid → remove member
- Team permission middleware: check user's role on the team for each request

---

## Phase 2: Server Connection (Weeks 2-3)

### 2.1 Server Registration (cloud-api)

```
crates/cloud-api/src/routes/servers.rs
```

**Endpoints:**
```
POST /servers/register       { name, region, os, cpu, memory, mhost_version } → { server_id, ws_token }
GET  /servers                → list team's servers
GET  /servers/:id            → server detail + processes
DELETE /servers/:id          → deregister
GET  /servers/map            → all servers with coordinates for fleet map
```

**Tasks:**
- Register: create server doc, generate ws_token (jwt with server_id claim), return to CLI
- List: filter by team_id, include process count and status
- Detail: include full process list, last metrics, health
- Map data: return array of { name, lat, lng, status, process_count, provider }
- Auto-detect region → coordinates mapping (us-east-1 → 39.04, -77.49 etc.)

### 2.2 WebSocket Relay (cloud-relay)

```
crates/cloud-relay/src/
├── main.rs
├── connection.rs      # Per-server WS handler
├── registry.rs        # Active connection map
├── commands.rs        # Forward commands to servers
├── metrics.rs         # Metric ingestion
└── logs.rs            # Log forwarding
```

**Tasks:**
- Accept WebSocket connections, authenticate via ws_token query param
- Registry: `HashMap<server_id, WebSocket sender>` behind RwLock
- Receive from server: metrics (every 10s), events (real-time), logs (streaming)
- Metrics → publish to Redis `metrics:ingest` channel (worker picks up → ClickHouse)
- Events → update server doc in MongoDB + publish to Redis `events` channel
- Logs → publish to Redis `logs:ingest` channel (worker picks up → R2)
- Forward commands: API calls `POST /relay/command` → relay finds server connection → sends command → waits for response → returns to API
- Heartbeat: ping every 30s, mark server offline after 3 missed pongs
- Auto-reconnect handling: buffer messages in Redis during brief disconnects
- Dashboard WebSocket: separate WS endpoint for browser clients to receive live updates

### 2.3 CLI Changes (mhost open-source repo)

Add to the existing `mhost` CLI repo:

```
crates/mhost-cli/src/commands/login.rs     # mhost login
crates/mhost-cli/src/commands/connect.rs   # mhost connect
crates/mhost-daemon/src/cloud_sync.rs      # Background sync to cloud
```

**Tasks:**
- `mhost login`: device code flow — POST to cloud API for code, open browser, poll for token, save to `~/.mhost/cloud-auth.json`
- `mhost logout`: delete cloud-auth.json
- `mhost connect`: POST to cloud API to register server, save server_id + ws_token, start background WebSocket
- `mhost disconnect`: close WebSocket, remove server registration
- `cloud_sync.rs` in daemon: on startup, check if `cloud-auth.json` exists, if so start WebSocket connection to ws.mhostai.com, send metrics/events/logs
- Every process start/stop/crash/restart → publish event to cloud via WebSocket
- Every 10s → send metrics snapshot (all processes CPU/memory/uptime)
- Graceful disconnect on daemon shutdown

---

## Phase 3: Core Dashboard (Weeks 3-5)

### 3.1 SvelteKit Frontend Setup

```
frontend/
├── src/
│   ├── routes/
│   │   ├── +layout.svelte         # Nav sidebar, auth guard
│   │   ├── +layout.server.ts      # Session validation
│   │   ├── auth/
│   │   │   ├── login/+page.svelte
│   │   │   ├── register/+page.svelte
│   │   │   └── link/+page.svelte  # Device code approval
│   │   ├── dashboard/+page.svelte  # Overview
│   │   ├── fleet/+page.svelte      # World map
│   │   ├── servers/
│   │   │   ├── +page.svelte        # Server list
│   │   │   └── [id]/+page.svelte   # Server detail
│   │   ├── settings/
│   │   │   ├── +page.svelte        # Account
│   │   │   ├── team/+page.svelte   # Team mgmt
│   │   │   └── billing/+page.svelte
│   │   └── ...more pages
│   ├── lib/
│   │   ├── api.ts                  # Fetch wrapper with auth
│   │   ├── ws.ts                   # WebSocket client
│   │   ├── stores/
│   │   │   ├── auth.ts             # User/team store
│   │   │   ├── servers.ts          # Server list store
│   │   │   └── notifications.ts    # Toast notifications
│   │   └── components/
│   │       ├── Sidebar.svelte
│   │       ├── ServerCard.svelte
│   │       ├── ProcessCard.svelte
│   │       ├── MetricsChart.svelte
│   │       └── StatusBadge.svelte
│   └── app.css                     # Global styles (dark theme)
├── package.json
├── svelte.config.js
├── tailwind.config.js              # Tailwind with mhost colors
└── vite.config.js
```

**Tasks:**
- SvelteKit project with SSR, Tailwind CSS, dark theme (same palette as landing page)
- Auth pages: login (email + GitHub + Google), register, device code approval
- Layout: sidebar nav with icons, top bar with user avatar + team switcher
- Dashboard page: fleet health donut, recent incidents, cost summary, top processes
- Server list: cards with status, process count, region, provider
- Server detail: process table, metrics charts, log viewer, actions (restart, stop, scale)
- Settings: profile, team management (invite, roles), API keys
- WebSocket client: connect to ws.mhostai.com, receive live metric/event updates, update stores reactively
- Toast notification system for real-time events

### 3.2 Fleet Map

```
frontend/src/lib/components/FleetMap.svelte
```

**Tasks:**
- World map using Canvas or SVG (no heavy map library — just a stylized world outline)
- Dots for each server, colored by status (green/yellow/red)
- Pulse animation on active servers
- Lines between servers showing connections/traffic
- Click dot → flyout panel with server summary
- Auto-updates via WebSocket store
- Responsive (works on mobile as smaller view)

### 3.3 Process Management (via relay)

```
crates/cloud-api/src/routes/processes.rs
```

**Endpoints (proxied through relay):**
```
GET    /servers/:id/processes
POST   /servers/:id/processes/:name/start
POST   /servers/:id/processes/:name/stop
POST   /servers/:id/processes/:name/restart
POST   /servers/:id/processes/:name/reload
POST   /servers/:id/processes/:name/scale    { instances }
DELETE /servers/:id/processes/:name
```

**Tasks:**
- Each endpoint sends command to relay via internal HTTP
- Relay forwards to server's WebSocket
- Server's mhostd executes command, sends response back
- API returns response to dashboard
- Timeout: 30s for commands, return error if server offline

---

## Phase 4: Monitoring + Logs (Weeks 5-7)

### 4.1 Metrics Pipeline

```
crates/cloud-worker/src/jobs/metric_ingest.rs
crates/cloud-api/src/routes/metrics.rs
```

**Tasks:**
- Worker subscribes to Redis `metrics:ingest` channel
- Batches metrics (100 rows or 5s, whichever first)
- Writes to ClickHouse: table `metrics` (timestamp, server_id, process_name, cpu, memory, uptime, restarts)
- API endpoints:
  - GET /servers/:id/metrics?range=1h|24h|7d|30d → query ClickHouse, return time series
  - GET /servers/:id/metrics/current → latest from MongoDB (updated by relay)
- Dashboard: MetricsChart.svelte with line charts (use Chart.js or lightweight canvas)

### 4.2 Log Pipeline

```
crates/cloud-worker/src/jobs/log_ingest.rs
crates/cloud-api/src/routes/logs.rs
```

**Tasks:**
- Worker subscribes to Redis `logs:ingest` channel
- Batches logs (1000 lines or 10s)
- Writes to R2 as gzipped JSONL: `logs/{team_id}/{server_id}/{date}/{hour}.jsonl.gz`
- Index in ClickHouse: table `log_index` (timestamp, server_id, process, level, message_preview)
- API endpoints:
  - GET /logs?server=id&process=name&search=query&since=1h → query ClickHouse index, fetch matching files from R2
  - GET /logs/stream?server=id&process=name → SSE stream (proxied from relay WebSocket)
- Dashboard: LogViewer.svelte with search, level filter, real-time streaming toggle
- Retention: scheduler deletes logs older than plan limit (14d/30d/1y)

### 4.3 Alerts

```
crates/cloud-api/src/routes/alerts.rs
crates/cloud-worker/src/jobs/alert.rs
crates/cloud-scheduler/src/alerts.rs  
```

**Tasks:**
- Alert rule model: metric, condition (>, <, ==), threshold, duration, notify channels, team_id
- API: CRUD for alert rules
- Scheduler: every 10s, evaluate all active rules against latest metrics
- When triggered: create alert event, dispatch notification job to worker
- Worker: send notification via email (Resend), Slack webhook, Telegram, Discord, PagerDuty
- Alert states: firing → acknowledged → resolved
- Dashboard: AlertRules.svelte + AlertHistory.svelte

---

## Phase 5: Deployments (Weeks 7-9)

### 5.1 Deployment Pipeline

```
crates/cloud-api/src/routes/deployments.rs
crates/cloud-worker/src/jobs/deploy.rs
```

**Tasks:**
- Deployment model: team_id, server_ids, process, source (git repo, branch, commit), pipeline steps, status
- API: create deployment, list, detail, rollback, promote
- Worker deploy job:
  1. Clone repo (git2 crate or `git clone`)
  2. Detect build system (package.json → npm, Cargo.toml → cargo, etc.)
  3. Run build command
  4. Run tests (optional)
  5. Package artifacts
  6. If canary: deploy to 10%, wait, check errors
  7. If healthy: promote to 100%
  8. If errors exceed threshold: auto-rollback
  9. Send status events to dashboard via Redis
- GitHub webhook: POST /webhooks/github → auto-trigger deploy on push
- Dashboard: DeployPipeline.svelte with visual step progression, live logs per step

### 5.2 GitHub App Integration

**Tasks:**
- Register GitHub App (for repo access)
- On connect: user installs GitHub App on their repos
- Webhook listener: receive push events, match to deployment configs
- Auto-deploy: push to main → trigger deployment pipeline
- Status checks: update GitHub commit status with deploy result

---

## Phase 6: AI + Incidents (Weeks 9-11)

### 6.1 AI Jobs

```
crates/cloud-worker/src/jobs/ai.rs
crates/cloud-api/src/routes/ai.rs
```

**Tasks:**
- AI diagnose: gather logs + metrics for process, send to OpenAI/Claude, return analysis
- AI optimize: analyze metrics trends, suggest scaling/resizing/migration
- AI cost optimizer: compare current spend vs alternatives across providers, generate recommendations
- Use platform's own API key (users don't need their own on Pro+)
- Usage metering: count AI calls per team per month
- API: POST /ai/diagnose { server_id, process }, GET /ai/recommendations

### 6.2 Incident System

```
crates/cloud-api/src/routes/incidents.rs
crates/cloud-worker/src/jobs/incident.rs
```

**Tasks:**
- Auto-create incident when: alert fires + process crash within 5min
- Incident model: title, severity, status, war room data, affected services, timeline
- War room: aggregates timeline events, AI diagnosis, affected service graph, action buttons
- Shareable link: `/incidents/:id/war-room?share_token=xxx` (no login required for viewers)
- Post-mortem: auto-generate from timeline + AI analysis + impact data
- Dashboard: WarRoom.svelte with timeline, diagnosis panel, action buttons (rollback, restart, scale)

### 6.3 Smart Autoscaling

```
crates/cloud-api/src/routes/autoscale.rs
crates/cloud-scheduler/src/autoscale.rs
```

**Tasks:**
- Autoscale rule model: process, min/max instances, metrics thresholds, cooldown, predictive config
- Scheduler: every 10s, evaluate rules:
  - Reactive: if cpu > threshold for duration → scale up
  - Predictive: learn from historical patterns (day-of-week + hour → expected load), pre-scale 10min before
- Execute scaling via relay → server's mhostd
- Track savings: "(4 instances × $10/mo) - (2 instances × $10/mo) = $20/mo saved this month"
- Dashboard: AutoscaleChart.svelte with instance count + CPU overlay + predicted scaling

---

## Phase 7: Premium Features (Weeks 11-14)

### 7.1 Secrets Vault

```
crates/cloud-api/src/routes/secrets.rs
```

**Tasks:**
- Secret model: name, encrypted_value (AES-256-GCM), scope (team/server/process), version history, rotation config
- Encryption: master key from env var, per-secret IV, stored encrypted in MongoDB
- API: set, get (masked), rotate, audit log
- Rotation: scheduler checks expiry, auto-rotates, re-deploys to servers
- Leak detection: worker scans logs for patterns matching known secrets
- Dashboard: SecretsVault.svelte with version history, rotation schedule, access audit

### 7.2 Cost Dashboard

```
crates/cloud-api/src/routes/cost.rs
crates/cloud-scheduler/src/cost.rs
```

**Tasks:**
- Scheduler: every 5min, query cloud provider APIs for current spend
- Aggregate by team, server, process, provider
- Store in MongoDB: cost snapshots with timestamp
- AI recommendations: "Move X from AWS to Railway, save $Y/mo" with one-click apply
- Forecast: project current month spend based on trend
- Dashboard: CostDashboard.svelte with bar charts, donut breakdown, recommendation cards

### 7.3 Billing (Stripe)

```
crates/cloud-api/src/routes/billing.rs
crates/cloud-shared/src/billing.rs
```

**Tasks:**
- Stripe Products: Free, Pro ($29), Team ($99), Enterprise ($499)
- Stripe Checkout: redirect to Stripe for subscription
- Webhook handler: process invoice.paid, subscription.updated, subscription.deleted
- Usage metering: report AI calls, log storage, bandwidth to Stripe
- Feature gating middleware: check team plan before allowing Pro/Team/Enterprise features
- Portal: redirect to Stripe Customer Portal for invoice/payment management
- Dashboard: BillingPage.svelte with plan comparison, usage meters, invoice history

### 7.4 Compliance

```
crates/cloud-api/src/routes/compliance.rs
crates/cloud-worker/src/jobs/compliance.rs
```

**Tasks:**
- Audit log: every API request logged with user, action, resource, timestamp, IP
- Report generation: worker generates PDF/HTML reports
- Uptime report: calculate from metrics data (% uptime per process per period)
- SOC 2 checklist: auto-fill based on security config (2FA, SSO, audit logs)
- GDPR data export: collect all user data into downloadable ZIP
- Dashboard: CompliancePage.svelte with report list, audit log viewer, data export button

### 7.5 Plugin Marketplace

```
crates/cloud-api/src/routes/plugins.rs
```

**Tasks:**
- Plugin model: name, slug, author, description, version, downloads, stars, pricing, hooks, source_url
- Browse: list with search, categories, sort by popularity
- Install: add plugin to team, configure hooks
- Publish: authors upload plugin package, worker validates (security scan)
- Pricing: free or paid (Stripe Connect for revenue share 70/30)
- Reviews: star rating + text reviews
- Dashboard: PluginMarketplace.svelte with grid cards, detail page, install button

### 7.6 Status Pages

```
crates/cloud-api/src/routes/status_pages.rs
```

**Tasks:**
- Status page model: team_id, name, subdomain, processes to monitor, custom branding
- Hosted at: `<name>.status.mhostai.com` or custom domain
- Auto-updates from process health data
- Incident reporting: manual or auto (from incident system)
- Edge rendering: Cloudflare Worker generates HTML from cached data
- Dashboard: StatusPageSettings.svelte with domain config, process selection, branding

---

## Phase 8: Polish + Launch (Weeks 14-16)

### 8.1 Testing
- Unit tests for all API routes (mock MongoDB)
- Integration tests with test database
- E2E tests for auth flow, deployment pipeline, WebSocket relay
- Load testing for relay (target: 10K concurrent WebSocket connections)

### 8.2 Security Audit
- Penetration testing on API
- Dependency audit (cargo audit)
- Rate limiting verification
- Token expiry verification
- CORS configuration
- CSP headers on frontend

### 8.3 Documentation
- API docs (auto-generated from route definitions)
- CLI docs for login/connect
- Architecture docs for self-hosters
- Pricing page on landing site

### 8.4 Launch
- Update mhostai.com with cloud platform section
- Blog post: "Introducing mhost Cloud"
- Product Hunt launch
- Show HN post
- Discord announcement
- Twitter/X thread

---

## Railway Service Config

```toml
# railway.toml
[services.api]
name = "mhost-api"
source = "crates/cloud-api"
build_command = "cargo build --release -p cloud-api"
start_command = "./target/release/cloud-api"
health_check = "/health"
instances = 2
plan = "pro"

[services.relay]
name = "mhost-relay"
source = "crates/cloud-relay"
build_command = "cargo build --release -p cloud-relay"
start_command = "./target/release/cloud-relay"
instances = 2

[services.worker]
name = "mhost-worker"
source = "crates/cloud-worker"
build_command = "cargo build --release -p cloud-worker"
start_command = "./target/release/cloud-worker"
instances = 2

[services.scheduler]
name = "mhost-scheduler"
source = "crates/cloud-scheduler"
build_command = "cargo build --release -p cloud-scheduler"
start_command = "./target/release/cloud-scheduler"
instances = 1

[services.frontend]
name = "mhost-frontend"
source = "frontend"
build_command = "npm run build"
start_command = "node build"
instances = 1
```

## Environment Variables

```
# Database
MONGODB_URI=mongodb+srv://...
REDIS_URL=redis://...
CLICKHOUSE_URL=https://...
CLICKHOUSE_USER=default
CLICKHOUSE_PASSWORD=...

# Auth
JWT_SECRET=random-256-bit-key
JWT_ACCESS_TTL=900
JWT_REFRESH_TTL=2592000
GITHUB_CLIENT_ID=...
GITHUB_CLIENT_SECRET=...
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...

# Billing
STRIPE_SECRET_KEY=sk_live_...
STRIPE_WEBHOOK_SECRET=whsec_...
STRIPE_PRICE_PRO=price_...
STRIPE_PRICE_TEAM=price_...
STRIPE_PRICE_ENTERPRISE=price_...

# Email
RESEND_API_KEY=re_...
FROM_EMAIL=noreply@mhostai.com

# Storage
R2_ACCOUNT_ID=...
R2_ACCESS_KEY=...
R2_SECRET_KEY=...
R2_BUCKET=mhost-cloud

# AI
OPENAI_API_KEY=sk-...

# Security
ENCRYPTION_KEY=random-256-bit-key
CORS_ORIGINS=https://app.mhostai.com

# URLs
API_URL=https://api.mhostai.com
WS_URL=wss://ws.mhostai.com
FRONTEND_URL=https://app.mhostai.com
```

## Estimated Costs (Monthly)

| Phase | Users | Railway | MongoDB | Redis | ClickHouse | R2 | Stripe | Total |
|---|---|---|---|---|---|---|---|---|
| Launch | 0-100 | $50 | $57 | $0 | $0 (free) | $0 | $0 | ~$107 |
| Growth | 100-1K | $150 | $57 | $10 | $195 | $5 | $50 | ~$467 |
| Scale | 1K-10K | $400 | $200 | $50 | $400 | $50 | $500 | ~$1,600 |
| Mature | 10K+ | $1,000 | $500 | $100 | $800 | $200 | $5,000 | ~$7,600 |

**Break-even:** ~50 Pro subscribers ($29 × 50 = $1,450/mo) covers Growth phase costs.

---

## Implementation Order (Critical Path)

```
Week 1:  Repo setup, cloud-shared, MongoDB models, config
Week 2:  Auth (register, login, OAuth, device code), JWT middleware
Week 3:  Server registration, WebSocket relay, CLI login/connect
Week 4:  Dashboard layout, auth pages, server list, fleet map
Week 5:  Metrics pipeline (relay → Redis → worker → ClickHouse → API → charts)
Week 6:  Log pipeline (relay → Redis → worker → R2 → API → viewer)
Week 7:  Alerts system, notification dispatch
Week 8:  Deployment pipeline, GitHub webhook
Week 9:  AI jobs (diagnose, optimize, cost), incident system
Week 10: Autoscaling (reactive + predictive), war room UI
Week 11: Secrets vault, cost dashboard
Week 12: Stripe billing, feature gating, usage metering
Week 13: Plugin marketplace, status pages, compliance
Week 14: Testing, security audit, load testing
Week 15: Documentation, marketing pages
Week 16: Launch
```

Each week is one deployable milestone. Every week ends with a working, deployed system on Railway.
