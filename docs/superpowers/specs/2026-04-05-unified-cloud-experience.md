# mhost Unified Cloud Experience — CLI ↔ Dashboard ↔ Agent

## The Core Concept

mhost Cloud is not a separate product — it's an upgrade to the same CLI. Free features work locally. When you subscribe, cloud features **unlock inside the same CLI** you already use. No new binary, no separate tool.

```
mhost (free)                          mhost (subscribed)
─────────────                         ──────────────────
mhost start server.js                 mhost start server.js          ← same
mhost list                            mhost list                     ← same
mhost logs api                        mhost logs api                 ← same
mhost ai diagnose api                 mhost ai diagnose api          ← uses YOUR key
                                      
✗ mhost deploy --provider aws         mhost deploy --provider aws    ← UNLOCKED
✗ mhost agent chat                    mhost agent chat               ← UNLOCKED (metered)
✗ mhost cloud open                    mhost cloud open               ← UNLOCKED
✗ mhost grafana                       mhost grafana                  ← UNLOCKED
✗ mhost terraform export              mhost terraform export         ← UNLOCKED
✗ mhost secrets set DB_URL ...        mhost secrets set DB_URL ...   ← UNLOCKED (vault)
```

## Subscription Gating in CLI

When a user runs a cloud command without subscription:

```bash
$ mhost deploy --provider aws

  ┌─────────────────────────────────────────────────┐
  │  This feature requires mhost Cloud (Pro plan)    │
  │                                                   │
  │  What you get:                                    │
  │  ✓ Deploy to 10 cloud providers                  │
  │  ✓ AI agent with 500 tokens/mo                   │
  │  ✓ Live fleet map + dashboard                    │
  │  ✓ Centralized logs + metrics                    │
  │  ✓ Smart autoscaling                             │
  │                                                   │
  │  Start free trial: mhost cloud subscribe         │
  │  Or login: mhost login                           │
  └─────────────────────────────────────────────────┘
```

Implementation:
```rust
fn require_cloud(paths: &MhostPaths) -> Result<CloudAuth, String> {
    let auth = CloudAuth::load(&paths.cloud_auth())?;
    if auth.plan == "free" || auth.plan.is_empty() {
        print_upgrade_prompt();
        return Err("Cloud subscription required".into());
    }
    Ok(auth)
}
```

## `mhost connect` — The Bridge

This is the most important command. It connects a local project to the cloud.

```bash
$ cd my-nextjs-app
$ mhost connect

  mhost Cloud Connect
  ─────────────────────────────────────
  
  Scanning project...
  ├── Detected: Next.js (package.json)
  ├── Framework: Next.js 14
  ├── Entry: npm run start
  ├── Port: 3000
  ├── Git: github.com/maher/my-nextjs-app (main branch)
  └── Env: .env.local (12 variables)

  Project linked to mhost Cloud:
  ├── Project ID: proj_a1b2c3
  ├── Team: Maher's Team
  └── Config: ~/.mhost/cloud-project.json

  What would you like to do?
  1) Deploy now (choose provider)
  2) Open dashboard
  3) Set up CI/CD (auto-deploy on push)
  4) Just connect (configure later)

  Choice: 1

  Select provider:
  1) Railway ($5/mo)         — simplest, zero config
  2) AWS ECS ($35/mo)        — production, scalable
  3) GCP Cloud Run ($12/mo)  — serverless, auto-scale
  4) Vercel ($0/mo)          — optimized for Next.js
  5) Fly.io ($7/mo)          — edge, multi-region

  Provider: 4

  Deploying to Vercel...
  ✓ Project created on Vercel
  ✓ GitHub repo connected
  ✓ Environment variables synced from .env.local
  ✓ First deployment triggered
  ✓ Build: npm run build (23s)
  ✓ Live: https://my-nextjs-app.vercel.app
  ✓ CI/CD: push to main → auto-deploy

  Dashboard: https://app.mhostai.com/projects/proj_a1b2c3
```

### What `mhost connect` creates

File: `~/.mhost/cloud-project.json`
```json
{
  "project_id": "proj_a1b2c3",
  "team_id": "team_xxx",
  "name": "my-nextjs-app",
  "repo": "github.com/maher/my-nextjs-app",
  "branch": "main",
  "framework": "nextjs",
  "providers": [
    {
      "name": "vercel",
      "service_id": "prj_xxx",
      "url": "https://my-nextjs-app.vercel.app",
      "status": "live"
    }
  ],
  "secrets": ["DB_URL", "API_KEY", "REDIS_URL"],
  "ci_cd": {
    "enabled": true,
    "trigger": "push_to_main",
    "pipeline": ["build", "test", "deploy"]
  },
  "monitoring": {
    "metrics": true,
    "logs": true,
    "alerts": true
  },
  "connected_at": "2026-04-05T12:00:00Z"
}
```

