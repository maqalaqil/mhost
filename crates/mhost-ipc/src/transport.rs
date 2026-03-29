use std::io;
use std::path::Path;

use tokio::net::{UnixListener, UnixStream};

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub type IpcStream = UnixStream;
pub type IpcListener = UnixListener;

// ---------------------------------------------------------------------------
// Connect
// ---------------------------------------------------------------------------

/// Connect to an existing Unix-domain socket at `socket_path`.
pub async fn connect(socket_path: &Path) -> io::Result<IpcStream> {
    UnixStream::connect(socket_path).await
}

// ---------------------------------------------------------------------------
// Bind
// ---------------------------------------------------------------------------

/// Bind a Unix-domain listener at `socket_path`.
/// Removes any stale socket file before binding.
pub fn bind(socket_path: &Path) -> io::Result<IpcListener> {
    // Remove a stale socket file so that the bind never fails with AddressInUse.
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }
    UnixListener::bind(socket_path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // -- Unix socket roundtrip: bind, connect, send "ping", receive "pong" --

    #[tokio::test]
    async fn test_unix_socket_roundtrip() {
        let socket_path = std::path::PathBuf::from("/tmp/mhost-transport-test.sock");

        let listener = bind(&socket_path).expect("bind");

        // Spawn server half: accept one connection, read "ping", write "pong".
        let server = tokio::spawn(async move {
            let (mut stream, _addr) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 4];
            stream.read_exact(&mut buf).await.expect("read");
            assert_eq!(&buf, b"ping");
            stream.write_all(b"pong").await.expect("write");
        });

        // Give the server a moment to reach accept().
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Client half.
        let mut client = connect(&socket_path).await.expect("connect");
        client.write_all(b"ping").await.expect("write");
        let mut resp = vec![0u8; 4];
        client.read_exact(&mut resp).await.expect("read");
        assert_eq!(&resp, b"pong");

        server.await.expect("server task");
    }

    // -- bind() replaces an existing stale socket file ---------------------

    #[tokio::test]
    async fn test_bind_replaces_existing_socket() {
        let socket_path = std::path::PathBuf::from("/tmp/mhost-transport-replace-test.sock");

        // Create a first listener at the path.
        let _first = bind(&socket_path).expect("first bind");
        // Drop it — the socket file remains on disk.
        drop(_first);

        // A second bind must succeed (removing the stale file).
        let second = bind(&socket_path);
        assert!(
            second.is_ok(),
            "bind must succeed when stale socket file exists: {:?}",
            second.err()
        );

        // Clean up.
        let _ = std::fs::remove_file(&socket_path);
    }

    // -- connect() to nonexistent path returns an error --------------------

    #[tokio::test]
    async fn test_connect_to_nonexistent_path_returns_error() {
        let result = connect(std::path::Path::new("/tmp/mhost-no-such.sock")).await;
        assert!(result.is_err(), "connect to missing socket must fail");
    }
}
