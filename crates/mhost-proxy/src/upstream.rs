use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// A single upstream backend server.
pub struct Backend {
    /// Network address of the backend.
    pub addr: SocketAddr,
    /// Number of requests currently being forwarded to this backend.
    pub active_connections: AtomicU32,
    /// Whether this backend is considered healthy.
    pub healthy: AtomicBool,
}

impl Backend {
    fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            active_connections: AtomicU32::new(0),
            healthy: AtomicBool::new(true),
        }
    }
}

/// A pool of upstream backends for a single named service.
pub struct BackendPool {
    pub backends: Vec<Backend>,
}

impl BackendPool {
    /// Create a pool from a list of socket addresses.
    /// All backends start healthy with zero active connections.
    pub fn new(addrs: Vec<SocketAddr>) -> Self {
        let backends = addrs.into_iter().map(Backend::new).collect();
        Self { backends }
    }

    /// Mark the backend at `idx` as unhealthy so the load balancer skips it.
    pub fn mark_unhealthy(&self, idx: usize) {
        if let Some(backend) = self.backends.get(idx) {
            backend.healthy.store(false, Ordering::Relaxed);
        }
    }

    /// Mark the backend at `idx` as healthy again.
    pub fn mark_healthy(&self, idx: usize) {
        if let Some(backend) = self.backends.get(idx) {
            backend.healthy.store(true, Ordering::Relaxed);
        }
    }

    /// Return the indices of all currently healthy backends.
    pub fn healthy_backends(&self) -> Vec<usize> {
        self.backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.healthy.load(Ordering::Relaxed))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn make_pool(n: usize) -> BackendPool {
        let addrs: Vec<SocketAddr> = (0..n)
            .map(|i| format!("127.0.0.1:{}", 8000 + i).parse().unwrap())
            .collect();
        BackendPool::new(addrs)
    }

    #[test]
    fn new_pool_all_healthy() {
        let pool = make_pool(3);
        assert_eq!(pool.healthy_backends(), vec![0, 1, 2]);
    }

    #[test]
    fn mark_unhealthy_removes_from_healthy_list() {
        let pool = make_pool(3);
        pool.mark_unhealthy(1);
        assert_eq!(pool.healthy_backends(), vec![0, 2]);
    }

    #[test]
    fn mark_healthy_restores_backend() {
        let pool = make_pool(3);
        pool.mark_unhealthy(0);
        pool.mark_unhealthy(2);
        assert_eq!(pool.healthy_backends(), vec![1]);

        pool.mark_healthy(0);
        let healthy = pool.healthy_backends();
        assert!(healthy.contains(&0));
        assert!(healthy.contains(&1));
        assert!(!healthy.contains(&2));
    }

    #[test]
    fn mark_unhealthy_all_returns_empty() {
        let pool = make_pool(2);
        pool.mark_unhealthy(0);
        pool.mark_unhealthy(1);
        assert!(pool.healthy_backends().is_empty());
    }

    #[test]
    fn out_of_bounds_mark_is_noop() {
        let pool = make_pool(2);
        pool.mark_unhealthy(99); // should not panic
        assert_eq!(pool.healthy_backends(), vec![0, 1]);
    }

    #[test]
    fn new_pool_active_connections_start_at_zero() {
        let pool = make_pool(3);
        for backend in &pool.backends {
            assert_eq!(
                backend.active_connections.load(Ordering::Relaxed),
                0,
                "all backends must start with zero active connections"
            );
        }
    }

    #[test]
    fn active_connections_can_be_incremented_and_decremented() {
        let pool = make_pool(2);
        pool.backends[0].active_connections.fetch_add(1, Ordering::Relaxed);
        pool.backends[0].active_connections.fetch_add(1, Ordering::Relaxed);
        assert_eq!(pool.backends[0].active_connections.load(Ordering::Relaxed), 2);
        pool.backends[0].active_connections.fetch_sub(1, Ordering::Relaxed);
        assert_eq!(pool.backends[0].active_connections.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn new_pool_stores_addresses_correctly() {
        let addrs: Vec<std::net::SocketAddr> = vec![
            "127.0.0.1:8000".parse().unwrap(),
            "127.0.0.1:8001".parse().unwrap(),
        ];
        let pool = BackendPool::new(addrs.clone());
        assert_eq!(pool.backends.len(), 2);
        assert_eq!(pool.backends[0].addr, addrs[0]);
        assert_eq!(pool.backends[1].addr, addrs[1]);
    }

    #[test]
    fn empty_pool_has_no_healthy_backends() {
        let pool = BackendPool::new(vec![]);
        assert!(pool.healthy_backends().is_empty());
    }
}
