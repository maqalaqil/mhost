# mhost Cloud-Native Provider Integrations — Design Spec

## Overview

Add direct SDK-level API integrations to 7 cloud providers (AWS, GCP, Azure, Railway, Fly.io, Vercel, DigitalOcean), replacing CLI shelling with native HTTP API calls. Unified `CloudAdapter` trait so the CLI and AI agent don't care which cloud you're on. Includes secrets management, cost tracking, drift detection, backup/failover, IaC export, multi-region deploy, and 12 new agent tools. Feature-flagged (`--features cloud-native`) so the default binary stays lean.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        mhost CLI / Agent                        │
│  cloud provision | cloud deploy | cloud scale | cloud logs      │
└──────────────────────────┬──────────────────────────────────────┘
                           │ unified CloudAdapter trait
┌──────────────────────────▼──────────────────────────────────────┐
│                     Cloud Adapter Layer                          │
│                                                                  │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌──────────┐ │
│  │   AWS    │ │  Azure  │ │   GCP   │ │ Railway │ │  Fly.io  │ │
│  │ECS/EKS/ │ │AKS/ACI/ │ │GKE/Run/ │ │         │ │          │ │
│  │EC2/Lambda│ │ VM/Func │ │GCE/Func │ │         │ │          │ │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └──────────┘ │
│  ┌─────────┐ ┌─────────┐ ┌──────────┐ ┌─────────┐ ┌──────────┐ │
│  │  Vercel │ │   DO    │ │Cloudflare│ │ Netlify │ │ Supabase │ │
│  │Edge/Sls │ │App/Drop │ │Workers/  │ │Func/Edge│ │Edge/DB   │ │
│  │         │ │  /Func  │ │Pages/D1  │ │  /Sites │ │          │ │
│  └─────────┘ └─────────┘ └──────────┘ └─────────┘ └──────────┘ │
└──────────────────────────┬──────────────────────────────────────┘
                           │ HTTP APIs (reqwest)
                    Cloud Provider APIs
```

### Key decisions

- **`CloudAdapter` async trait** — unified interface across all providers. CLI and agent use one set of commands that work everywhere.
- **All API calls via reqwest** — no CLI shelling. No `aws`, `az`, `gcloud` binaries required.
- **Feature-flagged** — `--features cloud-native` in Cargo. Default binary has zero cloud-native code. Existing SSH fleet code stays unconditional.
- **Extends existing `mhost-cloud` crate** — new adapter modules alongside existing SSH fleet.
- **Credential management** — env vars or `~/.mhost/cloud-credentials.json` with per-provider keys.
- **Resource abstraction** — everything maps to `CloudService` struct regardless of provider.

## CloudAdapter Trait

```rust
#[async_trait]
pub trait CloudAdapter: Send + Sync {
    fn provider_name(&self) -> &str;
    
    // Discovery
    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError>;
    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError>;
    
    // Provisioning
    async fn provision(&self, spec: ProvisionSpec) -> Result<CloudService, CloudError>;
    async fn destroy(&self, name: &str) -> Result<(), CloudError>;
    
    // Day-2 Operations
    async fn deploy(&self, name: &str, config: DeployConfig) -> Result<CloudService, CloudError>;
    async fn scale(&self, name: &str, instances: u32) -> Result<(), CloudError>;
    async fn restart(&self, name: &str) -> Result<(), CloudError>;
    
    // Observability
    async fn logs(&self, name: &str, lines: usize) -> Result<Vec<String>, CloudError>;
    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError>;
    async fn metrics(&self, name: &str) -> Result<ServiceMetrics, CloudError>;
    
    // Cost
    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError>;
}
```

## Core Types

```rust
pub struct CloudService {
    pub name: String,
    pub provider: String,
    pub service_type: ServiceType,
    pub region: String,
    pub status: ServiceStatus,
    pub instances: u32,
    pub url: Option<String>,
    pub image: Option<String>,
    pub resources: Resources,
    pub created_at: DateTime<Utc>,
    pub provider_id: String,
}

