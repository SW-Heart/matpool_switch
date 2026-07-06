# @matpool/switch

Lightweight command line entry for Matpool Switch. It installs the `matpool`
command and routes supported coding tools through the local Matpool proxy.

## Install

```bash
npm i -g @matpool/switch
```

Node.js 18 or newer is required. The package installs a small JavaScript shim
plus the native `matpool` binary for your platform.

Supported platforms:

- macOS arm64 and x64
- Linux arm64 and x64
- Windows x64

## Quick Start

```bash
matpool setup
```

`matpool setup` is the recommended first command. It can save your Matpool
token, install and start the local daemon, check proxy health, and enable
takeover for supported local coding tools.

Non-interactive install:

```bash
matpool setup --token <your_matpool_token>
```

Minimal flow:

```bash
matpool login --token <your_matpool_token>
matpool daemon install
matpool daemon start
matpool takeover all
matpool status
```

The local proxy must keep running while takeover is enabled. You can run it
through the daemon or by keeping the Matpool Switch desktop app open.

## What Takeover Changes

Takeover points supported tools at the local proxy on `127.0.0.1:15721`.
Matpool Switch writes managed settings for:

- Claude Code / Claude Desktop: `~/.claude/settings.json`
- Codex CLI: `~/.codex/auth.json` and `~/.codex/config.toml`
- Gemini CLI: `~/.gemini/.env`

The Matpool token is stored in the OS keychain using service `Matpool Switch`
and account `matpool-token`. It is not written into tool config files.

Disable takeover:

```bash
matpool takeover all --disable
```

## Commands

```bash
matpool setup
matpool login --token <token>
matpool status
matpool doctor
matpool models list
matpool models sync all
matpool provider list all
matpool provider seed
matpool takeover all
matpool takeover all --disable
matpool daemon status
matpool update
```

On first run, the CLI initializes the local Matpool Switch database, built-in
providers, and missing minimal tool config files needed for takeover.

## Troubleshooting

- `matpool status`: shows token, daemon, takeover, and provider state.
- `matpool doctor`: checks local files and common configuration problems.
- `matpool provider seed`: recreates built-in providers if the database is new
  or incomplete.
- `matpool models sync all`: refreshes the Matpool model catalog for supported
  tools.
- If a tool cannot connect, confirm the daemon is running with
  `matpool daemon status`.
- If you want to stop using the local proxy, run
  `matpool takeover all --disable`.

## Data Locations

- Database and preferences: `~/.matpool-switch/`
- Token storage: OS keychain, service `Matpool Switch`, account `matpool-token`
