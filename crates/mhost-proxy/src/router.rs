use std::collections::HashMap;

/// Routes incoming requests to named backends based on the Host header.
pub struct ProxyRouter {
    /// Maps hostname -> backend name
    routes: HashMap<String, String>,
    /// Fallback backend name when no hostname matches
    default_backend: Option<String>,
}

impl Default for ProxyRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyRouter {
    /// Create a new empty router.
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            default_backend: None,
        }
    }

    /// Register a hostname -> backend mapping.
    pub fn add_route(&mut self, hostname: &str, backend: &str) {
        self.routes
            .insert(hostname.to_lowercase(), backend.to_owned());
    }

    /// Set the fallback backend used when no hostname rule matches.
    pub fn set_default(&mut self, backend: &str) {
        self.default_backend = Some(backend.to_owned());
    }

    /// Resolve a `Host` header value to a backend name.
    ///
    /// The port suffix (`:8080`) is stripped before lookup so that
    /// `example.com:8080` and `example.com` resolve to the same route.
    /// Returns `None` when no rule matches and no default is set.
    pub fn resolve(&self, host: &str) -> Option<&str> {
        // Strip optional port from the Host header value.
        let bare = match host.rfind(':') {
            Some(pos) => &host[..pos],
            None => host,
        };
        let key = bare.to_lowercase();

        self.routes
            .get(&key)
            .map(String::as_str)
            .or_else(|| self.default_backend.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_exact_match() {
        let mut router = ProxyRouter::new();
        router.add_route("example.com", "backend-a");
        assert_eq!(router.resolve("example.com"), Some("backend-a"));
    }

    #[test]
    fn resolve_strips_port_from_host_header() {
        let mut router = ProxyRouter::new();
        router.add_route("example.com", "backend-a");
        assert_eq!(router.resolve("example.com:8080"), Some("backend-a"));
    }

    #[test]
    fn resolve_case_insensitive() {
        let mut router = ProxyRouter::new();
        router.add_route("Example.COM", "backend-a");
        assert_eq!(router.resolve("EXAMPLE.com"), Some("backend-a"));
    }

    #[test]
    fn resolve_default_fallback() {
        let mut router = ProxyRouter::new();
        router.add_route("known.com", "backend-a");
        router.set_default("backend-default");
        assert_eq!(router.resolve("unknown.com"), Some("backend-default"));
    }

    #[test]
    fn resolve_unknown_host_no_default_returns_none() {
        let mut router = ProxyRouter::new();
        router.add_route("known.com", "backend-a");
        assert_eq!(router.resolve("unknown.com"), None);
    }

    #[test]
    fn resolve_multiple_routes() {
        let mut router = ProxyRouter::new();
        router.add_route("api.example.com", "api-backend");
        router.add_route("www.example.com", "web-backend");
        assert_eq!(router.resolve("api.example.com"), Some("api-backend"));
        assert_eq!(router.resolve("www.example.com"), Some("web-backend"));
    }
}
