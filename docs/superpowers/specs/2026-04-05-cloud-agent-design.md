# mhost Cloud Agent — AI DevOps Engineer

## Vision

The mhost Cloud Agent is a conversational AI that acts as a full DevOps engineer. It reads your code, generates Dockerfiles, builds images, provisions infrastructure on any cloud provider, deploys, scales, monitors, fixes, and migrates — all through natural language commands or chat in the dashboard.

Every action is visualized on a live infrastructure map.

## What The Agent Can Do

### 1. Code → Docker → Deploy (Zero Config)

```
You: "deploy my app to aws"

Agent: Analyzing your repository github.com/you/myapp...
  
  Detected: Node.js (package.json)
  ├── Framework: Express.js
  ├── Entry: server.js
  ├── Port: 3000
  ├── Dependencies: 47 packages
  └── Database: PostgreSQL (detected from DATABASE_URL env)

  I'll create the following:

  1. Dockerfile (multi-stage, optimized)
     FROM node:20-alpine AS builder
     WORKDIR /app
     COPY package*.json ./
     RUN npm ci --production
     COPY . .
     FROM node:20-alpine
     WORKDIR /app
     COPY --from=builder /app .
     EXPOSE 3000
     CMD ["node", "server.js"]
     
  2. AWS ECS Fargate service
     ├── Cluster: myapp-cluster (us-east-1)
     ├── Task: 0.5 vCPU, 1GB RAM
     ├── Instances: 2
     ├── ALB with health check /health
     └── Auto-scaling: CPU > 70% → max 6

  3. RDS PostgreSQL
     ├── db.t3.micro
     ├── 20GB storage
     └── DATABASE_URL auto-injected

  4. ECR Repository for Docker images

  Estimated cost: $47/mo
  Proceed? [yes/no]

You: "yes"

Agent: Working...
  ✓ Dockerfile generated and committed to repo
  ✓ ECR repository created: myapp
  ✓ Docker image built (node:20-alpine, 89MB)
  ✓ Image pushed to ECR: 123456.dkr.ecr.us-east-1.amazonaws.com/myapp:v1
  ✓ ECS cluster created: myapp-cluster
  ✓ RDS PostgreSQL provisioned (endpoint: myapp-db.xxx.rds.amazonaws.com)
  ✓ Task definition registered
  ✓ ECS service created with ALB
  ✓ Health check passing: https://myapp-alb-xxx.us-east-1.elb.amazonaws.com/health
  ✓ Auto-scaling configured (2-6 instances)
  ✓ Secrets configured: DATABASE_URL
  ✓ Monitoring + alerts enabled

  Your app is live!
  URL: https://myapp-alb-xxx.us-east-1.elb.amazonaws.com
  Dashboard: https://app.mhostai.com/servers/srv_xxx
```

### 2. Multi-Cloud Orchestration

```
You: "I want my API on AWS us-east-1 and a replica on GCP europe-west1 
      with a global load balancer"

Agent: Multi-region deployment plan:

  Region 1: AWS ECS (us-east-1)
  ├── ECS Fargate: 2 instances
  ├── ALB with /health check
  └── RDS PostgreSQL (primary)

  Region 2: GCP Cloud Run (europe-west1)
  ├── Cloud Run: 2 instances
  ├── Health check: /health
  └── Cloud SQL PostgreSQL (read replica)

  Global Load Balancer: Cloudflare
  ├── DNS: api.myapp.com
  ├── Routing: geo-based (EU → GCP, US → AWS)
  ├── Failover: if one region down → route all to other
  └── SSL: auto-managed

  Database Sync:
  ├── AWS RDS (primary, read-write)
  └── GCP Cloud SQL (replica, read-only)
  
  Estimated cost: $156/mo
  Proceed?
```

### 3. Infrastructure from GitHub Repo

