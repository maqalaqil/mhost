# mhost Cloud Platform — Full Production Design

## Vision

**mhost Cloud** is the managed platform behind mhost CLI. Users connect their servers to mhost Cloud and get: live fleet visualization, AI cost optimization, smart autoscaling, deployment pipelines, incident war rooms, database proxy, edge CDN, secrets vault, compliance reports, and a plugin marketplace — all from a single dashboard at **app.mhostai.com**.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                    EDGE LAYER                                        │
│                                                                                       │
│   Cloudflare (DNS + CDN + WAF + DDoS)                                                │
│     ├── mhostai.com          → Landing page (static)                                 │
│     ├── app.mhostai.com      → Cloud Dashboard (SPA)                                 │
│     ├── api.mhostai.com      → API Gateway                                          │
│     ├── ws.mhostai.com       → WebSocket Relay                                      │
│     ├── status.mhostai.com   → Public status pages                                  │
│     └── cdn.mhostai.com      → User static assets / edge cache                      │
└──────────────────────────────────┬──────────────────────────────────────────────────┘
                                   │
┌──────────────────────────────────▼──────────────────────────────────────────────────┐
│                              RAILWAY (Compute)                                       │
│                                                                                       │
│   ┌─────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐         │
│   │  API Server  │  │  WS Relay    │  │  Worker      │  │  Scheduler       │         │
│   │  (Rust/Axum) │  │  (Rust/Tokio)│  │  (Rust)      │  │  (Rust)          │         │
│   │              │  │              │  │              │  │                  │         │
│   │  Auth        │  │  Agent relay │  │  AI jobs     │  │  Autoscale eval  │         │
│   │  REST API    │  │  Log stream  │  │  Cost calc   │  │  Cron triggers   │         │
│   │  Billing     │  │  Metrics     │  │  Alerts      │  │  Retention       │         │
│   │  Webhooks    │  │  Events      │  │  Deploys     │  │  Backups         │         │
│   │              │  │              │  │  Compliance   │  │  Health checks   │         │
│   └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────────┘         │
│          │                 │                 │                 │                      │
│   ┌──────▼─────────────────▼─────────────────▼─────────────────▼───────────┐         │
│   │                        Message Queue (Redis)                            │         │
│   │   Channels: jobs, events, metrics, logs, deploys, alerts               │         │
│   └──────────────────────────────┬─────────────────────────────────────────┘         │
│                                  │                                                    │
│   ┌──────────────────────────────▼─────────────────────────────────────────┐         │
│   │                        Data Layer                                       │         │
│   │                                                                          │         │
│   │   MongoDB Atlas        Redis            S3 (R2)         ClickHouse      │         │
│   │   ├── users            ├── sessions     ├── logs        ├── metrics     │         │
│   │   ├── teams            ├── cache        ├── backups     ├── events      │         │
│   │   ├── servers          ├── pubsub       ├── artifacts   ├── analytics   │         │
│   │   ├── processes        ├── rate limits  ├── snapshots   └───────────────│         │
│   │   ├── deployments      └────────────────├── secrets                     │         │
│   │   ├── incidents                         └───────────────────────────────│         │
│   │   ├── billing                                                           │         │
│   │   ├── plugins                                                           │         │
│   │   ├── webhooks                                                          │         │
│   │   ├── audit_logs                                                        │         │
│   │   └── compliance                                                        │         │
│   └─────────────────────────────────────────────────────────────────────────┘         │
└──────────────────────────────────────────────────────────────────────────────────────┘

                                   ▲
                                   │ WebSocket (persistent)
                                   │
┌──────────────────────────────────┴──────────────────────────────────────────────────┐
│                           USER'S SERVERS (worldwide)                                  │
│                                                                                       │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐               │
│   │  Server A    │  │  Server B    │  │  Server C    │  │  Server D    │               │
│   │  (AWS US)    │  │  (GCP EU)    │  │  (DO Asia)   │  │  (Railway)   │               │
│   │              │  │              │  │              │  │              │               │
│   │  mhostd ◄────┼──┼──────────────┼──┼──────────────┼──┼─► mhost Cloud│               │
│   │  agent      │  │  mhostd      │  │  mhostd      │  │  mhostd      │               │
│   │  brain      │  │  agent       │  │  agent       │  │  agent       │               │
│   └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘               │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

## Tech Stack

