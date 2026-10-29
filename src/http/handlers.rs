//! Request handlers. Each one extracts input, calls the service layer and
//! builds a response.

/// Liveness check for load balancers and monitoring.
pub async fn healthz() -> &'static str {
    "ok"
}
