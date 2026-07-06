//! 官方供应商种子数据
//!
//! 启动时调用 `Database::init_default_official_providers` 把这些条目
//! 写入 `providers` 表，让所有用户都能看到一个"一键切回官方"的入口。
//!
//! 字段与前端预设保持一致，参见：
//! - `src/config/claudeProviderPresets.ts`（"Claude Official"）
//! - `src/config/codexProviderPresets.ts`（"OpenAI Official"）
//! - `src/config/geminiProviderPresets.ts`（"Google Official"）
//!
//! 此外 Matpool Switch 把 3 个 Matpool 入口（Claude Code / Codex / Gemini）也作为 seed
//! 提前塞进 providers 表，让首次启动后用户看到 Matpool 已就位，只需输入 Token。
//! Matpool seed 的 settings_config 直接编码 `https://token.matpool.com` 网关。
//!
//! 每条 seed 还可选地携带 `api_format`，写入 provider.meta.api_format。这是
//! `proxy::providers::claude::get_claude_api_format` 读取的 SSOT，决定本地代理
//! 是否需要做协议转换（Anthropic ↔ OpenAI Chat / Responses / Gemini Native）。

use crate::app_config::AppType;

pub(crate) const CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID: &str = "claude-desktop-official";

/// Matpool 单租户入口的 provider id 前缀。
/// 每个 app type 一条 seed：matpool-claude / matpool-codex / matpool-gemini。
pub(crate) const MATPOOL_PROVIDER_ID_PREFIX: &str = "matpool-";

/// 单条官方供应商种子定义。
pub(crate) struct OfficialProviderSeed {
    pub id: &'static str,
    pub app_type: AppType,
    pub name: &'static str,
    pub website_url: &'static str,
    pub icon: &'static str,
    pub icon_color: &'static str,
    /// settings_config 的 JSON 字符串，每个 app 结构不同。
    pub settings_config_json: &'static str,
    /// API 协议归属，写入 provider.meta.api_format。
    /// 取值：`"anthropic"` / `"openai_chat"` / `"openai_responses"` / `"gemini_native"`。
    /// `None` = 不写入（让旧 matpool-switch 兜底逻辑接管）。
    pub api_format: Option<&'static str>,
}

/// Claude Code / Codex / Gemini 的官方预设 + Matpool 入口。
///
/// id 固定，便于幂等检查；name 直接用英文原名（与前端预设一致），不做 i18n。
pub(crate) const OFFICIAL_SEEDS: &[OfficialProviderSeed] = &[
    // ===== Matpool seeds（自动 seed 进去，用户首次启动看到的就是它们）=====
    //
    // settings_config 的字段必须与前端 `src/config/matpoolConstants.ts` /
    // `*ProviderPresets.ts` 里的 Matpool preset 保持一致：
    // - Anthropic / Gemini 协议 → 根路径 https://token.matpool.com
    // - OpenAI 协议 → 带 /v1 的 https://token.matpool.com/v1
    //
    // ANTHROPIC_AUTH_TOKEN / OPENAI_API_KEY / GEMINI_API_KEY 留空字符串：
    // - Token Wizard 把长 Key 写进 OS keychain，与 settings_config 解耦
    // - 切换到 Matpool 时由 services::matpool_inject 把 token 注入 live 配置
    //
    // api_format 决定 proxy 是否需要协议转换：
    // - Claude Code 默认走 Anthropic Messages 协议，passthrough
    // - Codex 默认走 OpenAI Responses 协议；如果用户选 Chat-only 模型（如 GLM-5.2），
    //   proxy 会按 settings_config 里的 model 名查上游能力，将 Responses 转成 Chat
    // - Gemini CLI 走 Gemini Native，passthrough
    OfficialProviderSeed {
        id: "matpool-claude",
        app_type: AppType::Claude,
        name: "Matpool",
        website_url: "https://matpool.com",
        icon: "generic",
        icon_color: "#1F6FEB",
        // 4 档模型：主模型 + /model opus|sonnet|haiku 三档
        settings_config_json: r#"{"env":{"ANTHROPIC_BASE_URL":"https://token.matpool.com","ANTHROPIC_AUTH_TOKEN":"","ANTHROPIC_MODEL":"claude-sonnet-4-6","ANTHROPIC_DEFAULT_OPUS_MODEL":"claude-opus-4-6","ANTHROPIC_DEFAULT_SONNET_MODEL":"claude-sonnet-4-6","ANTHROPIC_DEFAULT_HAIKU_MODEL":"claude-haiku-4-5"}}"#,
        api_format: Some("anthropic"),
    },
    OfficialProviderSeed {
        id: "matpool-codex",
        app_type: AppType::Codex,
        name: "Matpool",
        website_url: "https://matpool.com",
        icon: "generic",
        icon_color: "#1F6FEB",
        // auth.json: 留空 OPENAI_API_KEY；config.toml: 写 model_provider.custom 指向 Matpool 网关
        settings_config_json: r#"{"auth":{"OPENAI_API_KEY":""},"config":"model_provider = \"custom\"\nmodel = \"GPT-5.5\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n\n[model_providers.custom]\nname = \"matpool\"\nbase_url = \"https://token.matpool.com/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true","modelCatalog":{"models":[{"model":"GPT-5.5","displayName":"GPT-5.5","contextWindow":128000}]}}"#,
        // Codex 的 api_format 在 matpool-switch 里只对 Claude 类型有效；Codex 自己用
        // settings_config 的 wire_api 字段。这里不设，由后端 wire_api 决定。
        api_format: None,
    },
    OfficialProviderSeed {
        id: "matpool-gemini",
        app_type: AppType::Gemini,
        name: "Matpool",
        website_url: "https://matpool.com",
        icon: "generic",
        icon_color: "#1F6FEB",
        settings_config_json: r#"{"env":{"GOOGLE_GEMINI_BASE_URL":"https://token.matpool.com","GEMINI_API_KEY":"","GEMINI_MODEL":"gemini-2.5-pro"},"config":{}}"#,
        api_format: Some("gemini_native"),
    },
    // ===== matpool-switch 自带的官方 seed（保留，提供"切回官方"的逃生口）=====
    OfficialProviderSeed {
        id: "claude-official",
        app_type: AppType::Claude,
        name: "Claude Official",
        website_url: "https://www.anthropic.com/claude-code",
        icon: "anthropic",
        icon_color: "#D4915D",
        // 空 env 让用户走 Claude CLI 默认认证流程
        settings_config_json: r#"{"env":{}}"#,
        api_format: Some("anthropic"),
    },
    OfficialProviderSeed {
        id: "codex-official",
        app_type: AppType::Codex,
        name: "OpenAI Official",
        website_url: "https://chatgpt.com/codex",
        icon: "openai",
        icon_color: "#00A67E",
        // 空 auth + 空 config 让用户走 ChatGPT Plus/Pro OAuth
        settings_config_json: r#"{"auth":{},"config":""}"#,
        api_format: None,
    },
    OfficialProviderSeed {
        id: "gemini-official",
        app_type: AppType::Gemini,
        name: "Google Official",
        website_url: "https://ai.google.dev/",
        icon: "gemini",
        icon_color: "#4285F4",
        // 空 env + 空 config 让用户走 Google OAuth
        settings_config_json: r#"{"env":{},"config":{}}"#,
        api_format: Some("anthropic"),
    },
];

