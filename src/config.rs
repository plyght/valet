use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub root: Root,
    pub server: Server,
    pub auth: Auth,
    pub limits: Limits,
    pub exec: Exec,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Root { pub root_dir: PathBuf }

#[derive(Debug, Deserialize, Clone)]
pub struct Server {
    pub bind_addr: String,
    pub port: u16,
    #[serde(default = "default_base_path")]
    pub base_path: String,
}
fn default_base_path() -> String { "/mcp".to_string() }

#[derive(Debug, Deserialize, Clone)]
pub struct Auth {
    pub bearer_token: String,
    pub allowed_origins: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Limits {
    pub exec_timeout_s: u64,
    pub max_stdout_kb: usize,
    pub max_request_kb: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Exec {
    pub allowed_cmds: Vec<String>,
    #[serde(default)]
    pub pass_env: Vec<String>,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = fs::read_to_string(path)?;
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            Ok(serde_json::from_str(&raw)?)
        } else {
            Ok(toml::from_str(&raw)?)
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.root.root_dir.is_dir() {
            anyhow::bail!("root_dir does not exist or is not a directory: {}", self.root.root_dir.display());
        }
        if self.auth.bearer_token.trim().is_empty() { anyhow::bail!("bearer_token must not be empty"); }
        if self.auth.allowed_origins.is_empty() { anyhow::bail!("allowed_origins must not be empty"); }
        if self.limits.exec_timeout_s == 0 { anyhow::bail!("exec_timeout_s must be > 0"); }
        if self.limits.max_request_kb == 0 { anyhow::bail!("max_request_kb must be > 0"); }
        if self.limits.max_stdout_kb == 0 { anyhow::bail!("max_stdout_kb must be > 0"); }
        Ok(())
    }
}

pub fn canonical_root(root: &Path) -> anyhow::Result<PathBuf> {
    let c = dunce::canonicalize(root)?;
    Ok(c)
}
