use clap::{Parser, Subcommand};
use clap_complete::Shell;

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

    /// Tail log output for a process.
    Logs {
        /// Process name.
        name: String,
        /// Number of lines to show.
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
        /// Show stderr log instead of stdout.
        #[arg(long)]
        err: bool,
        /// Filter lines containing this substring.
        #[arg(long)]
        grep: Option<String>,
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

    /// Check for a newer mhost release and update if available.
    SelfUpdate,

    /// Generate shell completion scripts.
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}
