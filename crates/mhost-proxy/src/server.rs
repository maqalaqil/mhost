use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

use crate::balance::{Balancer, Strategy};
use crate::router::ProxyRouter;
use crate::upstream::BackendPool;

// ---------------------------------------------------------------------------
// ProxyServer
// ---------------------------------------------------------------------------

/// The main reverse-proxy server.
///
/// Accepts incoming TCP connections, routes each HTTP request to a named
/// backend pool via the [`ProxyRouter`], selects a healthy upstream using the
/// pool's [`Balancer`], and streams the response back to the client.
pub struct ProxyServer {
    router: Arc<ProxyRouter>,
    /// backend_name -> (pool, balancer)
    pools: Arc<HashMap<String, (BackendPool, Balancer)>>,
}

impl ProxyServer {
    /// Create a new server with an empty pool map.
    pub fn new(router: ProxyRouter) -> Self {
        Self {
            router: Arc::new(router),
            pools: Arc::new(HashMap::new()),
        }
    }

    /// Register a backend pool for a named service.
    ///
    /// `name` must match what [`ProxyRouter::add_route`] uses as the backend
    /// value.
    pub fn add_pool(&mut self, name: &str, addrs: Vec<SocketAddr>, strategy: Strategy) {
        let pools =
            Arc::get_mut(&mut self.pools).expect("add_pool must be called before Arc is shared");
        pools.insert(
            name.to_owned(),
            (BackendPool::new(addrs), Balancer::new(strategy)),
        );
    }

    /// Bind to `listen` and serve requests until the process is killed.
    pub async fn start(
        self: Arc<Self>,
        listen: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(listen).await?;
        info!("mhost-proxy listening on {listen}");

        loop {
            match listener.accept().await {
                Ok((stream, client_addr)) => {
                    let server = Arc::clone(&self);
                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);
                        let svc = hyper::service::service_fn(move |req| {
                            let server = Arc::clone(&server);
                            async move { server.handle_request(req, client_addr).await }
                        });
                        if let Err(err) = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, svc)
                            .await
                        {
                            warn!("connection error from {client_addr}: {err}");
                        }
                    });
                }
                Err(err) => {
                    error!("accept error: {err}");
                }
            }
        }
    }

    /// Handle a single HTTP request end-to-end.
    async fn handle_request(
        &self,
        req: Request<Incoming>,
        client_addr: SocketAddr,
    ) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
        // 1. Extract Host header.
        let host = req
            .headers()
            .get(hyper::header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();

        // 2. Route host -> backend name.
        let backend_name = match self.router.resolve(&host) {
            Some(name) => name.to_owned(),
            None => {
                warn!("no route for host '{host}' from {client_addr}");
                return Ok(bad_gateway("no route for host"));
            }
        };

        // 3. Select a healthy backend from the pool.
        let (pool, balancer) = match self.pools.get(&backend_name) {
            Some(entry) => entry,
            None => {
                warn!("backend pool '{backend_name}' not found");
                return Ok(bad_gateway("backend pool not configured"));
            }
        };

        let backend_idx = match balancer.select(pool, Some(client_addr.ip())) {
            Some(idx) => idx,
            None => {
                warn!("all backends in pool '{backend_name}' are unhealthy");
                return Ok(bad_gateway("no healthy backends"));
            }
        };

        let backend_addr = pool.backends[backend_idx].addr;

        // 4. Track active connections.
        pool.backends[backend_idx]
            .active_connections
            .fetch_add(1, Ordering::Relaxed);

        // 5. Forward the request to the chosen backend.
        let result = forward_request(req, backend_addr).await;

        // 6. Decrement active connections regardless of outcome.
        pool.backends[backend_idx]
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);

        match result {
            Ok(resp) => Ok(resp),
            Err(err) => {
                error!("upstream error for backend {backend_addr}: {err}");
                pool.mark_unhealthy(backend_idx);
                Ok(bad_gateway("upstream error"))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open a fresh TCP connection to `backend_addr`, forward `req`, and collect
/// the full response body.
async fn forward_request(
    req: Request<Incoming>,
    backend_addr: SocketAddr,
) -> Result<Response<Full<Bytes>>, Box<dyn std::error::Error + Send + Sync>> {
    let stream = tokio::net::TcpStream::connect(backend_addr).await?;
    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    tokio::spawn(conn);

    // Re-assemble the request with an owned body (collect incoming bytes).
    let (parts, body) = req.into_parts();
    let body_bytes = body.collect().await?.to_bytes();

    let upstream_req = Request::from_parts(parts, Full::new(body_bytes));
    let upstream_resp = sender.send_request(upstream_req).await?;

    // Collect the upstream response body before the connection drops.
    let (resp_parts, resp_body) = upstream_resp.into_parts();
    let resp_bytes = resp_body.collect().await?.to_bytes();

    Ok(Response::from_parts(resp_parts, Full::new(resp_bytes)))
}

/// Build a simple 502 Bad Gateway response.
fn bad_gateway(reason: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(Full::new(Bytes::from(format!("502 Bad Gateway: {reason}"))))
        .expect("building 502 response cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::Strategy;
    use crate::router::ProxyRouter;

    /// Verify that a ProxyServer can be constructed and that add_pool registers
    /// the pool correctly.  No real TCP connections are made.
    #[test]
    fn proxy_server_builds_and_registers_pool() {
        let mut router = ProxyRouter::new();
        router.add_route("example.com", "backend-a");

        let mut server = ProxyServer::new(router);
        server.add_pool(
            "backend-a",
            vec!["127.0.0.1:9001".parse().unwrap()],
            Strategy::RoundRobin,
        );

        // Both router and pool are accessible after construction.
        assert!(server.pools.contains_key("backend-a"));
    }

    #[test]
    fn proxy_server_pool_with_all_strategies_compiles() {
        let router = ProxyRouter::new();
        let mut server = ProxyServer::new(router);

        let addr: SocketAddr = "127.0.0.1:9002".parse().unwrap();
        server.add_pool("rr", vec![addr], Strategy::RoundRobin);
        server.add_pool("lc", vec![addr], Strategy::LeastConnections);
        server.add_pool("ih", vec![addr], Strategy::IpHash);

        assert_eq!(server.pools.len(), 3);
    }
}
