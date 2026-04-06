use clap::{Parser, Subcommand};
use clap_complete::Shell;

// ---------------------------------------------------------------------------
// Snapshot subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum SnapshotAction {
    /// Capture the current process list as a named snapshot.
    Create {
        /// Snapshot name.
        name: String,
    },
    /// List all saved snapshots.
    List,
    /// Restore a snapshot, stopping current processes and resurrecting saved ones.
    Restore {
        /// Snapshot name to restore.
        name: String,
    },
}

// ---------------------------------------------------------------------------
// Brain subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum BrainAction {
    /// Show fleet health scores.
    Status,
    /// Show past incidents and actions taken.
    History,
    /// List healing playbooks (built-in + auto-learned).
    Playbooks,
    /// Explain why a process has its current health score.
    Explain {
        /// Process name to analyse.
        process: String,
    },
}

// ---------------------------------------------------------------------------
// Metrics subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum MetricsAction {
    /// Show current CPU, memory, and uptime for a process.
    Show {
        /// Process name.
        name: String,
    },
    /// Show metric history for a process over a time window.
    History {
        /// Process name.
        name: String,
        /// Metric to retrieve, e.g. "cpu", "memory".
        #[arg(long)]
        metric: String,
        /// Time window to look back, e.g. "1h", "24h".
        #[arg(long, default_value = "24h")]
        since: String,
    },
    /// Start the Prometheus metrics exporter.
    Start {
        /// Address to listen on.
        #[arg(long, default_value = "0.0.0.0:9090")]
        listen: String,
    },
}

// ---------------------------------------------------------------------------
// AI subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum AiAction {
    /// Interactive setup — configure LLM provider and API key.
    Setup,
    /// Diagnose why a process crashed or is errored.
    Diagnose {
        /// Process name.
        name: String,
    },
    /// Query logs using natural language.
    Logs {
        /// Process name.
        name: String,
        /// Natural-language question about the logs.
        question: String,
    },
    /// Get performance optimization suggestions.
    Optimize {
        /// Process name.
        name: String,
    },
    /// Generate an mhost.toml config from a description.
    Config {
        /// Plain-English description of what you want to run.
        description: String,
    },
    /// Generate an incident post-mortem report.
    Postmortem {
        /// Process name.
        name: String,
    },
    /// Scan all processes for anomalies.
    Watch,
    /// Ask any question about your processes.
    Ask {
        /// Question to ask the AI.
        question: String,
    },
    /// Explain a config file in plain English.
    Explain {
        /// Path to the config file.
        #[arg(default_value = "mhost.toml")]
        file: String,
    },
    /// Get proactive improvement suggestions.
    Suggest,
}

