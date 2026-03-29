//! WebSocket upgrade detection for the mhost reverse proxy.
//!
//! Full bidirectional streaming via `hyper`'s upgrade API is complex.
//! We detect WebSocket upgrade requests here so the proxy layer can log and
//! forward the upgrade headers transparently (all headers are already forwarded
//! by the existing `forward_request` path).

use hyper::header::UPGRADE;
use hyper::Request;

/// Returns `true` when `req` carries a WebSocket upgrade request.
///
/// Detection follows RFC 6455: the `Upgrade` header must be present and its
/// value must equal `"websocket"` (case-insensitive).
pub fn is_websocket_upgrade<T>(req: &Request<T>) -> bool {
    req.headers()
        .get(UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::{CONNECTION, UPGRADE};
    use hyper::Request;

    fn build_request_with_upgrade(upgrade_value: &str) -> Request<()> {
        Request::builder()
            .header(UPGRADE, upgrade_value)
            .header(CONNECTION, "Upgrade")
            .body(())
            .unwrap()
    }

    #[test]
    fn detects_websocket_upgrade_exact_case() {
        let req = build_request_with_upgrade("websocket");
        assert!(is_websocket_upgrade(&req));
    }

    #[test]
    fn detects_websocket_upgrade_mixed_case() {
        let req = build_request_with_upgrade("WebSocket");
        assert!(is_websocket_upgrade(&req));
    }

    #[test]
    fn detects_websocket_upgrade_uppercase() {
        let req = build_request_with_upgrade("WEBSOCKET");
        assert!(is_websocket_upgrade(&req));
    }

    #[test]
    fn rejects_non_websocket_upgrade() {
        let req = build_request_with_upgrade("h2c");
        assert!(!is_websocket_upgrade(&req));
    }

    #[test]
    fn rejects_request_without_upgrade_header() {
        let req = Request::builder().body(()).unwrap();
        assert!(!is_websocket_upgrade(&req));
    }
}
