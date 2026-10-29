//! Shared helpers for the end-to-end user stories. They drive the real
//! router without opening a port.

#![allow(dead_code)] // Not every test file uses every helper.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use http_body_util::BodyExt;
use lowkey::config::Config;
use lowkey::service::hash_password;
use tower::ServiceExt;

/// A username and password for Basic auth.
pub type Login<'a> = Option<(&'a str, &'a str)>;

pub const ADMIN: Login = Some(("admin", "admin-secret"));

/// The client address requests come from unless a test says otherwise.
pub const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

const FORM: &str = "application/x-www-form-urlencoded";

/// A running app over a temporary data directory.
pub struct TestApp {
    router: Router,
    config: Config,
    _data_dir: tempfile::TempDir,
}

/// What a test sees of a response.
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub bytes: Bytes,
    /// The body as (lossy) text, for messages and text assertions.
    pub body: String,
}

impl Reply {
    pub fn header(&self, name: header::HeaderName) -> &str {
        self.headers[name].to_str().unwrap()
    }
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_config(|_| {}).await
    }

    /// An app whose default test config is adjusted by `customize`.
    pub async fn with_config(customize: impl FnOnce(&mut Config)) -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        let mut config = Config {
            data_dir: data_dir.path().to_owned(),
            admin_password_hash: Some(hash_password(ADMIN.unwrap().1)),
            ..Config::default()
        };
        customize(&mut config);
        let router = lowkey::build_app(&config).await.unwrap();
        Self {
            router,
            config,
            _data_dir: data_dir,
        }
    }

    /// Stops the app and starts a new one over the same data directory.
    pub async fn restart(&mut self) {
        self.router = lowkey::build_app(&self.config).await.unwrap();
    }

    /// Sends a request from `client` with an optional login, content type
    /// and body.
    pub async fn request_from(
        &self,
        client: IpAddr,
        method: Method,
        uri: &str,
        login: Login<'_>,
        content_type: Option<&str>,
        body: impl Into<Body>,
    ) -> Reply {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .extension(ConnectInfo(SocketAddr::new(client, 50000)));
        if let Some((user, password)) = login {
            let token = BASE64.encode(format!("{user}:{password}"));
            request = request.header(header::AUTHORIZATION, format!("Basic {token}"));
        }
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        let request = request.body(body.into()).unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&bytes).into_owned();
        Reply {
            status,
            headers,
            bytes,
            body,
        }
    }

    /// Sends a request from localhost.
    pub async fn request(
        &self,
        method: Method,
        uri: &str,
        login: Login<'_>,
        content_type: Option<&str>,
        body: impl Into<Body>,
    ) -> Reply {
        self.request_from(LOCALHOST, method, uri, login, content_type, body)
            .await
    }

    /// Sends a form-encoded body, or no body if `form` is empty.
    pub async fn send(&self, method: Method, uri: &str, login: Login<'_>, form: &str) -> Reply {
        let content_type = (!form.is_empty()).then_some(FORM);
        self.request(method, uri, login, content_type, form.to_owned())
            .await
    }

    /// An anonymous GET.
    pub async fn get(&self, uri: &str) -> Reply {
        self.request(Method::GET, uri, None, None, Body::empty())
            .await
    }

    /// Creates a user as the admin.
    pub async fn create_user(&self, user: &str, password: &str) {
        let form = format!("username={user}&password={password}");
        let reply = self.send(Method::POST, "/users", ADMIN, &form).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    }

    /// Creates a database as its owner.
    pub async fn create_db(&self, login: (&str, &str), db: &str) {
        let uri = format!("/{}", login.0);
        let form = format!("dbname={db}");
        let reply = self.send(Method::POST, &uri, Some(login), &form).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    }
}
