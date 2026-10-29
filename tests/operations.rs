//! User stories about running the server.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;

/// As an operator, I can ask the server whether it is alive, so that my
/// monitoring and load balancer know when to route traffic to it.
#[tokio::test]
async fn operator_checks_that_the_server_is_alive() {
    let app = TestApp::new().await;

    let reply = app.send(Method::GET, "/healthz", None, "").await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body, "ok");
}
