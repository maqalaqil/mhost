use std::io;

use mhost_core::protocol::{RpcRequest, RpcResponse};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_FRAME_SIZE: u32 = 10 * 1024 * 1024; // 10 MB

// ---------------------------------------------------------------------------
// Low-level frame I/O
// ---------------------------------------------------------------------------

/// Write a length-prefixed frame: 4-byte big-endian length + payload bytes.
pub async fn write_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    data: &[u8],
) -> io::Result<()> {
    let len = data.len() as u32;
    writer.write_u32(len).await?;
    writer.write_all(data).await?;
    Ok(())
}

/// Read a length-prefixed frame.  Returns the payload bytes.
/// Returns an error if the declared length exceeds `MAX_FRAME_SIZE`.
pub async fn read_frame<R: AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<Vec<u8>> {
    let len = reader.read_u32().await?;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {} bytes (max {})", len, MAX_FRAME_SIZE),
        ));
    }
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf).await?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Typed wrappers
// ---------------------------------------------------------------------------

/// Serialise an `RpcRequest` and write it as a length-prefixed frame.
pub async fn write_request<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    req: &RpcRequest,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(req).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_frame(writer, &bytes).await
}

/// Read a length-prefixed frame and deserialise it as an `RpcRequest`.
pub async fn read_request<R: AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<RpcRequest> {
    let bytes = read_frame(reader).await?;
    serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Serialise an `RpcResponse` and write it as a length-prefixed frame.
pub async fn write_response<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    resp: &RpcResponse,
) -> io::Result<()> {
    let bytes =
        serde_json::to_vec(resp).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_frame(writer, &bytes).await
}

/// Read a length-prefixed frame and deserialise it as an `RpcResponse`.
pub async fn read_response<R: AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<RpcResponse> {
    let bytes = read_frame(reader).await?;
    serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mhost_core::protocol::{RpcRequest, RpcResponse};
    use serde_json::json;
    use std::io::Cursor;

    // Helper: write into an in-memory buffer, then read back from the same bytes.
    async fn roundtrip_frame(payload: &[u8]) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, payload).await.expect("write_frame");
        let mut cursor = Cursor::new(buf);
        read_frame(&mut cursor).await.expect("read_frame")
    }

    // -- Frame roundtrip (write then read bytes) -----------------------------

    #[tokio::test]
    async fn test_frame_roundtrip() {
        let payload = b"hello, frame!";
        let result = roundtrip_frame(payload).await;
        assert_eq!(result, payload);
    }

    // -- Request roundtrip --------------------------------------------------

    #[tokio::test]
    async fn test_request_roundtrip() {
        let req = RpcRequest::new(1, "daemon.ping", json!(null));
        let mut buf: Vec<u8> = Vec::new();
        write_request(&mut buf, &req).await.expect("write_request");
        let mut cursor = Cursor::new(buf);
        let decoded = read_request(&mut cursor).await.expect("read_request");
        assert_eq!(decoded.id, req.id);
        assert_eq!(decoded.method, req.method);
        assert_eq!(decoded.jsonrpc, "2.0");
    }

    // -- Response roundtrip -------------------------------------------------

    #[tokio::test]
    async fn test_response_roundtrip() {
        let resp = RpcResponse::success(42, json!({"status": "ok"}));
        let mut buf: Vec<u8> = Vec::new();
        write_response(&mut buf, &resp).await.expect("write_response");
        let mut cursor = Cursor::new(buf);
        let decoded = read_response(&mut cursor).await.expect("read_response");
        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.result, Some(json!({"status": "ok"})));
    }

    // -- Oversized frame rejected -------------------------------------------

    #[tokio::test]
    async fn test_oversized_frame_rejected() {
        // Write a 4-byte length header claiming 20 MB without an actual payload.
        let oversized_len: u32 = 20 * 1024 * 1024; // 20 MB
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&oversized_len.to_be_bytes());
        let mut cursor = Cursor::new(buf);
        let err = read_frame(&mut cursor).await.expect_err("should reject oversized frame");
        assert!(
            err.to_string().contains("too large"),
            "error should mention 'too large', got: {}",
            err
        );
    }
}
