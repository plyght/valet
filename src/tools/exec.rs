use crate::{config::Config, errors::AppError, mcp::registry::Tool};
use async_trait::async_trait;
use axum::body::Body;
use base64::Engine;
use serde_json::json;
use std::{collections::HashSet, path::PathBuf, process::Stdio, time::Instant};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    time::{timeout, Duration},
};

pub struct ExecTool {
    allowed: HashSet<PathBuf>,
    pass_env: Vec<String>,
    timeout_s: u64,
    max_stdout_kb: usize,
}

impl ExecTool {
    pub fn new(cfg: &Config) -> anyhow::Result<Self> {
        let resolved = resolve_cmds(&cfg.exec.allowed_cmds)?;
        Ok(Self {
            allowed: resolved,
            pass_env: cfg.exec.pass_env.clone(),
            timeout_s: cfg.limits.exec_timeout_s,
            max_stdout_kb: cfg.limits.max_stdout_kb,
        })
    }
}

fn resolve_cmds(cmds: &[String]) -> anyhow::Result<HashSet<PathBuf>> {
    let mut set = HashSet::new();
    for c in cmds {
        let path = if c.contains('/') {
            PathBuf::from(c)
        } else {
            which::which(c)?
        };
        let can = dunce::canonicalize(path)?;
        set.insert(can);
    }
    Ok(set)
}

#[async_trait]
impl Tool for ExecTool {
    fn capabilities(&self) -> serde_json::Value {
        json!({"input": {"type":"object","required":["cmd"],"properties": {"cmd": {"type":"string"},"args":{"type":"array","items":{"type":"string"}},"timeout_s":{"type":"integer"}}}, "output": {"type":"object","properties": {"exit_code":{"type":"integer"},"stdout_b64":{"type":"string"},"stderr_b64":{"type":"string"},"duration_ms":{"type":"integer"},"truncated":{"type":"boolean"},"timed_out":{"type":"boolean"}}}})
    }

    async fn call(&self, params: serde_json::Value) -> Result<serde_json::Value, AppError> {
        let cmd = params
            .get("cmd")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::ToolError("missing cmd".into()))?;
        let args: Vec<String> = params
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let timeout_s = params
            .get("timeout_s")
            .and_then(|v| v.as_u64())
            .map(|t| t.min(self.timeout_s))
            .unwrap_or(self.timeout_s);

        let full = if cmd.contains('/') {
            dunce::canonicalize(cmd).map_err(|_| AppError::ExecDenied)?
        } else {
            which::which(cmd).map_err(|_| AppError::ExecDenied)?
        };
        let full = dunce::canonicalize(full).map_err(|_| AppError::ExecDenied)?;
        if !self.allowed.contains(&full) {
            return Err(AppError::ExecDenied);
        }

        let mut command = Command::new(&full);
        command.args(&args);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        // env policy: clear then insert pass_env
        command.env_clear();
        for k in &self.pass_env {
            if let Ok(v) = std::env::var(k) {
                command.env(k, v);
            }
        }

