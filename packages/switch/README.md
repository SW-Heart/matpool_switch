# @mtswitch/switch

Lightweight command line entry for Matpool Switch. It installs the `matpool`
command and routes supported coding tools through the local Matpool proxy.

## Install

```bash
npm i -g @mtswitch/switch
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

Claude Code takeover can run without the local proxy when the current Claude
provider uses Matpool's native Anthropic format. Codex, Gemini, and Claude
providers that need protocol conversion use the local proxy, so keep the daemon
or the Matpool Switch desktop app running for those modes.

## What Takeover Changes

Matpool Switch writes managed settings for:

- Claude Code / Claude Desktop: `~/.claude/settings.json`
- Codex CLI: `~/.codex/auth.json` and `~/.codex/config.toml`
- Gemini CLI: `~/.gemini/.env`

Codex, Gemini, and conversion-based Claude providers point at the local proxy on
`127.0.0.1:15721`. Matpool's native Claude provider is written directly to
Claude Code settings and does not require the local proxy.

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
matpool models claude
matpool models claude list
matpool models claude set --sonnet <matpool_model_id> --custom <matpool_model_id>
matpool provider list all
matpool provider seed
matpool takeover all
matpool takeover claude
matpool takeover all --disable
matpool daemon status
matpool update
```

On first run, the CLI initializes the local Matpool Switch database, built-in
providers, and missing minimal tool config files needed for takeover.

## Claude Model Selection

After `matpool takeover claude`, the CLI shows the Matpool model IDs currently
assigned to Claude Code's `/model` menu:

```text
Current Claude model configuration:
  Claude default   Claude-Sonnet-5
  Claude Sonnet    Claude-Sonnet-5
  Claude Opus      Claude-Opus-4.8
  Claude Haiku     Claude-Haiku-4.5
  Claude Fable     Claude-Fable-5
  Claude custom    Claude-Fable-5

Use current Claude model configuration? [Y/n]:
```

Press Enter to keep the defaults. Type `n` to edit each Claude menu position in
order. For each prompt, enter a Matpool model ID and press Enter to save that
slot immediately:

```text
Claude default [Claude-Sonnet-5]: <matpool_model_id>
Claude default saved: <matpool_model_id>
Claude Sonnet [Claude-Sonnet-5]: <matpool_model_id>
Claude Sonnet saved: <matpool_model_id>
```

Press Enter on an empty value to keep the current model. Type `?` at any slot
prompt to show available Matpool model IDs.

You can change the same configuration later:

```bash
matpool models claude list
matpool models claude set --sonnet <matpool_model_id>
matpool models claude set --default <matpool_model_id> --custom <matpool_model_id>
```

## Troubleshooting

- `matpool status`: shows token, daemon, takeover, and provider state.
- `matpool doctor`: checks local files and common configuration problems.
- `matpool provider seed`: recreates built-in providers if the database is new
  or incomplete.
- `matpool models sync all`: refreshes the Matpool model catalog for supported
  tools.
- `matpool models claude`: shows the current Claude menu position to Matpool
  model ID mapping.
- `matpool models claude set`: updates Claude Code model slots after takeover.
- If a tool cannot connect, confirm the daemon is running with
  `matpool daemon status`.
- If you want to stop using the local proxy, run
  `matpool takeover all --disable`.

## Data Locations

- Database and preferences: `~/.matpool-switch/`
- Token storage: OS keychain, service `Matpool Switch`, account `matpool-token`
