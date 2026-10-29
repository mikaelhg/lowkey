//! End-to-end tests through the real router, without opening a port.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lowkey::config::Config;
use tower::ServiceExt;

/// As an operator, I can ask the server whether it is alive, so that my
/// monitoring and load balancer know when to route traffic to it.
#[tokio::test]
async fn operator_checks_that_the_server_is_alive() {
    let app = lowkey::build_app(&Config::default());

    let response = app
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"ok");
}
