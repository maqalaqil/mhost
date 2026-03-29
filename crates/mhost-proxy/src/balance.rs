use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::upstream::BackendPool;

/// Available load-balancing strategies.
#[derive(Debug, Clone, Default)]
pub enum Strategy {
    /// Distribute requests evenly in round-robin order.
    #[default]
    RoundRobin,
    /// Always pick the backend with the fewest active connections.
    LeastConnections,
    /// Hash the client IP so the same client always reaches the same backend.
    IpHash,
}

/// Selects a backend index from a [`BackendPool`] using a chosen strategy.
pub struct Balancer {
    strategy: Strategy,
    /// Monotonically increasing counter used by RoundRobin.
    counter: AtomicUsize,
}

impl Balancer {
    /// Create a balancer with the given strategy.
    pub fn new(strategy: Strategy) -> Self {
        Self {
            strategy,
            counter: AtomicUsize::new(0),
        }
    }

    /// Select a healthy backend index.
    ///
    /// Returns `None` when no healthy backends are available.
    pub fn select(&self, pool: &BackendPool, client_ip: Option<IpAddr>) -> Option<usize> {
        let healthy = pool.healthy_backends();
        if healthy.is_empty() {
            return None;
        }

        let chosen = match self.strategy {
            Strategy::RoundRobin => {
                let idx = self.counter.fetch_add(1, Ordering::Relaxed);
                idx % healthy.len()
            }

            Strategy::LeastConnections => healthy
                .iter()
                .enumerate()
                .min_by_key(|(_, &backend_idx)| {
                    pool.backends[backend_idx]
                        .active_connections
                        .load(Ordering::Relaxed)
                })
                .map(|(pos, _)| pos)
                .unwrap_or(0),

            Strategy::IpHash => {
                let hash = match client_ip {
                    Some(ip) => {
                        let mut hasher = DefaultHasher::new();
                        ip.hash(&mut hasher);
                        hasher.finish() as usize
                    }
                    // No IP available — fall back to round-robin.
                    None => self.counter.fetch_add(1, Ordering::Relaxed),
                };
                hash % healthy.len()
            }
        };

        Some(healthy[chosen])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream::BackendPool;
    use std::net::SocketAddr;
    use std::sync::atomic::Ordering;

    fn make_pool(n: usize) -> BackendPool {
        let addrs: Vec<SocketAddr> = (0..n)
            .map(|i| format!("127.0.0.1:{}", 9000 + i).parse().unwrap())
            .collect();
        BackendPool::new(addrs)
    }

    // --- RoundRobin ---

    #[test]
    fn round_robin_cycles_through_all_backends() {
        let pool = make_pool(3);
        let balancer = Balancer::new(Strategy::RoundRobin);

        let results: Vec<usize> = (0..6)
            .map(|_| balancer.select(&pool, None).unwrap())
            .collect();

        // Each backend should appear exactly twice across 6 calls.
        for backend_idx in 0..3 {
            assert_eq!(
                results.iter().filter(|&&r| r == backend_idx).count(),
                2,
                "backend {backend_idx} should be selected twice"
            );
        }
    }

    #[test]
    fn round_robin_skips_unhealthy_backends() {
        let pool = make_pool(3);
        pool.mark_unhealthy(1);
        let balancer = Balancer::new(Strategy::RoundRobin);

        for _ in 0..10 {
            let chosen = balancer.select(&pool, None).unwrap();
            assert_ne!(chosen, 1, "unhealthy backend 1 should never be selected");
        }
    }

    #[test]
    fn round_robin_returns_none_when_all_unhealthy() {
        let pool = make_pool(2);
        pool.mark_unhealthy(0);
        pool.mark_unhealthy(1);
        let balancer = Balancer::new(Strategy::RoundRobin);
        assert_eq!(balancer.select(&pool, None), None);
    }

    // --- LeastConnections ---

    #[test]
    fn least_connections_picks_backend_with_lowest_active_count() {
        let pool = make_pool(3);
        // Simulate load: backends 0 and 2 are busy, backend 1 is idle.
        pool.backends[0]
            .active_connections
            .store(5, Ordering::Relaxed);
        pool.backends[1]
            .active_connections
            .store(1, Ordering::Relaxed);
        pool.backends[2]
            .active_connections
            .store(4, Ordering::Relaxed);

        let balancer = Balancer::new(Strategy::LeastConnections);
        assert_eq!(balancer.select(&pool, None), Some(1));
    }

    #[test]
    fn least_connections_ignores_unhealthy() {
        let pool = make_pool(3);
        // Backend 1 has fewest connections but is unhealthy.
        pool.backends[0]
            .active_connections
            .store(3, Ordering::Relaxed);
        pool.backends[1]
            .active_connections
            .store(0, Ordering::Relaxed);
        pool.backends[2]
            .active_connections
            .store(2, Ordering::Relaxed);
        pool.mark_unhealthy(1);

        let balancer = Balancer::new(Strategy::LeastConnections);
        // Should pick backend 2 (lowest among healthy).
        assert_eq!(balancer.select(&pool, None), Some(2));
    }

    // --- IpHash ---

    #[test]
    fn ip_hash_is_consistent_for_same_ip() {
        let pool = make_pool(3);
        let balancer = Balancer::new(Strategy::IpHash);
        let ip: IpAddr = "192.168.1.42".parse().unwrap();

        let first = balancer.select(&pool, Some(ip)).unwrap();
        for _ in 0..10 {
            assert_eq!(
                balancer.select(&pool, Some(ip)).unwrap(),
                first,
                "same IP must always map to the same backend"
            );
        }
    }

    #[test]
    fn ip_hash_different_ips_may_map_differently() {
        let pool = make_pool(5);
        let balancer = Balancer::new(Strategy::IpHash);

        let ips: Vec<IpAddr> = (1..=20u8)
            .map(|i| format!("10.0.0.{i}").parse().unwrap())
            .collect();

        let selections: std::collections::HashSet<usize> = ips
            .iter()
            .map(|&ip| balancer.select(&pool, Some(ip)).unwrap())
            .collect();

        // With 20 different IPs and 5 backends there should be more than one unique result.
        assert!(
            selections.len() > 1,
            "ip-hash should spread load across backends"
        );
    }

    #[test]
    fn ip_hash_no_ip_falls_back_to_round_robin() {
        // With no client IP, IpHash falls back to the internal counter.
        let pool = make_pool(2);
        let balancer = Balancer::new(Strategy::IpHash);
        // Just ensure we get a valid backend index without panicking.
        let result = balancer.select(&pool, None);
        assert!(result.is_some(), "IpHash fallback must return Some");
        assert!(result.unwrap() < 2, "index must be within pool bounds");
    }

    #[test]
    fn round_robin_single_backend_always_returns_zero() {
        let pool = make_pool(1);
        let balancer = Balancer::new(Strategy::RoundRobin);
        for _ in 0..5 {
            assert_eq!(balancer.select(&pool, None), Some(0));
        }
    }

    #[test]
    fn least_connections_empty_healthy_returns_none() {
        let pool = make_pool(1);
        pool.mark_unhealthy(0);
        let balancer = Balancer::new(Strategy::LeastConnections);
        assert_eq!(balancer.select(&pool, None), None);
    }

    #[test]
    fn least_connections_single_backend_returns_it() {
        let pool = make_pool(1);
        let balancer = Balancer::new(Strategy::LeastConnections);
        assert_eq!(balancer.select(&pool, None), Some(0));
    }

    #[test]
    fn ip_hash_returns_none_for_empty_pool() {
        let pool = make_pool(2);
        pool.mark_unhealthy(0);
        pool.mark_unhealthy(1);
        let balancer = Balancer::new(Strategy::IpHash);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        assert_eq!(balancer.select(&pool, Some(ip)), None);
    }
}
