# Matpool Switch

[English](#english) | 简体中文

> Matpool 一键接管 Vibe Coding CLI 工具流量的桌面客户端。

一键将 **Claude Code / Claude Desktop / Codex / Gemini CLI** 的流量切到 [Matpool](https://matpool.com) 平台，无需手动编辑多个 CLI 的配置文件。

## 功能

- **一次配置 Token**：首次启动引导输入 Matpool Token，加密保存到系统 Keychain，更安全。
- **一键接管**：客户端自动写入 `~/.claude/settings.json` / `~/.codex/auth.json,config.toml` / `~/.gemini/.env` 等本地配置文件，接管其流量。
- **协议适配**：Anthropic / Gemini 等工具自动通过本地代理进行请求协议转换与路由，无缝对接 Matpool 平台。
- **跨平台**：支持 Windows / macOS / Linux，基于 Tauri 2 + React 18 编写。

## CLI 快速安装

```bash
npm i -g @mtswitch/switch
matpool setup
```

常用命令：

```bash
matpool login --token <your_matpool_token>
matpool daemon status
matpool takeover all
matpool takeover claude
matpool models claude list
matpool models claude set --sonnet <matpool_model_id> --custom <matpool_model_id>
matpool takeover all --disable
matpool doctor
```

Claude Code 接管会写入 `~/.claude/settings.json`。当当前 Claude 供应商是
Matpool 原生 Anthropic 格式时，不需要本地代理；需要协议转换的 Claude 供应商、
Codex、Gemini 仍会使用本地代理 `127.0.0.1:15721`，此时需要保持 Matpool daemon
或桌面客户端运行。

执行 `matpool takeover claude` 后，CLI 会展示当前 Claude `/model` 菜单位置对应的
Matpool 模型 ID，并询问是否使用默认配置：

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

直接回车使用当前默认配置；输入 `n` 后会依次提示 `Claude default`、`Claude Sonnet`、
`Claude Opus`、`Claude Haiku`、`Claude Fable`、`Claude custom`。每一项输入
Matpool 模型 ID 并回车后会立即保存并提示成功；直接回车保留当前值；输入 `?`
可查看可用模型 ID。后续可用 `matpool models claude list` 查看可用模型，或用
`matpool models claude set --sonnet <matpool_model_id>` 修改指定位置。

## 开发指南

### 环境要求

- Node.js 18+
- pnpm 8+
- Rust 1.85+

### 开发命令

```bash
# 安装依赖
pnpm install

# 启动本地开发服务（前后端热重载）
pnpm tauri dev

# 运行前端测试
pnpm test:unit

# 运行后端 Rust 测试
cargo test --manifest-path src-tauri/Cargo.toml

# 生产环境打包构建
pnpm tauri build
```

## 数据存储

- **数据库与配置目录**：`~/.matpool-switch/` (存储 SQLite 数据库 `matpool-switch.db` 及设置文件 `settings.json`)
- **密钥存储**：系统 OS Keychain（使用服务名 `Matpool Switch`，账号 `matpool-token`）

## 开源协议

MIT License

---

<a name="english"></a>
# Matpool Switch (English)

English | [简体中文](#matpool-switch)

> Desktop client for one-click proxying and managing AI coding CLI tools' traffic to the Matpool platform.

Helps users route traffic from **Claude Code / Claude Desktop / Codex / Gemini CLI** to the [Matpool](https://matpool.com) platform without manually editing configuration files of multiple CLI tools.

## Features

- **One-time Token Setup**: Enter your Matpool Token during setup, which is securely saved in the system OS Keychain.
- **One-click Takeover**: Automatically writes to local configurations like `~/.claude/settings.json`, `~/.codex/auth.json,config.toml`, and `~/.gemini/.env` to intercept and manage CLI tool requests.
- **Protocol Adaptation**: Anthropic / Gemini CLI requests are seamlessly routed and translated locally via proxy to match the Matpool platform endpoints.
- **Cross-platform**: Supports Windows / macOS / Linux, built with Tauri 2 + React 18.

## CLI Quick Install

```bash
npm i -g @mtswitch/switch
matpool setup
```

Common commands:

```bash
matpool login --token <your_matpool_token>
matpool daemon status
matpool takeover all
matpool takeover claude
matpool models claude list
matpool models claude set --sonnet <matpool_model_id> --custom <matpool_model_id>
matpool takeover all --disable
matpool doctor
```

Claude Code takeover writes `~/.claude/settings.json`. When the current Claude
provider uses Matpool's native Anthropic format, no local proxy is required.
Claude providers that need protocol conversion, Codex, and Gemini still use the
local proxy `127.0.0.1:15721`; keep the Matpool daemon or desktop client running
for those takeover modes.

After `matpool takeover claude`, the CLI shows the current Matpool model IDs
used by Claude Code's `/model` menu:

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

Press Enter to keep the defaults. Type `n` to edit each menu position in order:
`Claude default`, `Claude Sonnet`, `Claude Opus`, `Claude Haiku`,
`Claude Fable`, and `Claude custom`. Enter a Matpool model ID and press Enter
to save that slot immediately; press Enter on an empty value to keep the
current model; type `?` to list available model IDs. Later, run
`matpool models claude list` or
`matpool models claude set --sonnet <matpool_model_id>` to update slots.

## Development

### Requirements

- Node.js 18+
- pnpm 8+
- Rust 1.85+

### Development Commands

```bash
# Install dependencies
pnpm install

# Start local dev server (hot reload for both frontend & backend)
pnpm tauri dev

# Run frontend tests
pnpm test:unit

# Run backend Rust tests
cargo test --manifest-path src-tauri/Cargo.toml

# Build for production
pnpm tauri build
```

## Data Storage

- **Database & Config Directory**: `~/.matpool-switch/` (stores the SQLite database `matpool-switch.db` and preferences `settings.json`)
- **Keyring**: System OS Keychain (Service: `Matpool Switch`, Account: `matpool-token`)

## License

MIT License
