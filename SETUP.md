# Valet — Simple Setup and Tailscale Funnel Guide

This guide walks you through getting Valet running on your Mac and safely sharing it using Tailscale Funnel. It includes copy‑paste commands you can run in Terminal.

## What Valet does (in simple words)

- Valet is a small gate for an AI to reach your Mac.
- It can only:
  1) Read files inside one folder you choose
  2) Write files inside that same folder
  3) Run a short approved list of commands
- Every call must include your secret token and come from your approved website URL. Valet also enforces time and size limits and logs what happens.

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

Optional: Health check locally (replace the token and origin with your values):

```bash
TOKEN="PASTE-YOUR-RANDOM-TOKEN-HERE"
curl -i \
  -H "Origin: https://yourname.ts.net" \
  -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:5555/healthz
```

You should get `HTTP/1.1 200 OK` and a small JSON body.

---

## Step 6 — Enable Tailscale Funnel

These steps assume you’ve already logged into Tailscale on this Mac.

- Decide your token and include it in the URL path. Example token (generate yours):

```bash
TOKEN=$(openssl rand -base64 48)
```

- Map your local Valet path to a public HTTPS route that includes the token:

```bash
# Forward public /mcp/$TOKEN to local 127.0.0.1:5555/mcp/$TOKEN
sudo tailscale serve https "/mcp/$TOKEN" "http://127.0.0.1:5555/mcp/$TOKEN"
```

- Turn on Funnel on this device (you may need admin approval in Tailscale):

```bash
sudo tailscale funnel 443 on
```

- Find your public name (hostname):

```bash
TS_NAME=$(tailscale status --json | jq -r .Self.HostName)
echo "https://$TS_NAME.ts.net/mcp"
```

- Put that exact URL’s origin (including `https://`) into `allowed_origins` in your `valet.toml`, then restart Valet. Your public paths will look like `https://<name>.ts.net/mcp/<TOKEN>/...`, and they must include the same token string that’s in your config.

---

## Step 7 — Connect your AI assistant

Provide the AI with:

- Your public URL: `https://<name>.ts.net/mcp/<TOKEN>` (include your token in the path)

The AI must send requests with:

- `Origin: https://<name>.ts.net`

If the token in the URL path doesn’t match your Valet config, Valet rejects the call.

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

- Unauthorized
  - Ensure `Authorization: Bearer <token>` is present and matches your config
- OriginDenied
  - Ensure `Origin` matches exactly your `https://<name>.ts.net` (no typos)
- RequestTooLarge
  - Your request body exceeded `max_request_kb`
- ExecDenied
  - The command you requested isn’t in `allowed_cmds`
- PathOutsideRoot
  - The path tries to leave your workspace folder (even through symlinks)

---

## Safety tips

- Keep `allowed_cmds` extremely small
- Rotate your token occasionally (generate a new one with `openssl rand -base64 48`)
- Use a dedicated workspace folder, not your whole home folder
- Review logs if anything looks odd
- Stop Valet when you’re not using it

---

## Recap

- Run Valet locally with your config
- Use Tailscale to publish `/mcp` at a public HTTPS URL
- Give the AI your public URL and the token
- Valet will only do what you explicitly allow, inside your chosen folder