## Full CLI Command Tree (Cloud-Gated)

### Always Free (local)
```bash
mhost start/stop/restart/delete/list/info/env/scale/reload
mhost logs/health/config/history
mhost monit/dashboard
mhost ai diagnose/optimize/ask (own API key)
mhost brain status/history/playbooks
mhost notify setup/list/test
mhost bot setup/enable/status
mhost dev/save/resurrect/startup
mhost docker run/list/stop/logs
mhost plugin install/list/remove
mhost template list/init
mhost init
mhost cron/limits/audit
```

### Requires Login (free tier)
```bash
mhost login                          # Auth to mhost Cloud
mhost logout
mhost connect                        # Link project to cloud
mhost disconnect
mhost cloud status                   # Show connection status
mhost cloud open                     # Open dashboard in browser
```

### Requires Pro ($29/mo)
```bash
# Cloud Deploy
mhost deploy --provider <p>          # Build + push + deploy to any provider
mhost deploy --provider aws --type ecs
mhost deploy --provider railway
mhost deploy --provider vercel
mhost deploy --provider gcp --type cloudrun

# Cloud Management
mhost cloud services                 # List all cloud services
mhost cloud scale <service> <n>      # Scale cloud service
mhost cloud restart <service>
mhost cloud destroy <service>
mhost cloud logs <service>           # Cloud-aggregated logs
mhost cloud metrics <service>        # Cloud-aggregated metrics

# AI Agent (metered)
mhost agent chat                     # Interactive AI DevOps agent
mhost agent run "<command>"          # One-shot agent command
mhost agent tokens                   # Show remaining tokens

# Secrets Vault
mhost secrets set <key> <value>      # Cloud-encrypted vault
mhost secrets list
mhost secrets rotate <key>

# CI/CD
mhost ci setup                       # Auto-create GitHub Actions
mhost ci status                      # Show pipeline status
mhost ci logs                        # Show build logs

# Monitoring
mhost grafana                        # Open Grafana dashboard
mhost grafana create <name>          # Create custom dashboard

# Cost
mhost cloud cost                     # Cost breakdown
mhost cloud cost optimize            # AI cost recommendations

# Export
mhost terraform export               # Generate Terraform files
mhost terraform plan                 # Show what would change
mhost terraform apply                # Apply Terraform state
mhost compose export                 # Generate docker-compose.yml
mhost k8s export                     # Generate K8s manifests
```

### Requires Team ($99/mo)
```bash
mhost cloud team invite <email> --role <role>
mhost cloud team list
mhost cloud team remove <email>
mhost cloud incidents                # List incidents
mhost cloud incidents <id> war-room  # Open war room
mhost cloud autoscale setup          # Configure smart autoscaling
mhost cloud autoscale status         # Show scaling history
mhost cloud status-page create       # Create public status page
mhost cloud compliance report        # Generate compliance report
```

### Requires Enterprise ($499/mo)
```bash
mhost cloud sso setup                # Configure SAML SSO
mhost cloud audit export             # Export full audit trail
mhost cloud self-host                # Download self-hostable platform
```

## Grafana Integration

Built-in managed Grafana that auto-configures from your mhost data:

```bash
$ mhost grafana

  Opening Grafana dashboard...
  URL: https://app.mhostai.com/grafana/proj_a1b2c3
  
  Auto-configured dashboards:
  ├── Overview: all services health + metrics
  ├── API: request rate, latency, errors
  ├── Worker: job throughput, queue depth
  └── Database: connections, query time, disk
```

**How it works:**
- mhost Cloud runs a managed Grafana instance per team
- When you `mhost connect`, it auto-creates:
  - ClickHouse data source (your metrics)
  - Dashboard per service (CPU, memory, requests, errors)
  - Alert rules matching your mhost alert config
- You can create custom dashboards in the Grafana UI
- Dashboards embedded in the mhost Cloud dashboard via iframe
- `mhost grafana create <name>` creates a dashboard from CLI

