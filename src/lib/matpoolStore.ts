/**
 * Matpool Token 客户端存储
 *
 * - Token 走 Tauri keychain bridge（macOS Keychain / Windows Credential Manager / Linux Secret Service）
 *   后端命令：`matpool_keychain_get/set/clear`，实现见 src-tauri/src/commands/matpool_keychain.rs
 * - 激活模型选择仍走 localStorage（非敏感、单设备状态）；跨设备同步等 OPE-2980 后端 API 就位后再统一改造
 *
 * Token 异步 API（async）；激活模型同步 API（基于 localStorage）。
 */

import { invoke } from "@tauri-apps/api/core";

const ACTIVE_MODELS_KEY = "matpool.activeModels";

export interface MatpoolActiveModels {
  /** Claude Code / Claude Desktop 选中的模型 ID（Anthropic 协议） */
  anthropic?: string;
  /** Codex / OpenCode / Hermes 选中的模型 ID（OpenAI 协议） */
  openai?: string;
  /** Gemini CLI 选中的模型 ID（Gemini 协议） */
  gemini?: string;
}

export const matpoolStore = {
  async getToken(): Promise<string> {
    try {
      const value = await invoke<string | null>("matpool_keychain_get");
      return (value ?? "").trim();
    } catch (err) {
      console.error("[matpoolStore] getToken failed:", err);
      return "";
    }
  },

  async setToken(token: string): Promise<void> {
    await invoke("matpool_keychain_set", { token: token.trim() });
  },

  async clearToken(): Promise<void> {
    await invoke("matpool_keychain_clear");
  },

  async hasToken(): Promise<boolean> {
    const t = await this.getToken();
    return t.length > 0;
  },

  getActiveModels(): MatpoolActiveModels {
    try {
      const raw = localStorage.getItem(ACTIVE_MODELS_KEY);
      return raw ? (JSON.parse(raw) as MatpoolActiveModels) : {};
    } catch {
      return {};
    }
  },

  setActiveModels(models: MatpoolActiveModels): void {
    try {
      localStorage.setItem(ACTIVE_MODELS_KEY, JSON.stringify(models));
    } catch (err) {
      console.error("[matpoolStore] setActiveModels failed:", err);
    }
  },
};
