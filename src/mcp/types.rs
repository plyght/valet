use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct Capabilities {
    pub mcp_version: &'static str,
    pub tools: Vec<ToolInfo>,
    pub streaming: bool,
}

#[derive(Debug, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorObj {
    pub code: String,
    pub message: String,
}
