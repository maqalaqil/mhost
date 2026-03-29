//! `mhost-proxy` — reverse proxy with host-based routing, backend pools, and
//! multiple load-balancing strategies.
//!
//! # Modules
//!
//! - [`router`]    — Map `Host` headers to named backend pools.
//! - [`upstream`]  — Backend pool and per-backend health/connection tracking.
//! - [`balance`]   — Load-balancing strategies (RoundRobin, LeastConnections, IpHash).
//! - [`server`]    — HTTP proxy server wiring everything together.
//! - [`websocket`] — WebSocket upgrade detection.
//! - [`tls`]       — Self-signed TLS cert generation and ACME stub.
//! - [`sticky`]    — Sticky-session cookie support.

pub mod balance;
pub mod router;
pub mod server;
pub mod sticky;
pub mod tls;
pub mod upstream;
pub mod websocket;
