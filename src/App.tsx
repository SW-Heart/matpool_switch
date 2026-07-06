/**
 * Matpool Switch 主应用
 *
 * 极简版：用户路径 = 输 Token → 一键开/关 5 个工具的接管。
 *
 * 这一版主动**抛弃**了上游 matpool-switch 的多供应商 / MCP / Skills / Sessions /
 * Proxy / OpenClaw / Hermes / Workspace / Universal / DeepLink / Agents 等
 * 复杂功能视图，转而用 MatpoolHome 一个页面承载主流程；只保留 Settings 作为
 * 辅助页，和 Token Wizard 作为首启向导。
 *
 * 后续 services/commands 层的清理会在专门的 commit 里做。
 */

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { ArrowLeft } from "lucide-react";
import { useSettingsQuery } from "@/lib/query";
import { isWindows, isLinux, DRAG_REGION_ATTR, DRAG_REGION_STYLE } from "@/lib/platform";
import { Button } from "@/components/ui/button";
import { MatpoolHome } from "@/components/MatpoolHome";
import { SettingsPage } from "@/components/settings/SettingsPage";
import { MatpoolTokenWizardDialog } from "@/components/MatpoolTokenWizardDialog";
import matpoolLogo from "@/assets/icons/matpool-logo.svg";

const DEFAULT_DRAG_BAR_HEIGHT = isWindows() || isLinux() ? 0 : 28; // px
const HEADER_HEIGHT = 56; // px

type View = "home" | "settings";

const VIEW_STORAGE_KEY = "matpool-last-view";
const VALID_VIEWS: View[] = ["home", "settings"];

const getInitialView = (): View => {
  try {
    const saved = localStorage.getItem(VIEW_STORAGE_KEY) as View | null;
    if (saved && VALID_VIEWS.includes(saved)) {
      return saved;
    }
  } catch {
    /* localStorage 不可用时回退默认 */
  }
  return "home";
};

function App() {
  const { data: settingsData } = useSettingsQuery();
  const useAppWindowControls =
    isLinux() && (settingsData?.useAppWindowControls ?? false);
  const dragBarHeight = useAppWindowControls ? 32 : DEFAULT_DRAG_BAR_HEIGHT;

  const [currentView, setCurrentView] = useState<View>(getInitialView);
  // 手动打开 Token Wizard 时使用；首次启动的自动弹出由 MatpoolTokenWizardDialog 内部
  // 根据 settings.firstRunNoticeConfirmed 控制，与这里的 manualOpen 逻辑解耦。
  const [manualWizardOpen, setManualWizardOpen] = useState(false);
  // 每次 wizard 成功保存 Token 时 +1，传给 MatpoolHome 触发其内部 useEffect 重读 keychain。
  // 否则 wizard 关闭后 home 仍显示"未配置"，要手动点"重新配置"再保存才刷新（已上报 bug）。
  const [tokenVersion, setTokenVersion] = useState(0);

  const goHome = () => {
    setCurrentView("home");
    try {
      localStorage.setItem(VIEW_STORAGE_KEY, "home");
    } catch {
      /* ignore */
    }
  };

  const openSettings = () => {
    setCurrentView("settings");
    try {
      localStorage.setItem(VIEW_STORAGE_KEY, "settings");
    } catch {
      /* ignore */
    }
  };

  return (
    <div className="h-screen w-screen flex flex-col bg-background text-foreground antialiased overflow-hidden">
      {/* macOS 顶部拖拽条（Linux/Windows 上 dragBarHeight = 0）*/}
      {dragBarHeight > 0 && (
        <div
          {...DRAG_REGION_ATTR}
          style={{
            ...DRAG_REGION_STYLE,
            height: dragBarHeight,
          } as any}
          className="shrink-0"
        />
      )}

      {/* 顶栏 */}
      <header
        className="w-full bg-background/80 backdrop-blur-md border-b border-border/40 shrink-0"
        {...DRAG_REGION_ATTR}
        style={{
          ...DRAG_REGION_STYLE,
          height: HEADER_HEIGHT,
        } as any}
      >
        <div
          className="flex h-full items-center justify-between px-4"
          style={{ WebkitAppRegion: "no-drag" } as any}
        >
          <div className="flex items-center gap-2">
            {currentView !== "home" && (
              <Button
                variant="ghost"
                size="icon"
                onClick={goHome}
                title="返回"
                className="h-8 w-8"
              >
                <ArrowLeft className="h-4 w-4" />
              </Button>
            )}
            <img
              src={matpoolLogo}
              alt="Matpool"
              className="h-7 w-7 select-none"
              draggable={false}
            />
            <a
              href="https://matpool.com"
              target="_blank"
              rel="noreferrer"
              className="text-[15px] font-semibold tracking-tight text-foreground hover:text-[#0068FF] transition-colors"
            >
              Matpool Switch
            </a>
            {currentView === "settings" && (
              <span className="ml-2 text-sm text-muted-foreground">/ 设置</span>
            )}
          </div>
        </div>
      </header>

      {/* 主体内容 */}
      <main
        className="flex-1 overflow-y-auto px-4 pb-12"
      >
        <AnimatePresence mode="wait">
          {currentView === "home" && (
            <motion.div
              key="home"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.15 }}
            >
              <MatpoolHome
                tokenVersion={tokenVersion}
                onOpenSettings={openSettings}
                onOpenTokenWizard={() => setManualWizardOpen(true)}
              />
            </motion.div>
          )}
          {currentView === "settings" && (
            <motion.div
              key="settings"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.15 }}
              className="mx-auto max-w-3xl"
            >
              <SettingsPage
                open={true}
                onOpenChange={(open) => {
                  if (!open) goHome();
                }}
                defaultTab="general"
              />
            </motion.div>
          )}
        </AnimatePresence>
      </main>

      <MatpoolTokenWizardDialog
        manualOpen={manualWizardOpen}
        onManualClose={() => setManualWizardOpen(false)}
        onSaved={() => setTokenVersion((v) => v + 1)}
      />
    </div>
  );
}

export default App;
