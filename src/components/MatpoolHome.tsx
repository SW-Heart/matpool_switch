import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { toast } from "sonner";
import {
  Settings as SettingsIcon,
  KeyRound,
  RefreshCw,
  AlertCircle,
  Loader2,
} from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useProvidersQuery } from "@/lib/query";
import { useSwitchProviderMutation } from "@/lib/query/mutations";
import { useProxyStatus } from "@/hooks/useProxyStatus";
import { providersApi, settingsApi, type AppId } from "@/lib/api";
import { proxyApi } from "@/lib/api/proxy";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { matpoolStore } from "@/lib/matpoolStore";
import {
  validateMatpoolToken,
  fetchMatpoolPricingModels,
  filterChatCapableModels,
  type MatpoolPricingModel,
} from "@/lib/matpoolApi";
import type { Provider } from "@/types";
import { extractErrorMessage } from "@/utils/errorUtils";
import { ClaudeIcon, CodexIcon, GeminiIcon } from "@/components/BrandIcons";

/**
 * Matpool Switch 主界面（极简单租户）
 *
 * 视觉风格参考矩池云国际版前端 (matcloud_global)：
 * - 主品牌色 #0068FF（蓝），强调色 #27D4DF（青）
 * - 卡片走"3 层背景"体系：页面 mg、卡片 lowest、内嵌 container-low
 * - 圆角 rounded-xl（卡片）/ rounded-lg（行项目）
 * - 轻阴影 shadow-sm（不要花哨的渐变）
 *
 * 用户路径：输 Token → 一键开/关每个 CLI 工具的接管 + 选模型。
 */

interface ToolEntry {
  appId: AppId;
  matpoolSeedId: string;
  /** 后端 get_tool_versions 用的工具名 */
  toolName: "claude" | "codex" | "gemini";
  label: string;
  subLabel: string;
  /** Logo 组件，复用 BrandIcons */
  Icon: React.FC<{ size?: number; className?: string }>;
  /** 用户没装时的安装命令 */
  installHint: string;
  /** 当前版 dropdown 是否激活——Codex toml 改写未实现，先 disable */
  modelEditable: boolean;
}

const TOOLS: ToolEntry[] = [
  {
    appId: "claude",
    matpoolSeedId: "matpool-claude",
    toolName: "claude",
    label: "Claude Code",
    subLabel: "Anthropic CLI",
    Icon: ClaudeIcon,
    installHint: "npm i -g @anthropic-ai/claude-code",
    modelEditable: true,
  },
  {
    appId: "codex",
    matpoolSeedId: "matpool-codex",
    toolName: "codex",
    label: "Codex",
    subLabel: "OpenAI Codex",
    Icon: CodexIcon,
    installHint: "npm i -g @openai/codex",
    modelEditable: true,
  },
  {
    appId: "gemini",
    matpoolSeedId: "matpool-gemini",
    toolName: "gemini",
    label: "Gemini CLI",
    subLabel: "Google",
    Icon: GeminiIcon,
    installHint: "npm i -g @google/gemini-cli",
    modelEditable: true,
  },
];

type DetectedTools = Record<string, { installed: boolean; version: string | null }>;

