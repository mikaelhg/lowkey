//! User stories about running the server.

mod common;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::http::{Method, StatusCode};
use common::TestApp;
use lowkey::config::{Config, Limits};

/// As an operator, I can ask the server whether it is alive, so that my
/// monitoring and load balancer know when to route traffic to it.
#[tokio::test]
async fn operator_checks_that_the_server_is_alive() {
    let app = TestApp::new().await;

    let reply = app.send(Method::GET, "/healthz", None, "").await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body, "ok");
}

/// As an operator, I can start from the example config file, so that I see
/// every setting and its default in one place.
#[test]
fn operator_starts_from_the_example_config() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/lowkey.toml");

    let config = Config::from_file(&path).unwrap();

    // Deployment-specific values, matching the systemd unit and Caddyfile.
    assert_eq!(config.listen, SocketAddr::from(([127, 0, 0, 1], 8080)));
    assert_eq!(config.data_dir, PathBuf::from("/var/lib/lowkey"));
    assert!(config.trust_forwarded_for);
    // Every limit is spelled out with its default value.
    assert_eq!(config.limits, Limits::default());
}
