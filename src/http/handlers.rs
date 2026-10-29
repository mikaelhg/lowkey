//! Request handlers. Each one extracts input, calls the service layer and
//! builds a response.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::{Form, Json};
use serde::Deserialize;

use super::AppState;
use crate::domain::{ContentType, DbName, DocName, Document, JsonPath, Username};
use crate::service::{Caller, ServiceError, Written};

/// Liveness check for load balancers and monitoring.
pub async fn healthz() -> &'static str {
    "ok"
}

#[derive(Deserialize)]
pub struct NewUser {
    username: String,
    password: String,
}

/// `POST /users`
pub async fn create_user(
    State(services): State<AppState>,
    caller: Caller,
    Form(form): Form<NewUser>,
) -> Result<impl IntoResponse, ServiceError> {
    let user: Username = form.username.parse()?;
    services
        .users
        .create(&caller, &user, &form.password)
        .await?;
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/{user}"))],
    ))
}

#[derive(Deserialize)]
pub struct NewPassword {
    password: String,
}

/// `PUT /users/{user}/password`
pub async fn change_password(
    State(services): State<AppState>,
    caller: Caller,
    Path(user): Path<String>,
    Form(form): Form<NewPassword>,
) -> Result<StatusCode, ServiceError> {
    let user: Username = user.parse()?;
    services
        .users
        .change_password(&caller, &user, &form.password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /users/{user}`
pub async fn delete_user(
    State(services): State<AppState>,
    caller: Caller,
    Path(user): Path<String>,
) -> Result<StatusCode, ServiceError> {
    let user: Username = user.parse()?;
    services.users.delete(&caller, &user).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct NewDatabase {
    dbname: String,
}

/// `GET /{user}`
pub async fn list_dbs(
    State(services): State<AppState>,
    Path(user): Path<String>,
) -> Result<Json<Vec<DbName>>, ServiceError> {
    let user: Username = user.parse()?;
    Ok(Json(services.databases.list(&user).await?))
}

/// `POST /{user}`
pub async fn create_db(
    State(services): State<AppState>,
    caller: Caller,
    Path(user): Path<String>,
    Form(form): Form<NewDatabase>,
) -> Result<impl IntoResponse, ServiceError> {
    let user: Username = user.parse()?;
    let db: DbName = form.dbname.parse()?;
    services.databases.create(&caller, &user, &db).await?;
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/{user}/{db}"))],
    ))
}

/// `DELETE /{user}/{db}`
pub async fn delete_db(
    State(services): State<AppState>,
    caller: Caller,
    Path((user, db)): Path<(String, String)>,
) -> Result<StatusCode, ServiceError> {
    let user: Username = user.parse()?;
    let db: DbName = db.parse()?;
    services.databases.delete(&caller, &user, &db).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /{user}/{db}`
pub async fn list_docs(
    State(services): State<AppState>,
    Path((user, db)): Path<(String, String)>,
) -> Result<Json<Vec<DocName>>, ServiceError> {
    let user: Username = user.parse()?;
    let db: DbName = db.parse()?;
    Ok(Json(services.documents.list(&user, &db).await?))
}

/// `POST /{user}/{db}`: stores a document under a generated name.
pub async fn push_doc(
    State(services): State<AppState>,
    caller: Caller,
    Path((user, db)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ServiceError> {
    let user: Username = user.parse()?;
    let db: DbName = db.parse()?;
    let document = document_from(&headers, body)?;
    let doc = services
        .documents
        .push(&caller, &user, &db, &document)
        .await?;
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/{user}/{db}/{doc}"))],
        Json(serde_json::json!({ "id": doc })),
    ))
}

/// `GET /{user}/{db}/{doc}`: the stored body with its content type.
pub async fn get_doc(
    State(services): State<AppState>,
    Path((user, db, doc)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ServiceError> {
    let user: Username = user.parse()?;
    let db: DbName = db.parse()?;
    let doc: DocName = doc.parse()?;
    let document = services.documents.read(&user, &db, &doc).await?;
    Ok((
        [(header::CONTENT_TYPE, document.content_type.as_mime())],
        document.body,
    ))
}

/// `GET /{user}/{db}/{doc}/{*path}`: one value from inside a JSON document.
pub async fn get_value(
    State(services): State<AppState>,
    Path((user, db, doc, path)): Path<(String, String, String, String)>,
) -> Result<Json<serde_json::Value>, ServiceError> {
    let user: Username = user.parse()?;
    let db: DbName = db.parse()?;
    let doc: DocName = doc.parse()?;
    let path: JsonPath = path.parse()?;
    let value = services
        .documents
        .read_value(&user, &db, &doc, &path)
        .await?;
    Ok(Json(value))
}

/// `PUT /{user}/{db}/{doc}`: 201 if created, 204 if replaced.
pub async fn put_doc(
    State(services): State<AppState>,
    caller: Caller,
    Path((user, db, doc)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ServiceError> {
    let user: Username = user.parse()?;
    let db: DbName = db.parse()?;
    let doc: DocName = doc.parse()?;
    let document = document_from(&headers, body)?;
    let written = services
        .documents
        .put(&caller, &user, &db, &doc, &document)
        .await?;
    Ok(match written {
        Written::Created => StatusCode::CREATED,
        Written::Replaced => StatusCode::NO_CONTENT,
    })
}

/// `DELETE /{user}/{db}/{doc}`
pub async fn delete_doc(
    State(services): State<AppState>,
    caller: Caller,
    Path((user, db, doc)): Path<(String, String, String)>,
) -> Result<StatusCode, ServiceError> {
    let user: Username = user.parse()?;
    let db: DbName = db.parse()?;
    let doc: DocName = doc.parse()?;
    services.documents.delete(&caller, &user, &db, &doc).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Pairs a request body with its declared content type.
fn document_from(headers: &HeaderMap, body: Bytes) -> Result<Document, ServiceError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(ContentType::from_mime)
        .ok_or(ServiceError::UnsupportedContentType)?;
    Ok(Document { content_type, body })
}
