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
        eprintln!("Failed to create directories: {e}");
        std::process::exit(1);
    }

    // -----------------------------------------------------------------------
    // PID file
    // -----------------------------------------------------------------------
    let pid_file = paths.pid_file();
    let my_pid = std::process::id();
    if let Err(e) = std::fs::write(&pid_file, my_pid.to_string()) {
        eprintln!("Failed to write PID file: {e}");
        std::process::exit(1);
    }
    tracing::info!("mhostd starting (PID {})", my_pid);

    // -----------------------------------------------------------------------
    // State store
    // -----------------------------------------------------------------------
    let state_store = match StateStore::open(&paths.db()) {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(e) => {
            eprintln!("Failed to open state database: {e}");
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
    let handler = Arc::new(Handler::new(
        Arc::clone(&supervisor),
        Arc::clone(&state_store),
    ));

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
        let sig_supervisor = Arc::clone(&supervisor);
        let sig_state = Arc::clone(&state_store);
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
            let mut sigint =
                signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");

            tokio::select! {
                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM, stopping children");
                }
                _ = sigint.recv() => {
                    tracing::info!("Received SIGINT, stopping children");
                }
            }

            // Stop all children before notifying shutdown
            let state_guard = sig_state.lock().await;
            sig_supervisor.stop_all(&state_guard).await;
            drop(state_guard);

            shutdown_signal.notify_one();
        });
    }

    #[cfg(windows)]
    {
        let shutdown_signal = Arc::clone(&shutdown);
        let sig_supervisor = Arc::clone(&supervisor);
        let sig_state = Arc::clone(&state_store);
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to register Ctrl+C handler");
            tracing::info!("Received Ctrl+C, stopping children");

            let state_guard = sig_state.lock().await;
            sig_supervisor.stop_all(&state_guard).await;
            drop(state_guard);

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
    tracing::info!("mhostd shutting down — stopping all child processes");

    // -----------------------------------------------------------------------
    // Stop all child processes before exiting
    // -----------------------------------------------------------------------
    {
        let state_guard = state_store.lock().await;
        supervisor.stop_all(&state_guard).await;
    }
    tracing::info!("All child processes stopped");

    // -----------------------------------------------------------------------
    // Kill any remaining child processes by PID file
    // -----------------------------------------------------------------------
    let pids_dir = paths.pids_dir();
    if pids_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&pids_dir) {
            for entry in entries.flatten() {
                if let Ok(pid_str) = std::fs::read_to_string(entry.path()) {
                    if let Ok(pid) = pid_str.trim().parse::<i32>() {
                        tracing::info!(pid = pid, file = ?entry.path(), "Killing child process");
                        #[cfg(unix)]
                        unsafe {
                            libc::kill(pid, libc::SIGTERM);
                        }
                        // Give it a moment, then force kill
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        #[cfg(unix)]
                        unsafe {
                            libc::kill(pid, libc::SIGKILL);
                        }
                    }
                }
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    // -----------------------------------------------------------------------
    // Cleanup
    // -----------------------------------------------------------------------
    let _ = std::fs::remove_file(&pid_file);
    let _ = std::fs::remove_file(&socket_path);
    tracing::info!("mhostd stopped");
}
