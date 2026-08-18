use actix_web::{HttpResponse, ResponseError};
use serde_json::json;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sea_orm::DbErr),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Auth error: {0}")]
    AuthError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Forbidden")]
    Forbidden,

    #[error("Permission denied")]
    PermissionDenied,

    #[error("External API error: {0}")]
    ExternalApiError(String),

    /// 外部 API 的瞬时错误（如上游服务抖动），可退避重试
    #[error("External API error (retryable): {0}")]
    ExternalApiRetryable(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("JWT error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),

    #[error("HTTP request error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("JSON serialization/deserialization error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),
}

impl AppError {
    /// 是否为可重试的瞬时错误（如上游服务抖动）
    pub fn is_retryable(&self) -> bool {
        matches!(self, AppError::ExternalApiRetryable(_))
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let (status_code, error_code, message) = match self {
            AppError::ValidationError(msg) => {
                log::warn!("Validation error: {msg}");
                (
                    actix_web::http::StatusCode::BAD_REQUEST,
                    "VALIDATION_ERROR",
                    msg,
                )
            }
            AppError::AuthError(msg) => {
                log::warn!("Authentication error: {msg}");
                (actix_web::http::StatusCode::UNAUTHORIZED, "AUTH_ERROR", msg)
            }
            AppError::NotFound(msg) => (actix_web::http::StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            AppError::Forbidden => {
                log::warn!("Forbidden access");
                (
                    actix_web::http::StatusCode::FORBIDDEN,
                    "FORBIDDEN",
                    &"Forbidden".to_string(),
                )
            }
            AppError::PermissionDenied => {
                log::warn!("Permission denied");
                (
                    actix_web::http::StatusCode::FORBIDDEN,
                    "FORBIDDEN",
                    &"Permission denied".to_string(),
                )
            }
            AppError::ExternalApiError(msg) => {
                log::error!("External API error: {msg}");
                (
                    actix_web::http::StatusCode::BAD_GATEWAY,
                    "EXTERNAL_API_ERROR",
                    msg,
                )
            }
            AppError::ExternalApiRetryable(msg) => {
                log::error!("External API error (retries exhausted): {msg}");
                (
                    actix_web::http::StatusCode::BAD_GATEWAY,
                    "EXTERNAL_API_ERROR",
                    msg,
                )
            }
            AppError::DatabaseError(err) => {
                log::error!("Database error: {err}");
                (
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "DATABASE_ERROR",
                    &"Database error".to_string(),
                )
            }
            _ => {
                log::error!("Internal error: {self}");
                (
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    &"Internal server error".to_string(),
                )
            }
        };

        HttpResponse::build(status_code).json(json!({
            "success": false,
            "error": {
                "code": error_code,
                "message": message
            }
        }))
    }
}