// ---------------------------------------------------------------------------
// Cloud subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum CloudAction {
    /// Add a remote server to the fleet.
    Add {
        /// Friendly name for the server.
        name: String,
        /// Hostname or IP address.
        #[arg(long)]
        host: String,
        /// SSH user (default: root).
        #[arg(long)]
        user: Option<String>,
        /// Path to the SSH private key.
        #[arg(long)]
        key: Option<String>,
        /// SSH port (default: 22).
        #[arg(long)]
        port: Option<u16>,
    },
    /// Remove a server from the fleet.
    Remove {
        /// Server name.
        name: String,
    },
    /// List all configured fleet servers.
    List,
    /// Check connectivity and mhost status on all servers.
    Status,
    /// Deploy a local config file to a remote server.
    Deploy {
        /// Target server name.
        server: String,
        /// Path to the local mhost.toml to deploy.
        config: String,
    },
    /// Execute a shell command on a remote server.
    Exec {
        /// Target server name.
        server: String,
        /// Shell command to run.
        command: String,
    },
    /// Stream recent logs from a process on a remote server.
    Logs {
        /// Target server name.
        server: String,
        /// Process name whose logs to stream.
        app: String,
    },
    /// Restart a process on a server, or use "all" to restart on every server.
    Restart {
        /// Server name or "all".
        server: String,
        /// Process name to restart.
        app: String,
    },
    /// Scale a process to N instances on a remote server.
    Scale {
        /// Target server name.
        server: String,
        /// Process name to scale.
        app: String,
        /// Desired number of instances.
        instances: u32,
    },
    /// Sync a local config file to all servers in the fleet.
    Sync {
        /// Path to the local mhost.toml to sync.
        config: String,
    },
    /// Open an interactive SSH shell to a server.
    Ssh {
        /// Server name.
        server: String,
    },
    /// Install mhost on a remote server.
    Install {
        /// Target server name.
        server: String,
    },
    /// Update mhost on a server (or "all" to update the entire fleet).
    Update {
        /// Server name or "all".
        target: String,
    },
    /// Import servers from a cloud provider (aws, digitalocean, azure, railway).
    Import {
        /// Cloud provider name.
        provider: String,
        /// Filter by region (optional).
        #[arg(long)]
        region: Option<String>,
        /// Filter by tag key=value (optional).
        #[arg(long)]
        tag: Option<String>,
    },
    /// Authenticate with a cloud provider (interactive token setup).
    Auth {
        /// Cloud provider name (railway, fly, vercel, digitalocean, cloudflare, netlify, supabase).
        provider: String,
    },
    /// List all configured cloud provider credentials.
    AuthList,
    /// Remove stored credentials for a cloud provider.
    AuthRemove {
        /// Cloud provider name to remove.
        provider: String,
    },
    /// AI: Plan infrastructure from a plain-English description.
    AiSetup {
        /// Description of the infrastructure you need.
        description: String,
    },
    /// AI: Diagnose a remote server (processes, logs, system state).
    AiDiagnose {
        /// Target server name.
        server: String,
    },
    /// AI: Plan a migration between two servers.
    AiMigrate {
        /// Source server name.
        from: String,
        /// Destination server name.
        to: String,
    },

    // ── Cloud-Native commands (no SSH, direct provider API) ──
    /// Provision a new cloud service on a provider.
    Provision {
        /// Cloud provider name (railway, fly, vercel, digitalocean, etc.).
        #[arg(long)]
        provider: String,
        /// Service name.
        #[arg(long)]
        name: String,
        /// Service type (web, worker, cron, static).
        #[arg(long, rename_all = "kebab-case", value_name = "TYPE")]
        r#type: String,
        /// Container image to deploy.
        #[arg(long)]
        image: Option<String>,
        /// Port to expose.
        #[arg(long)]
        port: Option<u16>,
        /// Number of instances.
        #[arg(long, default_value = "1")]
        instances: u32,
        /// Region / location.
        #[arg(long)]
        region: Option<String>,
        /// CPU allocation (e.g. "0.5", "2").
        #[arg(long)]
        cpu: Option<String>,
        /// Memory allocation (e.g. "512MB", "2GB").
        #[arg(long)]
        memory: Option<String>,
    },
    /// List all cloud-native services across providers.
    Services {
        /// Filter to a specific provider.
        #[arg(long)]
        provider: Option<String>,
    },
    /// Show details of a specific cloud-native service.
    Service {
        /// Service name.
        name: String,
        /// Filter to a specific provider.
        #[arg(long)]
        provider: Option<String>,
    },
    /// Deploy a new image to an existing cloud-native service.
    CloudDeploy {
        /// Service name.
        name: String,
        /// Container image to deploy.
        #[arg(long)]
        image: String,
        /// Provider (optional if service name is unique).
        #[arg(long)]
        provider: Option<String>,
    },
    /// Scale a cloud-native service to N instances.
    CloudScale {
        /// Service name.
        name: String,
        /// Desired number of instances.
        instances: u32,
        /// Provider (optional if service name is unique).
        #[arg(long)]
        provider: Option<String>,
    },
    /// Destroy a cloud-native service permanently.
    Destroy {
        /// Service name.
        name: String,
        /// Cloud provider name.
        #[arg(long)]
        provider: String,
        /// Confirm destruction (required).
        #[arg(long)]
        confirm: bool,
    },
    /// Show cost/spending across cloud providers.
    Cost {
        /// Filter to a specific provider.
        #[arg(long)]
        provider: Option<String>,
    },
    /// Detect configuration drift between local state and live cloud.
    Drift {
        /// Automatically fix detected drift.
        #[arg(long)]
        fix: bool,
    },
    /// Manage cloud service secrets.
    Secrets {
        #[command(subcommand)]
        action: SecretsAction,
    },
    /// Backup a cloud service's data/config.
    Backup {
        /// Service name to back up.
        service: String,
    },
    /// List all cloud backups.
    BackupList,
    /// Export infrastructure as code (terraform, docker-compose, kubernetes).
    Export {
        /// Output format: terraform, docker-compose, kubernetes.
        format: String,
    },
}