| Layer | Technology | Why |
|---|---|---|
| **API Server** | Rust + Axum | Same language as mhost, fastest possible, zero-cost abstractions |
| **WebSocket Relay** | Rust + Tokio + Tungstenite | Handle 100K concurrent connections |
| **Worker** | Rust + Tokio | Background jobs, AI calls, cost calculation |
| **Scheduler** | Rust + Tokio-cron | Autoscale evaluation, retention cleanup, health checks |
| **Database** | MongoDB Atlas | Flexible schema, horizontal scaling, great for nested docs (server configs, process trees) |
| **Cache/Queue** | Redis (Railway or Upstash) | Sessions, pub/sub, rate limiting, job queue |
| **Object Storage** | Cloudflare R2 | Logs, backups, artifacts, snapshots (S3-compatible, no egress fees) |
| **Time-Series** | ClickHouse (managed) | Metrics, events, analytics at scale |
| **Frontend** | SvelteKit | Fast SSR, small bundle, great DX |
| **Auth** | Custom JWT + OAuth2 (GitHub, Google) | No vendor lock-in |
| **Billing** | Stripe | Subscriptions + usage metering |
| **Email** | Resend | Transactional emails (welcome, alerts, invoices) |
| **DNS/CDN** | Cloudflare | DDoS, WAF, edge caching, Workers for edge logic |
| **Hosting** | Railway | All backend services, auto-deploy from GitHub |
| **CI/CD** | mhost GitHub Action | Eat our own dog food |

## Services (6 Rust binaries)

### 1. `mhost-api` — REST API Server
```
Port: 8000
Routes:
  /auth/*           — login, register, OAuth, refresh tokens
  /users/*          — profile, settings, API keys
  /teams/*          — create, invite, roles, remove
  /servers/*        — register, list, status, remove
  /processes/*      — list, start, stop, restart, scale (proxied to user's mhostd)
  /deployments/*    — create, list, status, rollback, promote
  /incidents/*      — list, detail, war room, post-mortem
  /metrics/*        — query, aggregate, dashboards
  /logs/*           — search, stream, export
  /alerts/*         — rules, history, acknowledge
  /billing/*        — plans, subscribe, usage, invoices
  /plugins/*        — browse, install, publish, review
  /webhooks/*       — create, list, test, logs
  /secrets/*        — set, get (masked), rotate, audit
  /compliance/*     — reports, exports, certifications
  /status-pages/*   — create, configure, incidents
  /cost/*           — breakdown, optimizer, recommendations
  /autoscale/*      — rules, history, predictions
  /fleet/*          — map data, regions, topology
```

### 2. `mhost-relay` — WebSocket Relay Server
```
Port: 8001
Responsibilities:
  - Accept persistent WebSocket connections from user mhostd daemons
  - Authenticate via server registration token
  - Receive: metrics (every 10s), logs (streaming), events (real-time)
  - Forward commands: start, stop, restart, scale, deploy
  - Heartbeat monitoring (detect offline servers)
  - Fan-out events to dashboard WebSocket clients
  - Buffer messages during brief disconnections (Redis-backed)
```

### 3. `mhost-worker` — Background Job Processor
```
Consumes jobs from Redis queue:
  - ai:diagnose     — call OpenAI/Claude for crash diagnosis
  - ai:optimize     — analyze metrics, suggest optimizations
  - ai:cost         — calculate cost optimization recommendations
  - deploy:build    — git clone, build, package
  - deploy:push     — send artifacts to target servers
  - alert:evaluate  — check alert rules against metrics
  - alert:send      — dispatch notifications (email, Slack, Telegram, webhook)
  - incident:create — auto-create incident from alert chain
  - log:ingest      — write log batches to R2 + ClickHouse
  - metric:ingest   — write metric batches to ClickHouse
  - compliance:gen  — generate compliance reports
  - backup:create   — snapshot server state to R2
  - plugin:validate — scan uploaded plugins for security
```

### 4. `mhost-scheduler` — Cron Job Service
```
Runs on interval:
  Every 10s:  Autoscale evaluation (check metrics → scale up/down)
  Every 30s:  Server health check (mark offline if no heartbeat)
  Every 5m:   Cost recalculation (aggregate provider APIs)
  Every 1h:   Log retention cleanup (delete expired logs)
  Every 6h:   Compliance report generation
  Every 24h:  Usage metering → Stripe (billing sync)
  Every 24h:  Certificate expiry check
  Every 7d:   Backup old metrics to cold storage
```

### 5. `mhost-edge` — Edge Workers (Cloudflare Workers)
```
  - Geo-routing for user traffic
  - Static asset caching
  - Rate limiting at edge
  - Bot protection
  - Status page rendering (cached HTML)
  - Webhook relay (fast acknowledgement)
```

### 6. `mhost-migrate` — Database Migration Tool
```
  - Schema versioning for MongoDB
  - Seed data for new accounts
  - Data backfill scripts
```

## Database Schema (MongoDB)

