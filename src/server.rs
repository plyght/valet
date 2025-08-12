use crate::{config::Config, errors::{into_response, AppError}, mcp::{registry::{CallRequest, ToolRegistry}, types::{Capabilities, ToolInfo}}, security};
use axum::{extract::State, http::{HeaderMap, StatusCode}, response::{IntoResponse, Response}, routing::{get, post}, Json, Router};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub registry: Arc<ToolRegistry>,
}

pub type StreamBody = axum::body::Body;

pub async fn serve(cfg: Config, registry: ToolRegistry) -> anyhow::Result<()> {
    let shared = AppState { cfg: Arc::new(cfg), registry: Arc::new(registry) };

    let base = shared.cfg.server.base_path.clone();

    let app = Router::new()
        .route("/healthz", get(health))
        .route(&format!("{}/capabilities", base), get(capabilities))
        .route(&format!("{}/call", base), post(call))
        .with_state(shared.clone());

    let addr: std::net::SocketAddr = format!("{}:{}", shared.cfg.server.bind_addr, shared.cfg.server.port).parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    match authorize(&state, &headers) {
        Ok(()) => (StatusCode::OK, Json(json!({"status":"ok"}))).into_response(),
        Err(e) => into_response(e).into_response(),
    }
}

async fn capabilities(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(e) = authorize(&state, &headers) { return into_response(e).into_response(); }
    let tools: Vec<ToolInfo> = state.registry.list_names().into_iter().map(|n| {
        let t = state.registry.get(&n).unwrap();
        ToolInfo { name: n, input_schema: t.capabilities()["input"].clone(), output_schema: t.capabilities()["output"].clone() }
    }).collect();
    let caps = Capabilities { mcp_version: "1.0", tools, streaming: true };
    (StatusCode::OK, Json(caps)).into_response()
}

async fn call(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CallRequest>) -> Response {
    if let Err(e) = authorize(&state, &headers) { return into_response(e).into_response(); }
    if let Err(e) = security::content_length_ok(&headers, state.cfg.limits.max_request_kb) { return into_response(e).into_response(); }

    let Some(tool) = state.registry.get(&req.tool) else { return into_response(AppError::NotFound).into_response(); };

    if req.stream {
        match tool.call_stream(req.params).await {
            Ok(body) => (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/x-ndjson")],
                body,
            ).into_response(),
            Err(e) => into_response(e).into_response(),
        }
    } else {
        match tool.call(req.params).await {
            Ok(result) => (StatusCode::OK, Json(json!({"id": req.id, "result": result}))).into_response(),
            Err(e) => {
                let body = json!({"id": req.id, "error": {"code": e.code(), "message": e.to_string()}});
                (e.status(), Json(body)).into_response()
            }
        }
    }
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    security::require_bearer(headers, &state.cfg.auth.bearer_token)?;
    security::check_origin(headers, &state.cfg.auth.allowed_origins)?;
    Ok(())
}