// ---------------------------------------------------------------------------
// Secrets subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum SecretsAction {
    /// Set a secret on a cloud service.
    Set {
        /// Service name.
        service: String,
        /// Secret key.
        key: String,
        /// Secret value.
        value: String,
    },
    /// List secrets for a cloud service.
    List {
        /// Service name.
        service: String,
    },
    /// Remove a secret from a cloud service.
    Remove {
        /// Service name.
        service: String,
        /// Secret key.
        key: String,
    },
}

// ---------------------------------------------------------------------------
// Agent subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum AgentAction {
    /// Interactive agent setup — configure LLM provider, API key, and Telegram.
    Setup,
    /// Start the autonomous agent as a managed mhost process.
    Start,
    /// Stop the running agent.
    Stop,
    /// Show agent configuration and status.
    Status,
}

// ---------------------------------------------------------------------------
// Bot subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum BotAction {
    /// Interactive bot setup (Telegram/Discord).
    Setup,
    /// Start the bot.
    Enable,
    /// Stop the bot.
    Disable,
    /// Show bot status and connected users.
    Status,
    /// Show permission configuration.
    Permissions,
    /// Add an admin user.
    AddAdmin {
        /// Telegram or Discord user ID.
        user_id: i64,
    },
    /// Add an operator user.
    AddOperator {
        /// Telegram or Discord user ID.
        user_id: i64,
    },
    /// Add a viewer user.
    AddViewer {
        /// Telegram or Discord user ID.
        user_id: i64,
    },
    /// Remove a user from all roles.
    RemoveUser {
        /// Telegram or Discord user ID.
        user_id: i64,
    },
    /// Show bot audit log.
    Logs,
    /// Get your Telegram chat ID (send /start to your bot first).
    ChatId {
        /// Your Telegram bot token.
        token: String,
    },
    /// (internal) Run the bot inline — used by the background process wrapper.
    #[command(hide = true)]
    RunInline,
}

// ---------------------------------------------------------------------------
// Notify subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum NotifyAction {
    /// Interactive setup wizard to add a notification channel.
    Setup,
    /// List all configured notification channels.
    List,
    /// Send a test notification to a channel.
    Test {
        /// Channel name to test.
        channel: String,
    },
    /// Remove a notification channel.
    Remove {
        /// Channel name to remove.
        channel: String,
    },
    /// Enable a notification channel.
    Enable {
        /// Channel name to enable.
        channel: String,
    },
    /// Disable a notification channel.
    Disable {
        /// Channel name to disable.
        channel: String,
    },
    /// Show available alert events and channel subscriptions.
    Events {
        /// Show events for a specific channel.
        channel: Option<String>,
    },
    /// Start the notifier as a managed mhost process.
    Start,
}

// ---------------------------------------------------------------------------
// Log alert subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum LogAlertAction {
    /// Add a new log alert for a process.
    Add {
        /// Process name to monitor.
        process: String,
        /// Regex pattern to match in log output.
        #[arg(long)]
        pattern: String,
        /// Notification channel (e.g. telegram, slack, webhook).
        #[arg(long)]
        notify: String,
        /// Minimum seconds between repeated alerts (default: 60).
        #[arg(long, default_value = "60")]
        cooldown: u64,
    },
    /// List all configured log alerts.
    List,
    /// Remove a log alert by ID.
    Remove {
        /// Alert ID to remove.
        id: String,
    },
}

// ---------------------------------------------------------------------------
// Plugin subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum PluginAction {
    /// List all installed plugins.
    List,
    /// Install a plugin from a local directory.
    Install {
        /// Path to the plugin directory (must contain plugin.json).
        path: String,
    },
    /// Remove an installed plugin.
    Remove {
        /// Plugin name.
        name: String,
    },
    /// Show detailed information about a plugin.
    Info {
        /// Plugin name.
        name: String,
    },
}