### users
```json
{
  "_id": "ObjectId",
  "email": "user@example.com",
  "name": "Maher",
  "password_hash": "argon2...",
  "avatar_url": "https://...",
  "oauth_providers": [{ "provider": "github", "provider_id": "12345" }],
  "plan": "pro",
  "stripe_customer_id": "cus_xxx",
  "api_keys": [{ "id": "key_xxx", "name": "CI", "hash": "...", "last_used": "..." }],
  "settings": {
    "timezone": "America/New_York",
    "notifications": { "email": true, "slack_webhook": "..." },
    "default_region": "us-east-1"
  },
  "created_at": "2026-04-05T00:00:00Z",
  "last_login": "2026-04-05T12:00:00Z"
}
```

### teams
```json
{
  "_id": "ObjectId",
  "name": "Acme Corp",
  "slug": "acme",
  "owner_id": "ObjectId(user)",
  "plan": "team",
  "stripe_subscription_id": "sub_xxx",
  "members": [
    { "user_id": "ObjectId", "role": "admin", "joined_at": "..." },
    { "user_id": "ObjectId", "role": "developer", "joined_at": "..." },
    { "user_id": "ObjectId", "role": "viewer", "joined_at": "..." }
  ],
  "invite_codes": [{ "code": "abc123", "role": "developer", "expires_at": "..." }],
  "settings": {
    "require_2fa": true,
    "allowed_providers": ["aws", "railway"],
    "ip_whitelist": ["10.0.0.0/8"],
    "sso": { "enabled": false }
  },
  "usage": {
    "servers": 12,
    "processes": 47,
    "log_bytes_month": 5368709120,
    "ai_calls_month": 234
  },
  "created_at": "2026-04-05T00:00:00Z"
}
```

### servers
```json
{
  "_id": "ObjectId",
  "team_id": "ObjectId",
  "name": "prod-api-1",
  "registration_token": "srv_xxx",
  "status": "online",
  "region": "us-east-1",
  "provider": "aws",
  "provider_meta": {
    "instance_id": "i-abc123",
    "instance_type": "t3.medium",
    "availability_zone": "us-east-1a"
  },
  "coordinates": { "lat": 39.0438, "lng": -77.4874 },
  "mhost_version": "0.22.0",
  "os": "Ubuntu 22.04",
  "cpu_cores": 2,
  "memory_gb": 4,
  "processes": [
    {
      "name": "api-server",
      "status": "online",
      "pid": 12345,
      "cpu_percent": 23.5,
      "memory_mb": 256,
      "uptime_secs": 86400,
      "restarts": 0,
      "health": "passing",
      "port": 3000
    }
  ],
  "last_heartbeat": "2026-04-05T12:00:00Z",
  "connected_since": "2026-04-01T08:00:00Z",
  "tags": ["production", "api"],
  "created_at": "2026-04-01T08:00:00Z"
}
```

### deployments
```json
{
  "_id": "ObjectId",
  "team_id": "ObjectId",
  "server_ids": ["ObjectId"],
  "process_name": "api-server",
  "type": "canary",
  "status": "live",
  "source": {
    "type": "git",
    "repo": "maqalaqil/myapp",
    "branch": "main",
    "commit": "abc123",
    "message": "fix: database timeout"
  },
  "pipeline": {
    "steps": [
      { "name": "clone", "status": "done", "duration_ms": 2400 },
      { "name": "build", "status": "done", "duration_ms": 45000 },
      { "name": "test", "status": "done", "duration_ms": 12000 },
      { "name": "canary_10%", "status": "done", "duration_ms": 300000 },
      { "name": "health_check", "status": "done", "duration_ms": 5000 },
      { "name": "promote_100%", "status": "done", "duration_ms": 3000 }
    ]
  },
  "canary": {
    "percent": 10,
    "duration_secs": 300,
    "error_threshold": 5,
    "errors_observed": 0,
    "promoted": true
  },
  "rollback_to": null,
  "triggered_by": { "user_id": "ObjectId", "source": "github_push" },
  "created_at": "2026-04-05T12:00:00Z",
  "completed_at": "2026-04-05T12:07:30Z"
}
```

### incidents
```json
{
  "_id": "ObjectId",
  "team_id": "ObjectId",
  "title": "api-server crash on prod-api-1",
  "severity": "critical",
  "status": "resolved",
  "war_room": {
    "share_url": "https://app.mhostai.com/incidents/inc_xxx/war-room",
    "timeline": [
      { "time": "12:00:00", "event": "Process api-server exited with code 1" },
      { "time": "12:00:01", "event": "Auto-restart attempt #1" },
      { "time": "12:00:05", "event": "Brain detected crash loop (3 crashes in 2min)" },
      { "time": "12:00:06", "event": "AI diagnosis initiated" },
      { "time": "12:00:12", "event": "Root cause: EADDRINUSE on port 3000" },
      { "time": "12:00:13", "event": "Auto-fix: killed orphan process on port 3000" },
      { "time": "12:00:14", "event": "Restart successful, health check passing" }
    ],
    "affected_services": ["api-server", "worker"],
    "ai_diagnosis": "Port 3000 was held by an orphan process...",
    "suggested_fixes": ["Add port-check to startup script"],
    "participants": ["ObjectId(user1)", "ObjectId(user2)"]
  },
  "post_mortem": {
    "summary": "...",
    "root_cause": "...",
    "impact": "12 seconds of downtime",
    "action_items": ["Add port cleanup to startup", "Add EADDRINUSE playbook"]
  },
  "servers": ["ObjectId"],
  "processes": ["api-server"],
  "duration_secs": 14,
  "created_at": "2026-04-05T12:00:00Z",
  "resolved_at": "2026-04-05T12:00:14Z"
}
```