```
You: "connect my repo github.com/myorg/platform and set up everything"

Agent: Scanning repository...
  
  Detected monorepo structure:
  ├── services/api/         → Node.js Express API
  ├── services/worker/      → Python Celery worker
  ├── services/frontend/    → React SPA
  ├── docker-compose.yml    → 3 services defined
  └── .env.example          → 12 env vars needed

  Proposed infrastructure:

  ┌─────────────────────────────────────────────┐
  │              Cloudflare CDN                  │
  │  api.myapp.com    app.myapp.com              │
  └──────┬─────────────────┬────────────────────┘
         │                 │
  ┌──────▼──────┐  ┌───────▼───────┐
  │  API (ECS)   │  │ Frontend (S3) │
  │  2 instances │  │ + CloudFront  │
  │  port 3000   │  └───────────────┘
  └──────┬───────┘
         │
  ┌──────▼──────┐  ┌───────────────┐
  │ Worker (ECS) │  │ Redis (EC)    │
  │ 1 instance   │  │ cache.r6g     │
  └──────┬───────┘  └───────────────┘
         │
  ┌──────▼──────┐
  │ PostgreSQL   │
  │ RDS db.t3    │
  └──────────────┘

  I'll create:
  1. 3 Dockerfiles (api, worker, frontend)
  2. ECR repos for each
  3. ECS cluster with 3 services
  4. RDS PostgreSQL + ElastiCache Redis
  5. S3 + CloudFront for frontend
  6. Cloudflare DNS + SSL
  7. CI/CD: push to main → auto-deploy all
  8. Monitoring + alerts for all services
  
  Total estimated cost: $89/mo
  Proceed?
```

### 4. Manage Existing Infrastructure

```
You: "show me what's running"

Agent: Here's your infrastructure:

  ┌─ AWS us-east-1 ────────────────────────────┐
  │                                              │
  │  ● api-server    online   2 inst   $34/mo   │
  │  ● worker        online   1 inst   $12/mo   │
  │  ◐ cron-job      running  1 inst   $5/mo    │
  │  ● postgres      healthy  1 inst   $28/mo   │
  │  ● redis         healthy  1 inst   $15/mo   │
  │                                              │
  │  Total: $94/mo                               │
  └──────────────────────────────────────────────┘

  ┌─ Railway ──────────────────────────────────┐
  │                                              │
  │  ● staging-api   online   1 inst   $5/mo    │
  │  ● staging-db    healthy  1 inst   $7/mo    │
  │                                              │
  │  Total: $12/mo                               │
  └──────────────────────────────────────────────┘

  Fleet: 7 services, 2 providers, $106/mo total
  Health: 100% (all passing)
  
You: "my api is slow, what's wrong?"

Agent: Investigating api-server on AWS...

  Metrics (last 1h):
  ├── CPU: 87% avg (↑ from 34% yesterday)
  ├── Memory: 412MB / 512MB (80%)
  ├── Response time: 1.2s avg (↑ from 200ms)
  └── Error rate: 2.3% (↑ from 0.1%)

  Logs analysis:
  ├── 47 "connection pool exhausted" errors
  ├── 12 "ETIMEDOUT" on database queries
  └── Pattern: started 45min ago

  Root cause: Database connection pool exhaustion.
  Your API has 2 instances but max_connections=10.
  With current load (340 req/s), you need ~25 connections.

  Recommended actions:
  1. Increase DB pool size to 25 (env var change)
  2. Scale API to 3 instances (handle current load)
  3. Add connection pooler (PgBouncer) for long-term fix

  Execute fix #1 and #2 now? [yes/no]

You: "yes and also add auto-scaling"

Agent: Executing...
  ✓ DATABASE_POOL_SIZE set to 25 (secret updated)
  ✓ API scaled to 3 instances
  ✓ Redeploying with new env...
  ✓ Health check passing, response time back to 180ms
  ✓ Auto-scale rule created: CPU > 70% → scale up (max 6)
  
  Crisis resolved. Added to brain playbook for future auto-fix.
```

### 5. Docker Image Generation

The agent can generate optimized Dockerfiles for any language:

| Language | Detection | Dockerfile Strategy |
|---|---|---|
| **Node.js** | package.json | Multi-stage, npm ci --production, alpine |
| **Python** | requirements.txt, pyproject.toml | Multi-stage, pip install, slim |
| **Rust** | Cargo.toml | Multi-stage, cargo build --release, scratch |
| **Go** | go.mod | Multi-stage, go build, scratch |
| **Java** | pom.xml, build.gradle | Multi-stage, maven/gradle build, JRE slim |
| **Ruby** | Gemfile | Multi-stage, bundle install, alpine |
| **PHP** | composer.json | Multi-stage, composer install, php-fpm |
| **.NET** | *.csproj | Multi-stage, dotnet publish, aspnet |
| **Static** | index.html | nginx:alpine, copy files |
| **Monorepo** | Multiple services | Per-service Dockerfile + docker-compose |

