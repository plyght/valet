use crate::errors::AppError;
use axum::http::HeaderMap;
use governor::{
    clock::DefaultClock,
    state::{keyed::DefaultKeyedStateStore, InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::num::NonZeroU32;
use std::sync::Arc;

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

pub fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
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

#[derive(Clone)]
pub struct RateLimiters {
    pub global: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    pub per_token: Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>,
}

impl RateLimiters {
    pub fn new(
        global_per_sec: u32,
        global_burst: u32,
        per_token_per_sec: u32,
        per_token_burst: u32,
    ) -> Self {
        let gq = Quota::per_second(NonZeroU32::new(global_per_sec).unwrap())
            .allow_burst(NonZeroU32::new(global_burst).unwrap());
        let pq = Quota::per_second(NonZeroU32::new(per_token_per_sec).unwrap())
            .allow_burst(NonZeroU32::new(per_token_burst).unwrap());
        let global = RateLimiter::direct(gq);
        let per_token = RateLimiter::keyed(pq);
        Self {
            global: Arc::new(global),
            per_token: Arc::new(per_token),
        }
    }

    pub fn check(&self, token: Option<&str>) -> Result<(), AppError> {
        self.global.check().map_err(|_| AppError::RequestTooLarge)?;
        if let Some(t) = token {
            self.per_token
                .check_key(&t.to_string())
                .map_err(|_| AppError::RequestTooLarge)?;
        }
        Ok(())
    }
}