### autoscale_rules
```json
{
  "_id": "ObjectId",
  "team_id": "ObjectId",
  "server_id": "ObjectId",
  "process_name": "api-server",
  "enabled": true,
  "min_instances": 1,
  "max_instances": 10,
  "rules": [
    {
      "metric": "cpu",
      "threshold": 80,
      "direction": "up",
      "cooldown_secs": 120,
      "scale_amount": 2
    },
    {
      "metric": "cpu",
      "threshold": 20,
      "direction": "down",
      "cooldown_secs": 300,
      "scale_amount": 1
    }
  ],
  "predictive": {
    "enabled": true,
    "model": "traffic_pattern",
    "pre_scale_minutes": 10,
    "patterns": [
      { "day": "weekday", "peak_hours": [11, 12, 13, 17, 18], "scale_to": 6 },
      { "day": "weekend", "peak_hours": [14, 15, 16], "scale_to": 3 }
    ]
  },
  "history": [
    { "timestamp": "...", "from": 2, "to": 4, "reason": "cpu > 80% for 2min", "source": "rule" },
    { "timestamp": "...", "from": 4, "to": 2, "reason": "predicted low traffic", "source": "predictive" }
  ],
  "savings_this_month": 47.50
}
```

### billing
```json
{
  "_id": "ObjectId",
  "team_id": "ObjectId",
  "stripe_subscription_id": "sub_xxx",
  "plan": "team",
  "status": "active",
  "current_period_start": "2026-04-01",
  "current_period_end": "2026-05-01",
  "usage": {
    "servers": { "included": 50, "used": 12 },
    "ai_calls": { "included": 1000, "used": 234, "overage_rate": 0.02 },
    "log_storage_gb": { "included": 10, "used": 5.1, "overage_rate": 0.50 },
    "bandwidth_gb": { "included": 100, "used": 23.4, "overage_rate": 0.10 },
    "build_minutes": { "included": 1000, "used": 456, "overage_rate": 0.01 }
  },
  "invoices": [
    { "id": "inv_xxx", "amount": 9900, "status": "paid", "date": "2026-04-01" }
  ]
}
```

### plugins (marketplace)
```json
{
  "_id": "ObjectId",
  "name": "mhost-datadog",
  "slug": "datadog",
  "author_id": "ObjectId",
  "description": "Forward mhost metrics to Datadog",
  "version": "1.2.0",
  "downloads": 1247,
  "stars": 89,
  "verified": true,
  "pricing": { "type": "free" },
  "hooks": ["on_metric", "on_start", "on_stop"],
  "source_url": "https://github.com/user/mhost-datadog",
  "readme": "...",
  "categories": ["monitoring", "integration"],
  "created_at": "2026-03-01T00:00:00Z",
  "updated_at": "2026-04-01T00:00:00Z"
}
```

### secrets_vault
```json
{
  "_id": "ObjectId",
  "team_id": "ObjectId",
  "name": "DATABASE_URL",
  "scope": "process:api-server",
  "encrypted_value": "AES-256-GCM encrypted...",
  "version": 3,
  "versions": [
    { "version": 1, "set_by": "ObjectId", "set_at": "2026-01-01", "rotated": true },
    { "version": 2, "set_by": "ObjectId", "set_at": "2026-02-15", "rotated": true },
    { "version": 3, "set_by": "ObjectId", "set_at": "2026-04-01", "rotated": false }
  ],
  "rotation": {
    "enabled": true,
    "interval_days": 90,
    "next_rotation": "2026-07-01",
    "auto_deploy": true
  },
  "audit": [
    { "action": "read", "by": "ObjectId", "at": "2026-04-05T12:00:00Z", "source": "deploy" },
    { "action": "rotate", "by": "ObjectId", "at": "2026-04-01T00:00:00Z", "source": "schedule" }
  ]
}
```

## API Design

