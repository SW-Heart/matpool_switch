/**
 * Matpool 后端 API 客户端
 *
 * 直接打 token.matpool.com 的 OpenAI 兼容路径（NewAPI 网关已暴露 /v1/models）。
 * 这一份只覆盖 PoC 必须的两件事：Token 校验、模型列表。
 *
 * 跨设备同步 / 更详细的 Profile API 等待 OPE-2980 后端就位后再加。
 */

import { MATPOOL_GATEWAY } from "@/config/matpoolConstants";

export interface MatpoolModel {
  id: string;
  /** 协议归属：客户端按这个字段过滤每个 CLI 工具能选的模型 */
  protocol?: "anthropic" | "openai" | "gemini";
  context_window?: number;
  billing_tier?: string;
  /** OpenAI /v1/models 风格响应里没有这些扩展字段时也能正常工作 */
  [key: string]: unknown;
}

export interface MatpoolValidateResult {
  ok: boolean;
  /** 后端返回的模型数（用于在 wizard 上给一行 "已检测到 N 个模型可用" 的反馈） */
  modelCount?: number;
  /** 失败原因；ok=true 时不展示 */
  error?: string;
}

/**
 * 用 Token 调一次 /v1/models，作为校验。
 * - 200 + 至少一个 model → ok
 * - 401 / 403 → Token 无效
 * - 其他网络错误 → 视为暂不可用，error 里保留原始信息
 */
export async function validateMatpoolToken(
  token: string,
): Promise<MatpoolValidateResult> {
  const trimmed = token.trim();
  if (!trimmed) {
    return { ok: false, error: "empty token" };
  }

  try {
    const resp = await fetch(`${MATPOOL_GATEWAY.openai}/models`, {
      method: "GET",
      headers: {
        Authorization: `Bearer ${trimmed}`,
        Accept: "application/json",
      },
    });

    if (resp.status === 401 || resp.status === 403) {
      return { ok: false, error: `auth failed (${resp.status})` };
    }
    if (!resp.ok) {
      return { ok: false, error: `HTTP ${resp.status}` };
    }

    const data = await resp.json().catch(() => null);
    const list: unknown =
      data && typeof data === "object"
        ? (data as { data?: unknown }).data ?? data
        : null;
    const modelCount = Array.isArray(list) ? list.length : undefined;
    return { ok: true, modelCount };
  } catch (err) {
    return {
      ok: false,
      error: err instanceof Error ? err.message : String(err),
    };
  }
}

/**
 * 拉取模型列表用于 UI 展示。
 *
 * NewAPI 默认返回 OpenAI 风格 ({ data: [{ id, ... }] })，这里返回扁平数组。
 * 如果未来后端独立 /v1/client/models 上线（OPE-2980 决定的字段含 protocol），
 * 这里加 fallback 优先调那个端点。
 */
export async function fetchMatpoolModels(token: string): Promise<MatpoolModel[]> {
  const trimmed = token.trim();
  if (!trimmed) return [];

  const resp = await fetch(`${MATPOOL_GATEWAY.openai}/models`, {
    headers: {
      Authorization: `Bearer ${trimmed}`,
      Accept: "application/json",
    },
  });

  if (!resp.ok) {
    throw new Error(`fetchMatpoolModels failed: HTTP ${resp.status}`);
  }

  const data = await resp.json();
  const list = Array.isArray(data) ? data : data?.data;
  if (!Array.isArray(list)) {
    return [];
  }
  return list as MatpoolModel[];
}

/**
 * 一条 Matpool 模型广场（matpool.com/models）的条目。
 *
 * Matpool 后端在 https://token.matpool.com/api/pricing 暴露真正的"产品级"模型清单
 * （含描述 / 标签 / 模态 / 计费比率）。这是 UI 上展示给用户的来源——比 OpenAI 风格
 * `/v1/models` 信息更全。
 */
export interface MatpoolPricingModel {
  model_name: string;
  description?: string;
  tags?: string;
  vendor_id?: number;
  /** TEXT / IMAGE / CODE / EMBEDDING / AUDIO / VIDEO / VISION / MATH / openai */
  supported_endpoint_types?: string[];
  /** default / domestic / internationality / MatPilot */
  enable_groups?: string[];
  model_ratio?: number;
  completion_ratio?: number;
  model_price?: number;
}

interface PricingCacheEntry {
  fetched_at: number;
  models: MatpoolPricingModel[];
}

const PRICING_CACHE_KEY = "matpool.pricingCache";
const PRICING_CACHE_TTL_MS = 5 * 60 * 1000; // 5 分钟

function readPricingCache(): PricingCacheEntry | null {
  try {
    const raw = localStorage.getItem(PRICING_CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PricingCacheEntry;
    if (
      typeof parsed?.fetched_at !== "number" ||
      !Array.isArray(parsed.models)
    ) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

function writePricingCache(models: MatpoolPricingModel[]): void {
  try {
    const entry: PricingCacheEntry = {
      fetched_at: Date.now(),
      models,
    };
    localStorage.setItem(PRICING_CACHE_KEY, JSON.stringify(entry));
  } catch (err) {
    console.warn("[matpoolApi] failed to write pricing cache:", err);
  }
}

/**
 * 拉取 matpool.com 的模型广场清单。
 *
 * 这个接口 **不需要 Token**（公开数据），但请求时带上 Token 也无妨；如果 token 为空
 * 也会照常工作。
 *
 * 缓存策略：
 * - 5 分钟 TTL
 * - `force=true` 强制刷新（用户点"刷新"按钮）
 * - 拉取失败时 fallback 到上次缓存（即使 stale）
 */
export async function fetchMatpoolPricingModels(options?: {
  force?: boolean;
}): Promise<MatpoolPricingModel[]> {
  const force = options?.force ?? false;

  if (!force) {
    const cached = readPricingCache();
    if (cached && Date.now() - cached.fetched_at < PRICING_CACHE_TTL_MS) {
      return cached.models;
    }
  }

  try {
    const resp = await fetch("https://token.matpool.com/api/pricing", {
      method: "GET",
      headers: { Accept: "application/json" },
    });
    if (!resp.ok) {
      throw new Error(`HTTP ${resp.status}`);
    }
    const json = await resp.json();
    const list = Array.isArray(json?.data) ? json.data : [];
    const models = list.filter(
      (m: any) => m && typeof m.model_name === "string",
    ) as MatpoolPricingModel[];
    writePricingCache(models);
    return models;
  } catch (err) {
    console.warn("[matpoolApi] /api/pricing failed, falling back to cache:", err);
    const stale = readPricingCache();
    return stale?.models ?? [];
  }
}

/**
 * 仅返回支持文本/代码生成的模型（适合放进 CLI dropdown）。
 *
 * 不在 supported_endpoint_types 里包含 TEXT 或 CODE 的（纯图片/音频/视频/embedding）
 * 不应出现在 CLI dropdown 里——CLI 工具不会用它们。
 */
export function filterChatCapableModels(
  all: MatpoolPricingModel[],
): MatpoolPricingModel[] {
  return all.filter((m) => {
    const types = m.supported_endpoint_types ?? [];
    return types.includes("TEXT") || types.includes("CODE");
  });
}
