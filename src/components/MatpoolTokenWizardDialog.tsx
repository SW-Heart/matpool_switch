import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { KeyRound, Loader2, Check, AlertTriangle } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useSettingsQuery } from "@/lib/query";
import { settingsApi } from "@/lib/api";
import { matpoolStore } from "@/lib/matpoolStore";
import { validateMatpoolToken } from "@/lib/matpoolApi";

/**
 * Matpool Token 首次配置向导
 *
 * 出现条件（任一）：
 * - 首次启动：`settings.firstRunNoticeConfirmed !== true`
 * - 用户主动点"配置 Token"按钮：`manualOpen` 为 true
 *
 * 行为：
 * - 用户粘贴长 Key（sk-...）
 * - 调 token.matpool.com/v1/models 校验
 * - 校验通过：写入 OS keychain + 写 firstRunNoticeConfirmed=true 关闭
 * - 用户可以点"稍后配置"跳过，但仍写入 firstRunNoticeConfirmed=true 避免每次启动都弹
 */
export interface MatpoolTokenWizardDialogProps {
  /** 主动打开模式（手动入口）—— 用户从主界面点 "配置 Token" 时为 true */
  manualOpen?: boolean;
  /** 主动模式关闭回调 */
  onManualClose?: () => void;
  /** Token 保存成功后立刻触发（无需等动画结束），让外层刷新 token 状态 */
  onSaved?: () => void;
}

export function MatpoolTokenWizardDialog({
  manualOpen = false,
  onManualClose,
  onSaved,
}: MatpoolTokenWizardDialogProps = {}) {
  const queryClient = useQueryClient();
  const { data: settings } = useSettingsQuery();
  const [token, setToken] = useState("");
  const [validating, setValidating] = useState(false);
  const [feedback, setFeedback] = useState<
    | { type: "ok"; message: string }
    | { type: "error"; message: string }
    | null
  >(null);

  const firstRunOpen =
    settings != null && settings.firstRunNoticeConfirmed !== true;
  const isOpen = firstRunOpen || manualOpen;

  const persistFirstRunConfirmed = async () => {
    if (!settings) return;
    try {
      const { webdavSync: _, ...rest } = settings;
      await settingsApi.save({ ...rest, firstRunNoticeConfirmed: true });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
    } catch (err) {
      console.error("Failed to persist firstRunNoticeConfirmed:", err);
    }
  };

  const handleSave = async () => {
    const trimmed = token.trim();
    if (!trimmed) {
      setFeedback({ type: "error", message: "请粘贴 Matpool Token (sk-...)" });
      return;
    }

    setValidating(true);
    setFeedback(null);
    const result = await validateMatpoolToken(trimmed);
    setValidating(false);

    if (!result.ok) {
      setFeedback({
        type: "error",
        message: `Token 校验失败：${result.error ?? "未知错误"}`,
      });
      return;
    }

    matpoolStore.setToken(trimmed).then(
      () => {
        // Token 一旦写入 keychain 就立刻通知外层刷新，不等动画
        // （之前依赖外层 setTimeout 800ms + 内部 600ms 关闭，竞态导致首次保存
        // 后 home 看到 missing，要再保存一次才显示；这是 bug-1 的根因）
        onSaved?.();
        setFeedback({
          type: "ok",
          message:
            result.modelCount != null
              ? `校验通过，可用模型 ${result.modelCount} 个`
              : "校验通过",
        });
        // 短暂展示成功反馈再关闭
        setTimeout(() => {
          void closeWizard();
        }, 600);
      },
      (err) => {
        setFeedback({
          type: "error",
          message: `保存到 keychain 失败：${
            err instanceof Error ? err.message : String(err)
          }`,
        });
      },
    );
  };

  const closeWizard = async () => {
    if (firstRunOpen) {
      // 首启模式：永远写入 firstRunNoticeConfirmed=true 以避免每次启动都弹
      await persistFirstRunConfirmed();
    }
    setToken("");
    setFeedback(null);
    if (manualOpen) {
      onManualClose?.();
    }
  };

  const handleSkip = async () => {
    await closeWizard();
  };

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) void closeWizard();
      }}
    >
      <DialogContent className="max-w-md" zIndex="top">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <KeyRound className="h-5 w-5 text-blue-500" />
            欢迎使用 Matpool Switch
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4 px-6 py-4">
          <DialogDescription className="leading-relaxed">
            粘贴你的 Matpool Token，即可一键把 Claude Code、Codex、Gemini CLI 等工具的流量
            接入 Matpool 平台。
          </DialogDescription>

          <div className="space-y-2">
            <Label htmlFor="matpool-token">Matpool Token</Label>
            <Input
              id="matpool-token"
              type="password"
              placeholder="sk-..."
              autoComplete="off"
              spellCheck={false}
              value={token}
              onChange={(e) => {
                setToken(e.target.value);
                setFeedback(null);
              }}
              disabled={validating}
            />
            <p className="text-xs text-muted-foreground">
              没有 Token？前往{" "}
              <a
                href="https://matpool.com/models"
                target="_blank"
                rel="noreferrer"
                className="underline"
              >
                matpool.com/models
              </a>{" "}
              开通模型服务获取。
            </p>
          </div>

          {feedback && (
            <div
              className={`flex items-start gap-2 rounded-md border p-3 text-sm ${
                feedback.type === "ok"
                  ? "border-green-300 bg-green-50 text-green-800 dark:border-green-800 dark:bg-green-950 dark:text-green-200"
                  : "border-red-300 bg-red-50 text-red-800 dark:border-red-800 dark:bg-red-950 dark:text-red-200"
              }`}
            >
              {feedback.type === "ok" ? (
                <Check className="mt-0.5 h-4 w-4 shrink-0" />
              ) : (
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
              )}
              <span>{feedback.message}</span>
            </div>
          )}
        </div>
        <DialogFooter className="gap-2 sm:gap-2">
          <Button variant="ghost" onClick={handleSkip} disabled={validating}>
            稍后配置
          </Button>
          <Button onClick={handleSave} disabled={validating}>
            {validating && (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            )}
            保存并校验
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