### Auth Endpoints
```
POST   /auth/register        { email, password, name }
POST   /auth/login            { email, password } → { access_token, refresh_token }
POST   /auth/refresh           { refresh_token } → { access_token }
GET    /auth/oauth/github     → redirect to GitHub OAuth
GET    /auth/oauth/google     → redirect to Google OAuth
GET    /auth/oauth/callback   → handle OAuth callback
POST   /auth/logout            → invalidate tokens
POST   /auth/forgot-password   { email }
POST   /auth/reset-password    { token, new_password }
POST   /auth/verify-email      { token }
```

### User Endpoints
```
GET    /users/me               → user profile
PATCH  /users/me               { name, avatar_url, settings }
POST   /users/me/api-keys      { name } → { api_key }
GET    /users/me/api-keys      → list API keys
DELETE /users/me/api-keys/:id  → revoke API key
POST   /users/me/2fa/enable    → generate TOTP secret
POST   /users/me/2fa/verify    { code } → enable 2FA
```

### Team Endpoints
```
POST   /teams                  { name } → create team
GET    /teams                  → list user's teams
GET    /teams/:id              → team detail
PATCH  /teams/:id              { name, settings }
POST   /teams/:id/invite       { email, role } → send invite
GET    /teams/:id/members      → list members
PATCH  /teams/:id/members/:uid { role } → change role
DELETE /teams/:id/members/:uid → remove member
```

### Server Endpoints
```
POST   /servers/register       { name, region, provider } → { registration_token }
GET    /servers                → list all servers
GET    /servers/:id            → server detail + processes
DELETE /servers/:id            → deregister server
GET    /servers/:id/metrics    ?range=1h|24h|7d|30d
GET    /servers/:id/logs       ?process=name&lines=100&search=query
POST   /servers/:id/exec       { command } → execute mhost command
GET    /servers/map            → all servers with coordinates for fleet map
```

### Process Endpoints (proxied to user's mhostd via relay)
```
GET    /servers/:id/processes              → list
POST   /servers/:id/processes/:name/start  → start
POST   /servers/:id/processes/:name/stop   → stop
POST   /servers/:id/processes/:name/restart → restart
POST   /servers/:id/processes/:name/reload  → zero-downtime reload
POST   /servers/:id/processes/:name/scale   { instances }
DELETE /servers/:id/processes/:name         → delete
```

### Deployment Endpoints
```
POST   /deployments            { server_ids, process, source, pipeline }
GET    /deployments            → list recent
GET    /deployments/:id        → detail with pipeline steps
POST   /deployments/:id/rollback → rollback to previous
POST   /deployments/:id/promote  → promote canary to 100%
GET    /deployments/:id/logs   → build/deploy logs
```

### Incident Endpoints
```
GET    /incidents              → list
GET    /incidents/:id          → detail
GET    /incidents/:id/war-room → war room data (timeline, diagnosis, fixes)
POST   /incidents/:id/acknowledge → acknowledge
POST   /incidents/:id/resolve  { post_mortem }
GET    /incidents/:id/share    → generate shareable link
```

### Cost Endpoints
```
GET    /cost                   → total across all providers
GET    /cost/breakdown         → per-server, per-process breakdown
GET    /cost/recommendations   → AI optimization suggestions
POST   /cost/recommendations/:id/apply → one-click migrate/resize
GET    /cost/forecast          → projected spend for current month
GET    /cost/savings           → how much autoscaling/optimizer saved
```

### Autoscale Endpoints
```
GET    /autoscale/rules        → list rules
POST   /autoscale/rules        { server_id, process, min, max, rules }
PATCH  /autoscale/rules/:id    → update rule
DELETE /autoscale/rules/:id    → remove rule
GET    /autoscale/history      → scaling events
GET    /autoscale/predictions  → predicted scaling for next 24h
```

### Secrets Endpoints
```
GET    /secrets                → list (values masked)
POST   /secrets                { name, value, scope }
PATCH  /secrets/:id            { value } → create new version
DELETE /secrets/:id            → soft delete
POST   /secrets/:id/rotate     → auto-generate new value
GET    /secrets/:id/audit      → access log
```

### Plugin Marketplace Endpoints
```
GET    /plugins                → browse marketplace
GET    /plugins/:slug          → plugin detail
POST   /plugins/:slug/install  → install to team
DELETE /plugins/:slug/uninstall → remove
POST   /plugins                → publish (plugin authors)
GET    /plugins/mine           → author's plugins
```

### Billing Endpoints
```
GET    /billing                → current plan, usage, next invoice
POST   /billing/subscribe      { plan } → create Stripe subscription
POST   /billing/upgrade        { plan } → change plan
POST   /billing/cancel         → cancel at period end
GET    /billing/invoices       → invoice history
GET    /billing/usage          → detailed usage breakdown
POST   /billing/portal         → Stripe customer portal URL
```

