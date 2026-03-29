//! `mhost-proxy` — reverse proxy with host-based routing, backend pools, and
//! multiple load-balancing strategies.
//!
//! # Modules
//!
//! - [`router`]   — Map `Host` headers to named backend pools.
//! - [`upstream`] — Backend pool and per-backend health/connection tracking.
//! - [`balance`]  — Load-balancing strategies (RoundRobin, LeastConnections, IpHash).
//! - [`server`]   — HTTP proxy server wiring everything together.

pub mod balance;
pub mod router;
pub mod server;
pub mod upstream;
