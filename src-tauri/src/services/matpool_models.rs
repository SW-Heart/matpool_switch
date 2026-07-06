//! Matpool model catalog synchronization.
//!
//! The desktop Matpool home page uses `https://token.matpool.com/api/pricing`
//! as the product-level model catalog, then projects chat-capable models into
//! Matpool seed providers. Keep the CLI on the same source of truth.

use crate::app_config::AppType;
use crate::database::Database;
use crate::provider::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const MATPOOL_PRICING_URL: &str = "https://token.matpool.com/api/pricing";
const CODEX_DEFAULT_CONTEXT_WINDOW: u64 = 128_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatpoolPricingModel {
    pub model_name: String,
    pub description: Option<String>,
    pub tags: Option<String>,
    pub vendor_id: Option<u64>,
    pub supported_endpoint_types: Option<Vec<String>>,
    pub enable_groups: Option<Vec<String>>,
    pub model_ratio: Option<f64>,
    pub completion_ratio: Option<f64>,
    pub model_price: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct MatpoolModelSyncOutcome {
    pub fetched: usize,
    pub chat_capable: usize,
    pub updated: Vec<String>,
    pub defaults: Vec<(String, String)>,
}

#[derive(Debug, Deserialize)]
struct PricingResponse {
    data: Option<Vec<MatpoolPricingModel>>,
}

pub async fn fetch_matpool_pricing_models() -> Result<Vec<MatpoolPricingModel>, String> {
    let response = crate::proxy::http_client::get()
        .get(MATPOOL_PRICING_URL)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("request Matpool pricing failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "request Matpool pricing failed: HTTP {status}: {}",
            truncate(&body, 512)
        ));
    }

    let parsed: PricingResponse = response
        .json()
        .await
        .map_err(|e| format!("parse Matpool pricing failed: {e}"))?;
    Ok(parsed.data.unwrap_or_default())
}

pub fn chat_capable_models(models: &[MatpoolPricingModel]) -> Vec<MatpoolPricingModel> {
    models
        .iter()
        .filter(|model| {
            let Some(types) = &model.supported_endpoint_types else {
                return false;
            };
            types.iter().any(|ty| ty == "TEXT" || ty == "CODE")
        })
        .cloned()
        .collect()
}

pub async fn sync_matpool_models_for_apps(
    db: &Database,
    apps: &[AppType],
) -> Result<MatpoolModelSyncOutcome, String> {
    let all_models = fetch_matpool_pricing_models().await?;
    let chat_models = chat_capable_models(&all_models);
    if chat_models.is_empty() {
        return Err("Matpool pricing returned no TEXT/CODE models".to_string());
    }

    let model_names: Vec<String> = chat_models
        .iter()
        .map(|model| model.model_name.trim().to_string())
        .filter(|model| !model.is_empty())
        .collect();
    if model_names.is_empty() {
        return Err("Matpool pricing returned empty model names".to_string());
    }

    let mut updated = Vec::new();
    let mut defaults = Vec::new();

    for app in apps {
        let app_key = app.as_str();
        let provider_id = matpool_provider_id(app);
        let Some(mut provider) = db
            .get_provider_by_id(provider_id, app_key)
            .map_err(|e| e.to_string())?
        else {
            continue;
        };

        let changed = match app {
            AppType::Claude => sync_claude_provider(&mut provider, &model_names, &mut defaults),
            AppType::Codex => sync_codex_provider(&mut provider, &model_names, &mut defaults),
            AppType::Gemini => sync_gemini_provider(&mut provider, &model_names, &mut defaults),
            _ => false,
        };

        if changed {
            db.save_provider(app_key, &provider)
                .map_err(|e| e.to_string())?;
            updated.push(app_key.to_string());
        }
    }

    Ok(MatpoolModelSyncOutcome {
        fetched: all_models.len(),
        chat_capable: model_names.len(),
        updated,
        defaults,
    })
}

fn matpool_provider_id(app: &AppType) -> &'static str {
    match app {
        AppType::Claude => "matpool-claude",
        AppType::Codex => "matpool-codex",
        AppType::Gemini => "matpool-gemini",
        _ => "",
    }
}

fn sync_claude_provider(
    provider: &mut Provider,
    model_names: &[String],
    defaults: &mut Vec<(String, String)>,
) -> bool {
    let mut changed = false;
    for key in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    ] {
        changed |= ensure_env_model(provider, key, model_names, preferred_claude_model);
    }
    if let Some(model) = read_env_model(provider, "ANTHROPIC_MODEL") {
        defaults.push(("claude".to_string(), model));
    }
    changed
}

fn sync_gemini_provider(
    provider: &mut Provider,
    model_names: &[String],
    defaults: &mut Vec<(String, String)>,
) -> bool {
    let changed = ensure_env_model(
        provider,
        "GEMINI_MODEL",
        model_names,
        preferred_gemini_model,
    );
    if let Some(model) = read_env_model(provider, "GEMINI_MODEL") {
        defaults.push(("gemini".to_string(), model));
    }
    changed
}

