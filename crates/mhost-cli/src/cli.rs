use clap::{Parser, Subcommand};
use clap_complete::Shell;

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

#[derive(Parser)]
#[command(
    name = "mhost",
    about = "Advanced process manager — PM2 replacement written in Rust",
    version
)]
pub struct Cli {
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
    List,

    /// Tail log output for a process, or search/aggregate via the daemon.
    Logs {
        /// Process name.
        name: String,
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
}
