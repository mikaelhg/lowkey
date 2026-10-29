//! End-to-end user stories through the real router, without opening a port.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use http_body_util::BodyExt;
use lowkey::config::Config;
use lowkey::service::hash_password;
use tower::ServiceExt;

const ADMIN: (&str, &str) = ("admin", "admin-secret");

/// A running app over a temporary data directory.
struct TestApp {
    router: Router,
    _data_dir: tempfile::TempDir,
}

/// What a test sees of a response.
struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

impl TestApp {
    async fn new() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let config = Config {
            data_dir: data_dir.path().to_owned(),
            admin_password_hash: Some(hash_password(ADMIN.1)),
            ..Config::default()
        };
        let router = lowkey::build_app(&config).await.unwrap();
        Self {
            router,
            _data_dir: data_dir,
        }
    }

    /// Sends a request with an optional Basic auth login and form body.
    async fn send(
        &self,
        method: Method,
        uri: &str,
        login: Option<(&str, &str)>,
        form: &str,
    ) -> Reply {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some((user, password)) = login {
            let token = BASE64.encode(format!("{user}:{password}"));
            request = request.header(header::AUTHORIZATION, format!("Basic {token}"));
        }
        if !form.is_empty() {
            request = request.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        }
        let request = request.body(Body::from(form.to_owned())).unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        Reply {
            status,
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        }
    }

    /// Creates a user as the admin.
    async fn create_user(&self, user: &str, password: &str) {
        let form = format!("username={user}&password={password}");
        let reply = self.send(Method::POST, "/users", Some(ADMIN), &form).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    }
}

/// As an operator, I can ask the server whether it is alive, so that my
/// monitoring and load balancer know when to route traffic to it.
#[tokio::test]
async fn operator_checks_that_the_server_is_alive() {
    let app = TestApp::new().await;

    let reply = app.send(Method::GET, "/healthz", None, "").await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body, "ok");
}

/// As the admin, I can create accounts for new users, so that only people I
/// choose can store data on my server.
#[tokio::test]
async fn admin_creates_an_account_for_a_new_user() {
    let app = TestApp::new().await;
    let form = "username=alice&password=wonderland";

    let reply = app.send(Method::POST, "/users", Some(ADMIN), form).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.headers[header::LOCATION], "/alice");

    // The name is now taken.
    let reply = app.send(Method::POST, "/users", Some(ADMIN), form).await;
    assert_eq!(reply.status, StatusCode::CONFLICT);

    // Nobody else may create accounts.
    let form = "username=mallory&password=x";
    let reply = app.send(Method::POST, "/users", None, form).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(reply.headers.contains_key(header::WWW_AUTHENTICATE));
    let alice = Some(("alice", "wonderland"));
    let reply = app.send(Method::POST, "/users", alice, form).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    let wrong_admin = Some(("admin", "guess"));
    let reply = app.send(Method::POST, "/users", wrong_admin, form).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    // Reserved and malformed names are refused.
    for form in ["username=users&password=x", "username=a.b&password=x"] {
        let reply = app.send(Method::POST, "/users", Some(ADMIN), form).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{form}");
    }
}

/// As a user, I can change my password, so that I can lock out anyone who
/// learned the old one.
#[tokio::test]
async fn user_changes_their_password() {
    let app = TestApp::new().await;
    app.create_user("alice", "old-secret").await;
    let old = Some(("alice", "old-secret"));
    let new = Some(("alice", "new-secret"));

    let reply = app
        .send(
            Method::PUT,
            "/users/alice/password",
            old,
            "password=new-secret",
        )
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);

    // The old password no longer works, the new one does.
    let reply = app
        .send(Method::PUT, "/users/alice/password", old, "password=x")
        .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let reply = app
        .send(Method::PUT, "/users/alice/password", new, "password=newer")
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);

    // Other users may not change it; the admin may.
    app.create_user("bob", "builder").await;
    let bob = Some(("bob", "builder"));
    let reply = app
        .send(Method::PUT, "/users/alice/password", bob, "password=x")
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    let reply = app
        .send(
            Method::PUT,
            "/users/alice/password",
            Some(ADMIN),
            "password=reset",
        )
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);

    // An empty password is refused.
    let reset = Some(("alice", "reset"));
    let reply = app
        .send(Method::PUT, "/users/alice/password", reset, "password=")
        .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
}

/// As a user, I can delete my account, so that my data does not outlive my
/// interest in the service.
#[tokio::test]
async fn user_deletes_their_account() {
    let app = TestApp::new().await;
    app.create_user("alice", "wonderland").await;
    app.create_user("bob", "builder").await;
    let alice = Some(("alice", "wonderland"));

    // Other users may not delete it.
    let reply = app
        .send(Method::DELETE, "/users/alice", Some(("bob", "builder")), "")
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);

    let reply = app.send(Method::DELETE, "/users/alice", alice, "").await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);

    // The account is gone: the login fails and the name is free again.
    let reply = app.send(Method::DELETE, "/users/alice", alice, "").await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    app.create_user("alice", "again").await;

    // The admin may delete any account, but only existing ones.
    let reply = app
        .send(Method::DELETE, "/users/bob", Some(ADMIN), "")
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let reply = app
        .send(Method::DELETE, "/users/bob", Some(ADMIN), "")
        .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
}
