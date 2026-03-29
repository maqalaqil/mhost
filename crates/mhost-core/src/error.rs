use thiserror::Error;

#[derive(Error, Debug)]
pub enum MhostError {
    #[error("Process '{name}' not found")]
    ProcessNotFound { name: String },

    #[error("Process '{name}' is already running (PID {pid})")]
    ProcessAlreadyRunning { name: String, pid: u32 },

    #[error("Daemon is not running")]
    DaemonNotRunning,

    #[error("Daemon connection failed: {reason}")]
    DaemonConnectionFailed { reason: String },

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{message}\n  → {suggestion}")]
    WithSuggestion { message: String, suggestion: String },
}

pub type Result<T> = std::result::Result<T, MhostError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_not_found_display() {
        let err = MhostError::ProcessNotFound { name: "api-server".into() };
        assert_eq!(err.to_string(), "Process 'api-server' not found");
    }

    #[test]
    fn test_already_running_display() {
        let err = MhostError::ProcessAlreadyRunning { name: "api".into(), pid: 1234 };
        assert_eq!(err.to_string(), "Process 'api' is already running (PID 1234)");
    }

    #[test]
    fn test_with_suggestion_display() {
        let err = MhostError::WithSuggestion {
            message: "Port 3000 in use".into(),
            suggestion: "Try: mhost proxy --port 3001".into(),
        };
        assert!(err.to_string().contains("Port 3000 in use"));
        assert!(err.to_string().contains("Try: mhost proxy --port 3001"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let mhost_err: MhostError = io_err.into();
        assert!(matches!(mhost_err, MhostError::Io(_)));
    }
}
