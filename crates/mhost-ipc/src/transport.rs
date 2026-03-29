#[cfg(unix)]
mod unix_impl {
    use std::io;
    use std::path::Path;

    use tokio::net::{UnixListener, UnixStream};

    pub type IpcStream = UnixStream;
    pub type IpcListener = UnixListener;

    pub async fn connect(socket_path: &Path) -> io::Result<IpcStream> {
        UnixStream::connect(socket_path).await
    }

    pub fn bind(socket_path: &Path) -> io::Result<IpcListener> {
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }
        UnixListener::bind(socket_path)
    }
}

#[cfg(unix)]
pub use unix_impl::*;

// Windows stub — uses TCP on localhost as fallback
#[cfg(not(unix))]
mod windows_impl {
    use std::io;
    use std::path::Path;

    use tokio::net::{TcpListener, TcpStream};

    pub type IpcStream = TcpStream;
    pub type IpcListener = TcpListener;

    pub async fn connect(_socket_path: &Path) -> io::Result<IpcStream> {
        TcpStream::connect("127.0.0.1:19515").await
    }

    pub fn bind(_socket_path: &Path) -> io::Result<IpcListener> {
        // Use a std TcpListener and convert, since tokio's TcpListener::bind is async
        let std_listener = std::net::TcpListener::bind("127.0.0.1:19515")?;
        std_listener.set_nonblocking(true)?;
        TcpListener::from_std(std_listener)
    }
}

#[cfg(not(unix))]
pub use windows_impl::*;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_unix_socket_roundtrip() {
        let socket_path = std::path::PathBuf::from("/tmp/mhost-transport-test.sock");
        let listener = bind(&socket_path).expect("bind");

        let server = tokio::spawn(async move {
            let (mut stream, _addr) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 4];
            stream.read_exact(&mut buf).await.expect("read");
            assert_eq!(&buf, b"ping");
            stream.write_all(b"pong").await.expect("write");
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let mut client = connect(&socket_path).await.expect("connect");
        client.write_all(b"ping").await.expect("write");
        let mut resp = vec![0u8; 4];
        client.read_exact(&mut resp).await.expect("read");
        assert_eq!(&resp, b"pong");

        server.await.expect("server task");
    }

    #[tokio::test]
    async fn test_bind_replaces_existing_socket() {
        let socket_path = std::path::PathBuf::from("/tmp/mhost-transport-replace-test.sock");
        let _first = bind(&socket_path).expect("first bind");
        drop(_first);
        let second = bind(&socket_path);
        assert!(second.is_ok());
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_connect_to_nonexistent_path_returns_error() {
        let result = connect(std::path::Path::new("/tmp/mhost-no-such.sock")).await;
        assert!(result.is_err());
    }
}