export function MatpoolHome({
  tokenVersion = 0,
  onOpenSettings,
  onOpenTokenWizard,
}: {
  tokenVersion?: number;
  onOpenSettings: () => void;
  onOpenTokenWizard: () => void;
}) {
  const [tokenStatus, setTokenStatus] = useState<
    | { kind: "loading" }
    | { kind: "missing" }
    | { kind: "ok"; preview: string }
  >({ kind: "loading" });

  const [detected, setDetected] = useState<DetectedTools | null>(null);
  const [showAll, setShowAll] = useState(false);

  // /api/pricing 模型列表（5min 缓存 + 共享 query）
  const {
    data: pricingModels,
    refetch: refetchPricing,
    isFetching: pricingFetching,
  } = useQuery({
    queryKey: ["matpoolPricingModels"],
    queryFn: () => fetchMatpoolPricingModels(),
    staleTime: 5 * 60 * 1000,
  });

  const refreshTokenStatus = async () => {
    try {
      const token = await matpoolStore.getToken();
      if (!token) {
        setTokenStatus({ kind: "missing" });
      } else {
        const preview =
          token.length > 12
            ? `${token.slice(0, 6)}…${token.slice(-4)}`
            : "已配置";
        setTokenStatus({ kind: "ok", preview });
      }
    } catch (err) {
      console.error("[MatpoolHome] failed to read keychain", err);
      setTokenStatus({ kind: "missing" });
    }
  };

  const refreshToolDetection = async () => {
    try {
      // 给 15s 超时：后端并行后最慢工具 ~2s，加上 IPC 等，正常 ≤ 3s。
      // 15s 是纯安全网（bad network / npm 超时等极端情况）。
      const versions = await Promise.race([
        settingsApi.getToolVersions(TOOLS.map((t) => t.toolName)),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error("tool detection timed out (>15s)")), 15000),
        ),
      ]);
      const next: DetectedTools = {};
      for (const v of versions) {
        next[v.name] = {
          installed: !!v.version && !v.installed_but_broken,
          version: v.version,
        };
      }
      setDetected(next);
    } catch (err) {
      console.error("[MatpoolHome] tool detection failed", err);
      setDetected({});
    }
  };

  useEffect(() => {
    void refreshTokenStatus();
    void refreshToolDetection();
  }, [tokenVersion]);

  const detecting = detected === null;
  const installedTools = detected
    ? TOOLS.filter((t) => detected[t.toolName]?.installed)
    : [];
  const missingTools = detected
    ? TOOLS.filter((t) => !detected[t.toolName]?.installed)
    : [];
  const visibleTools = showAll ? TOOLS : installedTools;

  const chatModels = useMemo(
    () => (pricingModels ? filterChatCapableModels(pricingModels) : []),
    [pricingModels],
  );

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
      className="mx-auto max-w-2xl space-y-5 p-6"
    >
      <header className="flex items-start justify-between gap-4">
        <div className="flex-1 min-w-0">
          <h1 className="text-[22px] font-semibold tracking-tight text-foreground leading-tight">
            欢迎使用 Matpool Switch
          </h1>
          <p className="text-[13px] text-muted-foreground mt-1.5 leading-relaxed">
            一键把 Claude Code、Codex、Gemini CLI 等工具的流量接入 Matpool 平台
          </p>
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={onOpenSettings}
          title="设置"
          className="rounded-full h-9 w-9 shrink-0"
        >
          <SettingsIcon className="h-[18px] w-[18px]" />
        </Button>
      </header>

      <TokenCard
        status={tokenStatus}
        onConfigure={onOpenTokenWizard}
        onRefresh={refreshTokenStatus}
      />

      <section className="space-y-2.5">
        <div className="flex items-center justify-between px-1">
          <h2 className="text-[13px] font-medium text-muted-foreground">
            CLI 工具接管
          </h2>
          <div className="flex items-center gap-3 text-[11px] text-muted-foreground/70">
            <button
              type="button"
              onClick={() => void refreshToolDetection()}
              className="hover:text-muted-foreground inline-flex items-center gap-1 transition-colors"
              title="重新扫描本机已装工具"
            >
              <RefreshCw className={`h-3 w-3 ${detecting ? "animate-spin" : ""}`} />
              {detecting ? "检测中..." : `${installedTools.length} / ${TOOLS.length} 已安装`}
            </button>
            <button
              type="button"
              onClick={() => void refetchPricing()}
              className="hover:text-muted-foreground inline-flex items-center gap-1 transition-colors"
              title="刷新模型列表"
            >
              <RefreshCw className={`h-3 w-3 ${pricingFetching ? "animate-spin" : ""}`} />
              {chatModels.length > 0 ? `${chatModels.length} 个可用模型` : "模型列表"}
            </button>
          </div>
        </div>

        {detecting ? (
          <div className="rounded-xl border border-border/60 bg-card flex items-center justify-center py-10">
            <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
          </div>
        ) : visibleTools.length > 0 ? (
          <div className="rounded-xl border border-border/60 bg-card overflow-hidden divide-y divide-border/40">
            {visibleTools.map((tool) => (
              <ToolRow
                key={tool.appId}
                tool={tool}
                tokenReady={tokenStatus.kind === "ok"}
                detected={detected![tool.toolName]}
                models={chatModels}
              />
            ))}
          </div>
        ) : (
          <div className="rounded-xl border border-dashed border-border/60 bg-card/40 px-5 py-8 text-center">
            <p className="text-[13px] text-muted-foreground">
              本机未检测到任何受支持的 CLI 工具
            </p>
            <p className="text-[11px] text-muted-foreground/70 mt-2 leading-relaxed">
              请先安装 Claude Code / Codex / Gemini CLI 任一种，
              <br />
              然后点击右上角"重新扫描"
            </p>
          </div>
        )}

        {!detecting && missingTools.length > 0 && (
          <div className="px-1 pt-1">
            <button
              type="button"
              onClick={() => setShowAll(!showAll)}
              className="text-[11px] text-muted-foreground/70 hover:text-muted-foreground transition-colors"
            >
              {showAll
                ? `隐藏 ${missingTools.length} 个未安装的工具`
                : `显示 ${missingTools.length} 个未安装的工具`}
            </button>
          </div>
        )}
      </section>

      <p className="text-center text-[11px] text-muted-foreground/60 pt-2">
        切换接管或模型后，请重启对应 CLI 工具（Claude Code 支持热切换无需重启）
      </p>
    </motion.div>
  );
}

