# Matpool Switch

> Matpool 一键接管 Vibe Coding CLI 工具流量的桌面客户端。

本项目是 [farion1231/matpool-switch](https://github.com/farion1231/matpool-switch) 的 Matpool 定制单租户版（fork 起点：v3.16.x，MIT License），仅保留**单供应商接管 + 模型选择 + 跨设备同步**核心能力。

## 当前阶段

PoC（[OPE-2979](mention://issue/9fd01ae2-3c11-4c69-afbb-696f09bc4270)）：本地开发，**暂未建立线上代码仓库**。

## 目录结构来源

代码来自 matpool-switch 仓库，已在本地剥离 .git 历史，作为新项目从空 git 历史重新开始。

后续将逐步：

1. 删除冗余模块（mcp / skills / sessions / prompts / proxy / openclaw / usage / universal / deeplink / 多供应商管理）
2. 品牌替换（应用名、Logo、主题色、i18n、关于页 MIT 致谢）
3. Token 输入 + OS keychain 加密保存
4. 工具检测器（仅本机扫描，PoC 只接管 Claude Code）
5. 配置写入器（复用 matpool-switch 的 Rust 适配层）
6. 健康检查

## 致谢

- 上游：[farion1231/matpool-switch](https://github.com/farion1231/matpool-switch) — MIT License © Jason Young

## 相关 Issue

- 父 Issue：[OPE-2951 Matpool Token 服务 一键部署 CLI/客户端](mention://issue/0d14022f-145a-4a45-a973-2eba95758c3e)
- PoC：[OPE-2979](mention://issue/9fd01ae2-3c11-4c69-afbb-696f09bc4270)
- 后端 API：[OPE-2980](mention://issue/b6675343-b30e-4320-83a0-6da989f8654f)
- MVP：[OPE-2981](mention://issue/1522f654-c6b9-41d7-acd1-381ba23c64e6)