### Compliance Endpoints
```
GET    /compliance/reports     → list generated reports
POST   /compliance/reports     { type: "soc2" | "gdpr" | "uptime" }
GET    /compliance/reports/:id → download report
GET    /compliance/audit-log   ?since=30d → full audit trail
GET    /compliance/data-export → GDPR data export
```

### Status Page Endpoints
```
GET    /status-pages           → list
POST   /status-pages           { name, domain, processes }
PATCH  /status-pages/:id       { settings }
DELETE /status-pages/:id
POST   /status-pages/:id/incidents { title, message, severity }
```

### Alert Endpoints
```
GET    /alerts/rules           → list alert rules
POST   /alerts/rules           { metric, threshold, notify }
PATCH  /alerts/rules/:id
DELETE /alerts/rules/:id
GET    /alerts/history         → fired alerts
POST   /alerts/:id/acknowledge
POST   /alerts/:id/silence     { duration }
```

### Fleet Map Endpoint
```
GET    /fleet/map              → { servers: [{ name, lat, lng, status, processes, connections }] }
GET    /fleet/topology         → service dependency graph
GET    /fleet/traffic          → real-time request flow between services
```

## Frontend (SvelteKit at app.mhostai.com)

### Pages

```
/                      → Landing / marketing (redirect to /dashboard if logged in)
/login                 → Email + OAuth login
/register              → Sign up
/dashboard             → Overview: fleet health, recent incidents, cost, top processes
/fleet                 → World map with servers, click → server detail
/servers/:id           → Server detail: processes, metrics, logs, health
/processes             → All processes across all servers
/deployments           → Deployment history, pipeline view
/deployments/new       → Create deployment wizard
/incidents             → Incident list
/incidents/:id         → Incident detail + war room
/cost                  → Cost dashboard: breakdown, forecast, recommendations
/cost/optimizer        → AI cost optimization with one-click actions
/autoscale             → Autoscale rules, history, predictions chart
/logs                  → Centralized log viewer with search
/alerts                → Alert rules + history
/secrets               → Secrets vault with rotation schedules
/plugins               → Plugin marketplace (browse, install, publish)
/status-pages          → Manage public status pages
/compliance            → Reports, audit log, data export
/settings              → Account settings
/settings/team         → Team management, invites, roles
/settings/billing      → Plan, usage, invoices
/settings/api-keys     → API key management
/settings/integrations → GitHub, Slack, Discord, PagerDuty connections
```

### Key UI Components

**Fleet Map:** Globe or flat map with animated dots per server. Lines between servers show traffic. Color = health (green/yellow/red). Click opens server flyout.

**Deployment Pipeline:** Visual step-by-step: Clone → Build → Test → Canary → Health → Promote. Each step shows duration, logs, status.

**Incident War Room:** Split view — timeline on left, affected services graph on right, AI diagnosis below, action buttons (rollback, restart, scale) at top.

**Cost Optimizer:** Cards showing current spend vs recommended. "Save $X/month" badges. One-click "Apply" buttons. Bar chart showing savings over time.

**Autoscale Chart:** Time-series graph showing instance count vs CPU/memory. Overlay predicted scaling. Toggle between reactive and predictive modes.

## Pricing (Final)

| | **Free** | **Pro $29/mo** | **Team $99/mo** | **Enterprise $499/mo** |
|---|---|---|---|---|
| Servers | 2 | 10 | 50 | Unlimited |
| Team members | 1 | 3 | 20 | Unlimited |
| Processes | 10 | 50 | 250 | Unlimited |
| **Fleet Map** | Static | Live | Live + traffic | Live + traffic + topology |
| **War Room** | No | View only | Full + sharing | Full + SLA tracking |
| **Cost Optimizer** | No | Suggestions | + one-click migrate | + reserved planning |
| **Smart Autoscale** | No | 2 processes | All + predictive | Custom rules + ML |
| **Deploy Pipeline** | No | 3 repos, manual | Unlimited + auto | + approval gates + rollback SLA |
| **Database Proxy** | No | 1 connection | 5 connections | Unlimited + read replicas |
| **Edge CDN** | No | No | 3 regions | Global 50+ PoPs |
| **Secrets Vault** | Local | 50 secrets | 500 + rotation | Unlimited + HashiCorp |
| **Compliance** | No | No | Basic reports | SOC 2 + GDPR + custom |
| **Marketplace** | Free plugins | All plugins | + private plugins | Custom + priority support |
| **AI Diagnose** | Own key | 100/mo | 1000/mo | Unlimited |
| **Log Retention** | Local | 14 days | 30 days | 1 year |
| **Metrics Retention** | Local | 7 days | 30 days | 1 year |
| **Status Pages** | Self-hosted | 1 | 5 + custom domain | Unlimited + branded |
| **Webhooks** | 5 | 25 | 100 | Unlimited |
| **Support** | Discord | Email 48h | Priority 4h | Dedicated + Slack |
| **SLA** | None | None | 99.9% | 99.99% + credits |
| **SSO/SAML** | No | No | No | Yes |
| **Audit Logs** | Local | 7 days | 90 days | 1 year + export |
| **API Rate Limit** | 100/min | 500/min | 2000/min | Custom |
| **Overage (AI)** | N/A | $0.02/call | $0.02/call | Included |
| **Overage (Logs)** | N/A | $0.50/GB | $0.50/GB | $0.25/GB |
| **Overage (BW)** | N/A | $0.10/GB | $0.10/GB | $0.05/GB |

