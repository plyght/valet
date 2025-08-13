#[cfg(test)]
mod tests {
    use super::*;
}

#[cfg(test)]
mod integration {
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt;

    #[tokio::test]
    async fn capabilities_ok() {
        use crate::{config::{Auth, Config, Exec, Limits, Root, Server}, mcp::registry::ToolRegistry, server::{AppState, build_router}};
        let cfg = Config {
            root: Root { root_dir: std::env::temp_dir() },
            server: Server { bind_addr: "127.0.0.1".into(), port: 0, base_path: "/mcp".into() },
            auth: Auth { bearer_token: "t".into(), allowed_origins: vec!["https://good".into()] },
            limits: Limits { exec_timeout_s: 2, max_stdout_kb: 8, max_request_kb: 64 },
            exec: Exec { allowed_cmds: vec!["/bin/echo".into()], pass_env: vec![] },
        };
        let registry = ToolRegistry::new(&cfg).unwrap();
        let app = build_router(AppState { cfg: std::sync::Arc::new(cfg), registry: std::sync::Arc::new(registry), rls: crate::security::RateLimiters::new(100, 100, 100, 100) });
        let req = Request::builder()
            .uri("/mcp/t/capabilities")
            .method("GET")
            .header("Origin", "https://good")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

#[cfg(test)]
mod unit {
    use crate::security;
    use crate::tools::ensure_within_root;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn path_within_root_allows() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let f = root.join("a.txt");
        fs::write(&f, b"hi").unwrap();
        let full = ensure_within_root(root, &PathBuf::from("a.txt")).unwrap();
        assert_eq!(full, f);
    }

    #[test]
    fn path_outside_root_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let outside = PathBuf::from("/etc/hosts");
        let err = ensure_within_root(root, &outside).err().unwrap();
        assert!(err.to_string().contains("escapes"));
    }

    #[test]
    fn origin_enforced() {
        use axum::http::HeaderMap;
        let mut h = HeaderMap::new();
        h.insert("Origin", "https://good.example".parse().unwrap());
        assert!(security::check_origin(&h, &["https://good.example".into()]).is_ok());
        assert!(security::check_origin(&h, &["https://bad.example".into()]).is_err());
    }

    #[test]
    fn bearer_required() {
        use axum::http::HeaderMap;
        let mut h = HeaderMap::new();
        h.insert(axum::http::header::AUTHORIZATION, "Bearer token".parse().unwrap());
        assert!(security::require_bearer(&h, "token").is_ok());
        assert!(security::require_bearer(&h, "wrong").is_err());
    }
}

#[cfg(test)]
mod exec_tests {
    use crate::config::{Auth, Config, Exec, Limits, Root, Server};
    use crate::tools::exec::ExecTool;
    use serde_json::json;

    fn test_config(allowed: Vec<String>) -> Config {
        Config {
            root: Root { root_dir: std::env::temp_dir() },
            server: Server { bind_addr: "127.0.0.1".into(), port: 0, base_path: "/mcp".into() },
            auth: Auth { bearer_token: "t".into(), allowed_origins: vec!["https://good".into()] },
            limits: Limits { exec_timeout_s: 2, max_stdout_kb: 8, max_request_kb: 64 },
            exec: Exec { allowed_cmds: allowed, pass_env: vec![] },
        }
    }

    #[tokio::test]
    async fn exec_truncates_large_output() {
        // Use `yes` to generate a lot of output; cap stdout small
        let cfg = test_config(vec!["/usr/bin/yes".into(), "/bin/echo".into()]);
        // Skip test if /usr/bin/yes doesn't exist on this system
        if !std::path::Path::new("/usr/bin/yes").exists() { return; }
        let tool = ExecTool::new(&cfg).unwrap();
        let params = json!({"cmd":"/usr/bin/yes","args":["x"],"timeout_s":1});
        let out = tool.call(params).await.unwrap();
        let truncated = out.get("truncated").and_then(|v| v.as_bool()).unwrap();
        assert!(truncated || out.get("timed_out").and_then(|v| v.as_bool()).unwrap());
    }

    #[tokio::test]
    async fn exec_allows_echo() {
        let cfg = test_config(vec!["/bin/echo".into()]);
        let tool = ExecTool::new(&cfg).unwrap();
        let params = json!({"cmd":"/bin/echo","args":["hello"]});
        let out = tool.call(params).await.unwrap();
        let stdout_b64 = out.get("stdout_b64").and_then(|v| v.as_str()).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(stdout_b64).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("hello"));
    }
}
