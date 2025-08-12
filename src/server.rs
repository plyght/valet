use crate::{
    config::Config,
    errors::{into_response, AppError},
    mcp::{
        registry::{CallRequest, ToolRegistry},
        types::{Capabilities, ToolInfo},
    },
    security,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub registry: Arc<ToolRegistry>,
    pub rls: crate::security::RateLimiters,
}

pub type StreamBody = axum::body::Body;

pub async fn serve(cfg: Config, registry: ToolRegistry) -> anyhow::Result<()> {
    let shared = AppState {
        cfg: Arc::new(cfg),
        registry: Arc::new(registry),
        rls: crate::security::RateLimiters::new(20, 40, 10, 20),
    };

    let app = build_router(shared.clone());

    let addr: std::net::SocketAddr =
        format!("{}:{}", shared.cfg.server.bind_addr, shared.cfg.server.port)
            .parse()
            .unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_router(shared: AppState) -> Router {
    let base = shared.cfg.server.base_path.clone();
    use tower_http::limit::RequestBodyLimitLayer;
    let limit_bytes = shared.cfg.limits.max_request_kb * 1024;
    Router::new()
        .route("/healthz", get(health))
        .route(&format!("{base}/capabilities"), get(capabilities))
        .route(
            &format!("{base}/call"),
            post(call).layer(RequestBodyLimitLayer::new(limit_bytes)),
        )
        .with_state(shared)
}

async fn health(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    match authorize(&state, &headers) {
        Ok(()) => (StatusCode::OK, Json(json!({"status":"ok"}))).into_response(),
        Err(e) => into_response(e).into_response(),
    }
}

async fn capabilities(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(e) = authorize(&state, &headers) {
        return into_response(e).into_response();
    }
    let tools: Vec<ToolInfo> = state
        .registry
        .list_names()
        .into_iter()
        .map(|n| {
            let t = state.registry.get(&n).unwrap();
            ToolInfo {
                name: n,
                input_schema: t.capabilities()["input"].clone(),
                output_schema: t.capabilities()["output"].clone(),
            }
        })
        .collect();
    let caps = Capabilities {
        mcp_version: "1.0",
        tools,
        streaming: true,
    };
    (StatusCode::OK, Json(caps)).into_response()
}

async fn call(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CallRequest>,
) -> Response {
    use std::time::Instant;
    let started = Instant::now();
    let request_id = uuid::Uuid::new_v4().to_string();
    let origin = headers
        .get("Origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let token_present = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.starts_with("Bearer "))
        .unwrap_or(false);

    if let Err(e) = authorize(&state, &headers) {
        audit_end(
            &request_id,
            &origin,
            token_present,
            &req.tool,
            "deny",
            e.code(),
            started.elapsed().as_millis() as u64,
            0,
            None,
        );
        return into_response(e).into_response();
    }
    if let Err(e) = security::content_length_ok(&headers, state.cfg.limits.max_request_kb) {
        audit_end(
            &request_id,
            &origin,
            token_present,
            &req.tool,
            "deny",
            e.code(),
            started.elapsed().as_millis() as u64,
            0,
            None,
        );
        return into_response(e).into_response();
    }
    // rate limit per-token and global
    let token = security::extract_bearer(&headers);
    if let Err(e) = state.rls.check(token.as_deref()) {
        audit_end(
            &request_id,
            &origin,
            token_present,
            &req.tool,
            "deny",
            e.code(),
            started.elapsed().as_millis() as u64,
            0,
            None,
        );
        return into_response(e).into_response();
    }

    let Some(tool) = state.registry.get(&req.tool) else {
        audit_end(
            &request_id,
            &origin,
            token_present,
            &req.tool,
            "deny",
            AppError::NotFound.code(),
            started.elapsed().as_millis() as u64,
            0,
            None,
        );
        return into_response(AppError::NotFound).into_response();
    };

    if req.stream {
        match tool.call_stream(req.params).await {
            Ok(body) => {
                audit_end(
                    &request_id,
                    &origin,
                    token_present,
                    &req.tool,
                    "allow",
                    "OK",
                    started.elapsed().as_millis() as u64,
                    0,
                    Some(true),
                );
                (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "application/x-ndjson")],
                    body,
                )
                    .into_response()
            }
            Err(e) => {
                audit_end(
                    &request_id,
                    &origin,
                    token_present,
                    &req.tool,
                    "error",
                    e.code(),
                    started.elapsed().as_millis() as u64,
                    0,
                    Some(true),
                );
                into_response(e).into_response()
            }
        }
    } else {
        match tool.call(req.params).await {
            Ok(result) => {
                // approximate bytes_out from JSON length
                let payload = json!({"id": req.id, "result": result});
                let bytes_out = serde_json::to_vec(&payload).map(|v| v.len()).unwrap_or(0) as u64;
                if req.tool == "exec" {
                    // extract exec fields for audit
                    let rc = payload["result"].clone();
                    let exit_code = rc.get("exit_code").and_then(|v| v.as_i64());
                    let truncated = rc.get("truncated").and_then(|v| v.as_bool());
                    let timed_out = rc.get("timed_out").and_then(|v| v.as_bool());
                    let stdout_len = rc
                        .get("stdout_b64")
                        .and_then(|v| v.as_str())
                        .map(|s| s.len())
                        .unwrap_or(0);
                    let stderr_len = rc
                        .get("stderr_b64")
                        .and_then(|v| v.as_str())
                        .map(|s| s.len())
                        .unwrap_or(0);
                    audit_end_exec(
                        &request_id,
                        &origin,
                        token_present,
                        "allow",
                        "OK",
                        started.elapsed().as_millis() as u64,
                        stdout_len,
                        stderr_len,
                        exit_code,
                        truncated,
                        timed_out,
                    );
                } else {
                    audit_end(
                        &request_id,
                        &origin,
                        token_present,
                        &req.tool,
                        "allow",
                        "OK",
                        started.elapsed().as_millis() as u64,
                        bytes_out,
                        Some(false),
                    );
                }
                (StatusCode::OK, Json(payload)).into_response()
            }
            Err(e) => {
                let body =
                    json!({"id": req.id, "error": {"code": e.code(), "message": e.to_string()}});
                let bytes_out = serde_json::to_vec(&body).map(|v| v.len()).unwrap_or(0) as u64;
                if req.tool == "exec" {
                    audit_end_exec(
                        &request_id,
                        &origin,
                        token_present,
                        "error",
                        e.code(),
                        started.elapsed().as_millis() as u64,
                        0,
                        0,
                        None,
                        None,
                        None,
                    );
                } else {
                    audit_end(
                        &request_id,
                        &origin,
                        token_present,
                        &req.tool,
                        "error",
                        e.code(),
                        started.elapsed().as_millis() as u64,
                        bytes_out,
                        Some(false),
                    );
                }
                (e.status(), Json(body)).into_response()
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn audit_end(
    request_id: &str,
    origin: &str,
    token_present: bool,
    tool: &str,
    decision: &str,
    code: &str,
    duration_ms: u64,
    bytes_out: u64,
    streaming: Option<bool>,
) {
    tracing::info!(
        request_id = request_id,
        origin = origin,
        token_present = token_present,
        tool = tool,
        decision = decision,
        code = code,
        duration_ms = duration_ms,
        bytes_out = bytes_out,
        streaming = ?streaming,
        "audit"
    );
}

#[allow(clippy::too_many_arguments)]
fn audit_end_exec(
    request_id: &str,
    origin: &str,
    token_present: bool,
    decision: &str,
    code: &str,
    duration_ms: u64,
    stdout_len: usize,
    stderr_len: usize,
    exit_code: Option<i64>,
    truncated: Option<bool>,
    timed_out: Option<bool>,
) {
    tracing::info!(
        request_id = request_id,
        origin = origin,
        token_present = token_present,
        tool = "exec",
        decision = decision,
        code = code,
        duration_ms = duration_ms,
        stdout_len = stdout_len,
        stderr_len = stderr_len,
        exit_code = exit_code,
        truncated = truncated,
        timed_out = timed_out,
        "audit"
    );
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    security::require_bearer(headers, &state.cfg.auth.bearer_token)?;
    security::check_origin(headers, &state.cfg.auth.allowed_origins)?;
    Ok(())
}
