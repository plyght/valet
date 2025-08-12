use crate::{config::Config, errors::AppError, mcp::registry::Tool, tools::ensure_within_root};
use async_trait::async_trait;
use base64::Engine;
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

pub struct FsWriteTool {
    root: PathBuf,
}
impl FsWriteTool {
    pub fn new(cfg: &Config) -> anyhow::Result<Self> {
        Ok(Self {
            root: cfg.root.root_dir.clone(),
        })
    }
}

#[async_trait]
impl Tool for FsWriteTool {
    fn capabilities(&self) -> serde_json::Value {
        json!({"input": {"type":"object","required":["path","content_b64"],"properties": {"path": {"type":"string"},"content_b64":{"type":"string"},"mode":{"type":"string"}}}, "output": {"type":"object","properties": {"bytes_written":{"type":"integer"}}}})
    }
    async fn call(&self, params: serde_json::Value) -> Result<serde_json::Value, AppError> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::ToolError("missing path".into()))?;
        let content_b64 = params
            .get("content_b64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::ToolError("missing content_b64".into()))?;
        let mode = params.get("mode").and_then(|v| v.as_str());
        let full = ensure_within_root(&self.root, &PathBuf::from(path))
            .map_err(|_| AppError::PathOutsideRoot)?;
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::Internal(e.to_string()))?;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(content_b64)
            .map_err(|_| AppError::ToolError("invalid base64".into()))?;
        fs::write(&full, &bytes).map_err(|e| AppError::Internal(e.to_string()))?;
        if let Some(m) = mode {
            if let Ok(parsed) = u32::from_str_radix(m, 8) {
                let perm = fs::Permissions::from_mode(parsed);
                let _ = fs::set_permissions(&full, perm);
            }
        }
        Ok(json!({"bytes_written": bytes.len()}))
    }
}
