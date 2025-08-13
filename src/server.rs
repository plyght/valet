use crate::{
    config::Config,
    errors::{into_response, AppError},
    mcp::registry::ToolRegistry,
    security,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub registry: Arc<ToolRegistry>,
    pub rls: crate::security::RateLimiters,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
    id: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
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
    use tower_http::cors::{CorsLayer, Any};
    use axum::http::{HeaderName, Method};
    let limit_bytes = shared.cfg.limits.max_request_kb * 1024;
    
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .expose_headers([
            HeaderName::from_static("mcp-session-id"),
            HeaderName::from_static("www-authenticate")
        ]);
        
    Router::new()
        .route("/healthz", get(health))
        .route(
            &base,
            get(mcp_root_handler),
        )
        .route(
            &format!("{base}/:token"),
            post(mcp_handler)
                .get(mcp_get_handler)
                .layer(RequestBodyLimitLayer::new(limit_bytes)),
        )
        .layer(cors)
        .with_state(shared)
}

async fn health(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    security::check_origin(&headers, &state.cfg.auth.allowed_origins)
        .map(|_| (StatusCode::OK, Json(json!({"status":"ok"}))).into_response())
        .unwrap_or_else(|e| into_response(e).into_response())
}

async fn mcp_root_handler() -> impl IntoResponse {
    let info = json!({
        "jsonrpc": "2.0",
        "error": {
            "code": -32600,
            "message": "Token required. Access /mcp/YOUR-TOKEN with your authentication token."
        },
        "id": null
    });
    (StatusCode::BAD_REQUEST, Json(info))
}

async fn mcp_get_handler(
    Path(path_token): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    // For GET requests, be more lenient with Origin checking for direct browser access
    if path_token != state.cfg.auth.bearer_token {
        return into_response(AppError::Unauthorized).into_response();
    }
    
    // Only check Origin if it's present (browsers don't send Origin for direct navigation)
    if headers.get("origin").is_some() {
        if let Err(e) = security::check_origin(&headers, &state.cfg.auth.allowed_origins) {
            return into_response(e).into_response();
        }
    }

    // Check if client accepts SSE
    let accept_header = headers.get("accept").and_then(|v| v.to_str().ok()).unwrap_or("");
    
    if accept_header.contains("text/event-stream") {
        // Return SSE connection for MCP clients
        let host = headers.get("host").and_then(|v| v.to_str().ok()).unwrap_or("localhost");
        let sse_data = format!("data: {{\"jsonrpc\":\"2.0\",\"method\":\"connected\",\"params\":{{\"endpoint\":\"{}/mcp/{}\"}}}}\n\n", host, path_token);
        
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("connection", "keep-alive")
            .header("access-control-allow-origin", "*")
            .header("access-control-expose-headers", "mcp-session-id,www-authenticate")
            .body(axum::body::Body::from(sse_data))
            .unwrap();
        response.into_response()
    } else {
        // Return JSON for browser requests
        let info = json!({
            "jsonrpc": "2.0",
            "result": {
                "name": "valet",
                "version": "0.1.0",
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {},
                    "logging": {}
                },
                "instructions": "This is a Valet MCP server. Send POST requests with JSON-RPC 2.0 payloads to interact."
            },
            "id": "info"
        });
        (StatusCode::OK, Json(info)).into_response()
    }
}

async fn mcp_handler(
    Path(path_token): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<JsonRpcRequest>,
) -> Response {
    if let Err(e) = authorize_path(&state, &headers, &path_token) {
        let error_resp = JsonRpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: e.to_string(),
            }),
            id: req.id,
        };
        return (e.status(), Json(error_resp)).into_response();
    }

    if req.jsonrpc != "2.0" {
        let error_resp = JsonRpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "Invalid JSON-RPC version".to_string(),
            }),
            id: req.id,
        };
        return (StatusCode::BAD_REQUEST, Json(error_resp)).into_response();
    }

    match req.method.as_str() {
        "initialize" => handle_initialize(req).await,
        "initialized" => handle_initialized(req).await,
        "tools/list" => handle_tools_list(state, req).await,
        "tools/call" => handle_tools_call(state, headers, req).await,
        _ => {
            let error_resp = JsonRpcResponse {
                jsonrpc: "2.0",
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: "Method not found".to_string(),
                }),
                id: req.id,
            };
            (StatusCode::NOT_FOUND, Json(error_resp)).into_response()
        }
    }
}

