//! Matpool 切换前的 token 注入逻辑
//!
//! Matpool seed 的 settings_config 里 token 字段（`ANTHROPIC_AUTH_TOKEN` /
//! `OPENAI_API_KEY` / `GEMINI_API_KEY`）始终为空字符串 —— Token 真值在 OS keychain
//! 里（参见 `commands::matpool_keychain`）。
//!
//! 在 `switch_normal` 真正写 live 配置之前，对 Matpool seed 用本模块的
//! `inject_matpool_token_into` 注入 keychain 里的 token，得到一份"完整的"
//! provider clone，再把它喂给 `write_live_with_common_config`。
//!
//! 这一份策略只 mutate 内存里的 clone：不写回数据库（保留 seed 在 DB 里 token 始终
//! 为空的设计 / 利于跨设备同步与审计）。

use crate::commands::read_token_from_keychain;
use crate::database::is_matpool_seed_id;
use crate::provider::Provider;
use serde_json::Value;

/// Token 字段在每种 app type 下的位置。
///
/// path 是 settings_config 里要写入的 JSON pointer，按层级访问。
/// 例如 Claude/Gemini 写到 `env.<KEY>`，Codex 写到 `auth.OPENAI_API_KEY`。
#[derive(Debug, Clone, Copy)]
struct TokenSlot {
    /// 第一层 key（"env" / "auth"）
    section: &'static str,
    /// 第二层 key（"ANTHROPIC_AUTH_TOKEN" / "OPENAI_API_KEY" / "GEMINI_API_KEY"）
    field: &'static str,
}

/// 给定 Matpool seed id，返回 token 在 settings_config 里的位置。
///
/// 与 `database::dao::providers_seed::OFFICIAL_SEEDS` 里的 4 条 Matpool 条目结构对齐：
/// - `matpool-claude` / `matpool-claude-desktop` → `env.ANTHROPIC_AUTH_TOKEN`
/// - `matpool-codex` → `auth.OPENAI_API_KEY`
/// - `matpool-gemini` → `env.GEMINI_API_KEY`
fn token_slot_for(seed_id: &str) -> Option<TokenSlot> {
    match seed_id {
        "matpool-claude" | "matpool-claude-desktop" => Some(TokenSlot {
            section: "env",
            field: "ANTHROPIC_AUTH_TOKEN",
        }),
        "matpool-codex" => Some(TokenSlot {
            section: "auth",
            field: "OPENAI_API_KEY",
        }),
        "matpool-gemini" => Some(TokenSlot {
            section: "env",
            field: "GEMINI_API_KEY",
        }),
        _ => None,
    }
}

/// 把 token 写入 settings_config 的指定槽位。
///
/// settings_config 必须已经是一个 object，section 必须存在且也是 object（这是 seed
/// 的硬约束，参见 `OFFICIAL_SEEDS` 的 settings_config_json 字面量）。
///
/// 若 settings_config 形态异常，记一行 warning 后跳过（保持 best-effort 语义）。
fn write_token_to_slot(settings: &mut Value, slot: TokenSlot, token: &str) {
    let Some(root) = settings.as_object_mut() else {
        log::warn!(
            "[matpool] settings_config not an object, skip token injection (slot={}.{})",
            slot.section,
            slot.field,
        );
        return;
    };

    let Some(section) = root.get_mut(slot.section).and_then(|v| v.as_object_mut()) else {
        log::warn!(
            "[matpool] section '{}' missing or not an object in settings_config",
            slot.section,
        );
        return;
    };

    section.insert(slot.field.to_string(), Value::String(token.to_string()));
}

/// 如果 provider 是 Matpool seed，从 keychain 拉 token 注入它的 settings_config 副本。
///
/// 返回：
/// - 非 Matpool seed → `None`（调用方继续用原 provider）
/// - Matpool seed 但 keychain 里没 token → `None`，并 log 一条 info（让 switch 流程不变）
/// - Matpool seed + 有 token → `Some(provider_clone_with_token)`
///
/// 这是一个 best-effort 的 helper：任何异常都不应阻塞 switch 主流程。
pub fn provider_with_injected_matpool_token(provider: &Provider) -> Option<Provider> {
    if !is_matpool_seed_id(&provider.id) {
        return None;
    }

    let Some(slot) = token_slot_for(&provider.id) else {
        log::warn!(
            "[matpool] no token slot for matpool seed id '{}'",
            provider.id
        );
        return None;
    };

    let Some(token) = read_token_from_keychain() else {
        log::info!(
            "[matpool] no token in keychain; switching to '{}' without filling {}.{} (user can run Token Wizard later)",
            provider.id,
            slot.section,
            slot.field,
        );
        return None;
    };

    let mut clone = provider.clone();
    write_token_to_slot(&mut clone.settings_config, slot, &token);
    Some(clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dummy_provider(id: &str, settings: Value) -> Provider {
        Provider::with_id(id.to_string(), id.to_string(), settings, None)
    }

    #[test]
    fn token_slot_covers_all_matpool_seeds() {
        assert!(token_slot_for("matpool-claude").is_some());
        assert!(token_slot_for("matpool-claude-desktop").is_some());
        assert!(token_slot_for("matpool-codex").is_some());
        assert!(token_slot_for("matpool-gemini").is_some());
        assert!(token_slot_for("matpool-unknown").is_none());
        assert!(token_slot_for("claude-official").is_none());
    }

    #[test]
    fn write_token_to_slot_overwrites_empty() {
        let mut s = json!({"env": {"ANTHROPIC_AUTH_TOKEN": ""}});
        write_token_to_slot(
            &mut s,
            TokenSlot {
                section: "env",
                field: "ANTHROPIC_AUTH_TOKEN",
            },
            "sk-test",
        );
        assert_eq!(s["env"]["ANTHROPIC_AUTH_TOKEN"], json!("sk-test"));
    }

    #[test]
    fn write_token_to_slot_creates_field_if_missing() {
        let mut s = json!({"env": {}});
        write_token_to_slot(
            &mut s,
            TokenSlot {
                section: "env",
                field: "ANTHROPIC_AUTH_TOKEN",
            },
            "sk-test",
        );
        assert_eq!(s["env"]["ANTHROPIC_AUTH_TOKEN"], json!("sk-test"));
    }

    #[test]
    fn write_token_to_slot_silently_skips_when_section_missing() {
        let mut s = json!({"other": {}});
        write_token_to_slot(
            &mut s,
            TokenSlot {
                section: "env",
                field: "ANTHROPIC_AUTH_TOKEN",
            },
            "sk-test",
        );
        // env 不存在，整体不变
        assert_eq!(s, json!({"other": {}}));
    }

    #[test]
    fn provider_with_injected_returns_none_for_non_matpool() {
        let p = dummy_provider("claude-official", json!({"env": {}}));
        assert!(provider_with_injected_matpool_token(&p).is_none());
    }
}