pub struct ProvisionSpec {
    pub name: String,
    pub service_type: ServiceType,
    pub region: String,
    pub image: Option<String>,
    pub command: Option<String>,
    pub instances: u32,
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub env: HashMap<String, String>,
    pub port: Option<u16>,
    pub public: bool,
}

pub enum ServiceType {
    Container,    // ECS Fargate, Cloud Run, Railway, Fly Machines, Azure ACI
    Kubernetes,   // EKS, GKE, AKS
    VM,           // EC2, GCE, Azure VM, DO Droplet
    Serverless,   // Lambda, Cloud Functions, Vercel, CF Workers, Netlify Functions
    AppRunner,    // AWS App Runner, Azure Container Apps, GCP App Engine
    EdgeFunction, // Cloudflare Workers, Vercel Edge, Netlify Edge, Supabase Edge
    StaticSite,   // Vercel, Netlify, Cloudflare Pages
}

pub enum ServiceStatus {
    Running,
    Stopped,
    Deploying,
    Failed,
    Unknown,
}

pub struct DeployConfig {
    pub image: Option<String>,
    pub command: Option<String>,
    pub env: HashMap<String, String>,
    pub port: Option<u16>,
    pub health_check: Option<String>,
}

pub struct Resources {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub disk: Option<String>,
}

pub struct CostEstimate {
    pub hourly: f64,
    pub monthly: f64,
    pub currency: String,
    pub breakdown: Vec<CostLine>,
}

pub struct CostLine {
    pub item: String,
    pub amount: f64,
}

pub struct ServiceMetrics {
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub requests_per_sec: Option<f64>,
    pub error_rate: Option<f64>,
}
```

## Provider Implementations

| Provider | Services | API | Auth Env Vars |
|---|---|---|---|
| **AWS** | ECS Fargate, EKS, EC2, Lambda, App Runner, Step Functions | AWS REST API (sigv4) | `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` |
| **GCP** | Cloud Run, GKE, Compute Engine, Cloud Functions, App Engine | Google REST API | `GOOGLE_APPLICATION_CREDENTIALS` |
| **Azure** | AKS, Container Instances, VMs, Functions, Container Apps | Azure REST API + OAuth2 | `AZURE_CLIENT_ID` + `AZURE_CLIENT_SECRET` + `AZURE_TENANT_ID` |
| **Railway** | Services | GraphQL API | `RAILWAY_TOKEN` |
| **Fly.io** | Machines | REST API v1 | `FLY_API_TOKEN` |
| **Vercel** | Deployments, Serverless Functions, Edge Functions | REST API v9 | `VERCEL_TOKEN` |
| **DigitalOcean** | App Platform, Droplets, Functions | REST API v2 | `DIGITALOCEAN_TOKEN` |
| **Cloudflare** | Workers, Pages, D1, R2 | REST API v4 | `CLOUDFLARE_API_TOKEN` |
| **Netlify** | Functions, Edge Functions, Sites | REST API | `NETLIFY_TOKEN` |
| **Supabase** | Edge Functions, Database | REST API | `SUPABASE_ACCESS_TOKEN` |

### Credential Storage

File: `~/.mhost/cloud-credentials.json`

```json
{
  "providers": {
    "aws": {
      "access_key_id": "AKIA...",
      "secret_access_key": "...",
      "default_region": "us-east-1"
    },
    "railway": { "token": "..." },
    "fly": { "token": "..." },
    "vercel": { "token": "..." },
    "digitalocean": { "token": "..." },
    "gcp": { "credentials_file": "~/.gcp/service-account.json" },
    "azure": {
      "client_id": "...",
      "client_secret": "...",
      "tenant_id": "...",
      "subscription_id": "..."
    },
    "cloudflare": { "token": "..." },
    "netlify": { "token": "..." },
    "supabase": { "token": "..." }
  }
}
```

## CLI Commands (~35 new)

### Authentication
```bash
mhost cloud auth aws                            # Interactive setup
mhost cloud auth list                           # Show configured providers
mhost cloud auth remove railway                 # Remove credentials
```

### Provisioning & Lifecycle
```bash
mhost cloud provision --provider aws --type container --name api \
    --image node:20 --command "node server.js" --port 3000 \
    --cpu 0.5 --memory 1GB --instances 2 --region us-east-1
