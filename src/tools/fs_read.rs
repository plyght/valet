use crate::{config::Config, errors::AppError, mcp::registry::Tool, tools::ensure_within_root};
use async_trait::async_trait;
use base64::Engine;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

pub struct FsReadTool { root: PathBuf }

impl FsReadTool { pub fn new(cfg: &Config) -> anyhow::Result<Self> { Ok(Self { root: cfg.root.root_dir.clone() }) } }

#[async_trait]
impl Tool for FsReadTool {
    fn name(&self) -> &'static str { "fs_read" }
    fn capabilities(&self) -> serde_json::Value {
        json!({"input": {"type":"object","required":["path"],"properties": {"path": {"type":"string"}}}, "output": {"type":"object","properties": {"content_b64":{"type":"string"},"encoding":{"type":"string"}}}})
    }
    async fn call(&self, params: serde_json::Value) -> Result<serde_json::Value, AppError> {
        let path = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| AppError::ToolError("missing path".into()))?;
        let full = ensure_within_root(&self.root, &PathBuf::from(path)).map_err(|_| AppError::PathOutsideRoot)?;
        let data = fs::read(&full).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { AppError::NotFound } else { AppError::Internal(e.to_string()) })?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(data);
        Ok(json!({"content_b64": b64, "encoding": "base64"}))
    }
}