async fn handle_initialize(req: JsonRpcRequest) -> Response {
    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        result: Some(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
                "logging": {}
            },
            "serverInfo": {
                "name": "valet",
                "version": "0.1.0"
            }
        })),
        error: None,
        id: req.id,
    };
    (StatusCode::OK, Json(resp)).into_response()
}

async fn handle_initialized(req: JsonRpcRequest) -> Response {
    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        result: Some(json!({})),
        error: None,
        id: req.id,
    };
    (StatusCode::OK, Json(resp)).into_response()
}

async fn handle_tools_list(state: AppState, req: JsonRpcRequest) -> Response {
    let tools: Vec<serde_json::Value> = state
        .registry
        .list_names()
        .into_iter()
        .map(|name| {
            let tool = state.registry.get(&name).unwrap();
            let caps = tool.capabilities();
            json!({
                "name": name,
                "description": format!("Tool: {}", name),
                "inputSchema": caps["input"].clone()
            })
        })
        .collect();

    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        result: Some(json!({"tools": tools})),
        error: None,
        id: req.id,
    };
    (StatusCode::OK, Json(resp)).into_response()
}

async fn handle_tools_call(state: AppState, headers: HeaderMap, req: JsonRpcRequest) -> Response {
    let params = req.params.clone();
    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

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

    if let Err(e) = security::content_length_ok(&headers, state.cfg.limits.max_request_kb) {
        audit_end(
            &request_id,
            &origin,
            token_present,
            tool_name,
            "deny",
            e.code(),
            started.elapsed().as_millis() as u64,
            0,
            None,
        );
        let error_resp = JsonRpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: e.to_string(),
            }),
            id: req.id,
        };
        return (e.status(), Json(error_resp)).into_response();
    }

    let token = security::extract_bearer(&headers);
    if let Err(e) = state.rls.check(token.as_deref()) {
        audit_end(
            &request_id,
            &origin,
            token_present,
            tool_name,
            "deny",
            e.code(),
            started.elapsed().as_millis() as u64,
            0,
            None,
        );
        let error_resp = JsonRpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: e.to_string(),
            }),
            id: req.id,
        };
        return (e.status(), Json(error_resp)).into_response();
    }

    let Some(tool) = state.registry.get(tool_name) else {
        audit_end(
            &request_id,
            &origin,
            token_present,
            tool_name,
            "deny",
            AppError::NotFound.code(),
            started.elapsed().as_millis() as u64,
            0,
            None,
        );
        let error_resp = JsonRpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Tool not found".to_string(),
            }),
            id: req.id,
        };
        return (StatusCode::NOT_FOUND, Json(error_resp)).into_response();
    };

    let is_streaming = params
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if is_streaming {
        match tool.call_stream(arguments).await {
            Ok(body) => {
                audit_end(
                    &request_id,
                    &origin,
                    token_present,
                    tool_name,
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
                    tool_name,
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
        match tool.call(arguments).await {
            Ok(result) => {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0",
                    result: Some(result.clone()),
                    error: None,
                    id: req.id.clone(),
                };
                let bytes_out = serde_json::to_vec(&resp).map(|v| v.len()).unwrap_or(0) as u64;
                if tool_name == "exec" {
                    let rc = result.clone();
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
                        tool_name,
                        "allow",
                        "OK",
                        started.elapsed().as_millis() as u64,
                        bytes_out,
                        Some(false),
                    );
                }
                (StatusCode::OK, Json(resp)).into_response()
            }
            Err(e) => {
                let error_resp = JsonRpcResponse {
                    jsonrpc: "2.0",
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: e.to_string(),
                    }),
                    id: req.id,
                };
                let bytes_out = serde_json::to_vec(&error_resp)
                    .map(|v| v.len())
                    .unwrap_or(0) as u64;
                if tool_name == "exec" {
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
                        tool_name,
                        "error",
                        e.code(),
                        started.elapsed().as_millis() as u64,
                        bytes_out,
                        Some(false),
                    );
                }
                (e.status(), Json(error_resp)).into_response()
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

fn authorize_path(state: &AppState, headers: &HeaderMap, path_token: &str) -> Result<(), AppError> {
    if path_token != state.cfg.auth.bearer_token {
        return Err(AppError::Unauthorized);
    }
    security::check_origin(headers, &state.cfg.auth.allowed_origins)?;
    Ok(())
}