// ---------------------------------------------------------------------------
// Docker subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum DockerAction {
    /// Run a new Docker container managed by mhost.
    Run {
        /// Docker image to run.
        image: String,
        /// Container name.
        #[arg(long)]
        name: String,
        /// Port to expose (maps host:container with same port).
        #[arg(long)]
        port: Option<u16>,
        /// Environment variable in KEY=VAL format (repeatable).
        #[arg(long = "env", value_name = "KEY=VAL")]
        envs: Vec<String>,
    },
    /// List mhost-managed containers.
    List,
    /// Stop a container.
    Stop {
        /// Container name.
        name: String,
    },
    /// Restart a container.
    Restart {
        /// Container name.
        name: String,
    },
    /// Show container logs.
    Logs {
        /// Container name.
        name: String,
        /// Number of log lines to show.
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
    },
    /// Remove a container.
    Rm {
        /// Container name.
        name: String,
    },
    /// Pull a Docker image.
    Pull {
        /// Image to pull.
        image: String,
    },
}

// ---------------------------------------------------------------------------
// Template subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum TemplateAction {
    /// List all available templates.
    List,
    /// Generate an mhost.toml in the current directory from a template.
    Init {
        /// Template name (e.g. nextjs, express, fastapi).
        name: String,
    },
}

// ---------------------------------------------------------------------------
// Workspace subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum WorkspaceAction {
    /// List all workspaces.
    List,
    /// Create a new workspace.
    Create {
        /// Workspace name.
        name: String,
    },
    /// Switch to a workspace.
    Switch {
        /// Workspace name.
        name: String,
    },
    /// Show the active workspace.
    Current,
    /// Delete a workspace.
    Delete {
        /// Workspace name.
        name: String,
    },
}

// ---------------------------------------------------------------------------
// Status page subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum StatusPageAction {
    /// Generate static HTML to stdout.
    Generate,
}

// ---------------------------------------------------------------------------
// Hooks subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum HooksAction {
    /// Create a new incoming webhook.
    Create {
        /// Action to perform when triggered (restart, stop, start, delete, reload).
        #[arg(long)]
        action: String,
        /// Process name the action applies to.
        #[arg(long)]
        process: String,
    },
    /// List all configured webhooks.
    List,
    /// Remove a webhook by ID.
    Remove {
        /// Webhook ID.
        id: String,
    },
    /// Simulate triggering a webhook.
    Test {
        /// Webhook ID.
        id: String,
    },
}

#[derive(Parser)]
#[command(
    name = "mhost",
    about = "Advanced process manager — PM2 replacement written in Rust",
    version,
    disable_version_flag = true
)]
pub struct Cli {
    /// Print version
    #[arg(short = 'v', short_alias = 'V', long = "version", action = clap::ArgAction::Version)]
    version: (),

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start a process or ecosystem config file.
    Start {
        /// Command, binary path, or .toml/.yaml/.json config file.
        target: String,
        /// Override or set the process name.
        #[arg(short, long)]
        name: Option<String>,
        /// Start all processes belonging to this group.
        #[arg(long)]
        group: Option<String>,
        /// Tag to attach to the process (repeatable).
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// CPU limit (e.g. "50%" or "1.0" for cores).
        #[arg(long)]
        cpu_limit: Option<String>,
        /// Memory hard limit in MB.
        #[arg(long)]
        memory_limit: Option<u64>,
    },

    /// Stop a running process (use "all" to stop everything).
    Stop {
        /// Process name or "all".
        target: String,
        /// Stop all processes belonging to this group.
        #[arg(long)]
        group: Option<String>,
    },

    /// Restart a process (use "all" to restart everything).
    Restart {
        /// Process name or "all".
        target: String,
    },

    /// Remove a process from the registry (use "all" to remove everything).
    Delete {
        /// Process name or "all".
        target: String,
    },

    /// List all managed processes.
    #[command(alias = "ls")]
    List {
        /// Filter processes by tag.
        #[arg(long = "tag", value_name = "TAG")]
        tag: Option<String>,
    },