mhost cloud destroy api --provider aws --confirm
```

### Deploy & Promote
```bash
mhost cloud deploy api --image myapp:v2.1
mhost cloud promote api --from staging --to production
mhost cloud deploy api --regions us-east-1,eu-west-1 --lb round-robin
```

### Day-2 Operations
```bash
mhost cloud scale api 4
mhost cloud restart api
mhost cloud logs api --lines 100 --follow
mhost cloud status
mhost cloud status api
mhost cloud metrics api
```

### Secrets
```bash
mhost cloud secrets set api DATABASE_URL "postgres://..."
mhost cloud secrets list api
mhost cloud secrets remove api DATABASE_URL
mhost cloud secrets sync api
```

### Cost
```bash
mhost cloud cost
mhost cloud cost --provider aws
mhost cloud cost --service api
mhost cloud cost --alert 500
```

### Drift Detection
```bash
mhost cloud drift
mhost cloud drift --fix
mhost cloud drift --watch
```

### Backup & Failover
```bash
mhost cloud backup api
mhost cloud backup list
mhost cloud failover api --to gcp
```

### Export
```bash
mhost cloud export terraform
mhost cloud export docker-compose
mhost cloud export pulumi
mhost cloud export kubernetes
```

## Secrets Management

File: `~/.mhost/cloud-secrets.json` (encrypted at rest)

- Encrypted with AES-256-GCM, key at `~/.mhost/cloud-secrets.key`
- `secrets set` encrypts locally + pushes to provider-native secret store
- `secrets sync` pushes all secrets to provider (AWS Secrets Manager, Railway variables, Fly secrets, etc.)
- Never logged, never shown in status output
- Agent can set secrets but never read values (tool returns `"***"`)

## Cost Tracking

- Each adapter implements `estimate_cost(spec)` returning hourly/monthly estimates
- `cloud cost` aggregates across all providers via their billing APIs
- Cost data cached in `~/.mhost/cloud-cost-cache.json` (refreshed every 6 hours)
- Budget alerts sent via existing notification channels (Telegram, Slack, etc.)
- Brain tracks cost trends — flags anomalies

## Drift Detection

- Compares `~/.mhost/cloud-state.toml` (desired state) vs live provider state
- `--fix` pushes desired state to provider
- `--watch` runs every 5 minutes, alerts on drift
- Agent can run drift checks autonomously and propose fixes

## Backup & Failover

- `cloud backup` saves: service config, env vars (encrypted), image, instance count → `~/.mhost/cloud-backups/<name>-<timestamp>.json`
- `cloud failover` reads backup, provisions on target provider, deploys same image, waits for health, reports new URL
- Cross-provider failover: AWS → GCP, Railway → Fly, etc.

## Multi-Region Deploy

- `--regions us-east-1,eu-west-1` deploys same service to multiple regions
- `--lb round-robin` configures DNS-based load balancing
- Health-check gated: only routes traffic to healthy regions
- Region state tracked in `~/.mhost/cloud-state.toml`

## IaC Export

| Format | Output File | Maps From |
|---|---|---|
| Terraform | `mhost-infra.tf` | CloudService → resource blocks |
| Docker Compose | `docker-compose.yml` | CloudService → service definitions |
| Pulumi | `index.ts` | CloudService → Pulumi resources |
| Kubernetes | `k8s-manifests.yaml` | CloudService → Deployment + Service |

## Agent Cloud Tools (12 new)

| Tool | Description | Autonomy |
|---|---|---|
| `cloud_list_services` | List all services across all providers | autonomous |
| `cloud_get_service` | Get details of a specific service | autonomous |
| `cloud_provision` | Spin up a new service | supervised |
| `cloud_deploy` | Deploy new version | supervised |
| `cloud_scale` | Scale up/down | supervised |
| `cloud_restart` | Restart a service | autonomous |
| `cloud_destroy` | Tear down a service | blocked (always manual) |
| `cloud_logs` | Fetch recent logs | autonomous |
| `cloud_cost` | Get cost breakdown | autonomous |
| `cloud_drift_check` | Check for config drift | autonomous |
| `cloud_secrets_set` | Set a secret | supervised |
| `cloud_failover` | Failover to backup provider | supervised |

Brain integration: Every cloud event feeds into the brain as incidents. The brain learns patterns for autonomous response.

## Feature Flag

```toml
# mhost-cloud/Cargo.toml
[features]
default = []
cloud-native = ["aes-gcm", "base64"]