fn sync_codex_provider(
    provider: &mut Provider,
    model_names: &[String],
    defaults: &mut Vec<(String, String)>,
) -> bool {
    let current = provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .and_then(extract_codex_model_from_toml);
    let selected =
        select_existing_or_preferred(current.as_deref(), model_names, preferred_codex_model);
    let mut changed = false;

    if let Some(config) = provider
        .settings_config
        .get_mut("config")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
    {
        if let Ok(updated) =
            crate::codex_config::update_codex_toml_field(&config, "model", selected.as_str())
        {
            if updated != config {
                provider.settings_config["config"] = Value::String(updated);
                changed = true;
            }
        }
    }

    changed |= set_codex_model_catalog(provider, model_names);
    let api_format = if selected.to_lowercase().contains("gpt-5.5") {
        "openai_responses"
    } else {
        "openai_chat"
    };
    if provider
        .settings_config
        .get("apiFormat")
        .and_then(|v| v.as_str())
        != Some(api_format)
    {
        ensure_settings_object(&mut provider.settings_config).insert(
            "apiFormat".to_string(),
            Value::String(api_format.to_string()),
        );
        changed = true;
    }

    defaults.push(("codex".to_string(), selected));
    changed
}

fn ensure_env_model(
    provider: &mut Provider,
    key: &str,
    model_names: &[String],
    preferred: fn(&[String]) -> Option<String>,
) -> bool {
    let current = read_env_model(provider, key);
    let selected = select_existing_or_preferred(current.as_deref(), model_names, preferred);
    let root = ensure_settings_object(&mut provider.settings_config);
    let env = root.entry("env".to_string()).or_insert_with(|| json!({}));
    if !env.is_object() {
        *env = json!({});
    }
    let env_obj = env.as_object_mut().expect("env object just initialized");
    if env_obj.get(key).and_then(|v| v.as_str()) == Some(selected.as_str()) {
        return false;
    }
    env_obj.insert(key.to_string(), Value::String(selected));
    true
}

fn read_env_model(provider: &Provider, key: &str) -> Option<String> {
    provider
        .settings_config
        .get("env")
        .and_then(|env| env.get(key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

fn select_existing_or_preferred(
    current: Option<&str>,
    model_names: &[String],
    preferred: fn(&[String]) -> Option<String>,
) -> String {
    if let Some(current) = current.map(str::trim).filter(|model| !model.is_empty()) {
        if model_names.iter().any(|model| model == current) {
            return current.to_string();
        }
        if let Some(canonical) = model_names
            .iter()
            .find(|model| model.eq_ignore_ascii_case(current))
        {
            return canonical.clone();
        }
    }
    preferred(model_names)
        .or_else(|| model_names.first().cloned())
        .unwrap_or_default()
}

fn preferred_claude_model(model_names: &[String]) -> Option<String> {
    find_first_containing(model_names, &["claude", "sonnet"])
        .or_else(|| find_first_containing(model_names, &["claude"]))
}

fn preferred_codex_model(model_names: &[String]) -> Option<String> {
    find_first_containing(model_names, &["gpt-5.5"])
        .or_else(|| find_first_containing(model_names, &["gpt"]))
}

fn preferred_gemini_model(model_names: &[String]) -> Option<String> {
    find_first_containing(model_names, &["gemini", "pro"])
        .or_else(|| find_first_containing(model_names, &["gemini"]))
}

fn find_first_containing(model_names: &[String], needles: &[&str]) -> Option<String> {
    model_names
        .iter()
        .find(|model| {
            let lower = model.to_lowercase();
            needles.iter().all(|needle| lower.contains(needle))
        })
        .cloned()
}

fn set_codex_model_catalog(provider: &mut Provider, model_names: &[String]) -> bool {
    let models: Vec<Value> = model_names
        .iter()
        .map(|model| {
            json!({
                "model": model,
                "display_name": model,
                "context_window": CODEX_DEFAULT_CONTEXT_WINDOW,
            })
        })
        .collect();
    let catalog = json!({ "models": models });
    if provider.settings_config.get("modelCatalog") == Some(&catalog) {
        return false;
    }
    ensure_settings_object(&mut provider.settings_config)
        .insert("modelCatalog".to_string(), catalog);
    true
}

fn extract_codex_model_from_toml(config_text: &str) -> Option<String> {
    config_text
        .parse::<toml::Value>()
        .ok()?
        .get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

fn ensure_settings_object(value: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    value
        .as_object_mut()
        .expect("settings object just initialized")
}

fn truncate(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        input.to_string()
    } else {
        let mut output: String = input.chars().take(max_chars).collect();
        output.push('…');
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_capable_models_keep_text_and_code() {
        let models = vec![
            MatpoolPricingModel {
                model_name: "text-model".to_string(),
                description: None,
                tags: None,
                vendor_id: None,
                supported_endpoint_types: Some(vec!["TEXT".to_string()]),
                enable_groups: None,
                model_ratio: None,
                completion_ratio: None,
                model_price: None,
            },
            MatpoolPricingModel {
                model_name: "image-model".to_string(),
                description: None,
                tags: None,
                vendor_id: None,
                supported_endpoint_types: Some(vec!["IMAGE".to_string()]),
                enable_groups: None,
                model_ratio: None,
                completion_ratio: None,
                model_price: None,
            },
        ];

        let filtered = chat_capable_models(&models);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].model_name, "text-model");
    }
}