    /// Tail log output for a process (or all processes if no name given).
    Logs {
        /// Process name or ID (omit to show all).
        name: Option<String>,
        /// Number of lines to show (file-tail mode).
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
        /// Show stderr log instead of stdout (file-tail mode).
        #[arg(long)]
        err: bool,
        /// Filter lines containing this substring (file-tail mode).
        #[arg(long)]
        grep: Option<String>,
        /// Full-text search query (uses daemon LOG_SEARCH RPC).
        #[arg(long)]
        search: Option<String>,
        /// SQL-style WHERE filter applied server-side when using --search.
        #[arg(long, value_name = "EXPR")]
        r#where: Option<String>,
        /// Time window for --search / --count-by, e.g. "1h", "24h".
        #[arg(long, value_name = "DURATION")]
        since: Option<String>,
        /// Output format for --search results: "text" or "json".
        #[arg(long, default_value = "text")]
        format: String,
        /// Group log counts by a field, e.g. "level" (uses LOG_COUNT_BY RPC).
        #[arg(long, value_name = "FIELD")]
        count_by: Option<String>,
        /// Follow the log output in real-time (like tail -f).
        #[arg(long)]
        follow: bool,
    },

    /// Show detailed information about a process.
    Info {
        /// Process name.
        name: String,
    },

    /// Print environment variables for a process.
    Env {
        /// Process name.
        name: String,
    },

    /// Scale a process to a specific number of instances.
    Scale {
        /// Process name.
        name: String,
        /// Desired number of instances.
        instances: u32,
    },

    /// Save the current process list for resurrection on next startup.
    Save,

    /// Restore all previously saved processes.
    Resurrect,

    /// Generate a startup script so mhost launches at login/boot.
    Startup,

    /// Remove the startup script.
    Unstartup,

    /// Ping the daemon.
    Ping,

    /// Kill the daemon.
    Kill,

    /// Show event history for a process.
    History {
        /// Process name.
        name: String,
    },

    /// Print the configuration for a process as JSON.
    Config {
        /// Process name.
        name: String,
    },

    /// Show health status for each instance of a process.
    Health {
        /// Process name.
        name: String,
    },

    /// Set the number of running instances for a process (alias for scale).
    Cluster {
        /// Process name.
        name: String,
        /// Desired number of instances.
        instances: u32,
    },

    /// Metrics commands (show, history, start Prometheus exporter).
    Metrics {
        #[command(subcommand)]
        action: MetricsAction,
    },

    /// Configure and manage notification channels (Telegram, Slack, Discord, Webhook).
    Notify {
        #[command(subcommand)]
        action: NotifyAction,
    },

    /// AI-powered process intelligence (diagnose, optimize, ask, watch).
    Ai {
        #[command(subcommand)]
        action: AiAction,
    },

    /// Check for a newer mhost release and update if available.
    SelfUpdate,

    /// Generate shell completion scripts.
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Launch the interactive TUI dashboard.
    Monit,

    /// Deploy an environment defined in the ecosystem config.
    Deploy {
        /// Environment name (e.g. "production").
        env: String,
    },

    /// Rollback an environment to the previous deploy.
    Rollback {
        /// Environment name (e.g. "production").
        env: String,
    },

    /// Show proxy routes.
    Proxy,

    /// Remote fleet management — manage processes across cloud servers.
    Cloud {
        #[command(subcommand)]
        action: CloudAction,
    },

    /// Chat-based remote control via Telegram or Discord.
    Bot {
        #[command(subcommand)]
        action: BotAction,
    },