For each, the agent:
1. Reads project files to understand dependencies
2. Generates optimized Dockerfile (smallest image, proper layer caching)
3. Generates .dockerignore
4. Optionally commits to repo via PR
5. Builds image locally or via cloud builder
6. Pushes to registry (ECR, GCR, Docker Hub, GHCR)

### 6. Provider-Specific Infrastructure

The agent knows how to set up full infrastructure on each provider:

**AWS:**
```
ECS Fargate | EKS | EC2 | Lambda | App Runner
RDS (Postgres/MySQL) | DynamoDB | ElastiCache (Redis)
S3 + CloudFront | ALB | Route53
ECR | SQS | SNS | CloudWatch
VPC + Security Groups + IAM Roles
```

**GCP:**
```
Cloud Run | GKE | Compute Engine | Cloud Functions | App Engine
Cloud SQL | Firestore | Memorystore (Redis)
Cloud Storage + CDN | Load Balancer | Cloud DNS
Artifact Registry | Pub/Sub | Cloud Monitoring
VPC + Firewall Rules + Service Accounts
```

**Azure:**
```
Container Instances | AKS | VMs | Functions | Container Apps
Azure Database (Postgres/MySQL) | CosmosDB | Azure Cache (Redis)
Blob Storage + CDN | Application Gateway | Azure DNS
ACR | Service Bus | Azure Monitor
VNet + NSG + Managed Identities
```

**Railway:**
```
Services | Databases (Postgres, MySQL, Redis, MongoDB)
Volumes | Cron Jobs | TCP Proxies
Custom Domains | Environment Variables
```

**Fly.io:**
```
Machines | Volumes | Postgres (managed)
Redis (managed) | Custom Domains | Certificates
Multi-region | Anycast
```

**Vercel / Netlify / Cloudflare:**
```
Serverless Functions | Edge Functions | Static Sites
KV Storage | D1 (SQLite) | R2 (S3)
Custom Domains | Analytics
```

## Agent Tools (LLM Function Calling)

The cloud agent gets 40+ tools organized by category:

### Code Analysis
| Tool | Description |
|---|---|
| `scan_repo` | Scan GitHub repo, detect language/framework/dependencies |
| `read_file` | Read a specific file from the connected repo |
| `list_files` | List files in repo directory |
| `detect_services` | Detect microservices in monorepo |

### Docker
| Tool | Description |
|---|---|
| `generate_dockerfile` | Generate optimized Dockerfile for detected language |
| `generate_dockerignore` | Generate .dockerignore |
| `generate_compose` | Generate docker-compose.yml for multi-service |
| `build_image` | Build Docker image (local or cloud builder) |
| `push_image` | Push image to registry (ECR/GCR/GHCR/Docker Hub) |
| `list_images` | List images in registry |

### Infrastructure Provisioning
| Tool | Description |
|---|---|
| `create_cluster` | Create container cluster (ECS/EKS/GKE/AKS) |
| `create_service` | Create service/deployment on cluster |
| `create_database` | Provision managed database (RDS/Cloud SQL/etc.) |
| `create_cache` | Provision Redis/Memcached |
| `create_storage` | Create object storage bucket |
| `create_cdn` | Set up CDN + custom domain |
| `create_loadbalancer` | Set up load balancer with health checks |
| `create_dns` | Configure DNS records |
| `create_ssl` | Provision SSL certificate |
| `create_vpc` | Set up VPC/network (auto for cloud providers) |
| `create_queue` | Create message queue (SQS/Pub-Sub) |

### Deployment
| Tool | Description |
|---|---|
| `deploy_service` | Deploy new version to a service |
| `rollback_service` | Rollback to previous version |
| `canary_deploy` | Deploy to subset, monitor, promote/rollback |
| `blue_green_deploy` | Deploy new version alongside old, switch traffic |
| `promote_canary` | Promote canary to full traffic |

