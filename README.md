# Valet — MCP bridge for local macOS

Valet is a small, local adapter that exposes a minimal MCP server over HTTP to a hosted AI agent. It provides a secure, auditable interface to read/write files within an allow‑listed directory and run a small allow‑listed set of shell commands with timeouts and output caps. Valet listens on `127.0.0.1` and can be published safely with Tailscale Funnel to obtain a public HTTPS URL.

## Features

- Minimal MCP server over HTTP with streaming via NDJSON
- Tools:
  - `fs_read(path)`
  - `fs_write(path, content_b64, mode?)`
  - `exec(cmd, args[], timeout_s?)`
- Strong security defaults: Bearer auth, Origin allowlist, per‑token and global rate limits, payload caps
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

- `GET /mcp/capabilities` — capability discovery (tools and streaming support)
- `POST /mcp/call` — invoke a tool; set `stream=true` to receive NDJSON events
- `GET /healthz` — shallow health (still requires auth and valid Origin)

All endpoints require `Authorization: Bearer <token>` and a valid `Origin` header.

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

- `fs_read`
  - Input: `{ "path": "relative/or/absolute/under/root" }`
  - Output: `{ "content_b64": "...", "encoding": "base64" }`
- `fs_write`
  - Input: `{ "path": "...", "content_b64": "...", "mode": "0644"? }`
  - Output: `{ "bytes_written": 123 }`
- `exec`
  - Input: `{ "cmd": "...", "args": ["..."], "timeout_s": 10? }`
  - Output: `{ "exit_code": 0, "stdout_b64": "...", "stderr_b64": "...", "duration_ms": 42, "truncated": false, "timed_out": false }`

## Security

- Bearer token required on all endpoints. Tokens in query strings are rejected.
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

Expose locally served `/mcp` via Funnel to a public HTTPS URL like `https://<name>.ts.net/mcp`. You may set a different `base_path` in config to match your route.

## License

Apache-2.0
