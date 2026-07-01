use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;

/// Unified error response format
#[derive(Debug, serde::Serialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, serde::Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

/// Application error types
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    ValidationError(String),
    PayloadTooLarge(String),
    UnsupportedMediaType(String),
    Internal(String),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::ValidationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn error_code(&self) -> &str {
        match self {
            AppError::BadRequest(_) => "BAD_REQUEST",
            AppError::Unauthorized(_) => "UNAUTHORIZED",
            AppError::Forbidden(_) => "FORBIDDEN",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::Conflict(_) => "CONFLICT",
            AppError::ValidationError(_) => "VALIDATION_ERROR",
            AppError::PayloadTooLarge(_) => "PAYLOAD_TOO_LARGE",
            AppError::UnsupportedMediaType(_) => "UNSUPPORTED_MEDIA_TYPE",
            AppError::Internal(_) => "INTERNAL",
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            AppError::BadRequest(m)
            | AppError::Unauthorized(m)
            | AppError::Forbidden(m)
            | AppError::NotFound(m)
            | AppError::Conflict(m)
            | AppError::ValidationError(m)
            | AppError::PayloadTooLarge(m)
            | AppError::UnsupportedMediaType(m)
            | AppError::Internal(m) => m,
        };
        write!(f, "{msg}")
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.error_code().to_string(),
                message: self.to_string(),
            },
        };
        (status, Json(body)).into_response()
    }
}

/// Convert sqlx errors to AppError
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => {
                AppError::NotFound("Resource not found".to_string())
            }
            sqlx::Error::Database(db_err) => {
                let msg = db_err.message().to_string();
                if msg.contains("UNIQUE constraint failed") {
                    AppError::Conflict(msg)
                } else if msg.contains("FOREIGN KEY constraint failed") {
                    AppError::NotFound("Referenced resource not found".to_string())
                } else {
                    AppError::Internal(format!("Database error: {msg}"))
                }
            }
            _ => AppError::Internal(format!("Database error: {err}")),
        }
    }
}

/// Convert JWT errors to AppError
impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        AppError::Unauthorized(format!("Authentication error: {err}"))
    }
}

/// Unified API response type
pub type ApiResult<T> = Result<(StatusCode, Json<T>), AppError>;

/// Helper: return 200 OK with JSON body
pub fn ok_response<T: serde::Serialize>(
    data: T,
) -> Result<(StatusCode, Json<T>), AppError> {
    Ok((StatusCode::OK, Json(data)))
}

/// Helper: return 201 Created with JSON body
pub fn created_response<T: serde::Serialize>(
    data: T,
) -> Result<(StatusCode, Json<T>), AppError> {
    Ok((StatusCode::CREATED, Json(data)))
}

/// Helper: return 204 No Content
pub fn no_content() -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    Ok((StatusCode::NO_CONTENT, Json(json!({}))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn test_error_response_format() {
        let err = AppError::NotFound("Channel not found".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_error_code_mapping() {
        assert_eq!(AppError::Conflict("dup".into()).error_code(), "CONFLICT");
        assert_eq!(
            AppError::Forbidden("nope".into()).error_code(),
            "FORBIDDEN"
        );
        assert_eq!(
            AppError::Unauthorized("no".into()).error_code(),
            "UNAUTHORIZED"
        );
        assert_eq!(AppError::Internal("boom".into()).error_code(), "INTERNAL");
    }
}
