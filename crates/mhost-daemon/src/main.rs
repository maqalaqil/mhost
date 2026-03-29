mod cron_scheduler;
mod handler;
mod memory_monitor;
mod remote;
mod state;
mod supervisor;
mod watcher;

use std::sync::Arc;

use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use mhost_core::paths::MhostPaths;
use mhost_ipc::server::{HandlerFn, IpcServer};

use handler::Handler;
use state::StateStore;
use supervisor::Supervisor;

#[tokio::main]
async fn main() {
    // -----------------------------------------------------------------------
    // Tracing
    // -----------------------------------------------------------------------
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // -----------------------------------------------------------------------
    // Paths + directories
    // -----------------------------------------------------------------------
    let paths = MhostPaths::new();
    if let Err(e) = paths.ensure_dirs() {
        eprintln!("Failed to create directories: {}", e);
        std::process::exit(1);
    }

    // -----------------------------------------------------------------------
    // PID file
    // -----------------------------------------------------------------------
    let pid_file = paths.pid_file();
    let my_pid = std::process::id();
    if let Err(e) = std::fs::write(&pid_file, my_pid.to_string()) {
        eprintln!("Failed to write PID file: {}", e);
        std::process::exit(1);
    }
    tracing::info!("mhostd starting (PID {})", my_pid);

    // -----------------------------------------------------------------------
    // State store
    // -----------------------------------------------------------------------
    let state_store = match StateStore::open(&paths.db()) {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(e) => {
            eprintln!("Failed to open state database: {}", e);
            std::process::exit(1);
        }
    };

    // -----------------------------------------------------------------------
    // Supervisor
    // -----------------------------------------------------------------------
    let socket_path_for_supervisor = paths.socket(); // capture before move
    let supervisor = Arc::new(Supervisor::new(MhostPaths::with_root(paths.root().clone())));

    // -----------------------------------------------------------------------
    // Handler
    // -----------------------------------------------------------------------
    let handler = Arc::new(Handler::new(Arc::clone(&supervisor), Arc::clone(&state_store)));

    // -----------------------------------------------------------------------
    // IPC Server
    // -----------------------------------------------------------------------
    let socket_path = socket_path_for_supervisor;
    // Remove stale socket if it exists
    let _ = std::fs::remove_file(&socket_path);

    let server = IpcServer::new(&socket_path);
    let shutdown = server.shutdown_handle();
    let shutdown_clone = Arc::clone(&shutdown);

    // -----------------------------------------------------------------------
    // Signal handler
    // -----------------------------------------------------------------------
    #[cfg(unix)]
    {
        let shutdown_signal = Arc::clone(&shutdown);
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
            let mut sigint =
                signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");

            tokio::select! {
                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM, shutting down");
                }
                _ = sigint.recv() => {
                    tracing::info!("Received SIGINT, shutting down");
                }
            }

            shutdown_signal.notify_one();
        });
    }

    #[cfg(windows)]
    {
        let shutdown_signal = Arc::clone(&shutdown);
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to register Ctrl+C handler");
            tracing::info!("Received Ctrl+C, shutting down");
            shutdown_signal.notify_one();
        });
    }

    // -----------------------------------------------------------------------
    // Handler closure
    // -----------------------------------------------------------------------
    let handler_fn: HandlerFn = Arc::new(move |req| {
        let handler = Arc::clone(&handler);
        let shutdown = Arc::clone(&shutdown_clone);
        Box::pin(async move {
            let (resp, kill) = handler.dispatch(req).await;
            if kill {
                // Notify shutdown after returning the response
                tokio::spawn(async move {
                    // Small delay to let the response be written before we close
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    shutdown.notify_one();
                });
            }
            resp
        })
    });

    // -----------------------------------------------------------------------
    // Run
    // -----------------------------------------------------------------------
    tracing::info!("mhostd listening on {:?}", socket_path);
    server.run(handler_fn).await;
    tracing::info!("mhostd shutting down");

    // -----------------------------------------------------------------------
    // Cleanup
    // -----------------------------------------------------------------------
    let _ = std::fs::remove_file(&pid_file);
    let _ = std::fs::remove_file(&socket_path);
    tracing::info!("mhostd stopped");
}
