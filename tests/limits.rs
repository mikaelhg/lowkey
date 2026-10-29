//! User stories about the limits that keep one client from exhausting a
//! small server.

mod common;

use std::net::{IpAddr, Ipv4Addr};

use axum::body::Body;
use axum::http::{Method, StatusCode, header};
use common::{LOCALHOST, TestApp};

const ALICE: (&str, &str) = ("alice", "wonderland");
const JSON: Option<&str> = Some("application/json");
const ELSEWHERE: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 7));

/// As an operator, I can cap the size of a single document, so that one
/// upload can't fill the disk or memory.
#[tokio::test]
async fn operator_caps_the_size_of_a_document() {
    let app = TestApp::with_config(|config| config.limits.max_document_bytes = 16).await;
    app.create_user(ALICE.0, ALICE.1).await;
    app.create_db(ALICE, "notes").await;

    let at_limit = r#""12345678901234""#;
    assert_eq!(at_limit.len(), 16);
    let reply = app
        .request(Method::PUT, "/alice/notes/doc", Some(ALICE), JSON, at_limit)
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);

    let too_big = r#""123456789012345""#;
    let reply = app
        .request(Method::PUT, "/alice/notes/doc", Some(ALICE), JSON, too_big)
        .await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
    let reply = app
        .request(Method::POST, "/alice/notes", Some(ALICE), JSON, too_big)
        .await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);

    assert_eq!(app.get("/alice/notes/doc").await.body, at_limit);
}

/// As an operator, I can give each user a storage quota, so that no single
/// user can take over the server.
#[tokio::test]
async fn operator_gives_each_user_a_storage_quota() {
    let mut app = TestApp::with_config(|config| {
        config.limits.max_bytes_per_user = 10;
        config.limits.max_documents_per_user = 3;
        config.limits.max_databases_per_user = 2;
    })
    .await;
    app.create_user(ALICE.0, ALICE.1).await;
    let put = |uri: &'static str, body: &'static str| {
        app.request(Method::PUT, uri, Some(ALICE), JSON, body)
    };

    // Databases: two fit, a third doesn't.
    app.create_db(ALICE, "notes").await;
    app.create_db(ALICE, "spare").await;
    let reply = app
        .send(Method::POST, "/alice", Some(ALICE), "dbname=third")
        .await;
    assert_eq!(reply.status, StatusCode::INSUFFICIENT_STORAGE);
    assert!(reply.body.contains("quota"), "{}", reply.body);

    // Bytes: 5 + 5 fill the quota, one more byte doesn't fit.
    assert_eq!(
        put("/alice/notes/a", "12345").await.status,
        StatusCode::CREATED
    );
    assert_eq!(
        put("/alice/notes/b", "12345").await.status,
        StatusCode::CREATED
    );
    let reply = put("/alice/notes/c", "1").await;
    assert_eq!(reply.status, StatusCode::INSUFFICIENT_STORAGE);

    // Replacing a document only counts the difference.
    assert_eq!(
        put("/alice/notes/a", "1").await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(put("/alice/notes/c", "1").await.status, StatusCode::CREATED);

    // Documents: three is the limit, for anonymous pushes too.
    let reply = app
        .request(Method::POST, "/alice/notes", Some(ALICE), JSON, "1")
        .await;
    assert_eq!(reply.status, StatusCode::INSUFFICIENT_STORAGE);

    // Deleting a document frees its share.
    let reply = app
        .send(Method::DELETE, "/alice/notes/c", Some(ALICE), "")
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let reply = app
        .request(Method::POST, "/alice/notes", Some(ALICE), JSON, "1")
        .await;
    assert_eq!(reply.status, StatusCode::CREATED);

    // Usage survives a restart.
    app.restart().await;
    let put = |uri: &'static str, body: &'static str| {
        app.request(Method::PUT, uri, Some(ALICE), JSON, body)
    };
    let reply = put("/alice/spare/x", "1").await;
    assert_eq!(reply.status, StatusCode::INSUFFICIENT_STORAGE);

    // Deleting a database frees it and everything in it.
    let reply = app
        .send(Method::DELETE, "/alice/notes", Some(ALICE), "")
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    app.create_db(ALICE, "third").await;
    let reply = put("/alice/third/x", "1234567890").await;
    assert_eq!(reply.status, StatusCode::CREATED);
}

/// As an operator, I can limit how often one client may call the server, so
/// that a runaway script can't starve everyone else.
#[tokio::test]
async fn operator_limits_how_often_a_client_calls() {
    let app = TestApp::with_config(|config| config.limits.requests_per_minute = 5).await;
    let get = |client| app.request_from(client, Method::GET, "/healthz", None, None, Body::empty());

    for _ in 0..5 {
        assert_eq!(get(LOCALHOST).await.status, StatusCode::OK);
    }
    let reply = get(LOCALHOST).await;
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(reply.headers.contains_key(header::RETRY_AFTER));

    // Other clients are unaffected.
    assert_eq!(get(ELSEWHERE).await.status, StatusCode::OK);
}

/// As a user, I am protected from password guessing, so that a weak
/// password can't be brute-forced quickly.
#[tokio::test]
async fn password_guessers_are_locked_out() {
    let app = TestApp::with_config(|config| config.limits.failed_logins_per_minute = 3).await;
    app.create_user(ALICE.0, ALICE.1).await;
    let app = &app;
    let create_db = |client, password: &'static str, db: &'static str| {
        let form = format!("dbname={db}");
        async move {
            app.request_from(
                client,
                Method::POST,
                "/alice",
                Some(("alice", password)),
                Some("application/x-www-form-urlencoded"),
                form,
            )
            .await
        }
    };

    for _ in 0..3 {
        let reply = create_db(LOCALHOST, "guess", "x").await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }

    // Now even the right password is refused from that client...
    let reply = create_db(LOCALHOST, ALICE.1, "mine").await;
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(reply.headers.contains_key(header::RETRY_AFTER));

    // ...but alice can still log in from elsewhere, and reads stay open.
    let reply = create_db(ELSEWHERE, ALICE.1, "mine").await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(app.get("/alice").await.status, StatusCode::OK);
}
