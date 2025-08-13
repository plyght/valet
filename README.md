# Valet — MCP bridge for local macOS

Valet is a small, local adapter that exposes an MCP (Model Context Protocol) server over HTTP to hosted AI agents. It implements JSON-RPC 2.0 over HTTP and provides a secure, auditable interface to read/write files within an allow‑listed directory and run a small allow‑listed set of shell commands with timeouts and output caps. Valet listens on `127.0.0.1` and can be published safely with Tailscale Funnel to obtain a public HTTPS URL.

## Features

- MCP server over HTTP using JSON-RPC 2.0 with streaming via NDJSON
- MCP-compliant tools:
  - `fs_read` — read files from allowed directory
  - `fs_write` — write files to allowed directory  
  - `exec` — execute allowed shell commands
- Strong security defaults: Token-in-path auth, Origin allowlist, per‑token and global rate limits, payload caps
- Typed errors with clear HTTP status mapping
- Structured audit logging (JSON) with redactions
- Single TOML config file; validated on startup with a concise readiness line

## Quick start

1. Install Rust stable (1.79+)
2. Create a config at `valet.toml`:

```toml
[root]
# filesystem root for tools
root_dir = "/Users/you/workdir"

[server]
bind_addr = "127.0.0.1"
port = 5555
base_path = "/mcp"

[auth]
bearer_token = "replace-with-a-strong-token"
allowed_origins = ["https://example.ts.net"]

[limits]
exec_timeout_s = 15
max_stdout_kb = 512
max_request_kb = 256

[exec]
# absolute paths or names resolved at startup
allowed_cmds = ["/bin/echo", "/usr/bin/yes", "ls"]
pass_env = ["LANG"]
```

3. Run Valet:

```bash
cargo run --release -- --config valet.toml
```

You should see a readiness line like:

```
valet ready addr=127.0.0.1:5555 base_path=/mcp tools=[fs_read,fs_write,exec]
```

## HTTP API (MCP over HTTP)

- `POST /mcp/<token>` — JSON-RPC 2.0 endpoint for MCP methods
- `GET /healthz` — shallow health (requires Origin only)

All endpoints require a valid `Origin` header and a token embedded in the URL path. Generate a token with `openssl rand -base64 48` and set it in your config; the URL path must include the same token to work.

### JSON-RPC 2.0 Methods

**List available tools:**
```json
POST /mcp/<token>
{
  "jsonrpc": "2.0",
  "method": "tools/list",
  "id": 1
}
```

**Call a tool:**
```json
POST /mcp/<token>
{
  "jsonrpc": "2.0", 
  "method": "tools/call",
  "params": {
    "name": "fs_read",
    "arguments": {"path": "example.txt"}
  },
  "id": 2
}
```

**Call a tool with streaming:**
```json
POST /mcp/<token>
{
  "jsonrpc": "2.0",
  "method": "tools/call", 
  "params": {
    "name": "exec",
    "arguments": {"cmd": "ls", "args": ["-la"]},
    "stream": true
  },
  "id": 3
}
```

### Streaming format (NDJSON)

`content-type: application/x-ndjson` with one JSON object per line:

```json
{"event":"start","id":"...","tool":"exec"}
{"event":"stdout","chunk_b64":"..."}
{"event":"stderr","chunk_b64":"..."}
{"event":"end","result":{}}
```

On errors:

```json
{"event":"error","error":{"code":"ExecTimeout","message":"..."}}
```

### Tools

**fs_read**
- Arguments: `{ "path": "relative/or/absolute/under/root" }`
- Result: `{ "content_b64": "...", "encoding": "base64" }`

**fs_write**
- Arguments: `{ "path": "...", "content_b64": "...", "mode": "0644"? }`
- Result: `{ "bytes_written": 123 }`

**exec**
- Arguments: `{ "cmd": "...", "args": ["..."], "timeout_s": 10? }`
- Result: `{ "exit_code": 0, "stdout_b64": "...", "stderr_b64": "...", "duration_ms": 42, "truncated": false, "timed_out": false }`

## Security

- Token embedded in URL path required for access. Tokens must match config exactly.
- Origin allowlist enforced. Missing or unexpected `Origin` is rejected.
- Rate limits: per‑token and global, conservative defaults.
- Payload caps via `max_request_kb` and capped stdout/stderr with early termination.
- Audit logs redact sensitive content; log sizes and outcomes instead.

## Development

Quality gates:

```bash
# format
cargo fmt --all

# lint
cargo clippy --all-targets -- -D warnings

# tests
cargo test

# run
cargo run -- --config valet.toml
```

## Tailscale Funnel

Expose locally served `/mcp` via Funnel to a public HTTPS URL like `https://<name>.ts.net/mcp/<token>`. 

**Setup:**
```bash
# Route entire /mcp path to local server
sudo tailscale funnel --bg --set-path /mcp http://127.0.0.1:5555/mcp

# Your public URL will be:
# https://<name>.ts.net/mcp/<your-token>
```

**For AI Assistants like Cobot:**
- URL: `https://<name>.ts.net/mcp/<your-token>`
- Origin header: `https://<name>.ts.net` 
- Use standard MCP JSON-RPC 2.0 requests

## License

Apache-2.0