function TokenCard({
  status,
  onConfigure,
  onRefresh,
}: {
  status:
    | { kind: "loading" }
    | { kind: "missing" }
    | { kind: "ok"; preview: string };
  onConfigure: () => void;
  onRefresh: () => void | Promise<void>;
}) {
  const [validating, setValidating] = useState(false);

  const handleValidate = async () => {
    setValidating(true);
    try {
      const token = await matpoolStore.getToken();
      if (!token) {
        toast.error("未配置 Token");
        return;
      }
      const result = await validateMatpoolToken(token);
      if (result.ok) {
        toast.success(
          result.modelCount != null
            ? `Token 有效，可用模型 ${result.modelCount} 个`
            : "Token 有效",
        );
      } else {
        toast.error(`Token 校验失败：${result.error ?? "未知错误"}`);
      }
    } catch (err) {
      toast.error(extractErrorMessage(err) || "校验失败");
    } finally {
      setValidating(false);
    }
  };

  const isOk = status.kind === "ok";

  return (
    <div
      className={`rounded-xl border p-5 shadow-sm transition-colors ${
        isOk
          ? "border-blue-200 bg-blue-50/50 dark:border-blue-800/50 dark:bg-blue-950/20"
          : "border-border bg-card"
      }`}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-3 min-w-0">
          <div
            className={`rounded-lg p-2.5 shrink-0 ${
              isOk
                ? "bg-blue-500/10 text-blue-600 dark:text-blue-400"
                : "bg-muted text-muted-foreground"
            }`}
          >
            <KeyRound className="h-[18px] w-[18px]" />
          </div>
          <div className="min-w-0">
            <p className="text-[14px] font-medium leading-tight">
              Matpool Token
            </p>
            <p className="text-[12px] text-muted-foreground mt-1 font-mono truncate">
              {status.kind === "loading"
                ? "读取中..."
                : status.kind === "missing"
                  ? "未配置 — 请先粘贴 Token"
                  : status.preview}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          {isOk && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleValidate}
              disabled={validating}
              className="h-8 px-2.5 gap-1 text-[12px]"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${validating ? "animate-spin" : ""}`} />
              校验
            </Button>
          )}
          <Button
            variant={isOk ? "outline" : "default"}
            size="sm"
            onClick={() => {
              onConfigure();
              setTimeout(() => void onRefresh(), 800);
            }}
            className="h-8 text-[12px]"
          >
            {isOk ? "重新配置" : "配置 Token"}
          </Button>
        </div>
      </div>
    </div>
  );
}

/**
 * 从 provider.settingsConfig 里读出 model 字段（用于 dropdown 默认值）。
 *
 * 三个 app 的存储位置不同：
 * - Claude  : settingsConfig.env.ANTHROPIC_MODEL
 * - Codex   : settingsConfig.config 是一段 TOML 字符串（model = "..."）
 * - Gemini  : settingsConfig.env.GEMINI_MODEL
 */
/**
 * 各工具支持的模型 slot。
 *
 * Claude Code 用 4 档（主 + 3 个 /model 命令角色），Codex 只默认主模型（reasoning 后续加）。
 */
interface ModelSlotDef {
  key: string;
  label: string;
  /** env 变量名（Claude/Gemini） */
  envKey?: string;
  /** 是否是 Codex 的 toml model 字段 */
  isCodexTomlModel?: boolean;
}

const CLAUDE_MODEL_SLOTS: ModelSlotDef[] = [
  { key: "main", label: "默认模型", envKey: "ANTHROPIC_MODEL" },
  { key: "opus", label: "Opus (/model opus)", envKey: "ANTHROPIC_DEFAULT_OPUS_MODEL" },
  { key: "sonnet", label: "Sonnet (/model sonnet)", envKey: "ANTHROPIC_DEFAULT_SONNET_MODEL" },
  { key: "haiku", label: "Haiku (/model haiku)", envKey: "ANTHROPIC_DEFAULT_HAIKU_MODEL" },
];

const GEMINI_MODEL_SLOTS: ModelSlotDef[] = [
  { key: "main", label: "默认模型", envKey: "GEMINI_MODEL" },
];

const CODEX_MODEL_SLOTS: ModelSlotDef[] = [
  { key: "main", label: "默认模型", isCodexTomlModel: true },
];

/** Codex 默认 context_window 完全值地 */
const CODEX_DEFAULT_CONTEXT_WINDOW = 128_000;

/** reasoning_effort 预选值（固定，非 /api/pricing 模型） */
const REASONING_EFFORTS = ["high", "medium", "low"];

function modelSlotsFor(appId: AppId): ModelSlotDef[] {
  switch (appId) {
    case "claude": return CLAUDE_MODEL_SLOTS;
    case "codex": return CODEX_MODEL_SLOTS;
    case "gemini": return GEMINI_MODEL_SLOTS;
    default: return [];
  }
}

function readProviderModelSlot(provider: Provider | undefined, _appId: AppId, slot: ModelSlotDef): string {
  if (!provider) return "";
  const sc = provider.settingsConfig as any;
  if (slot.envKey) {
    return sc?.env?.[slot.envKey] ?? "";
  }
  if (slot.isCodexTomlModel) {
    const config = typeof sc?.config === "string" ? sc.config : "";
    const match = config.match(/^\s*model\s*=\s*"([^"]+)"/m);
    return match?.[1] ?? "";
  }
  return "";
}

function writeProviderModelSlot(
  provider: Provider,
  _appId: AppId,
  slot: ModelSlotDef,
  newModel: string,
): Provider | null {
  const sc = JSON.parse(JSON.stringify(provider.settingsConfig ?? {}));
  if (slot.envKey) {
    sc.env = sc.env ?? {};
    sc.env[slot.envKey] = newModel;
    return { ...provider, settingsConfig: sc };
  }
  if (slot.isCodexTomlModel) {
    const config = typeof sc?.config === "string" ? sc.config : "";
    if (!config) return null;
    const escaped = newModel.replace(/"/g, '\\"');
    const pattern = /^(\s*)model\s*=\s*"[^"]*"/m;
    if (!pattern.test(config)) {
      sc.config = `model = "${escaped}"\n${config}`;
    } else {
      sc.config = config.replace(pattern, `$1model = "${escaped}"`);
    }
    return { ...provider, settingsConfig: sc };
  }
  return null;
}

function ToolRow({
  tool,
  tokenReady,
  detected,
  models,
}: {
  tool: ToolEntry;
  tokenReady: boolean;
  detected: { installed: boolean; version: string | null } | undefined;
  models: MatpoolPricingModel[];
}) {
  const { data, refetch } = useProvidersQuery(tool.appId);
  const switchMutation = useSwitchProviderMutation(tool.appId);
  const { takeoverStatus } = useProxyStatus();
  const queryClient = useQueryClient();

  const isInstalled = detected?.installed ?? false;
  const version = detected?.version ?? null;
  const seedExists = data?.providers?.[tool.matpoolSeedId] != null;
  // takeover 状态才是"接管开关"的真正 source of truth：cancel takeover 后
  // current_provider_id 仍是 matpool seed，但 live config 已恢复为 backup 内容，
  // 此时开关应显示"关闭"。
  const isMatpoolActive = !!takeoverStatus?.[tool.appId];

  const provider = data?.providers?.[tool.matpoolSeedId];
  const slots = modelSlotsFor(tool.appId);
  const [savingModel, setSavingModel] = useState(false);

  /** 把 /api/pricing 的 TEXT 模型同步进 codex provider 的 modelCatalog */
  const syncCodexModelCatalog = async () => {
    if (tool.appId !== "codex" || !provider || models.length === 0) return;
    const catalogModels = models.map((m) => ({
      model: m.model_name,
      display_name: m.model_name,
      context_window: CODEX_DEFAULT_CONTEXT_WINDOW,
    }));
    const sc = JSON.parse(JSON.stringify(provider.settingsConfig ?? {}));
    sc.modelCatalog = { models: catalogModels };
    // 设 apiFormat 告诉 proxy 当前模型是否原生支持 /v1/responses
    const currentModel = readProviderModelSlot(provider, tool.appId, CODEX_MODEL_SLOTS[0]);
    sc.apiFormat = currentModel.toLowerCase().includes("gpt-5.5") ? "openai_responses" : "openai_chat";
    const updated = { ...provider, settingsConfig: sc };
    await providersApi.update(updated, tool.appId);
  };

  const handleToggle = async (checked: boolean) => {
    if (!isInstalled) {
      toast.error(`${tool.label} 未安装：${tool.installHint}`);
      return;
    }
    if (!seedExists) {
      toast.error(`${tool.label}: Matpool 供应商未就绪，请重启应用`);
      return;
    }
    if (!tokenReady && checked) {
      toast.error("请先配置 Matpool Token");
      return;
    }
    if (checked) {
      try {
        await switchMutation.mutateAsync(tool.matpoolSeedId);
        // Codex: 先把 /api/pricing 所有 TEXT 模型同步进 modelCatalog，再接管
        if (tool.appId === "codex") {
          await syncCodexModelCatalog().catch(() => {});
        }
        await proxyApi.setProxyTakeoverForApp(tool.appId, true);
        // takeover 状态改了之后必须 invalidate `proxyTakeoverStatus` query，否则
        // useProxyStatus 缓存的 takeoverStatus 不变 → 开关不刷新（虽然 toast 说成功）。
        // 同步 invalidate proxyStatus（运行状态可能从 stopped 变 running）。
        await queryClient.invalidateQueries({ queryKey: ["proxyTakeoverStatus"] });
        await queryClient.invalidateQueries({ queryKey: ["proxyStatus"] });
        toast.success(`${tool.label} 已接管`);
        await refetch();
      } catch (err) {
        toast.error(`接管失败：${extractErrorMessage(err) || "未知错误"}`);
      }
    } else {
      try {
        await proxyApi.setProxyTakeoverForApp(tool.appId, false);
        await queryClient.invalidateQueries({ queryKey: ["proxyTakeoverStatus"] });
        await queryClient.invalidateQueries({ queryKey: ["proxyStatus"] });
        toast.success(`${tool.label} 已取消接管`);
        await refetch();
      } catch (err) {
        toast.error(`取消接管失败：${extractErrorMessage(err) || "未知错误"}`);
      }
    }
  };

  const handleModelChange = async (slot: ModelSlotDef, newModel: string) => {
    if (!provider) return;
    const updated = writeProviderModelSlot(provider, tool.appId, slot, newModel);
    if (!updated) {
      toast.error(`模型 ${slot.label} 切换失败`);
      return;
    }
    setSavingModel(true);
    try {
      // Codex: 根据模型类型设 apiFormat 让 proxy 决定是否转换协议
      // GPT-5.5 是唯一已知支持 /v1/responses 的模型，其他模型都需要本地
      // proxy 做 Responses→Chat 转换后再发往 token.matpool.com。
      if (tool.appId === "codex") {
        const isResponsesNative = newModel.toLowerCase().includes("gpt-5.5");
        (updated.settingsConfig as any).apiFormat = isResponsesNative ? "openai_responses" : "openai_chat";
      }
      await providersApi.update(updated, tool.appId);
      if (isMatpoolActive) {
        await proxyApi.setProxyTakeoverForApp(tool.appId, true);
        await queryClient.invalidateQueries({ queryKey: ["proxyTakeoverStatus"] });
      }
      await refetch();
      toast.success(`${tool.label} — ${slot.label} 已切换为 ${newModel}`);
    } catch (err) {
      toast.error(`切换模型失败：${extractErrorMessage(err) || "未知错误"}`);
    } finally {
      setSavingModel(false);
    }
  };

  /** 从 toml 里解析 model_reasoning_effort */
  const currentReasoning = useMemo(() => {
    if (tool.appId !== "codex") return "high";
    const sc = provider?.settingsConfig as any;
    const config: string = sc?.config ?? "";
    const match = config.match(/^\s*model_reasoning_effort\s*=\s*"([^"]+)"/m);
    return match?.[1] ?? "high";
  }, [provider, tool.appId]);

  const handleReasoningChange = async (value: string) => {
    if (!provider) return;
    const sc = JSON.parse(JSON.stringify(provider.settingsConfig ?? {}));
    const config: string = sc?.config ?? "";
    if (!config) return;
    const escaped = value.replace(/"/g, '\\"');
    const pattern = /^(\s*)model_reasoning_effort\s*=\s*"[^"]*"/m;
    if (!pattern.test(config)) {
      sc.config = `model_reasoning_effort = "${escaped}"\n${config}`;
    } else {
      sc.config = config.replace(pattern, `$1model_reasoning_effort = "${escaped}"`);
    }
    const updated = { ...provider, settingsConfig: sc };
    setSavingModel(true);
    try {
      await providersApi.update(updated, tool.appId);
      if (isMatpoolActive) {
        await proxyApi.setProxyTakeoverForApp(tool.appId, true);
        await queryClient.invalidateQueries({ queryKey: ["proxyTakeoverStatus"] });
      }
      await refetch();
    } catch (err) {
      toast.error(extractErrorMessage(err) || "切换推理强度失败");
    } finally {
      setSavingModel(false);
    }
  };

  const Logo = tool.Icon;

  return (
    <div
      className={`px-4 py-3.5 transition-colors ${
        isMatpoolActive ? "bg-blue-50/30 dark:bg-blue-950/10" : "hover:bg-muted/30"
      } ${!isInstalled ? "opacity-60" : ""}`}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-3 min-w-0 flex-1">
          <div
            className={`rounded-lg p-2 shrink-0 flex items-center justify-center w-9 h-9 ${
              isMatpoolActive ? "bg-emerald-500/10" : "bg-muted/60"
            }`}
          >
            <Logo size={20} />
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <p className="text-[14px] font-medium leading-tight">{tool.label}</p>
              {isMatpoolActive && isInstalled && (
                <span className="text-[10px] font-semibold uppercase tracking-wider text-emerald-600 dark:text-emerald-400 bg-emerald-500/10 px-1.5 py-0.5 rounded">
                  已接管
                </span>
              )}
              {!isInstalled && (
                <span className="text-[10px] uppercase tracking-wider text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
                  未安装
                </span>
              )}
              {!seedExists && isInstalled && (
                <span className="text-[10px] uppercase tracking-wider text-amber-600 dark:text-amber-400 bg-amber-500/10 px-1.5 py-0.5 rounded inline-flex items-center gap-0.5">
                  <AlertCircle className="h-2.5 w-2.5" />
                  待初始化
                </span>
              )}
            </div>
            <p className="text-[12px] text-muted-foreground mt-0.5 truncate">
              {tool.subLabel}
              {version && (
                <span className="ml-1.5 text-muted-foreground/70 font-mono">· {version}</span>
              )}
              {!isInstalled && (
                <span className="ml-1.5 text-muted-foreground/70 font-mono">
                  · {tool.installHint}
                </span>
              )}
            </p>
          </div>
        </div>
        <Switch
          checked={isMatpoolActive}
          disabled={
            switchMutation.isPending ||
            !seedExists ||
            !isInstalled ||
            (!tokenReady && !isMatpoolActive)
          }
          onCheckedChange={handleToggle}
        />
      </div>

      {/* Model selectors: 每个 slot 一行 */}
      {isInstalled && seedExists && (
        <div className="mt-3 ml-12 space-y-1.5">
          {slots.map((slot) => {
            const currentVal = readProviderModelSlot(provider, tool.appId, slot);
            return (
              <div key={slot.key} className="flex items-center gap-2">
                <span className="text-[11px] text-muted-foreground shrink-0 w-28 truncate" title={slot.label}>
                  {slot.label}
                </span>
                {tool.modelEditable ? (
                  <Select
                    value={currentVal}
                    onValueChange={(v) => void handleModelChange(slot, v)}
                    disabled={savingModel || models.length === 0}
                  >
                    <SelectTrigger className="h-7 text-[12px] flex-1 max-w-xs">
                      <SelectValue placeholder={models.length === 0 ? "加载中..." : "选模型"} />
                    </SelectTrigger>
                    <SelectContent className="max-h-72">
                      {models.map((m) => (
                        <SelectItem key={m.model_name} value={m.model_name} className="text-[12px]">
                          <span className="font-medium">{m.model_name}</span>
                          {m.tags && (
                            <span className="ml-2 text-[10px] text-muted-foreground">{m.tags}</span>
                          )}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <span className="text-[12px] text-muted-foreground/70 font-mono">
                    {currentVal || "—"}
                  </span>
                )}
              </div>
            );
          })}
          {/* Codex: 额外 reasoning_effort 下拉 */}
          {tool.appId === "codex" && (
            <div className="flex items-center gap-2">
              <span className="text-[11px] text-muted-foreground shrink-0 w-28">推理强度</span>
              <Select
                value={currentReasoning}
                onValueChange={handleReasoningChange}
                disabled={savingModel}
              >
                <SelectTrigger className="h-7 text-[12px] flex-1 max-w-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {REASONING_EFFORTS.map((r) => (
                    <SelectItem key={r} value={r} className="text-[12px]">{r}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
          {savingModel && <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />}
        </div>
      )}
    </div>
  );
}