    /// Autonomous AI agent that monitors and manages processes.
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },

    /// Self-healing brain — memory, health scores, playbooks, and trend detection.
    Brain {
        #[command(subcommand)]
        action: BrainAction,
    },

    /// Zero-downtime reload (start new, health check, kill old).
    Reload {
        /// Process name to reload.
        target: String,
    },

    /// Development mode — auto-restart on file changes.
    Dev {
        /// Script to run.
        script: String,
        /// Directory to watch (default: current dir).
        #[arg(long)]
        watch: Option<String>,
        /// File extensions to watch, comma-separated (default: js,ts,py,...).
        #[arg(long)]
        ext: Option<String>,
        /// .env file to load (default: .env).
        #[arg(long)]
        env: Option<String>,
    },

    /// Start a web dashboard for monitoring.
    Dashboard {
        /// Port to listen on.
        #[arg(long, default_value = "9400")]
        port: u16,
    },

    /// Replay an incident timeline for a process.
    Replay {
        /// Process name to replay.
        process: String,
        /// Filter events around this timestamp (e.g. "3:47am").
        #[arg(long)]
        time: Option<String>,
    },

    /// Load test an HTTP endpoint.
    Bench {
        /// URL to benchmark.
        url: String,
        /// Duration of the test in seconds.
        #[arg(long, default_value = "10")]
        duration: u64,
        /// Number of concurrent workers.
        #[arg(long, default_value = "10")]
        concurrency: u32,
    },

    /// Show process dependency graph.
    Link,

    /// Estimate cloud resource costs from running process memory usage.
    Cost,

    /// Check SSL certificate expiry for one or more hosts.
    Certs {
        /// URLs to check (e.g. https://example.com). Defaults to https://localhost:443.
        #[arg(long, value_name = "URL")]
        url: Option<Vec<String>>,
    },

    /// SLA uptime report for a process.
    Sla {
        /// Process name to report on.
        app: String,
        /// Target SLA percentage (default: 99.9).
        #[arg(long, default_value = "99.9")]
        target: f64,
    },

    /// Migrate configuration from another process manager (e.g. pm2).
    Migrate {
        /// Source process manager to migrate from.
        #[arg(long)]
        from: String,
    },

    /// Team management (coming soon).
    Team,

    /// Interactive playground tutorial.
    Playground,

    /// Canary deployment — scale up, monitor, promote or rollback.
    Canary {
        /// Process name to canary-deploy.
        app: String,
        /// Percentage of traffic to route to the canary (informational).
        #[arg(long, default_value = "10")]
        percent: u32,
        /// How long to monitor the canary in seconds before deciding.
        #[arg(long, default_value = "300")]
        duration: u64,
    },

    /// Run a recipe file (sequential mhost commands).
    Run {
        /// Path to the recipe file.
        file: String,
    },

    /// Expose a local process to the internet via a tunnel.
    Share {
        /// Process name whose port to expose.
        app: String,
        /// Override the port to expose.
        #[arg(long)]
        port: Option<u16>,
    },

    /// Compare two environments or configs.
    Diff {
        /// First environment or config name.
        env_a: String,
        /// Second environment or config name.
        env_b: String,
    },

    /// Snapshot management — create, list, and restore process snapshots.
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },

    /// API server management — start/stop server, manage tokens and webhooks.
    Api {
        #[command(subcommand)]
        action: crate::commands::api::ApiCommands,
    },

    /// Login to mhost Cloud.
    Login,

    /// Logout from mhost Cloud.
    Logout,

    /// Connect this server to mhost Cloud.
    Connect {
        /// Friendly server name (defaults to hostname).
        #[arg(long)]
        name: Option<String>,
    },

    /// Disconnect this server from mhost Cloud.
    Disconnect,

    /// Open the mhost Cloud dashboard in your browser.
    CloudOpen,

    /// Scan current directory and generate mhost.toml.
    Init,

    /// Manage log-based alerts (add, list, remove).
    #[command(name = "log-alert")]
    LogAlert {
        #[command(subcommand)]
        action: LogAlertAction,
    },

    /// Show processes with cron_restart schedules and next fire times.
    Cron,

    /// Show resource limits and current usage for a process.
    Limits {
        /// Process name.
        process: String,
    },

    /// Docker container management — run, list, stop, restart, logs, rm, pull.
    Docker {
        #[command(subcommand)]
        action: DockerAction,
    },

    /// Process templates — list available templates or generate an mhost.toml.
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },

    /// Manage mhost plugins (list, install, remove, info).
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Show the audit trail of CLI actions.
    Audit {
        /// Filter by process name.
        #[arg(long)]
        process: Option<String>,
        /// Filter by time window (e.g. "24h", "7d").
        #[arg(long)]
        since: Option<String>,
        /// Number of entries to show.
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,
    },

    /// Watch a config file for changes and auto-reload.
    Watch {
        /// Path to the config file (defaults to mhost.toml / mhost.yaml / mhost.json).
        config: Option<String>,
    },

    /// Show rollback information for a process config (stub — requires daemon).
    RollbackProcess {
        /// Process name.
        process: String,
    },

    /// Show config version history for a process.
    ConfigHistory {
        /// Process name.
        process: String,
    },

    /// Multi-tenancy workspaces — isolate processes and configs.
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },

    /// Generate and serve a public status page.
    #[command(name = "status-page")]
    StatusPage {
        /// Port to serve the status page on.
        #[arg(long, default_value = "8080")]
        port: u16,
        /// Subcommand (e.g. generate).
        #[command(subcommand)]
        action: Option<StatusPageAction>,
    },

    /// Manage incoming webhooks.
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },
}
