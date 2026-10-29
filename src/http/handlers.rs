//! Request handlers. Each one extracts input, calls the service layer and
//! builds a response.

use axum::Form;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

use super::AppState;
use crate::domain::Username;
use crate::service::{Caller, ServiceError};

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
