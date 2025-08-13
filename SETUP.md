# Valet — MCP Server Setup and Tailscale Funnel Guide

This guide walks you through getting Valet running on your Mac as an MCP (Model Context Protocol) server and safely sharing it using Tailscale Funnel. It includes copy‑paste commands you can run in Terminal.

## What Valet does (in simple words)

- Valet is an MCP server that lets AI assistants safely interact with your Mac
- It implements JSON-RPC 2.0 over HTTP and provides three tools:
  1) Read files inside one folder you choose (`fs_read`)
  2) Write files inside that same folder (`fs_write`) 
  3) Run a short approved list of commands (`exec`)
- Every call must include your secret token in the URL path and come from your approved website URL. Valet also enforces time and size limits and logs what happens.

---

## Prerequisites

- macOS (13+ recommended)
- Tailscale installed and logged in (Funnel feature enabled on your plan)
- Rust toolchain (for building and running Valet)

---

## Step 1 — Choose a workspace folder

Pick a folder the AI can use. Valet won’t touch files outside this folder. Example:

- Workspace folder: `/Users/you/workdir`

Create it if it doesn’t exist:

```bash
mkdir -p /Users/you/workdir
```

## Step 2 — Choose safe commands (keep it short)

Examples that are generally safe:

- `/bin/echo`
- `/bin/ls`

You’ll list them in the config next.

## Step 3 — Create a strong secret token

Generate a long random token and keep it safe:

```bash
openssl rand -base64 48
```

Copy the output string. You’ll paste it into the config and also give it to the AI service.

## Step 4 — Make a Valet config file

Create a file named `valet.toml` (adjust paths and values to yours):

```toml
[root]
root_dir = "/Users/you/workdir"

[server]
bind_addr = "127.0.0.1"
port = 5555
base_path = "/mcp"

[auth]
# paste the random token you generated above
bearer_token = "PASTE-YOUR-RANDOM-TOKEN-HERE"
# put your Tailscale Funnel URL here (you can set this after Step 7 and restart)
allowed_origins = ["https://yourname.ts.net"]

[limits]
exec_timeout_s = 15
max_stdout_kb = 512
max_request_kb = 256

[exec]
# absolute paths or names (names are resolved at startup)
allowed_cmds = ["/bin/echo", "/bin/ls"]
pass_env = ["LANG"]
```

Save this file somewhere you can reference, e.g. `/Users/you/valet.toml`.

## Step 5 — Run Valet locally (no internet exposure yet)

From the Valet project directory:

```bash
# build and run
cargo run --release -- --config /Users/you/valet.toml
```

You should see a line like:

```
valet ready addr=127.0.0.1:5555 base_path=/mcp tools=[exec,fs_read,fs_write]
```

Optional: Test the MCP server locally (replace values with yours):

```bash
# Health check (no token required)
curl -i \
  -H "Origin: https://yourname.ts.net" \
  http://127.0.0.1:5555/healthz

# List available MCP tools
curl -i \
  -H "Origin: https://yourname.ts.net" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"tools/list","id":1}' \
  "http://127.0.0.1:5555/mcp/PASTE-YOUR-TOKEN-HERE"

# Test exec tool
curl -i \
  -H "Origin: https://yourname.ts.net" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"exec","arguments":{"cmd":"echo","args":["Hello MCP"]}},"id":2}' \
  "http://127.0.0.1:5555/mcp/PASTE-YOUR-TOKEN-HERE"
```

You should get `HTTP/1.1 200 OK` responses with JSON-RPC results.

---

## Step 6 — Enable Tailscale Funnel

These steps assume you've already logged into Tailscale on this Mac.

**Important:** Use the full path to Tailscale if the command isn't in your PATH:
```bash
/Applications/Tailscale.app/Contents/MacOS/Tailscale
```

1. **Route the entire /mcp path to your local server:**

```bash
# This routes all /mcp requests to your local Valet server
sudo tailscale funnel --bg --set-path /mcp http://127.0.0.1:5555/mcp
```

2. **Find your public hostname:**

```bash
TS_NAME=$(tailscale status --json | jq -r .Self.HostName)
echo "Your public MCP URL will be: https://$TS_NAME.ts.net/mcp/YOUR-TOKEN-HERE"
```

3. **Update your config and restart Valet:**

Put the Tailscale origin (just the `https://hostname.ts.net` part) into `allowed_origins` in your `valet.toml`:

```toml
allowed_origins = ["https://your-hostname.ts.net"]
```

Then restart Valet.

---

## Step 7 — Connect your AI assistant

Provide your AI assistant (like Cobot) with:

**URL:** `https://<name>.ts.net/mcp/<YOUR-TOKEN>`

**The AI must send:**
- `Origin: https://<name>.ts.net` header
- Standard MCP JSON-RPC 2.0 requests to the URL

**Example working request:**
```bash
curl -i \
  -H "Origin: https://your-hostname.ts.net" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"tools/list","id":1}' \
  "https://your-hostname.ts.net/mcp/YOUR-TOKEN-HERE"
```

If the token in the URL path doesn't match your Valet config, Valet rejects the call.

---

## Step 8 — Optional: keep Valet running in the background

Use the included launchd example. Edit it first to point to your binary and config paths:

- File: `launchd/valet.plist`

Then load it (example paths; adjust as needed):

```bash
# Copy the plist where launchd expects it
cp launchd/valet.plist ~/Library/LaunchAgents/com.valet.service.plist

# Edit the copied file to set the correct paths to your valet binary and config
open -e ~/Library/LaunchAgents/com.valet.service.plist

# Load and start
launchctl load ~/Library/LaunchAgents/com.valet.service.plist
launchctl start com.valet.service

# View logs (based on the paths you set in the plist)
tail -f /usr/local/var/log/valet.out.log
```

---

## Troubleshooting quick checks

- **HTTP 401 Unauthorized**
  - Token in URL path doesn't match your config's `bearer_token`
  - Check that the URL includes the exact token: `/mcp/YOUR-TOKEN-HERE`
- **OriginDenied** 
  - Ensure `Origin` header matches exactly your `https://<name>.ts.net` (no typos)
  - Check `allowed_origins` in your config
- **HTTP 404 Not Found**
  - Tailscale Funnel routing issue
  - Valet server not running
  - Wrong URL format (should be `/mcp/TOKEN` not `/mcp/TOKEN/anything`)
- **RequestTooLarge**
  - Your request body exceeded `max_request_kb`
- **ExecDenied** 
  - The command you requested isn't in `allowed_cmds`
- **PathOutsideRoot**
  - The path tries to leave your workspace folder (even through symlinks)

**Debug commands:**
```bash
# Check if Valet is running locally
curl -i -H "Origin: https://your-hostname.ts.net" http://127.0.0.1:5555/healthz

# Check Tailscale Funnel status
tailscale funnel status

# Check Tailscale status
tailscale status
```

---

## Safety tips

- Keep `allowed_cmds` extremely small
- Rotate your token occasionally (generate a new one with `openssl rand -base64 48`)
- Use a dedicated workspace folder, not your whole home folder
- Review logs if anything looks odd
- Stop Valet when you’re not using it

---

## Recap

- Run Valet locally as an MCP server with your config
- Use Tailscale Funnel to publish `/mcp` at a public HTTPS URL
- Give your AI assistant the public URL: `https://hostname.ts.net/mcp/YOUR-TOKEN`
- The AI sends MCP JSON-RPC 2.0 requests to interact with your Mac safely
- Valet will only do what you explicitly allow, inside your chosen folder
