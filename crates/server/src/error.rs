use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use snapline_domain::ApiErrorBody;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("request is invalid")]
    Validation,
    #[error("authentication is required")]
    Unauthorized,
    #[error("resource was not found")]
    NotFound,
    #[error("request conflicts with current state")]
    Conflict,
    #[error("internal server error")]
    Internal,
}

impl ApiError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Validation => (StatusCode::BAD_REQUEST, "validation_error"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict => (StatusCode::CONFLICT, "conflict"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        (
            status,
            Json(ApiErrorBody {
                code: code.into(),
                message: self.to_string(),
                request_id: None,
            }),
        )
            .into_response()
    }
}
