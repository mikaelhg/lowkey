//! User stories about databases and the documents stored in them.

mod common;

use axum::body::Body;
use axum::http::{Method, StatusCode, header};
use common::{ADMIN, TestApp};

const ALICE: (&str, &str) = ("alice", "wonderland");
const JSON: Option<&str> = Some("application/json");

/// An app with user alice and her database `notes`.
async fn app_with_notes() -> TestApp {
    let app = TestApp::new().await;
    app.create_user(ALICE.0, ALICE.1).await;
    app.create_db(ALICE, "notes").await;
    app
}

/// As a user, I can create a database and store documents under names I
/// choose, so that I can find them again later.
#[tokio::test]
async fn user_stores_named_documents_in_a_database() {
    let app = TestApp::new().await;
    app.create_user(ALICE.0, ALICE.1).await;

    let reply = app
        .send(Method::POST, "/alice", Some(ALICE), "dbname=notes")
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.header(header::LOCATION), "/alice/notes");
    let reply = app
        .send(Method::POST, "/alice", Some(ALICE), "dbname=notes")
        .await;
    assert_eq!(reply.status, StatusCode::CONFLICT);

    let todo = r#"{"any": "valid", "json": "document", "or": ["you", "know"]}"#;
    let reply = app
        .request(Method::PUT, "/alice/notes/todo", Some(ALICE), JSON, todo)
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);

    let reply = app.get("/alice/notes/todo").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(reply.body, todo);

    // Storing under the same name replaces the document.
    let text = Some("text/plain");
    let reply = app
        .request(
            Method::PUT,
            "/alice/notes/todo",
            Some(ALICE),
            text,
            "Whatever, I do what I want!",
        )
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    let reply = app.get("/alice/notes/todo").await;
    assert_eq!(
        reply.header(header::CONTENT_TYPE),
        "text/plain; charset=utf-8"
    );
    assert_eq!(reply.body, "Whatever, I do what I want!");

    // Binary documents come back byte for byte.
    let binary = Some("application/octet-stream");
    let bytes = vec![0u8, 159, 146, 150, 255];
    let reply = app
        .request(
            Method::PUT,
            "/alice/notes/blob",
            Some(ALICE),
            binary,
            bytes.clone(),
        )
        .await;
    assert_eq!(reply.status, StatusCode::CREATED);
    assert_eq!(app.get("/alice/notes/blob").await.bytes, bytes);

    // Listings show what exists, as JSON.
    let reply = app.get("/alice").await;
    assert_eq!(reply.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(reply.body, r#"["notes"]"#);
    assert_eq!(app.get("/alice/notes").await.body, r#"["blob","todo"]"#);
}

/// As a user, I can push documents without naming them, so that I can log
/// events without inventing keys.
#[tokio::test]
async fn user_pushes_anonymous_documents() {
    let app = app_with_notes().await;

    let first = app
        .request(
            Method::POST,
            "/alice/notes",
            Some(ALICE),
            JSON,
            r#"{"n": 1}"#,
        )
        .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    let second = app
        .request(
            Method::POST,
            "/alice/notes",
            Some(ALICE),
            JSON,
            r#"{"n": 2}"#,
        )
        .await;
    assert_eq!(second.status, StatusCode::CREATED);

    // Each gets its own generated name, returned in the body and Location.
    let first_location = first.header(header::LOCATION);
    let second_location = second.header(header::LOCATION);
    assert_ne!(first_location, second_location);
    let id: serde_json::Value = serde_json::from_str(&first.body).unwrap();
    assert_eq!(
        first_location,
        format!("/alice/notes/{}", id["id"].as_str().unwrap())
    );

    assert_eq!(app.get(first_location).await.body, r#"{"n": 1}"#);
    assert_eq!(app.get(second_location).await.body, r#"{"n": 2}"#);
}

/// As a user, I can let anyone read my data while only I can change it, so
/// that I can publish data safely.
#[tokio::test]
async fn anyone_reads_but_only_the_owner_writes() {
    let app = app_with_notes().await;
    app.create_user("bob", "builder").await;
    let bob = Some(("bob", "builder"));
    app.request(Method::PUT, "/alice/notes/todo", Some(ALICE), JSON, "[1]")
        .await;

    // Anyone can read.
    assert_eq!(app.get("/alice").await.status, StatusCode::OK);
    assert_eq!(app.get("/alice/notes").await.status, StatusCode::OK);
    assert_eq!(app.get("/alice/notes/todo").await.body, "[1]");

    // Strangers can't write.
    for (login, status) in [
        (None, StatusCode::UNAUTHORIZED),
        (bob, StatusCode::FORBIDDEN),
    ] {
        let attempts = [
            app.send(Method::POST, "/alice", login, "dbname=evil").await,
            app.send(Method::DELETE, "/alice/notes", login, "").await,
            app.request(Method::POST, "/alice/notes", login, JSON, "[2]")
                .await,
            app.request(Method::PUT, "/alice/notes/todo", login, JSON, "[2]")
                .await,
            app.send(Method::DELETE, "/alice/notes/todo", login, "")
                .await,
        ];
        for reply in attempts {
            assert_eq!(reply.status, status, "{}", reply.body);
        }
    }
    assert_eq!(app.get("/alice/notes/todo").await.body, "[1]");

    // The admin can, for moderation.
    let reply = app
        .request(Method::PUT, "/alice/notes/todo", ADMIN, JSON, "[3]")
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
}

/// As a user, I can delete documents and whole databases, so that I can
/// clean up what I no longer need.
#[tokio::test]
async fn user_deletes_documents_and_databases() {
    let app = app_with_notes().await;
    for doc in ["a", "b"] {
        let uri = format!("/alice/notes/{doc}");
        app.request(Method::PUT, &uri, Some(ALICE), JSON, "{}")
            .await;
    }

    let reply = app
        .send(Method::DELETE, "/alice/notes/a", Some(ALICE), "")
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert_eq!(
        app.get("/alice/notes/a").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.get("/alice/notes").await.body, r#"["b"]"#);
    let reply = app
        .send(Method::DELETE, "/alice/notes/a", Some(ALICE), "")
        .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);

    let reply = app
        .send(Method::DELETE, "/alice/notes", Some(ALICE), "")
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert_eq!(app.get("/alice/notes").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.get("/alice/notes/b").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.get("/alice").await.body, "[]");
}

/// As a user, I get a clear error when I send something the server can't
/// store, so that I can fix my request.
#[tokio::test]
async fn server_explains_what_it_cannot_store() {
    let app = app_with_notes().await;
    let put = |content_type: Option<&'static str>, body: Body| {
        app.request(
            Method::PUT,
            "/alice/notes/doc",
            Some(ALICE),
            content_type,
            body,
        )
    };

    let reply = put(JSON, Body::from("{not json")).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert!(reply.body.contains("JSON"), "{}", reply.body);

    let reply = put(Some("text/plain"), Body::from(vec![0xff, 0xfe])).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);

    let reply = put(Some("image/png"), Body::from("png")).await;
    assert_eq!(reply.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let reply = put(None, Body::from("what am I")).await;
    assert_eq!(reply.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // Names must be valid, and the database and user must exist.
    let reply = app
        .request(Method::PUT, "/alice/notes/a.b", Some(ALICE), JSON, "{}")
        .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    let reply = app
        .request(Method::PUT, "/alice/nope/doc", Some(ALICE), JSON, "{}")
        .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/nobody").await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/alice/nope").await.status, StatusCode::NOT_FOUND);

    // Nothing was stored along the way.
    assert_eq!(app.get("/alice/notes").await.body, "[]");
}