        let start = Instant::now();
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(_) => return Err(AppError::Internal("failed to spawn".into())),
        };

        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();

        let max_bytes = self.max_stdout_kb * 1024;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut truncated = false;

        let read_fut = async {
            let mut buf_out = [0u8; 8192];
            let mut buf_err = [0u8; 8192];
            loop {
                tokio::select! {
                    r = stdout.read(&mut buf_out) => {
                        let n = r.unwrap_or(0);
                        if n == 0 { break; }
                        out.extend_from_slice(&buf_out[..n]);
                        if out.len() > max_bytes { truncated = true; let _ = child.kill().await; break; }
                    }
                    r = stderr.read(&mut buf_err) => {
                        let n = r.unwrap_or(0);
                        if n == 0 { continue; }
                        err.extend_from_slice(&buf_err[..n]);
                        if err.len() > max_bytes { truncated = true; let _ = child.kill().await; break; }
                    }
                }
            }
        };

        let to = Duration::from_secs(timeout_s);
        let timed_out = timeout(to, read_fut).await.is_err();
        if timed_out {
            let _ = child.kill().await;
        }
        let status = match timeout(to, child.wait()).await {
            Ok(Ok(s)) => s,
            _ => return Err(AppError::ExecTimeout),
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let exit_code = status.code().unwrap_or_default();
        let stdout_b64 = base64::engine::general_purpose::STANDARD.encode(&out);
        let stderr_b64 = base64::engine::general_purpose::STANDARD.encode(&err);

        Ok(
            json!({"exit_code": exit_code, "stdout_b64": stdout_b64, "stderr_b64": stderr_b64, "duration_ms": duration_ms, "truncated": truncated, "timed_out": timed_out}),
        )
    }

    async fn call_stream(
        &self,
        params: serde_json::Value,
    ) -> Result<crate::server::StreamBody, AppError> {
        use futures::StreamExt;
        use tokio::sync::mpsc;
        use tokio_stream::wrappers::ReceiverStream;

        let cmd = params
            .get("cmd")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::ToolError("missing cmd".into()))?;
        let args: Vec<String> = params
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let timeout_s = params
            .get("timeout_s")
            .and_then(|v| v.as_u64())
            .map(|t| t.min(self.timeout_s))
            .unwrap_or(self.timeout_s);

        let full = if cmd.contains('/') {
            dunce::canonicalize(cmd).map_err(|_| AppError::ExecDenied)?
        } else {
            which::which(cmd).map_err(|_| AppError::ExecDenied)?
        };
        let full = dunce::canonicalize(full).map_err(|_| AppError::ExecDenied)?;
        if !self.allowed.contains(&full) {
            return Err(AppError::ExecDenied);
        }

        let pass_env = self.pass_env.clone();
        let max_bytes = self.max_stdout_kb * 1024;

        let (tx, rx) = mpsc::channel::<String>(32);
        tokio::spawn(async move {
            let _ = tx.send(line(json!({"event":"start","tool":"exec"}))).await;
            let mut command = Command::new(&full);
            command.args(&args);
            command.stdin(Stdio::null());
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());
            command.env_clear();
            for k in &pass_env {
                if let Ok(v) = std::env::var(k) {
                    command.env(k, v);
                }
            }
            let mut child = match command.spawn() {
                Ok(c) => c,
                Err(_) => {
                    let _ = tx.send(line(json!({"event":"error","error":{"code":"Internal","message":"failed to spawn"}}))).await;
                    return;
                }
            };
            let mut stdout = child.stdout.take().unwrap();
            let mut stderr = child.stderr.take().unwrap();
            let mut out_total = 0usize;
            let mut err_total = 0usize;
            let mut buf_out = [0u8; 4096];
            let mut buf_err = [0u8; 4096];
            let to = Duration::from_secs(timeout_s);
            let start = Instant::now();
            let read_fut = async {
                loop {
                    tokio::select! {
                        r = stdout.read(&mut buf_out) => {
                            let n = r.unwrap_or(0);
                            if n == 0 { break; }
                            out_total += n;
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&buf_out[..n]);
                            let _ = tx.send(line(json!({"event":"stdout","chunk_b64": b64}))).await;
                            if out_total > max_bytes { let _ = child.kill().await; break; }
                        }
                        r = stderr.read(&mut buf_err) => {
                            let n = r.unwrap_or(0);
                            if n == 0 { continue; }
                            err_total += n;
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&buf_err[..n]);
                            let _ = tx.send(line(json!({"event":"stderr","chunk_b64": b64}))).await;
                            if err_total > max_bytes { let _ = child.kill().await; break; }
                        }
                    }
                }
            };
            let _ = timeout(to, read_fut).await;
            let status = timeout(to, child.wait()).await;
            match status {
                Err(_) => {
                    let _ = tx.send(line(json!({"event":"error","error":{"code":"ExecTimeout","message":"timeout"}}))).await;
                }
                Ok(_) => {
                    let _ = tx.send(line(json!({"event":"end","result": {"duration_ms": start.elapsed().as_millis() as u64 }}))).await;
                }
            }
        });

        let body = Body::from_stream(ReceiverStream::new(rx).map(Ok::<_, std::io::Error>));
        Ok(body)
    }
}

fn line(v: serde_json::Value) -> String {
    format!("{v}\n")
}
