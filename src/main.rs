mod config;
mod errors;
mod logging;
mod mcp;
mod security;
mod server;
mod tools;

use crate::config::Config;
use anyhow::Context;
use std::path::PathBuf;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();

    let args: Vec<String> = std::env::args().collect();
    let mut config_path = PathBuf::from("valet.toml");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                if i >= args.len() { eprintln!("--config requires a path"); std::process::exit(2); }
                config_path = PathBuf::from(&args[i]);
            }
            _ => {}
        }
        i += 1;
    }

    let cfg = Config::load(&config_path).context("loading config")?;
    cfg.validate().context("validating config")?;

    let addr = format!("{}:{}", cfg.server.bind_addr, cfg.server.port);

    // Build tool registry
    let registry = mcp::registry::ToolRegistry::new(&cfg)?;

    info!(addr = %addr, base_path = %cfg.server.base_path, tools = ?registry.list_names(), "valet ready");
    println!(
        "valet ready addr={} base_path={} tools=[{}]",
        addr,
        cfg.server.base_path,
        registry.list_names().join(",")
    );

    server::serve(cfg, registry).await
}