**Dashboard templates:**
| Template | Panels |
|---|---|
| **API Service** | Request rate, latency (p50/p95/p99), error rate, status codes, active connections |
| **Worker** | Jobs processed/s, queue depth, job duration, failure rate |
| **Database** | Active connections, query time, rows read/written, disk usage |
| **Infrastructure** | CPU per instance, memory per instance, network I/O, disk I/O |
| **Cost** | Daily spend, per-service cost, month projection, savings from autoscaling |
| **SLA** | Uptime %, incident count, MTTR, error budget remaining |

## Terraform State Management

mhost Cloud can be the Terraform backend, or export to your existing Terraform:

### Option 1: mhost manages Terraform state
```bash
$ mhost terraform export

  Generated: mhost-infra.tf (47 resources)
  
  Resources:
  ├── aws_ecs_cluster.myapp
  ├── aws_ecs_service.api (2 instances)
  ├── aws_ecs_service.worker (1 instance)
  ├── aws_rds_instance.postgres
  ├── aws_elasticache_cluster.redis
  ├── aws_lb.api_alb
  ├── aws_lb_target_group.api
  ├── aws_security_group.api
  ├── aws_iam_role.ecs_task
  └── ... 38 more resources

  State stored in: mhost Cloud (s3-compatible backend)
  
$ mhost terraform plan

  Plan: 0 to add, 0 to change, 0 to destroy.
  (Infrastructure matches desired state)

$ mhost terraform apply

  Apply complete! Resources: 0 added, 0 changed, 0 destroyed.
```

### Option 2: Export to your own Terraform
```bash
$ mhost terraform export --output ./infra/

  Written:
  ├── infra/main.tf          (provider + resources)
  ├── infra/variables.tf     (configurable values)
  ├── infra/outputs.tf       (endpoints, IDs)
  └── infra/terraform.tfvars (current values)

  Use with your own backend:
  $ cd infra && terraform init && terraform plan
```

## Secret Management (Full)

Cloud-grade secrets vault, accessible from CLI and dashboard:

```bash
# Set a secret (encrypted, stored in cloud)
$ mhost secrets set DATABASE_URL "postgres://user:pass@host:5432/db"
  ✓ Secret 'DATABASE_URL' saved (v1)
  ✓ Synced to: api-server, worker (2 services)

# List secrets (values masked)
$ mhost secrets list
  
  Name              Scope          Version   Last Rotated
  ────────────────────────────────────────────────────────
  DATABASE_URL      api,worker     v3        14 days ago
  REDIS_URL         api,worker     v1        30 days ago
  API_KEY           api            v2        7 days ago
  STRIPE_KEY        api            v1        60 days ago ⚠️

  ⚠️ STRIPE_KEY hasn't been rotated in 60 days

# Rotate a secret
$ mhost secrets rotate STRIPE_KEY
  Enter new value: sk_live_xxx...
  ✓ Secret 'STRIPE_KEY' rotated (v2)
  ✓ Redeploying api-server with new secret...
  ✓ Health check passing

# Auto-rotation schedule
$ mhost secrets auto-rotate DATABASE_URL --every 90d
  ✓ Auto-rotation enabled for DATABASE_URL (every 90 days)
  Next rotation: 2026-07-04

# Sync secrets to a specific provider
$ mhost secrets sync --to railway
  ✓ Synced 4 secrets to Railway environment variables

# Leak detection
$ mhost secrets scan
  Scanning logs and code for leaked secrets...
  ⚠️ WARNING: DATABASE_URL pattern found in:
    - logs/api-server-0-out.log (line 4521)
    - Recommendation: rotate immediately
```

**How secrets flow:**
```
CLI: mhost secrets set X Y
  │
  ├─► Cloud API (encrypted at rest with AES-256-GCM)
  │   ├── Stored in MongoDB (encrypted value)
  │   ├── Version history maintained
  │   └── Audit log entry created
  │
  ├─► Synced to providers:
  │   ├── Railway: service variables API
  │   ├── AWS: Secrets Manager / ECS task env
  │   ├── Vercel: environment variables API
  │   ├── Fly.io: fly secrets set
  │   └── etc.
  │
  └─► Dashboard shows masked values
      └── Only "Reveal" with re-authentication
```

## CI/CD Pipeline

Automatic CI/CD from git push:

```bash
$ mhost ci setup

  CI/CD Setup
  ─────────────────────────────────────
  
  Repo: github.com/maher/my-nextjs-app
  Branch: main
  Provider: Vercel
  
  Pipeline:
  1. Push to main
  2. mhost builds Docker image
  3. Run tests (npm test)
  4. Deploy to staging (auto)
  5. Health check passes → deploy to production
  6. If fails → auto-rollback + alert
  
  GitHub Actions workflow will be created:
  .github/workflows/mhost-deploy.yml
  
  Create? [yes/no]: yes
  
  ✓ GitHub Actions workflow created
  ✓ Webhook configured
  ✓ First deploy will trigger on next push
```

Generated workflow:
```yaml
# .github/workflows/mhost-deploy.yml
name: mhost Deploy
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
          api-key: ${{ secrets.MHOST_API_KEY }}
          command: deploy --provider vercel
          
      - name: Health Check
        run: mhost cloud health api-server --wait 60s
        
      - name: Notify
        if: always()
        run: mhost cloud notify "${{ job.status }}"
```

```bash
# Check CI status
$ mhost ci status

  Pipeline: main → my-nextjs-app
  ─────────────────────────────────────
  #23  ● Build succeeded    2m ago     abc1234 "fix: login redirect"
  #22  ● Deploy succeeded   1h ago     def5678 "feat: add dashboard"
  #21  ✖ Tests failed       3h ago     ghi9012 "refactor: api routes"
  #20  ● Deploy succeeded   5h ago     jkl3456 "update deps"

# View build logs
$ mhost ci logs 23

  [12:00:01] Cloning repo...
  [12:00:03] Installing dependencies...
  [12:00:15] Building...
  [12:00:38] Running tests...
  [12:00:45] All 47 tests passed ✓
  [12:00:46] Deploying to Vercel...
  [12:01:02] Health check passing ✓
  [12:01:03] Deploy complete!
```

## The Complete Flow (Real-World Example)

```bash
# Day 1: Start a new project
$ npx create-next-app my-saas
$ cd my-saas

# Connect to mhost Cloud
$ mhost login                    # One-time
$ mhost connect                  # Links project
# Choose: Vercel for frontend

# Add a backend API
$ mhost agent chat
> "I need a Node.js API with Postgres and Redis, deploy to Railway"
# Agent creates everything, deploys, configures secrets

# Set up secrets
$ mhost secrets set STRIPE_KEY sk_live_xxx
$ mhost secrets set DATABASE_URL postgres://...   # Auto-synced to all services

# Set up CI/CD
$ mhost ci setup                 # Creates GitHub Actions workflow

# Set up monitoring
$ mhost grafana                  # Auto-configured dashboards

# Day 2: Push code
$ git push origin main
# → GitHub Actions → mhost builds → tests → deploys → health check → live

# Day 7: Traffic grows
$ mhost agent chat
> "my API is getting slow during peak hours, set up autoscaling"
# Agent configures predictive autoscaling, shows cost estimate

# Day 30: Check costs
$ mhost cloud cost
# Shows breakdown, agent suggests: "Move Redis to Upstash, save $12/mo"

# Day 60: Add staging environment
$ mhost agent chat
> "create a staging environment identical to production"
# Agent clones infrastructure, sets up staging secrets, configures deploy-on-PR

# Day 90: Export to Terraform (for compliance)
$ mhost terraform export --output ./infra/
# Full Terraform files generated from your running infrastructure
```

## What Makes This Different From Vercel/Railway/Heroku

| | Vercel | Railway | Heroku | **mhost Cloud** |
|---|---|---|---|---|
| Lock-in | Vercel only | Railway only | Heroku only | **Any provider** |
| Multi-cloud | No | No | No | **Yes, 10 providers** |
| AI agent | No | No | No | **Full DevOps agent** |
| CLI-first | Partial | Partial | Yes | **Yes, everything** |
| Self-hostable | No | No | No | **Enterprise plan** |
| Open source CLI | No | No | No | **MIT licensed** |
| Docker gen | No | Yes (nixpacks) | Yes (buildpacks) | **AI-optimized per language** |
| Grafana | No | No | No | **Built-in managed** |
| Terraform | No | No | No | **Full export + state** |
| Secret rotation | Basic | Basic | No | **Auto-rotate + audit + leak detection** |
| Fleet map | No | No | No | **Live world map** |
| War rooms | No | No | No | **Auto-created incidents** |
| Cost optimizer | No | No | No | **AI cross-provider optimization** |
| Predictive scale | No | No | No | **ML-based traffic prediction** |
