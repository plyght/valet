# Valet — Simple Setup and Tailscale Funnel Guide

This guide explains in plain English how to get Valet running on your Mac and safely share it over the internet using Tailscale Funnel.

## What Valet does (in simple terms)

- Valet gives an AI assistant a single, safe doorway to your Mac.
- The AI can only do three things through that doorway:
  1) Read a file in a specific folder you choose
  2) Write a file in that same folder
  3) Run a very small list of commands you approve
- Everything is locked down with a secret token, an approved website list, size limits, time limits, and detailed logs.

## What you need

- A Mac (macOS 13 or later is ideal)
- Tailscale installed and logged in (Pro or Enterprise if you want Funnel)
- A Tailscale node where you’ll run Valet (your Mac)

## Step 1 — Pick a workspace folder

Choose a folder where Valet is allowed to read and write files. Valet will refuse to touch anything outside this folder. Example: your project folder.

## Step 2 — Choose the few safe commands

Decide which commands you want to allow. Keep the list short, safe, and predictable. Good examples: `echo` or `ls`. Avoid anything that can modify the system broadly.

## Step 3 — Create a strong secret token

Make up a long, unique token (a random string). You’ll give this to the AI service so it can call Valet. Never put this token in a URL; only send it as a header (Valet enforces this).

## Step 4 — Tell Valet who’s allowed to call it

Valet only accepts calls from websites you list. Add your public URL (the Tailscale Funnel URL you’ll create in step 7) to the allowed list. If you don’t know it yet, you can add it later and restart Valet.

## Step 5 — Create Valet’s config file

You’ll need to provide Valet with:
- The workspace folder (root directory)
- The list of allowed commands
- The bearer token
- The allowed website origins (your public URL from Funnel)
- Limits (reasonable defaults are fine): timeouts, maximum output size, maximum request size

Keep this file somewhere safe on your Mac and remember its path.

## Step 6 — Start Valet on your Mac

- Run Valet and point it at your config file.
- Valet listens only on your Mac (127.0.0.1). It is not exposed to the internet yet.
- When Valet is ready, it prints one short line with the address and enabled tools.

Tip: If you want Valet to run in the background after login, use the included launchd example (in `launchd/valet.plist`). Update the paths inside that file for your system before loading it.

## Step 7 — Publish Valet with Tailscale Funnel

Funnel gives you a public HTTPS URL that points at your Mac.

- Turn on Tailscale Funnel for the Mac running Valet
- Tell Funnel to forward a public URL (like `https://<name>.ts.net/mcp`) to your local Valet address (127.0.0.1:5555 by default)
- In your Valet config, set the allowed origin to that exact public URL from Tailscale
- Restart Valet so it picks up the new allowed origin

Now, your AI assistant can use that HTTPS URL to reach your Valet, with your secret token.

## Step 8 — Connect your AI assistant

- Give the AI assistant your public HTTPS URL (from Tailscale)
- Give it the bearer token you chose
- The AI must send its calls with:
  - The `Origin` header matching your allowed origin (the Funnel URL)
  - The `Authorization` header set to `Bearer <your-token>`

If either is missing or wrong, Valet refuses the request.

## How Valet protects you

- Only runs on your Mac; you control when it’s on
- Only accepts calls with the correct secret token
- Only accepts calls from the allowed website origins
- Only reads/writes inside your chosen folder
- Only runs commands you listed
- Limits time and output size for commands
- Limits request sizes
- Logs what happens (without recording file contents)

## Tips for safe use

- Keep the allowed commands list very short
- Keep the token secret and rotate it occasionally
- Use a dedicated workspace folder for the AI (not your entire home directory)
- Review logs if something looks unusual
- Stop Valet when you don’t need it

## Troubleshooting

- Getting “Unauthorized”?
  - Check your bearer token header is present and matches
- Getting “OriginDenied”?
  - Make sure the `Origin` header exactly matches your Tailscale Funnel URL (including `https://`)
- Getting “RequestTooLarge”?
  - Your request body is bigger than the limit in your config
- Getting “ExecDenied”?
  - The command you requested is not on the allowed list
- Getting “PathOutsideRoot”?
  - The file path points outside your chosen folder (even through symlinks)

If you need to change security settings (token, origins, limits, allowed commands), update the config file and restart Valet.

## When you’re done

- Stop Valet
- Turn off Tailscale Funnel (or keep it on if you plan to use again soon)
- Keep your config and token safe
