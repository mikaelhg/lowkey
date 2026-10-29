//! User stories about accounts: creating users, changing passwords and
//! deleting accounts.

mod common;

use axum::http::{Method, StatusCode, header};
use common::{ADMIN, TestApp};

/// As the admin, I can create accounts for new users, so that only people I
/// choose can store data on my server.
#[tokio::test]
async fn admin_creates_an_account_for_a_new_user() {
    let app = TestApp::new().await;
    let form = "username=alice&password=wonderland";

    let reply = app.send(Method::POST, "/users", ADMIN, form).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.headers[header::LOCATION], "/alice");

    // The name is now taken.
    let reply = app.send(Method::POST, "/users", ADMIN, form).await;
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
        let reply = app.send(Method::POST, "/users", ADMIN, form).await;
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
            ADMIN,
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
    let reply = app.send(Method::DELETE, "/users/bob", ADMIN, "").await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let reply = app.send(Method::DELETE, "/users/bob", ADMIN, "").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
}
