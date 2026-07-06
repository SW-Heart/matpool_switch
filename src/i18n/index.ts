import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import en from "./locales/en.json";
import ja from "./locales/ja.json";
import zh from "./locales/zh.json";
import zhTW from "./locales/zh-TW.json";

type Language = "zh" | "zh-TW" | "en" | "ja";

/**
 * Matpool Switch 默认中文。
 *
 * 与 matpool-switch 上游不同：上游会根据 navigator.language 自动挑语言，
 * 我们这里固定中文兜底，因为产品定位是中文用户优先。
 * 用户在设置里手动切换的语言会写进 localStorage，那一项始终优先于默认。
 */
const DEFAULT_LANGUAGE: Language = "zh";

const getInitialLanguage = (): Language => {
  if (typeof window !== "undefined") {
    try {
      const stored = window.localStorage.getItem("language");
      if (
        stored === "zh" ||
        stored === "zh-TW" ||
        stored === "en" ||
        stored === "ja"
      ) {
        return stored;
      }
    } catch (error) {
      console.warn("[i18n] Failed to read stored language preference", error);
    }
  }

  // 没有用户偏好时，固定使用简体中文（不再看 navigator.language）
  return DEFAULT_LANGUAGE;
};

const resources = {
  en: {
    translation: en,
  },
  ja: {
    translation: ja,
  },
  zh: {
    translation: zh,
  },
  "zh-TW": {
    translation: zhTW,
  },
};

i18n.use(initReactI18next).init({
  resources,
  lng: getInitialLanguage(), // 默认中文，localStorage 里的用户偏好优先
  fallbackLng: "zh", // 缺翻译时退回中文（产品默认语言）

  interpolation: {
    escapeValue: false, // React 已经默认转义
  },

  // 开发模式下显示调试信息
  debug: false,
});

export default i18n;
