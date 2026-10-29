//! The single mapping from service errors to HTTP responses.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::service::ServiceError;

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound | Self::NotJson => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::UnsupportedContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::QuotaExceeded(_) => StatusCode::INSUFFICIENT_STORAGE,
            Self::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // Internal details go to the log, not to the client.
        let message = if let Self::Storage(error) = &self {
            tracing::error!(%error, "storage failure");
            "internal error".to_owned()
        } else {
            self.to_string()
        };

        let mut response = (status, message).into_response();
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static(r#"Basic realm="lowkey", charset="UTF-8""#),
            );
        }
        response
    }
}
