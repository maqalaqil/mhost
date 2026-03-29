//! Sticky-session support for the mhost reverse proxy.
//!
//! A sticky session pins a client to the same backend across multiple requests
//! by storing the chosen backend index in a cookie (`MHOST_STICKY`).
//!
//! # Usage
//!
//! 1. On an *incoming* request call [`StickySession::get_backend_from_request`].
//!    If it returns `Some(idx)`, skip load-balancing and use that backend directly.
//! 2. After choosing (or confirming) the backend, call
//!    [`StickySession::set_backend_on_response`] to stamp the cookie on the
//!    outgoing response so the client sends it back next time.

use hyper::header::{COOKIE, SET_COOKIE};
use hyper::{Request, Response};

/// Name of the sticky-session cookie.
const STICKY_COOKIE: &str = "MHOST_STICKY";

/// Stateless helper for reading and writing sticky-session cookies.
pub struct StickySession;

impl StickySession {
    /// Extract the backend index from the sticky cookie in `req`, if present.
    ///
    /// Returns `None` when no valid sticky cookie is found (first visit, cookie
    /// cleared, or unparseable value).
    pub fn get_backend_from_request<T>(req: &Request<T>) -> Option<usize> {
        let cookie_header = req.headers().get(COOKIE)?.to_str().ok()?;

        for cookie in cookie_header.split(';') {
            let cookie = cookie.trim();
            if let Some(value) = cookie.strip_prefix(&format!("{STICKY_COOKIE}=")) {
                return value.parse::<usize>().ok();
            }
        }

        None
    }

    /// Append a `Set-Cookie` header to `resp` pinning the client to
    /// `backend_idx`.
    ///
    /// The cookie is scoped to all paths (`Path=/`), inaccessible to JavaScript
    /// (`HttpOnly`), and restricted to same-site requests (`SameSite=Lax`).
    pub fn set_backend_on_response<T>(resp: &mut Response<T>, backend_idx: usize) {
        let cookie = format!(
            "{STICKY_COOKIE}={backend_idx}; Path=/; HttpOnly; SameSite=Lax"
        );
        resp.headers_mut()
            .insert(SET_COOKIE, cookie.parse().unwrap());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::SET_COOKIE;
    use hyper::{Request, Response};

    // --- get_backend_from_request ---

    #[test]
    fn returns_none_when_no_cookie_header() {
        let req = Request::builder().body(()).unwrap();
        assert_eq!(StickySession::get_backend_from_request(&req), None);
    }

    #[test]
    fn returns_none_when_cookie_header_missing_sticky_key() {
        let req = Request::builder()
            .header(COOKIE, "session=abc123; theme=dark")
            .body(())
            .unwrap();
        assert_eq!(StickySession::get_backend_from_request(&req), None);
    }

    #[test]
    fn returns_backend_index_from_sticky_cookie() {
        let req = Request::builder()
            .header(COOKIE, "MHOST_STICKY=2")
            .body(())
            .unwrap();
        assert_eq!(StickySession::get_backend_from_request(&req), Some(2));
    }

    #[test]
    fn returns_backend_index_when_multiple_cookies_present() {
        let req = Request::builder()
            .header(COOKIE, "session=xyz; MHOST_STICKY=5; other=val")
            .body(())
            .unwrap();
        assert_eq!(StickySession::get_backend_from_request(&req), Some(5));
    }

    #[test]
    fn returns_none_when_sticky_cookie_value_is_not_a_number() {
        let req = Request::builder()
            .header(COOKIE, "MHOST_STICKY=notanumber")
            .body(())
            .unwrap();
        assert_eq!(StickySession::get_backend_from_request(&req), None);
    }

    #[test]
    fn returns_zero_when_backend_index_is_zero() {
        let req = Request::builder()
            .header(COOKIE, "MHOST_STICKY=0")
            .body(())
            .unwrap();
        assert_eq!(StickySession::get_backend_from_request(&req), Some(0));
    }

    // --- set_backend_on_response ---

    #[test]
    fn set_backend_writes_set_cookie_header() {
        let mut resp = Response::builder().body(()).unwrap();
        StickySession::set_backend_on_response(&mut resp, 3);

        let cookie = resp
            .headers()
            .get(SET_COOKIE)
            .expect("SET_COOKIE header must be present")
            .to_str()
            .unwrap();

        assert!(
            cookie.contains("MHOST_STICKY=3"),
            "cookie value must contain MHOST_STICKY=3, got: {cookie}"
        );
        assert!(cookie.contains("Path=/"), "cookie must have Path=/");
        assert!(cookie.contains("HttpOnly"), "cookie must be HttpOnly");
        assert!(cookie.contains("SameSite=Lax"), "cookie must be SameSite=Lax");
    }

    #[test]
    fn set_backend_writes_correct_index_for_various_values() {
        for idx in [0usize, 1, 7, 100] {
            let mut resp = Response::builder().body(()).unwrap();
            StickySession::set_backend_on_response(&mut resp, idx);

            let cookie = resp
                .headers()
                .get(SET_COOKIE)
                .unwrap()
                .to_str()
                .unwrap();

            assert!(
                cookie.contains(&format!("MHOST_STICKY={idx}")),
                "expected MHOST_STICKY={idx} in cookie, got: {cookie}"
            );
        }
    }
}
