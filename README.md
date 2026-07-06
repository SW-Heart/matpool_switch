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
matpool takeover all --disable
matpool doctor
```

接管模式会把 Claude Code / Claude Desktop / Codex / Gemini CLI 指向本地代理
`127.0.0.1:15721`。启用接管后，需要保持 Matpool daemon 或桌面客户端运行。

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
matpool takeover all --disable
matpool doctor
```

Takeover points Claude Code / Claude Desktop / Codex / Gemini CLI at the local
proxy `127.0.0.1:15721`. Keep the Matpool daemon or desktop client running
while takeover is enabled.

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
