use crate::{config::Config, errors::AppError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type DynTool = Arc<dyn Tool + Send + Sync + 'static>;

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Vec<(String, DynTool)>,
}

impl ToolRegistry {
    pub fn new(cfg: &Config) -> anyhow::Result<Self> {
        use crate::tools::{exec::ExecTool, fs_read::FsReadTool, fs_write::FsWriteTool};
        let exec = ExecTool::new(cfg)?;
        let mut tools: Vec<(String, DynTool)> = vec![
            ("fs_read".to_string(), Arc::new(FsReadTool::new(cfg)?)),
            ("fs_write".to_string(), Arc::new(FsWriteTool::new(cfg)?)),
            ("exec".to_string(), Arc::new(exec)),
        ];
        tools.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Self { tools })
    }

    pub fn get(&self, name: &str) -> Option<DynTool> { self.tools.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone()) }
    pub fn list_names(&self) -> Vec<String> { self.tools.iter().map(|(n, _)| n.clone()).collect() }
}

#[derive(Debug, Deserialize)]
pub struct CallRequest {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default)]
    pub stream: bool,
}

#[derive(Debug, Serialize)]
pub struct CallResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")] pub error: Option<super::types::ErrorObj>,
}

#[async_trait]
pub trait Tool {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> serde_json::Value;
    async fn call(&self, params: serde_json::Value) -> Result<serde_json::Value, AppError>;
    async fn call_stream(&self, _params: serde_json::Value) -> Result<crate::server::StreamBody, AppError> {
        Err(AppError::ToolError("streaming not supported".into()))
    }
}
