import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Save, Loader2 } from "lucide-react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { LanguageSettings } from "@/components/settings/LanguageSettings";
import { ThemeSettings } from "@/components/settings/ThemeSettings";
import { WindowSettings } from "@/components/settings/WindowSettings";
import { AboutSection } from "@/components/settings/AboutSection";
import { useSettings } from "@/hooks/useSettings";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultTab?: string;
}

/**
 * Matpool Switch 极简设置页
 *
 * 仅保留：通用（语言 / 窗口）/ 外观（主题）/ 关于 三个 tab。
 * matpool-switch 的 sync / proxy / usage / mcp / skills / 终端 / 目录 / 备份 /
 * 导入导出 / 鉴权中心等设置项已经下线（不属于 Matpool 单租户产品定位）。
 *
 * UI 组成：上下结构 = Header (px-6 py-5) / Tabs body (px-6) / Footer (px-6 py-4)，
 * 不依赖 DialogContent 自身 padding（项目里 DialogContent 默认无 padding）。
 */
export function SettingsPage({
  open,
  onOpenChange,
  defaultTab = "general",
}: SettingsDialogProps) {
  const { t } = useTranslation();
  const { settings, isLoading, isSaving, isPortable, updateSettings, saveSettings } =
    useSettings();
  const [dirty, setDirty] = useState(false);

  const handleLanguageChange = (lang: string) => {
    updateSettings({ language: lang as any });
    setDirty(true);
  };

  const handleWindowSettingsChange = (
    updates: Parameters<typeof updateSettings>[0],
  ) => {
    updateSettings(updates);
    setDirty(true);
  };

  const handleSaveAll = async () => {
    try {
      await saveSettings();
      setDirty(false);
      toast.success(t("settings.saved", { defaultValue: "设置已保存" }));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[85vh] overflow-hidden flex flex-col p-0 gap-0">
        <DialogHeader className="px-6 pt-5 pb-4 border-b border-border/40 space-y-0">
          <DialogTitle className="text-base font-semibold">
            {t("settings.title", { defaultValue: "设置" })}
          </DialogTitle>
        </DialogHeader>

        {isLoading || !settings ? (
          <div className="flex-1 flex items-center justify-center py-16">
            <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
          </div>
        ) : (
          <Tabs
            defaultValue={defaultTab}
            className="flex-1 flex flex-col overflow-hidden"
          >
            <div className="px-6 pt-4 pb-3 border-b border-border/40">
              <TabsList className="bg-muted/50">
                <TabsTrigger value="general" className="text-[13px]">
                  {t("settings.general", { defaultValue: "通用" })}
                </TabsTrigger>
                <TabsTrigger value="appearance" className="text-[13px]">
                  {t("settings.appearance", { defaultValue: "外观" })}
                </TabsTrigger>
                <TabsTrigger value="about" className="text-[13px]">
                  {t("settings.about", { defaultValue: "关于" })}
                </TabsTrigger>
              </TabsList>
            </div>

            <div className="flex-1 overflow-y-auto px-6 py-5">
              <TabsContent value="general" className="space-y-6 mt-0">
                <LanguageSettings
                  value={settings.language}
                  onChange={handleLanguageChange}
                />
                <WindowSettings
                  settings={settings}
                  onChange={handleWindowSettingsChange}
                />
              </TabsContent>

              <TabsContent value="appearance" className="space-y-6 mt-0">
                <ThemeSettings />
              </TabsContent>

              <TabsContent value="about" className="space-y-6 mt-0">
                <AboutSection isPortable={isPortable} />
              </TabsContent>
            </div>
          </Tabs>
        )}

        <DialogFooter className="px-6 py-4 border-t border-border/40 gap-2 sm:gap-2">
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isSaving}
          >
            {t("common.close", { defaultValue: "关闭" })}
          </Button>
          <Button onClick={handleSaveAll} disabled={!dirty || isSaving}>
            {isSaving ? (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <Save className="mr-2 h-4 w-4" />
            )}
            {t("settings.save", { defaultValue: "保存" })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