### Management
| Tool | Description |
|---|---|
| `scale_service` | Scale up/down instances |
| `restart_service` | Restart all instances |
| `stop_service` | Stop a service |
| `destroy_service` | Tear down a service |
| `update_env` | Set/update environment variables |
| `rotate_secret` | Rotate a secret and redeploy |

### Observability
| Tool | Description |
|---|---|
| `get_metrics` | Get CPU/memory/request metrics |
| `get_logs` | Query logs with search |
| `get_health` | Check health status |
| `get_cost` | Get cost breakdown |
| `get_incidents` | Get recent incidents |
| `diagnose_issue` | AI-powered root cause analysis |

### GitHub Integration
| Tool | Description |
|---|---|
| `connect_repo` | Connect a GitHub repo via GitHub App |
| `setup_ci_cd` | Create GitHub Actions workflow for auto-deploy |
| `create_pr` | Create a PR (for Dockerfile, config changes) |
| `list_repos` | List connected repos |
| `get_commits` | Get recent commits for a repo |

### Fleet Visualization
| Tool | Description |
|---|---|
| `get_fleet_map` | Get all services with coordinates for map |
| `get_topology` | Get service dependency graph |
| `get_traffic_flow` | Get real-time request flow between services |

## Dashboard Chat UI

The agent lives in a chat panel on the right side of the dashboard:

```
┌──────────────────────────────────┬────────────────────────┐
│                                  │  mhost Agent           │
│  Fleet Map / Server Detail       │                        │
│  (main content area)             │  ● Online              │
│                                  │                        │
│  ┌─────────────────────────┐     │  Agent: What would you │
│  │ ● api-server (AWS)      │     │  like to do?           │
│  │ ● worker (AWS)          │     │                        │
│  │ ● frontend (Vercel)     │     │  You: deploy my app    │
│  │ ● db (RDS)              │     │  to railway             │
│  └─────────────────────────┘     │                        │
│                                  │  Agent: Scanning your  │
│  Metrics charts                  │  repo...               │
│  ┌─────────────────────────┐     │                        │
│  │ CPU ▁▂▃▅▇▅▃▂           │     │  [Progress bar]        │
│  │ MEM ▃▃▃▃▃▄▄▄           │     │                        │
│  └─────────────────────────┘     │  ✓ Dockerfile created  │
│                                  │  ✓ Image built         │
│                                  │  ✓ Deployed to Railway │
│                                  │                        │
│                                  │  ┌──────────────────┐  │
│                                  │  │ Type a message...│  │
│                                  │  └──────────────────┘  │
└──────────────────────────────────┴────────────────────────┘
```

Features:
- Real-time progress indicators for long-running operations
- Inline code blocks for generated Dockerfiles, configs
- Clickable service cards that navigate to server detail
- Infrastructure diagram rendering inline
- "Approve" / "Reject" buttons for provisioning actions
- History: full conversation saved per team
- Context: agent sees all your servers, processes, metrics, logs

## CLI Agent Mode

Same agent accessible from terminal:

```bash
# Interactive chat mode
$ mhost agent chat
  mhost Agent (Pro: 467/500 tokens remaining)
  Type 'exit' to quit

  > deploy github.com/me/myapp to aws ecs
  
  Scanning repository...
  [... same flow as dashboard ...]

# One-shot commands
$ mhost agent run "scale api to 4 instances on aws"
$ mhost agent run "show me my infrastructure cost"
$ mhost agent run "create a staging environment on railway"

# Autonomous mode (agent monitors and acts)
$ mhost agent start --mode autonomous
  Agent running in autonomous mode
  Monitoring 7 services across 2 providers
  Press Ctrl+C to stop
```

## Token Metering

| Action | Tokens |
|---|---|
| Simple query (status, list, cost) | 1 |
| Diagnosis (analyze logs + metrics) | 2 |
| Dockerfile generation | 2 |
| Infrastructure provisioning plan | 3 |
| Full deploy (scan + dockerfile + build + deploy) | 5 |
| Multi-region setup | 8 |
| Migration between providers | 5 |

Tokens are metered per LLM API call, not per user message. Tool executions (actual docker build, API calls) are free — only the AI reasoning costs tokens.
