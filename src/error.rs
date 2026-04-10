use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("forbidden: {0}")]
    Forbidden(&'static str),

    #[error("path traversal attempt")]
    PathTraversal,

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("upload offset conflict: server={server}, client={client}")]
    OffsetConflict { server: u64, client: u64 },

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("is a directory")]
    IsADirectory,

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            Self::PathTraversal => (StatusCode::FORBIDDEN, "path_traversal"),
            Self::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            Self::ChecksumMismatch { .. } => (StatusCode::CONFLICT, "checksum_mismatch"),
            Self::OffsetConflict { .. } => (StatusCode::CONFLICT, "offset_conflict"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::IsADirectory => (StatusCode::BAD_REQUEST, "is_directory"),
            Self::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (StatusCode::NOT_FOUND, "not_found")
            }
            Self::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "io_error"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };

        if status.is_server_error() {
            tracing::error!(error = %self);
        } else if status.is_client_error() {
            tracing::warn!(error = %self);
        }

        let body = Json(ErrorBody {
            code,
            message: self.to_string(),
        });

        (status, body).into_response()
    }
}
