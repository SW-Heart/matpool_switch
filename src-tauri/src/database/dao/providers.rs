use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::provider::{Provider, ProviderMeta};
use indexmap::IndexMap;
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

type OmoProviderRow = (
    String,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<usize>,
    Option<String>,
    String,
);

const MATPOOL_CODEX_PROVIDER_ID: &str = "matpool-codex";
const MATPOOL_CODEX_DEFAULT_MODEL: &str = "GPT-5.5";
const MATPOOL_CODEX_LEGACY_MODELS: &[&str] = &["gpt-5.5", "gpt-5"];
const MATPOOL_CODEX_DEFAULT_CONTEXT_WINDOW: u64 = 128_000;

fn codex_config_model(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;
    doc.get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

fn normalize_matpool_codex_config_model(settings: &mut Value) -> bool {
    let Some(config) = settings.get_mut("config") else {
        return false;
    };
    let Some(config_text) = config.as_str() else {
        return false;
    };

    let current_model = codex_config_model(config_text);
    let should_update = current_model
        .as_deref()
        .map(|model| {
            model != MATPOOL_CODEX_DEFAULT_MODEL
                && MATPOOL_CODEX_LEGACY_MODELS
                    .iter()
                    .any(|legacy| model.eq_ignore_ascii_case(legacy))
        })
        .unwrap_or(true);
    if !should_update {
        return false;
    }

    let Ok(updated) = crate::codex_config::update_codex_toml_field(
        config_text,
        "model",
        MATPOOL_CODEX_DEFAULT_MODEL,
    ) else {
        return false;
    };
    if updated == config_text {
        return false;
    }

    *config = Value::String(updated);
    true
}

fn ensure_matpool_codex_model_catalog(settings: &mut Value) -> bool {
    let Some(root) = settings.as_object_mut() else {
        return false;
    };

    let catalog = root
        .entry("modelCatalog".to_string())
        .or_insert_with(|| json!({ "models": [] }));
    if !catalog.is_object() {
        *catalog = json!({ "models": [] });
    }
    if catalog
        .get("models")
        .and_then(|models| models.as_array())
        .is_none()
    {
        catalog["models"] = json!([]);
    }

    let Some(models) = catalog
        .get_mut("models")
        .and_then(|models| models.as_array_mut())
    else {
        return false;
    };

    let mut dirty = false;
    let mut found = false;
    for entry in models.iter_mut() {
        let Some(model) = entry.get("model").and_then(|value| value.as_str()) else {
            continue;
        };
        let is_target = model == MATPOOL_CODEX_DEFAULT_MODEL
            || MATPOOL_CODEX_LEGACY_MODELS
                .iter()
                .any(|legacy| model.eq_ignore_ascii_case(legacy));
        if !is_target {
            continue;
        }

        found = true;
        if entry.get("model").and_then(|value| value.as_str()) != Some(MATPOOL_CODEX_DEFAULT_MODEL)
        {
            entry["model"] = json!(MATPOOL_CODEX_DEFAULT_MODEL);
            dirty = true;
        }
        if entry
            .get("displayName")
            .or_else(|| entry.get("display_name"))
            .and_then(|value| value.as_str())
            != Some(MATPOOL_CODEX_DEFAULT_MODEL)
        {
            entry["displayName"] = json!(MATPOOL_CODEX_DEFAULT_MODEL);
            dirty = true;
        }
        if entry
            .get("contextWindow")
            .or_else(|| entry.get("context_window"))
            .and_then(|value| value.as_u64())
            .is_none()
        {
            entry["contextWindow"] = json!(MATPOOL_CODEX_DEFAULT_CONTEXT_WINDOW);
            dirty = true;
        }
    }

    if !found {
        models.push(json!({
            "model": MATPOOL_CODEX_DEFAULT_MODEL,
            "displayName": MATPOOL_CODEX_DEFAULT_MODEL,
            "contextWindow": MATPOOL_CODEX_DEFAULT_CONTEXT_WINDOW,
        }));
        dirty = true;
    }

    dirty
}

fn migrate_matpool_codex_seed_settings(provider: &mut Provider) -> bool {
    normalize_matpool_codex_config_model(&mut provider.settings_config)
        | ensure_matpool_codex_model_catalog(&mut provider.settings_config)
}

impl Database {
    pub fn get_all_providers(
        &self,
        app_type: &str,
    ) -> Result<IndexMap<String, Provider>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn.prepare(
            "SELECT id, name, settings_config, website_url, category, created_at, sort_index, notes, icon, icon_color, meta, in_failover_queue
             FROM providers WHERE app_type = ?1
             ORDER BY COALESCE(sort_index, 999999), created_at ASC, id ASC"
        ).map_err(|e| AppError::Database(e.to_string()))?;

        let provider_iter = stmt
            .query_map(params![app_type], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let settings_config_str: String = row.get(2)?;
                let website_url: Option<String> = row.get(3)?;
                let category: Option<String> = row.get(4)?;
                let created_at: Option<i64> = row.get(5)?;
                let sort_index: Option<usize> = row.get(6)?;
                let notes: Option<String> = row.get(7)?;
                let icon: Option<String> = row.get(8)?;
                let icon_color: Option<String> = row.get(9)?;
                let meta_str: String = row.get(10)?;
                let in_failover_queue: bool = row.get(11)?;

                let settings_config =
                    serde_json::from_str(&settings_config_str).unwrap_or(serde_json::Value::Null);
                let meta: ProviderMeta = serde_json::from_str(&meta_str).unwrap_or_default();

                Ok((
                    id,
                    Provider {
                        id: "".to_string(), // Placeholder, set below
                        name,
                        settings_config,
                        website_url,
                        category,
                        created_at,
                        sort_index,
                        notes,
                        meta: Some(meta),
                        icon,
                        icon_color,
                        in_failover_queue,
                    },
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut providers = IndexMap::new();
        for provider_res in provider_iter {
            let (id, mut provider) = provider_res.map_err(|e| AppError::Database(e.to_string()))?;
            provider.id = id.clone();

            let mut stmt_endpoints = conn.prepare(
                "SELECT url, added_at FROM provider_endpoints WHERE provider_id = ?1 AND app_type = ?2 ORDER BY added_at ASC, url ASC"
            ).map_err(|e| AppError::Database(e.to_string()))?;

            let endpoints_iter = stmt_endpoints
                .query_map(params![id, app_type], |row| {
                    let url: String = row.get(0)?;
                    let added_at: Option<i64> = row.get(1)?;
                    Ok((
                        url,
                        crate::settings::CustomEndpoint {
                            url: "".to_string(),
                            added_at: added_at.unwrap_or(0),
                            last_used: None,
                        },
                    ))
                })
                .map_err(|e| AppError::Database(e.to_string()))?;

            let mut custom_endpoints = HashMap::new();
            for ep_res in endpoints_iter {
                let (url, mut ep) = ep_res.map_err(|e| AppError::Database(e.to_string()))?;
                ep.url = url.clone();
                custom_endpoints.insert(url, ep);
            }

            if let Some(meta) = &mut provider.meta {
                meta.custom_endpoints = custom_endpoints;
            }

            providers.insert(id, provider);
        }

        Ok(providers)
    }

    pub fn get_current_provider(&self, app_type: &str) -> Result<Option<String>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id FROM providers WHERE app_type = ?1 AND is_current = 1 LIMIT 1")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![app_type])
            .map_err(|e| AppError::Database(e.to_string()))?;

        if let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
            Ok(Some(
                row.get(0).map_err(|e| AppError::Database(e.to_string()))?,
            ))
        } else {
            Ok(None)
        }
    }

    pub fn get_provider_by_id(
        &self,
        id: &str,
        app_type: &str,
    ) -> Result<Option<Provider>, AppError> {
        let conn = lock_conn!(self.conn);
        let result = conn.query_row(
            "SELECT name, settings_config, website_url, category, created_at, sort_index, notes, icon, icon_color, meta, in_failover_queue
             FROM providers WHERE id = ?1 AND app_type = ?2",
            params![id, app_type],
            |row| {
                let name: String = row.get(0)?;
                let settings_config_str: String = row.get(1)?;
                let website_url: Option<String> = row.get(2)?;
                let category: Option<String> = row.get(3)?;
                let created_at: Option<i64> = row.get(4)?;
                let sort_index: Option<usize> = row.get(5)?;
                let notes: Option<String> = row.get(6)?;
                let icon: Option<String> = row.get(7)?;
                let icon_color: Option<String> = row.get(8)?;
                let meta_str: String = row.get(9)?;
                let in_failover_queue: bool = row.get(10)?;

                let settings_config = serde_json::from_str(&settings_config_str).unwrap_or(serde_json::Value::Null);
                let meta: ProviderMeta = serde_json::from_str(&meta_str).unwrap_or_default();

                Ok(Provider {
                    id: id.to_string(),
                    name,
                    settings_config,
                    website_url,
                    category,
                    created_at,
                    sort_index,
                    notes,
                    meta: Some(meta),
                    icon,
                    icon_color,
                    in_failover_queue,
                })
            },
        );

        match result {
            Ok(provider) => Ok(Some(provider)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    pub fn save_provider(&self, app_type: &str, provider: &Provider) -> Result<(), AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut meta_clone = provider.meta.clone().unwrap_or_default();
        let endpoints = std::mem::take(&mut meta_clone.custom_endpoints);

        let existing: Option<(bool, bool)> = tx
            .query_row(
                "SELECT is_current, in_failover_queue FROM providers WHERE id = ?1 AND app_type = ?2",
                params![provider.id, app_type],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let is_update = existing.is_some();
        let (is_current, in_failover_queue) =
            existing.unwrap_or((false, provider.in_failover_queue));

        if is_update {
            tx.execute(
                "UPDATE providers SET
                    name = ?1,
                    settings_config = ?2,
                    website_url = ?3,
                    category = ?4,
                    created_at = ?5,
                    sort_index = ?6,
                    notes = ?7,
                    icon = ?8,
                    icon_color = ?9,
                    meta = ?10,
                    is_current = ?11,
                    in_failover_queue = ?12
                WHERE id = ?13 AND app_type = ?14",
                params![
                    provider.name,
                    serde_json::to_string(&provider.settings_config).map_err(|e| {
                        AppError::Database(format!("Failed to serialize settings_config: {e}"))
                    })?,
                    provider.website_url,
                    provider.category,
                    provider.created_at,
                    provider.sort_index,
                    provider.notes,
                    provider.icon,
                    provider.icon_color,
                    serde_json::to_string(&meta_clone).map_err(|e| AppError::Database(format!(
                        "Failed to serialize meta: {e}"
                    )))?,
                    is_current,
                    in_failover_queue,
                    provider.id,
                    app_type,
                ],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        } else {
            tx.execute(
                "INSERT INTO providers (
                    id, app_type, name, settings_config, website_url, category,
                    created_at, sort_index, notes, icon, icon_color, meta, is_current, in_failover_queue
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    provider.id,
                    app_type,
                    provider.name,
                    serde_json::to_string(&provider.settings_config)
                        .map_err(|e| AppError::Database(format!("Failed to serialize settings_config: {e}")))?,
                    provider.website_url,
                    provider.category,
                    provider.created_at,
                    provider.sort_index,
                    provider.notes,
                    provider.icon,
                    provider.icon_color,
                    serde_json::to_string(&meta_clone)
                        .map_err(|e| AppError::Database(format!("Failed to serialize meta: {e}")))?,
                    is_current,
                    in_failover_queue,
                ],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

            for (url, endpoint) in endpoints {
                tx.execute(
                    "INSERT INTO provider_endpoints (provider_id, app_type, url, added_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![provider.id, app_type, url, endpoint.added_at],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
        }

        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn delete_provider(&self, app_type: &str, id: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM providers WHERE id = ?1 AND app_type = ?2",
            params![id, app_type],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn set_current_provider(&self, app_type: &str, id: &str) -> Result<(), AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;

        tx.execute(
            "UPDATE providers SET is_current = 0 WHERE app_type = ?1",
            params![app_type],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        tx.execute(
            "UPDATE providers SET is_current = 1 WHERE id = ?1 AND app_type = ?2",
            params![id, app_type],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn update_provider_settings_config(
        &self,
        app_type: &str,
        provider_id: &str,
        settings_config: &serde_json::Value,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE providers SET settings_config = ?1 WHERE id = ?2 AND app_type = ?3",
            params![
                serde_json::to_string(settings_config).map_err(|e| AppError::Database(format!(
                    "Failed to serialize settings_config: {e}"
                )))?,
                provider_id,
                app_type
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn add_custom_endpoint(
        &self,
        app_type: &str,
        provider_id: &str,
        url: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        let added_at = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO provider_endpoints (provider_id, app_type, url, added_at) VALUES (?1, ?2, ?3, ?4)",
            params![provider_id, app_type, url, added_at],
        ).map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn remove_custom_endpoint(
        &self,
        app_type: &str,
        provider_id: &str,
        url: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM provider_endpoints WHERE provider_id = ?1 AND app_type = ?2 AND url = ?3",
            params![provider_id, app_type, url],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn set_omo_provider_current(
        &self,
        app_type: &str,
        provider_id: &str,
        category: &str,
    ) -> Result<(), AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;
        tx.execute(
            "UPDATE providers SET is_current = 0 WHERE app_type = ?1 AND category = ?2",
            params![app_type, category],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        // OMO ↔ OMO Slim mutually exclusive: deactivate the opposite category
        let opposite = match category {
            "omo" => Some("omo-slim"),
            "omo-slim" => Some("omo"),
            _ => None,
        };
        if let Some(opp) = opposite {
            tx.execute(
                "UPDATE providers SET is_current = 0 WHERE app_type = ?1 AND category = ?2",
                params![app_type, opp],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        }
        let updated = tx
            .execute(
                "UPDATE providers SET is_current = 1 WHERE id = ?1 AND app_type = ?2 AND category = ?3",
                params![provider_id, app_type, category],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        if updated != 1 {
            return Err(AppError::Database(format!(
                "Failed to set {category} provider current: provider '{provider_id}' not found in app '{app_type}'"
            )));
        }
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn is_omo_provider_current(
        &self,
        app_type: &str,
        provider_id: &str,
        category: &str,
    ) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        match conn.query_row(
            "SELECT is_current FROM providers
             WHERE id = ?1 AND app_type = ?2 AND category = ?3",
            params![provider_id, app_type, category],
            |row| row.get(0),
        ) {
            Ok(is_current) => Ok(is_current),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    pub fn clear_omo_provider_current(
        &self,
        app_type: &str,
        provider_id: &str,
        category: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE providers SET is_current = 0
             WHERE id = ?1 AND app_type = ?2 AND category = ?3",
            params![provider_id, app_type, category],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn get_current_omo_provider(
        &self,
        app_type: &str,
        category: &str,
    ) -> Result<Option<Provider>, AppError> {
        let conn = lock_conn!(self.conn);
        let row_data: Result<OmoProviderRow, rusqlite::Error> = conn.query_row(
            "SELECT id, name, settings_config, category, created_at, sort_index, notes, meta
             FROM providers
             WHERE app_type = ?1 AND category = ?2 AND is_current = 1
             LIMIT 1",
            params![app_type, category],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        );

        let (id, name, settings_config_str, _row_category, created_at, sort_index, notes, meta_str) =
            match row_data {
                Ok(v) => v,
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(e) => return Err(AppError::Database(e.to_string())),
            };

        let settings_config = serde_json::from_str(&settings_config_str).map_err(|e| {
            AppError::Database(format!(
                "Failed to parse {category} provider settings_config (provider_id={id}): {e}"
            ))
        })?;
        let meta: crate::provider::ProviderMeta = if meta_str.trim().is_empty() {
            crate::provider::ProviderMeta::default()
        } else {
            serde_json::from_str(&meta_str).map_err(|e| {
                AppError::Database(format!(
                    "Failed to parse {category} provider meta (provider_id={id}): {e}"
                ))
            })?
        };

        Ok(Some(Provider {
            id,
            name,
            settings_config,
            website_url: None,
            category: Some(category.to_string()),
            created_at,
            sort_index,
            notes,
            meta: Some(meta),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }))
    }

    /// 判断 providers 表是否为空（全 app_type 一起算）。
    ///
    /// 用于区分"全新安装"和"升级用户"：在启动流程 import/seed 之前调用。
    /// 使用 `EXISTS` 短路查询，比 `COUNT(*)` 在将来表变大时更高效。
    pub fn is_providers_empty(&self) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let exists: bool = conn
            .query_row("SELECT EXISTS(SELECT 1 FROM providers)", [], |row| {
                row.get(0)
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(!exists)
    }

    /// 仅获取指定 app 下所有 provider 的 id 集合。
    ///
    /// 比 `get_all_providers` 轻量得多：只读 id 列、无 endpoint 子查询。
    /// 用于只需要做存在性检查的场景（如 additive 模式的 live 同步去重）。
    pub fn get_provider_ids(&self, app_type: &str) -> Result<HashSet<String>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id FROM providers WHERE app_type = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![app_type], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut ids = HashSet::new();
        for row in rows {
            ids.insert(row.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(ids)
    }

    /// 判断指定 app 下是否已存在任意 provider。
    ///
    /// 启动阶段的 live import 需要使用这个更严格的判断：
    /// 只要该 app 已经有任何 provider（包括官方 seed），就不应再自动导入 `default`。
    pub fn has_any_provider_for_app(&self, app_type: &str) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM providers WHERE app_type = ?1)",
                params![app_type],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(exists)
    }

    /// 判断指定 app 下是否存在非官方种子的供应商。
    ///
    /// 比 `get_all_providers` 轻量得多：只读 id 列、无 endpoint 子查询、首条命中即返回。
    /// 用于 `import_default_config` 决定是否跳过 live 导入。
    pub fn has_non_official_seed_provider(&self, app_type: &str) -> Result<bool, AppError> {
        use crate::database::dao::providers_seed::is_official_seed_id;
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id FROM providers WHERE app_type = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut rows = stmt
            .query(params![app_type])
            .map_err(|e| AppError::Database(e.to_string()))?;
        while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
            let id: String = row.get(0).map_err(|e| AppError::Database(e.to_string()))?;
            if !is_official_seed_id(&id) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 计算指定 app 下一个可用的 sort_index（追加到末尾）。
    fn next_sort_index_for_app(&self, app_type: &str) -> Result<usize, AppError> {
        let conn = lock_conn!(self.conn);
        let max: Option<i64> = conn
            .query_row(
                "SELECT MAX(sort_index) FROM providers WHERE app_type = ?1",
                params![app_type],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(max.map(|v| (v + 1) as usize).unwrap_or(0))
    }

    /// 启动时调用：补齐缺失的官方预设供应商（Claude / Codex / Gemini）。
    ///
    /// 使用 settings flag `official_providers_seeded` 保证每个数据库只执行一次：
    /// - 全新用户：seed 三条官方预设
    /// - 老用户升级：同样会触发一次（flag 不存在），追加到末尾，不影响已有排序
    /// - 用户删除 seed 后：不再重建（flag 已为 true），尊重用户意图
    ///
    /// 与 `Database::save_provider` 的 UPSERT 语义配合，即使被意外重复调用
    /// 也不会覆盖用户当前激活的供应商（is_current 字段会被保留）。
    pub fn init_default_official_providers(&self) -> Result<usize, AppError> {
        use crate::database::dao::providers_seed::OFFICIAL_SEEDS;

        // 注意：以前这里靠 `official_providers_seeded` flag 全局短路，导致对
        // 既有数据库追加新 seed（例如 Matpool）时不会再被插入。改为每条 seed
        // 独立幂等（按 id 检查存在性），让 OFFICIAL_SEEDS 数组成为单一事实源。
        // flag 仍然保留写入，便于其他逻辑/迁移识别"曾经做过 seed 初始化"。

        let mut inserted = 0_usize;
        let now_ms = chrono::Utc::now().timestamp_millis();

        for seed in OFFICIAL_SEEDS {
            let app_type_str = seed.app_type.as_str();

            // 若该 id 已存在：检查是否需要修补（迁移老用户 / 修我之前的 bug）
            if let Some(mut existing) = self.get_provider_by_id(seed.id, app_type_str)? {
                let mut dirty = false;

                // 修补 matpool-* seed 的 category：之前误设为 "official" 导致 proxy::switch()
                // 的 "代理接管模式下不能切换到官方供应商" guard 被错误触发。
                if super::providers_seed::is_matpool_seed_id(seed.id)
                    && existing.category.as_deref() == Some("official")
                {
                    existing.category = Some("aggregator".to_string());
                    dirty = true;
                    log::info!(
                        "Migrating matpool seed {} category: 'official' -> 'aggregator'",
                        seed.id
                    );
                }

                // 修补 meta.api_format：之前的 init 没写这个字段（B3a 之前），导致 proxy 路由
                // 拿不到协议提示。补齐时不覆盖用户/旧迁移已设置的值。
                if let Some(api_format) = seed.api_format {
                    let needs_set = existing
                        .meta
                        .as_ref()
                        .and_then(|m| m.api_format.as_deref())
                        .is_none();
                    if needs_set {
                        let mut meta = existing.meta.clone().unwrap_or_default();
                        meta.api_format = Some(api_format.to_string());
                        existing.meta = Some(meta);
                        dirty = true;
                        log::info!(
                            "Migrating seed {} meta.api_format: now '{}'",
                            seed.id,
                            api_format
                        );
                    }
                }

                if seed.id == MATPOOL_CODEX_PROVIDER_ID
                    && migrate_matpool_codex_seed_settings(&mut existing)
                {
                    dirty = true;
                    log::info!(
                        "Migrating matpool-codex seed model/catalog to {MATPOOL_CODEX_DEFAULT_MODEL}"
                    );
                }

                if dirty {
                    self.save_provider(app_type_str, &existing)?;
                }
                continue;
            }

            let next_sort_index = self.next_sort_index_for_app(app_type_str)?;

            let settings_config: serde_json::Value =
                serde_json::from_str(seed.settings_config_json).map_err(|e| {
                    AppError::Database(format!("Seed JSON parse failed for {}: {e}", seed.id))
                })?;

            let mut provider = Provider::with_id(
                seed.id.to_string(),
                seed.name.to_string(),
                settings_config,
                Some(seed.website_url.to_string()),
            );
            // Matpool seed 是聚合器（指向 token.matpool.com 网关），不是 anthropic/openai/google
            // 真正的 official API。如果给它打 "official" 标签，proxy::switch() 里的
            // "代理接管模式下不能切换到官方供应商" guard 会误把 Matpool 当作真官方拦截，
            // 导致用户开启 Codex 接管时报错。这个 guard 的本意是防止用代理访问 anthropic.com /
            // api.openai.com 触发账号封禁，所以只对真 official 才该生效。
            //
            // 上游 matpool-switch / matcloud_global 的 ProviderCategory 列表里 "aggregator"
            // 正是给"中转/聚合服务"用的。
            let category = if super::providers_seed::is_matpool_seed_id(seed.id) {
                "aggregator"
            } else {
                "official"
            };
            provider.category = Some(category.to_string());
            provider.icon = Some(seed.icon.to_string());
            provider.icon_color = Some(seed.icon_color.to_string());
            provider.sort_index = Some(next_sort_index);
            provider.created_at = Some(now_ms);
            // 把 seed 上的 api_format 写入 provider.meta.api_format（serde rename = "apiFormat"）。
            // 这是 proxy::providers::claude::get_claude_api_format 的 SSOT，决定本地代理
            // 是否要做 Anthropic ↔ OpenAI Chat / Responses / Gemini 协议转换。
            if let Some(api_format) = seed.api_format {
                let mut meta = provider.meta.clone().unwrap_or_default();
                meta.api_format = Some(api_format.to_string());
                provider.meta = Some(meta);
            }

            self.save_provider(app_type_str, &provider)?;
            inserted += 1;
            log::info!(
                "✓ Seeded official provider: {} ({})",
                seed.name,
                app_type_str
            );
        }

        // 即使 inserted=0 也设置 flag，记录"已运行过 seed 初始化"
        self.set_setting("official_providers_seeded", "true")?;

        Ok(inserted)
    }

    /// 按 id 兜底插入单条 official seed（仅当目标表中该 id 不存在时插入）。
    ///
    /// 与 `init_default_official_providers` 不同：
    /// - 不触碰 `official_providers_seeded` 全局 flag，是 on-demand 修复
    /// - 只处理一条 seed，由调用方决定 id + app_type
    /// - 已存在则尊重用户自定义，不覆盖
    ///
    /// 返回 Ok(true) 表示插入了新行，Ok(false) 表示已存在被跳过。
    pub fn ensure_official_seed_by_id(
        &self,
        seed_id: &str,
        app_type: crate::app_config::AppType,
    ) -> Result<bool, AppError> {
        use crate::database::dao::providers_seed::OFFICIAL_SEEDS;

        let seed = OFFICIAL_SEEDS
            .iter()
            .find(|s| s.id == seed_id && s.app_type == app_type)
            .ok_or_else(|| {
                AppError::Database(format!(
                    "unknown official seed: id={seed_id}, app_type={}",
                    app_type.as_str()
                ))
            })?;

        let app_type_str = seed.app_type.as_str();

        if self.get_provider_by_id(seed_id, app_type_str)?.is_some() {
            return Ok(false);
        }

        let settings_config: serde_json::Value = serde_json::from_str(seed.settings_config_json)
            .map_err(|e| {
                AppError::Database(format!("Seed JSON parse failed for {}: {e}", seed.id))
            })?;

        let next_sort_index = self.next_sort_index_for_app(app_type_str)?;
        let now_ms = chrono::Utc::now().timestamp_millis();

        let mut provider = Provider::with_id(
            seed.id.to_string(),
            seed.name.to_string(),
            settings_config,
            Some(seed.website_url.to_string()),
        );
        provider.category = Some("official".to_string());
        provider.icon = Some(seed.icon.to_string());
        provider.icon_color = Some(seed.icon_color.to_string());
        provider.sort_index = Some(next_sort_index);
        provider.created_at = Some(now_ms);

        self.save_provider(app_type_str, &provider)?;

        Ok(true)
    }
}

#[cfg(test)]
mod ensure_official_seed_tests {
    use crate::app_config::AppType;
    use crate::database::Database;

    #[test]
    fn ensure_inserts_when_missing() {
        let db = Database::memory().expect("memory db");
        let inserted = db
            .ensure_official_seed_by_id("claude-official", AppType::Claude)
            .expect("ensure ok");
        assert!(inserted, "should insert when missing");

        let provider = db
            .get_provider_by_id("claude-official", AppType::Claude.as_str())
            .expect("query ok")
            .expect("provider exists after ensure");

        assert_eq!(provider.id, "claude-official");
        assert_eq!(provider.name, "Claude Official");
        assert_eq!(provider.category.as_deref(), Some("official"));
        assert_eq!(provider.icon.as_deref(), Some("anthropic"));
        assert_eq!(provider.icon_color.as_deref(), Some("#D4915D"));
    }

    #[test]
    fn ensure_skips_when_present_and_preserves_customization() {
        let db = Database::memory().expect("memory db");
        db.init_default_official_providers().expect("seed");

        let mut renamed = db
            .get_provider_by_id("claude-official", AppType::Claude.as_str())
            .expect("query ok")
            .expect("seed present");
        renamed.name = "My Custom Backup".to_string();
        db.save_provider(AppType::Claude.as_str(), &renamed)
            .expect("save customization");

        let inserted = db
            .ensure_official_seed_by_id("claude-official", AppType::Claude)
            .expect("ensure ok");
        assert!(!inserted, "should skip when present");

        let after = db
            .get_provider_by_id("claude-official", AppType::Claude.as_str())
            .expect("query ok")
            .expect("still present");
        assert_eq!(
            after.name, "My Custom Backup",
            "customization must not be overwritten"
        );
    }

    #[test]
    fn ensure_rejects_unknown_seed() {
        let db = Database::memory().expect("memory db");
        let result = db.ensure_official_seed_by_id("nonexistent-id", AppType::Claude);
        assert!(result.is_err(), "unknown seed id should be Err");
    }

    #[test]
    fn ensure_rejects_seed_app_type_mismatch() {
        let db = Database::memory().expect("memory db");
        let result = db.ensure_official_seed_by_id("claude-official", AppType::Codex);
        assert!(result.is_err(), "(id, app_type) mismatch should be Err");
    }
}

#[cfg(test)]
mod matpool_codex_seed_migration_tests {
    use super::*;

    #[test]
    fn migration_updates_legacy_model_and_adds_catalog() {
        let mut provider = Provider::with_id(
            MATPOOL_CODEX_PROVIDER_ID.to_string(),
            "Matpool".to_string(),
            json!({
                "auth": {"OPENAI_API_KEY": ""},
                "config": "model_provider = \"custom\"\nmodel = \"gpt-5.5\"\n\n[model_providers.custom]\nbase_url = \"https://token.matpool.com/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true",
            }),
            None,
        );

        assert!(migrate_matpool_codex_seed_settings(&mut provider));
        assert!(provider.settings_config["config"]
            .as_str()
            .expect("config")
            .contains("model = \"GPT-5.5\""));
        assert_eq!(
            provider.settings_config["modelCatalog"]["models"][0]["model"],
            json!(MATPOOL_CODEX_DEFAULT_MODEL)
        );
    }

    #[test]
    fn migration_preserves_other_catalog_entries() {
        let mut provider = Provider::with_id(
            MATPOOL_CODEX_PROVIDER_ID.to_string(),
            "Matpool".to_string(),
            json!({
                "auth": {"OPENAI_API_KEY": ""},
                "config": "model = \"GPT-5.5\"",
                "modelCatalog": {
                    "models": [
                        {"model": "custom-model", "displayName": "Custom"},
                        {"model": "gpt-5.5"}
                    ]
                }
            }),
            None,
        );

        assert!(migrate_matpool_codex_seed_settings(&mut provider));
        let models = provider.settings_config["modelCatalog"]["models"]
            .as_array()
            .expect("models");
        assert!(models
            .iter()
            .any(|model| model["model"] == json!("custom-model")));
        assert!(models
            .iter()
            .any(|model| model["model"] == json!(MATPOOL_CODEX_DEFAULT_MODEL)));
    }

    #[test]
    fn migration_does_not_override_custom_model() {
        let mut provider = Provider::with_id(
            MATPOOL_CODEX_PROVIDER_ID.to_string(),
            "Matpool".to_string(),
            json!({
                "auth": {"OPENAI_API_KEY": ""},
                "config": "model = \"GLM-5.2\"",
                "modelCatalog": {
                    "models": [
                        {"model": "GLM-5.2", "displayName": "GLM-5.2"}
                    ]
                }
            }),
            None,
        );

        assert!(migrate_matpool_codex_seed_settings(&mut provider));
        assert!(provider.settings_config["config"]
            .as_str()
            .expect("config")
            .contains("model = \"GLM-5.2\""));
    }
}