/// 判断给定的 provider id 是否属于内置官方种子。
///
/// 单一事实源：直接扫描 `OFFICIAL_SEEDS`，避免在多处重复维护 id 列表。
pub(crate) fn is_official_seed_id(id: &str) -> bool {
    OFFICIAL_SEEDS.iter().any(|seed| seed.id == id)
}

/// 判断给定的 provider id 是否属于 Matpool seed（matpool-claude / matpool-codex / ...）。
///
/// 用于"启动时把 Matpool seed 设为 current"等需要单独识别 Matpool 的场景。
#[allow(dead_code)]
pub(crate) fn is_matpool_seed_id(id: &str) -> bool {
    id.starts_with(MATPOOL_PROVIDER_ID_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matpool_seeds_present_for_each_supported_app() {
        // Matpool Switch 当前不支持 Claude Desktop（OPE-2951 范围里没有），
        // 所以只校验 Claude Code / Codex / Gemini CLI 三条。
        let mut covered = std::collections::HashSet::new();
        for seed in OFFICIAL_SEEDS {
            if is_matpool_seed_id(seed.id) {
                covered.insert(seed.app_type.clone());
            }
        }
        assert!(covered.contains(&AppType::Claude), "claude matpool seed");
        assert!(covered.contains(&AppType::Codex), "codex matpool seed");
        assert!(covered.contains(&AppType::Gemini), "gemini matpool seed");
    }

    #[test]
    fn matpool_seed_settings_config_parses() {
        for seed in OFFICIAL_SEEDS {
            if !is_matpool_seed_id(seed.id) {
                continue;
            }
            let parsed: serde_json::Value = serde_json::from_str(seed.settings_config_json)
                .unwrap_or_else(|e| panic!("matpool seed {} json invalid: {e}", seed.id));
            assert!(parsed.is_object(), "matpool seed {} not an object", seed.id);
        }
    }

    #[test]
    fn matpool_claude_seed_uses_anthropic_format() {
        let seed = OFFICIAL_SEEDS
            .iter()
            .find(|s| s.id == "matpool-claude")
            .expect("matpool-claude seed");
        assert_eq!(seed.api_format, Some("anthropic"));
    }

    #[test]
    fn matpool_gemini_seed_uses_gemini_native_format() {
        let seed = OFFICIAL_SEEDS
            .iter()
            .find(|s| s.id == "matpool-gemini")
            .expect("matpool-gemini seed");
        assert_eq!(seed.api_format, Some("gemini_native"));
    }

    #[test]
    fn matpool_codex_seed_uses_case_sensitive_gateway_model_and_catalog() {
        let seed = OFFICIAL_SEEDS
            .iter()
            .find(|s| s.id == "matpool-codex")
            .expect("matpool-codex seed");
        let parsed: serde_json::Value =
            serde_json::from_str(seed.settings_config_json).expect("seed json");
        assert!(parsed["config"]
            .as_str()
            .expect("config")
            .contains("model = \"GPT-5.5\""));
        assert_eq!(
            parsed["modelCatalog"]["models"][0]["model"],
            serde_json::json!("GPT-5.5")
        );
    }

    #[test]
    fn claude_desktop_official_id_constant_unused_but_preserved() {
        // 我们删掉了 ClaudeDesktop seed 但保留 CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID 常量
        // （仍被 codex_history_migration.rs / claude_desktop_config.rs 等地方引用）。
        // 这条测试在常量被删时立刻失败，提醒同步检查残留引用。
        assert_eq!(
            CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID,
            "claude-desktop-official"
        );
    }
}
