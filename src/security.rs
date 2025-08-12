use crate::errors::AppError;
use axum::http::HeaderMap;

pub fn require_bearer(headers: &HeaderMap, expected: &str) -> Result<(), AppError> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    if !auth.starts_with("Bearer ") {
        return Err(AppError::Unauthorized);
    }
    let token = &auth[7..];
    if token != expected {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

pub fn check_origin(headers: &HeaderMap, allowed: &[String]) -> Result<(), AppError> {
    let origin = headers
        .get("Origin")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::OriginDenied)?;
    if allowed.iter().any(|o| o == origin) {
        Ok(())
    } else {
        Err(AppError::OriginDenied)
    }
}

pub fn content_length_ok(headers: &HeaderMap, max_kb: usize) -> Result<(), AppError> {
    if let Some(len) = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
    {
        if len > max_kb * 1024 {
            return Err(AppError::RequestTooLarge);
        }
    }
    Ok(())
}