## CLI Integration (`mhost login`)

```bash
# Link CLI to cloud account
mhost login
# → Opens browser: app.mhostai.com/auth/device
# → Enter code shown in terminal
# → CLI receives API token, stores in ~/.mhost/cloud-auth.json

# Register this server
mhost cloud connect
# → Sends: hostname, OS, CPU, memory, mhost version, region (auto-detected)
# → Receives: registration token
# → Starts WebSocket connection to ws.mhostai.com

# All local commands now sync to cloud
mhost start server.js --name api
# → Process starts locally
# → Event sent to cloud: { server_id, process: "api", action: "start" }
# → Dashboard updates in real-time

# Cloud-only commands
mhost cloud dashboard        # Open app.mhostai.com in browser
mhost cloud billing          # Show current plan + usage
mhost cloud team invite user@example.com --role developer
mhost cloud deploy --repo github.com/user/app --branch main
```

## Deployment (Railway)

### Services on Railway
```
mhost-api         → 2 instances, 1 vCPU, 2GB RAM
mhost-relay       → 2 instances, 1 vCPU, 1GB RAM (WebSocket optimized)
mhost-worker      → 2 instances, 2 vCPU, 4GB RAM (AI/build jobs)
mhost-scheduler   → 1 instance, 0.5 vCPU, 512MB RAM
mhost-frontend    → 1 instance (SvelteKit SSR)
```

### External Services
```
MongoDB Atlas     → M10 cluster ($57/mo) → scale to M30 as needed
Redis (Upstash)   → Pay-per-request ($0 to start)
Cloudflare R2     → Free tier (10GB) → pay as you grow
ClickHouse Cloud  → Developer tier ($195/mo)
Stripe            → 2.9% + $0.30 per transaction
Resend            → Free tier (3000 emails/mo)
Cloudflare        → Free plan (DNS, CDN, WAF)
```

### Estimated Monthly Infrastructure Cost
```
Phase 1 (0-1000 users):    ~$300/mo
Phase 2 (1000-10000):      ~$800/mo  
Phase 3 (10000-100000):    ~$3000/mo
```

### CI/CD (using our own GitHub Action)
```yaml
# .github/workflows/deploy.yml
name: Deploy to Railway
on:
  push:
    branches: [main]
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: maqalaqil/mhost-action@v1
        with:
          command: cloud deploy production
          config: mhost-cloud.toml
```

## Repository Structure

