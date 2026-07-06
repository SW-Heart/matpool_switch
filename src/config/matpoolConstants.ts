/**
 * Matpool 网关常量
 *
 * 本项目作为 Matpool 单租户定制版，所有协议入口均统一指向同一域名。
 * - Anthropic / Gemini 协议直接挂在根路径
 * - OpenAI 协议沿用业界惯例带 /v1 后缀
 *
 * 改动 BASE_URL 时只改这一处，全项目 preset 都会跟随。
 */
export const MATPOOL_BASE_URL = "https://token.matpool.com";

export const MATPOOL_GATEWAY = {
  /** Anthropic Messages API 入口（Claude Code、Claude Desktop） */
  anthropic: MATPOOL_BASE_URL,
  /** OpenAI Chat / Responses 入口（Codex、OpenCode、Hermes 等） */
  openai: `${MATPOOL_BASE_URL}/v1`,
  /** Gemini generateContent 入口（Gemini CLI） */
  gemini: MATPOOL_BASE_URL,
} as const;

export const MATPOOL_BRAND = {
  name: "Matpool",
  nameZh: "矩池云",
  websiteUrl: "https://matpool.com",
  /**
   * 用户获取 Token 的入口 = Matpool 主站模型广场。
   * 在这里开通模型服务后，用户中心会颁发 Matpool Token。
   * （注意：不是 NewAPI 网关 `token.matpool.com`，那个是 API 入口，普通用户不直接访问）
   */
  apiKeyUrl: "https://matpool.com/models",
  description: "Matpool 一站式 AI 编程网关",
} as const;

/** 默认推荐模型 ID（PoC 阶段硬编码，后续由 /v1/client/models 拉取） */
export const MATPOOL_DEFAULT_MODELS = {
  anthropic: "claude-sonnet-4-6",
  openai: "GPT-5.5",
  gemini: "gemini-2.5-pro",
} as const;