# mhost-daemon/Cargo.toml
[features]
cloud-native = ["mhost-cloud/cloud-native"]

# Root Cargo.toml
[features]
full = ["mhost-daemon/api", "mhost-daemon/cloud-native"]
```

- `cargo install mhost` — lean, SSH fleet only
- `cargo install mhost --features cloud-native` — adds provider APIs
- `cargo install mhost --features full` — API + cloud-native
- Commands print install instructions when feature not compiled

## File Structure

```
crates/mhost-cloud/src/
├── lib.rs                      # existing + new module declarations
├── fleet.rs                    # existing (unchanged)
├── provider.rs                 # existing (unchanged)
├── ssh.rs                      # existing (unchanged)
├── remote.rs                   # existing (unchanged)
│
├── adapter/                    # NEW — unified cloud adapter
│   ├── mod.rs                  # CloudAdapter trait + core types
│   ├── aws.rs                  # AWS (ECS/EKS/EC2/Lambda/App Runner/Step Functions)
│   ├── gcp.rs                  # GCP (Cloud Run/GKE/GCE/Cloud Functions/App Engine)
│   ├── azure.rs                # Azure (AKS/ACI/VM/Functions/Container Apps)
│   ├── railway.rs              # Railway
│   ├── fly.rs                  # Fly.io Machines
│   ├── vercel.rs               # Vercel (Deployments/Serverless/Edge)
│   ├── digitalocean.rs         # DigitalOcean (App Platform/Droplets/Functions)
│   ├── cloudflare.rs           # Cloudflare (Workers/Pages/D1/R2)
│   ├── netlify.rs              # Netlify (Functions/Edge/Sites)
│   └── supabase.rs             # Supabase (Edge Functions/Database)
│
├── credentials.rs              # NEW — credential storage
├── secrets.rs                  # NEW — encrypted secrets (AES-256-GCM)
├── cost.rs                     # NEW — cost aggregation + alerts
├── drift.rs                    # NEW — drift detection
├── backup.rs                   # NEW — backup + failover
├── export.rs                   # NEW — Terraform/Compose/Pulumi/K8s
├── state.rs                    # NEW — desired state (cloud-state.toml)
└── multi_region.rs             # NEW — multi-region + LB routing
```

## File Locations

| Path | Purpose |
|---|---|
| `~/.mhost/cloud-credentials.json` | Provider API credentials |
| `~/.mhost/cloud-secrets.json` | Encrypted service secrets |
| `~/.mhost/cloud-secrets.key` | AES-256-GCM encryption key |
| `~/.mhost/cloud-state.toml` | Desired state for drift detection |
| `~/.mhost/cloud-cost-cache.json` | Cached cost data |
| `~/.mhost/cloud-backups/` | Service backup snapshots |

## Dependencies

New (only compiled with `--features cloud-native`):
- `aes-gcm = "0.10"` — secret encryption
- `base64 = "0.22"` — secret encoding

Already in workspace:
- `reqwest` — HTTP client for all provider APIs
- `serde` / `serde_json` — serialization
- `chrono` — timestamps
- `tokio` — async runtime
- `hmac` / `sha2` — AWS sigv4 signing
- `uuid` — resource IDs
