use axum::{http::StatusCode, Json};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("origin denied")]
    OriginDenied,
    #[error("request too large")]
    RequestTooLarge,
    #[error("path outside root")]
    PathOutsideRoot,
    #[error("not found")]
    NotFound,
    #[error("exec denied")]
    ExecDenied,
    #[error("exec timeout")]
    ExecTimeout,
    #[error("tool error: {0}")]
    ToolError(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Serialize)]
pub struct ErrorBody<'a> {
    pub code: &'a str,
    pub message: &'a str,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Unauthorized => "Unauthorized",
            AppError::Forbidden => "Forbidden",
            AppError::OriginDenied => "OriginDenied",
            AppError::RequestTooLarge => "RequestTooLarge",
            AppError::PathOutsideRoot => "PathOutsideRoot",
            AppError::NotFound => "NotFound",
            AppError::ExecDenied => "ExecDenied",
            AppError::ExecTimeout => "ExecTimeout",
            AppError::ToolError(_) => "ToolError",
            AppError::Internal(_) => "Internal",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden | AppError::OriginDenied | AppError::PathOutsideRoot | AppError::ExecDenied => StatusCode::FORBIDDEN,
            AppError::RequestTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::ExecTimeout => StatusCode::REQUEST_TIMEOUT,
            AppError::ToolError(_) => StatusCode::BAD_REQUEST,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub fn into_response(err: AppError) -> (StatusCode, Json<ErrorBody<'static>>) {
    let code = err.code();
    let message = err.to_string();
    (err.status(), Json(ErrorBody { code, message: Box::leak(message.into_boxed_str()) }))
}