```
mhost-cloud/                          # New repo: github.com/maqalaqil/mhost-cloud
├── Cargo.toml                        # Rust workspace
├── crates/
│   ├── cloud-api/                    # REST API server (Axum)
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── config.rs
│   │   │   ├── routes/
│   │   │   │   ├── auth.rs
│   │   │   │   ├── users.rs
│   │   │   │   ├── teams.rs
│   │   │   │   ├── servers.rs
│   │   │   │   ├── processes.rs
│   │   │   │   ├── deployments.rs
│   │   │   │   ├── incidents.rs
│   │   │   │   ├── cost.rs
│   │   │   │   ├── autoscale.rs
│   │   │   │   ├── secrets.rs
│   │   │   │   ├── plugins.rs
│   │   │   │   ├── billing.rs
│   │   │   │   ├── compliance.rs
│   │   │   │   ├── alerts.rs
│   │   │   │   ├── status_pages.rs
│   │   │   │   ├── fleet.rs
│   │   │   │   └── webhooks.rs
│   │   │   ├── middleware/
│   │   │   │   ├── auth.rs           # JWT validation
│   │   │   │   ├── rate_limit.rs     # Per-user/team rate limiting
│   │   │   │   ├── team_access.rs    # Team permission checks
│   │   │   │   └── plan_gate.rs      # Feature gating by plan
│   │   │   ├── models/               # MongoDB models (serde structs)
│   │   │   ├── services/             # Business logic
│   │   │   └── db.rs                 # MongoDB connection pool
│   │   └── Cargo.toml
│   │
│   ├── cloud-relay/                  # WebSocket relay server
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── connection.rs         # Per-server WebSocket handler
│   │   │   ├── registry.rs           # Active connection registry
│   │   │   ├── commands.rs           # Forward commands to servers
│   │   │   ├── metrics.rs            # Metric ingestion pipeline
│   │   │   └── logs.rs               # Log streaming pipeline
│   │   └── Cargo.toml
│   │
│   ├── cloud-worker/                 # Background job processor
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── jobs/
│   │   │   │   ├── ai.rs             # AI diagnosis, optimization
│   │   │   │   ├── deploy.rs         # Build + deploy pipeline
│   │   │   │   ├── alert.rs          # Alert evaluation + dispatch
│   │   │   │   ├── incident.rs       # Auto-create incidents
│   │   │   │   ├── log_ingest.rs     # Batch write to R2 + ClickHouse
│   │   │   │   ├── metric_ingest.rs  # Batch write to ClickHouse
│   │   │   │   ├── compliance.rs     # Report generation
│   │   │   │   ├── backup.rs         # Snapshot to R2
│   │   │   │   └── plugin.rs         # Plugin security scan
│   │   │   └── queue.rs              # Redis job consumer
│   │   └── Cargo.toml
│   │
│   ├── cloud-scheduler/              # Cron job service
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── autoscale.rs          # Evaluate autoscale rules
│   │   │   ├── health.rs             # Server heartbeat monitoring
│   │   │   ├── cost.rs               # Cost recalculation
│   │   │   ├── retention.rs          # Log/metric cleanup
│   │   │   ├── billing.rs            # Usage metering → Stripe
│   │   │   └── certs.rs              # Certificate expiry checks
│   │   └── Cargo.toml
│   │
│   └── cloud-shared/                 # Shared types, DB, auth
│       ├── src/
│       │   ├── lib.rs
│       │   ├── models.rs             # All MongoDB document types
│       │   ├── db.rs                 # MongoDB + Redis + ClickHouse clients
│       │   ├── auth.rs               # JWT creation/validation
│       │   ├── billing.rs            # Stripe helpers
│       │   ├── email.rs              # Resend helpers
│       │   └── config.rs             # Environment config
│       └── Cargo.toml
│
├── frontend/                         # SvelteKit app
│   ├── src/
│   │   ├── routes/
│   │   │   ├── +layout.svelte
│   │   │   ├── dashboard/
│   │   │   ├── fleet/
│   │   │   ├── servers/
│   │   │   ├── deployments/
│   │   │   ├── incidents/
│   │   │   ├── cost/
│   │   │   ├── autoscale/
│   │   │   ├── logs/
│   │   │   ├── alerts/
│   │   │   ├── secrets/
│   │   │   ├── plugins/
│   │   │   ├── compliance/
│   │   │   ├── settings/
│   │   │   └── auth/
│   │   ├── lib/
│   │   │   ├── api.ts                # API client
│   │   │   ├── ws.ts                 # WebSocket client
│   │   │   ├── stores/               # Svelte stores
│   │   │   └── components/
│   │   │       ├── FleetMap.svelte
│   │   │       ├── DeployPipeline.svelte
│   │   │       ├── WarRoom.svelte
│   │   │       ├── CostOptimizer.svelte
│   │   │       ├── AutoscaleChart.svelte
│   │   │       ├── LogViewer.svelte
│   │   │       └── MetricsChart.svelte
│   │   └── app.css
│   ├── package.json
│   └── svelte.config.js
│
├── edge/                             # Cloudflare Workers
│   ├── geo-router/
│   ├── rate-limiter/
│   └── status-renderer/
│
├── migrations/                       # MongoDB migrations
├── scripts/                          # Deployment scripts
├── mhost-cloud.toml                  # mhost config for self-hosting
├── railway.toml                      # Railway project config
└── .github/workflows/deploy.yml      # CI/CD using mhost GitHub Action
```

## Security

| Concern | Solution |
|---|---|
| Authentication | JWT with short-lived access tokens (15min) + refresh tokens (30d) |
| Authorization | Team-level RBAC (admin/developer/viewer) checked per-request |
| API Keys | Argon2 hashed, scoped to team, revocable |
| Secrets | AES-256-GCM encrypted at rest, decrypted only during deploy |
| WebSocket Auth | Server registration token verified on connect |
| Rate Limiting | Redis sliding window per user/team/IP |
| DDoS | Cloudflare WAF + rate limiting at edge |
| CSRF | SameSite cookies + token validation |
| XSS | CSP headers + SvelteKit auto-escaping |
| SQL Injection | N/A (MongoDB with Rust driver — no raw queries) |
| Audit | Every state change logged with user, timestamp, IP |
| 2FA | TOTP (Google Authenticator / Authy) |
| SSO | SAML 2.0 for Enterprise (via third-party like WorkOS) |
| Data Encryption | TLS everywhere, R2 encryption at rest |
| Compliance | SOC 2 Type II readiness, GDPR data handling |
