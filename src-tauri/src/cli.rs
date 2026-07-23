#![allow(dead_code, unused_imports, unused_mut, unused_variables)]

mod app_config;
#[path = "cli_app_store.rs"]
mod app_store;
mod claude_desktop_config;
mod claude_mcp;
mod codex_config;
#[path = "cli_commands.rs"]
mod commands;
mod config;
mod database;
mod error;
mod gemini_config;
mod gemini_mcp;
mod hermes_config;
mod mcp;
mod openclaw_config;
mod opencode_config;
mod prompt;
mod prompt_files;
mod provider;
mod provider_defaults;
mod proxy;
#[path = "cli_services.rs"]
mod services;
mod settings;
mod store;
#[cfg(feature = "desktop")]
mod tray {
    pub const TRAY_ID: &str = "matpool-switch";

    pub fn create_tray_menu(
        _app: &tauri::AppHandle,
        _state: &crate::store::AppState,
    ) -> Result<tauri::menu::Menu<tauri::Wry>, crate::error::AppError> {
        Err(crate::error::AppError::Message(
            "tray menu is not available in CLI".to_string(),
        ))
    }
}
#[path = "cli_usage_events.rs"]
mod usage_events;
mod usage_script;

use app_config::AppType;
use codex_config::{get_codex_auth_path, get_codex_config_path};
use commands::{matpool_keychain_clear, matpool_keychain_get, matpool_keychain_set};
use config::{
    get_app_config_dir, get_claude_mcp_path, get_claude_settings_path, write_json_file,
    write_text_file,
};
use database::Database;
use gemini_config::get_gemini_env_path;
use rusqlite::{Connection, OpenFlags};
use std::env;
use std::fs;
#[cfg(target_os = "windows")]
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Write};
use std::net::{SocketAddr, TcpStream};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::ExitCode;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use store::AppState;
#[cfg(target_os = "windows")]
use winreg::{enums::*, RegKey};

type CliResult<T = ()> = Result<T, String>;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> ExitCode {
    let _ = rustls::crypto::ring::default_provider().install_default();

    match run(env::args().skip(1).collect()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

async fn run(args: Vec<String>) -> CliResult {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };

    match command {
        "-h" | "--help" | "help" => print_help(),
        "-V" | "--version" | "version" => println!("matpool {VERSION}"),
        "login" => login(&args[1..])?,
        "logout" => logout()?,
        "status" => status().await?,
        "doctor" => doctor().await?,
        "models" | "model" => models_cli(&args[1..]).await?,
        "provider" | "providers" => provider_cli(&args[1..]).await?,
        "takeover" => takeover(&args[1..]).await?,
        "daemon" => daemon(&args[1..]).await?,
        "proxy" => proxy(&args[1..]).await?,
        "setup" => setup(&args[1..]).await?,
        unknown => {
            return Err(format!(
                "unknown command '{unknown}'. Run 'matpool help' for usage."
            ));
        }
    }

    Ok(())
}

fn print_help() {
    println!(
        "matpool {VERSION}

Usage:
  matpool login [--token <token>]      Save Matpool token to OS keychain
  matpool logout                       Remove Matpool token from OS keychain
  matpool setup [options]              Login, install daemon, start proxy, and enable takeover
  matpool update                       Update the npm CLI package to latest
  matpool status                       Show local Matpool Switch state
  matpool doctor                       Check key files and local state
  matpool models list                  List Matpool TEXT/CODE models
  matpool models sync [app|all]        Sync Matpool models into CLI providers
  matpool models claude                Show Claude model slot config
  matpool models claude list           List synced Matpool Claude models
  matpool models claude set [options]  Set Claude model slots, then sync live config
  matpool provider list [app|all]      List configured providers
  matpool provider seed                Ensure built-in Matpool/official providers exist
  matpool provider switch <app> <id>   Switch current provider (claude/codex; 'default' restores your original config)
  matpool takeover <app|all>           Enable local proxy takeover for an app
  matpool takeover <app|all> --disable Disable local proxy takeover for an app
  matpool daemon install               Install user-level background daemon
  matpool daemon start                 Start installed background daemon
  matpool daemon stop                  Stop installed background daemon
  matpool daemon uninstall             Remove installed background daemon
  matpool daemon run                   Run local proxy daemon in foreground
  matpool daemon status                Check local proxy health
  matpool proxy start                  Alias for daemon run

Takeover apps:
  claude, codex, gemini, all

Setup options:
  --token <token>                      Save token during setup
  --apps <list>                        Comma-separated apps, default: all
  --skip-login                         Do not read or write token
  --skip-daemon                        Do not install/start background daemon
  --skip-takeover                      Do not enable takeover
  --dry-run                            Print planned actions without changing anything
"
    );
}

fn login(args: &[String]) -> CliResult {
    let token = parse_option_value(args, "--token")
        .or_else(|| env::var("MATPOOL_TOKEN").ok())
        .unwrap_or_else(|| {
            print!("Matpool token: ");
            let _ = io::stdout().flush();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_ok() {
                input
            } else {
                String::new()
            }
        });

    let token = token.trim();
    if token.is_empty() {
        return Err("token is empty".to_string());
    }

    matpool_keychain_set(token.to_string())?;
    println!("Matpool token saved to OS keychain.");
    Ok(())
}

fn logout() -> CliResult {
    matpool_keychain_clear()?;
    println!("Matpool token removed from OS keychain.");
    Ok(())
}

async fn status() -> CliResult {
    let _ = init_db()?;
    let db_state = read_db_state();

    println!("Matpool Switch");
    println!("  version: {VERSION}");
    println!("  config_dir: {}", get_app_config_dir().display());
    println!(
        "  database: {}",
        match &db_state {
            Ok(_) => "readable".to_string(),
            Err(err) => format!("unavailable ({err})"),
        }
    );
    println!(
        "  token: {}",
        if matpool_keychain_get()?.is_some() {
            "configured"
        } else {
            "missing"
        }
    );
    println!(
        "  local_proxy_required: {}",
        if db_state
            .as_ref()
            .map(|state| state.local_proxy_required())
            .unwrap_or(false)
        {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  local_proxy_health: {}",
        match probe_proxy_health(db_state.as_ref().ok().and_then(|state| state.proxy_addr())) {
            Ok(Some(addr)) => format!("running at {addr}"),
            Ok(None) => "not running".to_string(),
            Err(err) => format!("unknown ({err})"),
        }
    );

    println!();
    println!("Apps:");
    for app in AppType::all() {
        let app_key = app.as_str();
        let current = db_state
            .as_ref()
            .ok()
            .and_then(|state| state.current_provider(app_key))
            .unwrap_or("-");
        let takeover_state = db_state
            .as_ref()
            .map(|state| state.takeover_state(app_key))
            .unwrap_or("false");
        println!(
            "  {:<15} current_provider={:<24} takeover={}",
            app_key, current, takeover_state,
        );
    }

    Ok(())
}

async fn doctor() -> CliResult {
    let _ = init_db()?;
    status().await?;

    println!();
    println!("Files:");
    print_path_status("claude_settings", get_claude_settings_path());
    print_path_status("claude_mcp", get_claude_mcp_path());
    print_path_status("codex_auth", get_codex_auth_path());
    print_path_status("codex_config", get_codex_config_path());
    print_path_status("gemini_env", get_gemini_env_path());

    println!();
    println!("Daemon:");
    match probe_proxy_health(read_db_state().ok().and_then(|state| state.proxy_addr())) {
        Ok(Some(addr)) => println!("  status: running at {addr}"),
        Ok(None) => println!("  status: not running"),
        Err(err) => println!("  status: unknown ({err})"),
    }
    println!("  start: matpool daemon start");

    Ok(())
}

async fn provider_cli(args: &[String]) -> CliResult {
    let command = args.first().map(String::as_str).unwrap_or("list");
    match command {
        "list" | "ls" => provider_list(args.get(1).map(String::as_str))?,
        "seed" => {
            let inserted = ensure_provider_seeds()?;
            println!("Provider seeds ensured. inserted={inserted}");
        }
        "switch" => {
            let app = args
                .get(1)
                .map(String::as_str)
                .ok_or_else(|| "usage: matpool provider switch <app> <provider_id>".to_string())?;
            let provider_id = args
                .get(2)
                .map(String::as_str)
                .ok_or_else(|| "usage: matpool provider switch <app> <provider_id>".to_string())?;
            provider_switch(app, provider_id).await?;
        }
        unknown => {
            return Err(format!(
                "unknown provider command '{unknown}'. Usage: matpool provider list [app|all] | matpool provider seed | matpool provider switch <app> <provider_id>"
            ));
        }
    }
    Ok(())
}

async fn provider_switch(app: &str, provider_id: &str) -> CliResult {
    let db = init_db()?;
    let state = AppState::new(db);
    match app {
        "claude" => switch_claude_provider(&state, provider_id).await,
        "codex" => switch_cli_provider(&state, AppType::Codex, provider_id),
        _ => Err(format!(
            "provider switch for '{app}' is not supported in CLI yet; supported apps: claude, codex"
        )),
    }
}

fn switch_cli_provider(state: &AppState, app_type: AppType, provider_id: &str) -> CliResult {
    let result = services::provider::ProviderService::switch(state, app_type.clone(), provider_id)
        .map_err(|e| e.to_string())?;

    println!(
        "Switched {} provider to {}.",
        app_type.as_str(),
        provider_id
    );
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(())
}

fn takeover_provider_id(app_type: &AppType) -> Option<&'static str> {
    match app_type {
        // Claude has a separate direct/proxy preparation flow below.
        AppType::Codex => Some("matpool-codex"),
        _ => None,
    }
}

async fn models_cli(args: &[String]) -> CliResult {
    let command = args.first().map(String::as_str).unwrap_or("list");
    match command {
        "list" | "ls" => {
            let models = services::matpool_models::fetch_matpool_pricing_models().await?;
            let chat_models = services::matpool_models::chat_capable_models(&models);
            println!(
                "Matpool models: fetched={} chat_capable={}",
                models.len(),
                chat_models.len()
            );
            for model in chat_models.iter().take(50) {
                println!("  - {}", model.model_name);
            }
            if chat_models.len() > 50 {
                println!("  ... {} more", chat_models.len() - 50);
            }
        }
        "sync" => {
            let app = args.get(1).map(String::as_str).unwrap_or("all");
            let db = init_db()?;
            let apps = expand_app_types(app)?;
            let outcome =
                services::matpool_models::sync_matpool_models_for_apps(&db, &apps).await?;
            sync_claude_live_after_model_sync(db.clone(), &apps).await?;
            sync_codex_live_after_model_sync(db.clone(), &apps).await?;
            print_model_sync_outcome(&outcome);
        }
        "claude" => claude_models_cli(&args[1..]).await?,
        unknown => {
            return Err(format!(
                "unknown models command '{unknown}'. Usage: matpool models list | matpool models sync [app|all] | matpool models claude [list|set]"
            ));
        }
    }
    Ok(())
}

async fn claude_models_cli(args: &[String]) -> CliResult {
    let command = args.first().map(String::as_str).unwrap_or("show");
    match command {
        "show" | "status" => show_claude_model_slots()?,
        "list" | "ls" => list_claude_model_catalog()?,
        "set" => set_claude_model_slots(&args[1..]).await?,
        unknown => {
            return Err(format!(
                "unknown Claude models command '{unknown}'. Usage: matpool models claude [show|list|set]"
            ));
        }
    }
    Ok(())
}

fn show_claude_model_slots() -> CliResult {
    let db = init_db()?;
    let provider = load_matpool_claude_provider(&db)?;
    print_claude_model_slots(&provider);
    println!();
    println!("These values are Matpool model IDs. Change them with:");
    println!("  matpool models claude set --sonnet <model> --opus <model> --haiku <model>");
    println!("  matpool models claude set --default <model> --custom <model>");
    Ok(())
}

fn list_claude_model_catalog() -> CliResult {
    let db = init_db()?;
    let provider = load_matpool_claude_provider(&db)?;
    let models = claude_catalog_model_names(&provider);
    if models.is_empty() {
        println!("No synced Matpool Claude model catalog found.");
        println!("Run: matpool models sync claude");
        return Ok(());
    }
    println!("Synced Matpool Claude models:");
    for model in models {
        println!("  - {model}");
    }
    Ok(())
}

async fn set_claude_model_slots(args: &[String]) -> CliResult {
    let db = init_db()?;
    let mut provider = load_matpool_claude_provider(&db)?;
    let updates = parse_claude_model_slot_updates(args)?;
    if updates.is_empty() {
        return Err(
            "usage: matpool models claude set [--default <model>] [--sonnet <model>] [--opus <model>] [--haiku <model>] [--custom <model>]"
                .to_string(),
        );
    }

    let catalog = claude_catalog_model_names(&provider);
    for (label, _, requested) in &updates {
        canonicalize_claude_model(requested, &catalog).ok_or_else(|| {
            if catalog.is_empty() {
                format!(
                    "Claude model catalog is empty. Run 'matpool models sync claude' before setting {label}."
                )
            } else {
                format!(
                    "unknown Matpool Claude model for {label}: {requested}. Run 'matpool models claude list' to see available models."
                )
            }
        })?;
    }

    for (_, env_key, requested) in updates {
        let model = canonicalize_claude_model(&requested, &catalog).unwrap_or(requested);
        set_claude_provider_env_model(&mut provider, env_key, &model);
    }

    db.save_provider(AppType::Claude.as_str(), &provider)
        .map_err(|e| e.to_string())?;

    let apps = [AppType::Claude];
    sync_claude_live_after_model_sync(db.clone(), &apps).await?;
    println!("Matpool Claude model slots updated.");
    println!("Run '/model' in Claude Code after restarting or reloading Claude Code if it was already open.");
    Ok(())
}

fn load_matpool_claude_provider(db: &Database) -> CliResult<provider::Provider> {
    db.get_provider_by_id("matpool-claude", AppType::Claude.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "matpool-claude provider is missing. Run: matpool provider seed".to_string())
}

const CLAUDE_MODEL_SLOT_OPTIONS: [(&str, &str); 5] = [
    ("default", "ANTHROPIC_MODEL"),
    ("sonnet", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
    ("opus", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
    ("haiku", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
    ("custom", "ANTHROPIC_CUSTOM_MODEL_OPTION"),
];

fn claude_model_slot_display_name(label: &str) -> &'static str {
    match label {
        "default" => "Claude default",
        "sonnet" => "Claude Sonnet",
        "opus" => "Claude Opus",
        "haiku" => "Claude Haiku",
        "custom" => "Claude custom",
        _ => "Claude model",
    }
}

fn print_claude_model_slots(provider: &provider::Provider) {
    println!("Current Claude model configuration:");
    for (label, env_key) in CLAUDE_MODEL_SLOT_OPTIONS {
        let value = read_claude_provider_env_model(provider, env_key).unwrap_or_else(|| "-".into());
        println!("  {:<16} {}", claude_model_slot_display_name(label), value);
    }
}

fn read_claude_provider_env_model(provider: &provider::Provider, env_key: &str) -> Option<String> {
    provider
        .settings_config
        .pointer(&format!("/env/{env_key}"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

async fn prompt_claude_model_slots_after_takeover(db: Arc<Database>) -> CliResult {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        let provider = load_matpool_claude_provider(db.as_ref())?;
        print_claude_model_slots(&provider);
        println!();
        println!("Change Matpool model IDs later with:");
        println!("  matpool models claude list");
        println!("  matpool models claude set --sonnet <model> --custom <model>");
        return Ok(());
    }

    let mut provider = load_matpool_claude_provider(db.as_ref())?;
    let catalog = claude_catalog_model_names(&provider);
    if catalog.is_empty() {
        print_claude_model_slots(&provider);
        println!();
        println!("No Matpool model catalog is synced yet. Run: matpool models sync claude");
        return Ok(());
    }

    println!();
    print_claude_model_slots(&provider);
    println!();
    println!("These are Matpool model IDs used by Claude Code's /model menu.");
    println!("Press Enter to use the current defaults, or type 'n' to edit them now.");
    print!("Use current Claude model configuration? [Y/n]: ");
    io::stdout()
        .flush()
        .map_err(|e| format!("failed to flush stdout: {e}"))?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|e| format!("failed to read input: {e}"))?;
    let answer = answer.trim();
    if answer.is_empty() || answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
        println!("Keeping current Claude model configuration.");
        println!(
            "Change it later with: matpool models claude set --sonnet <model> --custom <model>"
        );
        return Ok(());
    }
    if !(answer.eq_ignore_ascii_case("n") || answer.eq_ignore_ascii_case("no")) {
        println!("Unrecognized answer; keeping current Claude model configuration.");
        return Ok(());
    }

    println!();
    println!("Enter a Matpool model ID for each Claude menu position.");
    println!("Use Claude custom for an extra model, such as GPT-5.5 or Claude-Fable-5.");
    println!("Press Enter to keep the current value. Type '?' to show available model IDs.");

    let mut changed = false;
    for (label, env_key) in CLAUDE_MODEL_SLOT_OPTIONS {
        let display_name = claude_model_slot_display_name(label);
        let current = read_claude_provider_env_model(&provider, env_key).unwrap_or_default();
        if let Some(model) = prompt_for_claude_model_id(display_name, current.as_str(), &catalog)? {
            set_claude_provider_env_model(&mut provider, env_key, &model);
            db.save_provider(AppType::Claude.as_str(), &provider)
                .map_err(|e| e.to_string())?;
            let apps = [AppType::Claude];
            sync_claude_live_after_model_sync(db.clone(), &apps).await?;
            println!("{display_name} saved: {model}");
            changed = true;
        } else if current.is_empty() {
            println!("{display_name} kept unset.");
        } else {
            println!("{display_name} kept: {current}");
        }
    }

    if changed {
        println!("Claude model configuration updated.");
    } else {
        println!("Claude model configuration unchanged.");
    }
    println!("Run '/model' in Claude Code after restarting or reloading Claude Code if it was already open.");
    Ok(())
}

fn prompt_for_claude_model_id(
    display_name: &str,
    current: &str,
    catalog: &[String],
) -> CliResult<Option<String>> {
    loop {
        print!("{display_name} [{current}]: ");
        io::stdout()
            .flush()
            .map_err(|e| format!("failed to flush stdout: {e}"))?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("failed to read input: {e}"))?;
        let requested = input.trim();
        if requested.is_empty() {
            return Ok(None);
        }
        if requested == "?" {
            print_claude_model_catalog_sample(catalog);
            continue;
        }
        if let Some(model) = canonicalize_claude_model(requested, catalog) {
            return Ok(Some(model));
        }
        println!(
            "Unknown Matpool model ID: {requested}. Type '?' to show available model IDs, or press Enter to keep current."
        );
    }
}

fn print_claude_model_catalog_sample(catalog: &[String]) {
    println!("Available Matpool model IDs:");
    for model in catalog.iter().take(80) {
        println!("  - {model}");
    }
    if catalog.len() > 80 {
        println!(
            "  ... {} more. Run 'matpool models claude list' to see all.",
            catalog.len() - 80
        );
    }
}

fn parse_claude_model_slot_updates(
    args: &[String],
) -> CliResult<Vec<(&'static str, &'static str, String)>> {
    let mut updates = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let Some((label, env_key)) = claude_model_slot_for_option(arg) else {
            return Err(format!("unknown Claude model slot option '{arg}'"));
        };
        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing model value after {arg}"));
        };
        if value.trim().is_empty() || value.starts_with("--") {
            return Err(format!("missing model value after {arg}"));
        }
        updates.push((label, env_key, value.trim().to_string()));
        index += 2;
    }
    Ok(updates)
}

fn claude_model_slot_for_option(option: &str) -> Option<(&'static str, &'static str)> {
    let normalized = option.trim_start_matches('-');
    if normalized == "model" {
        return Some(("default", "ANTHROPIC_MODEL"));
    }
    CLAUDE_MODEL_SLOT_OPTIONS
        .iter()
        .copied()
        .find(|(label, _)| *label == normalized)
}

fn claude_catalog_model_names(provider: &provider::Provider) -> Vec<String> {
    provider
        .settings_config
        .pointer("/modelCatalog/models")
        .and_then(serde_json::Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|entry| entry.get("model").and_then(serde_json::Value::as_str))
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn canonicalize_claude_model(requested: &str, catalog: &[String]) -> Option<String> {
    let requested = requested.trim();
    if requested.is_empty() {
        return None;
    }
    catalog
        .iter()
        .find(|model| model == &requested)
        .cloned()
        .or_else(|| {
            catalog
                .iter()
                .find(|model| model.eq_ignore_ascii_case(requested))
                .cloned()
        })
}

fn set_claude_provider_env_model(provider: &mut provider::Provider, env_key: &str, model: &str) {
    if !provider.settings_config.is_object() {
        provider.settings_config = serde_json::json!({});
    }
    let root = provider
        .settings_config
        .as_object_mut()
        .expect("settings object just initialized");
    let env = root
        .entry("env".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !env.is_object() {
        *env = serde_json::json!({});
    }
    let env_obj = env.as_object_mut().expect("env object just initialized");
    env_obj.insert(
        env_key.to_string(),
        serde_json::Value::String(model.to_string()),
    );
    let name_key = format!("{env_key}_NAME");
    env_obj.insert(name_key, serde_json::Value::String(model.to_string()));
}

fn provider_list(app: Option<&str>) -> CliResult {
    let db = init_db()?;
    let db_state = read_db_state().unwrap_or_default();
    let apps = expand_apps(app.unwrap_or("all"))?;
    let mut any = false;

    for app_key in apps {
        let providers = db.get_all_providers(app_key).map_err(|e| e.to_string())?;
        println!("{app_key}:");
        if providers.is_empty() {
            println!("  - no providers configured");
            continue;
        }
        any = true;
        for provider in providers.values() {
            let current = db_state
                .current_provider(app_key)
                .map(|id| id == provider.id)
                .unwrap_or(false);
            println!(
                "  - {:<22} {:<18} current={} category={}",
                provider.id,
                provider.name,
                current,
                provider.category.as_deref().unwrap_or("-")
            );
        }
    }

    if !any {
        println!("No providers configured.");
    }

    Ok(())
}

async fn takeover(args: &[String]) -> CliResult {
    let Some(app) = args.first().map(String::as_str) else {
        return Err("usage: matpool takeover <app|all> [--disable]".to_string());
    };
    let enabled = !args.iter().any(|arg| arg == "--disable");
    let db = init_db()?;
    let state = AppState::new(db);
    let app_keys = expand_apps(app)?;
    let proxy_app_keys = proxy_takeover_apps(&state.db, &app_keys)?;

    if enabled {
        sync_matpool_models_best_effort(&state.db, &app_keys).await;
        ensure_live_configs_for_apps(&app_keys)?;
        if !proxy_app_keys.is_empty() {
            let db_state = read_db_state().unwrap_or_default();
            if probe_proxy_health(db_state.proxy_addr())?.is_none() {
                return Err(
                    "local proxy daemon is not running. Start it first with: matpool daemon start"
                        .to_string(),
                );
            }
        }
        for app_type in expand_app_types_from_keys(&app_keys)? {
            if let Some(provider_id) = takeover_provider_id(&app_type) {
                switch_cli_provider(&state, app_type, provider_id)?;
            }
        }
    }

    if enabled {
        if proxy_app_keys
            .iter()
            .any(|key| key == AppType::Claude.as_str())
        {
            prepare_claude_matpool_proxy_takeover(&state)?;
        }
        set_takeover_apps(&state, &proxy_app_keys, true).await?;
        if app_keys.contains(&AppType::Claude.as_str())
            && !proxy_app_keys.iter().any(|key| key == "claude")
        {
            enable_claude_direct_takeover(&state).await?;
        }
        if app_keys.contains(&AppType::Claude.as_str()) {
            prompt_claude_model_slots_after_takeover(state.db.clone()).await?;
        }
    } else {
        let disable_app_keys: Vec<String> = app_keys.iter().map(|key| key.to_string()).collect();
        set_takeover_apps(&state, &disable_app_keys, false).await?;
        // 直接模式 claude 的 proxy_config.enabled 本来就是 false，
        // set_takeover_apps 幂等返回不会恢复 settings.json，需要显式处理。
        if app_keys.contains(&AppType::Claude.as_str()) {
            disable_claude_direct_takeover(&state).await?;
        }
    }

    if enabled {
        if proxy_app_keys.is_empty() {
            println!();
            println!("Note: selected apps were synced directly.");
        } else {
            println!();
            println!("Note: takeover points managed tools at the local Matpool proxy.");
            println!("Keep the Matpool daemon or desktop app running while takeover is enabled.");
        }
    }

    Ok(())
}

async fn set_takeover_apps(state: &AppState, apps: &[String], enabled: bool) -> CliResult {
    for app_key in apps {
        state
            .proxy_service
            .set_takeover_for_app_without_managing_proxy(app_key.as_str(), enabled)
            .await?;
        println!(
            "{} takeover {}.",
            app_key,
            if enabled { "enabled" } else { "disabled" }
        );
    }
    Ok(())
}

fn proxy_takeover_apps(db: &Database, apps: &[&str]) -> CliResult<Vec<String>> {
    let mut proxy_apps = Vec::new();
    for app_key in apps {
        if *app_key == AppType::Claude.as_str() && !claude_takeover_target_requires_proxy(db)? {
            continue;
        }
        proxy_apps.push((*app_key).to_string());
    }
    Ok(proxy_apps)
}

fn claude_takeover_target_requires_proxy(db: &Database) -> CliResult<bool> {
    let Some(provider) = db
        .get_provider_by_id("matpool-claude", AppType::Claude.as_str())
        .map_err(|e| e.to_string())?
    else {
        return Ok(true);
    };
    Ok(claude_provider_requires_proxy(&provider))
}

fn current_provider_for_app(db: &Database, app: &AppType) -> CliResult<Option<provider::Provider>> {
    let Some(provider_id) = settings::get_effective_current_provider(db, app)
        .map_err(|e| format!("failed to get current {} provider: {e}", app.as_str()))?
    else {
        return Ok(None);
    };
    db.get_provider_by_id(&provider_id, app.as_str())
        .map_err(|e| format!("failed to load current {} provider: {e}", app.as_str()))
}

fn claude_provider_requires_proxy(provider: &provider::Provider) -> bool {
    if provider.id == "matpool-claude" {
        return true;
    }
    if provider.is_github_copilot() || provider.uses_managed_account_auth() {
        return true;
    }
    if provider
        .meta
        .as_ref()
        .and_then(|meta| meta.is_full_url)
        .unwrap_or(false)
        || provider
            .settings_config
            .get("isFullUrl")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    {
        return true;
    }

    let api_format = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.api_format.as_deref())
        .or_else(|| {
            provider
                .settings_config
                .get("apiFormat")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| {
            provider
                .settings_config
                .get("api_format")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("anthropic");
    matches!(
        api_format,
        "openai_chat" | "openai_responses" | "gemini_native"
    )
}

fn prepare_claude_matpool_proxy_takeover(state: &AppState) -> CliResult {
    if state
        .db
        .get_provider_by_id("matpool-claude", AppType::Claude.as_str())
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("matpool-claude provider is missing. Run: matpool provider seed".to_string());
    }
    sync_default_provider_from_live(state, &AppType::Claude)?;
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "matpool-claude")
        .map_err(|e| format!("failed to set claude current provider: {e}"))?;
    settings::set_current_provider(&AppType::Claude, Some("matpool-claude"))
        .map_err(|e| format!("failed to set local claude current provider: {e}"))?;
    Ok(())
}

async fn enable_claude_direct_takeover(state: &AppState) -> CliResult {
    let proxy_enabled = state
        .db
        .get_proxy_config_for_app(AppType::Claude.as_str())
        .await
        .map(|config| config.enabled)
        .unwrap_or(false);
    if proxy_enabled {
        state
            .proxy_service
            .set_takeover_for_app_without_managing_proxy(AppType::Claude.as_str(), false)
            .await?;
    }
    // 默认切换到 matpool-claude（start 时自动接管 matpool 渠道）。
    let matpool_claude_exists = state
        .db
        .get_provider_by_id("matpool-claude", AppType::Claude.as_str())
        .map_err(|e| e.to_string())?
        .is_some();
    if !matpool_claude_exists {
        return Err("matpool-claude provider is missing. Run: matpool provider seed".to_string());
    }
    switch_claude_provider(state, "matpool-claude").await?;
    println!("claude takeover enabled directly.");
    println!("Switch back to your original config with: matpool provider switch claude default");
    Ok(())
}

async fn disable_claude_direct_takeover(state: &AppState) -> CliResult {
    // 直接模式不写 proxy_config.enabled，set_takeover_apps 的 disable 路径
    // 会因 enabled=false 幂等返回。这里通过切回 default 走用户原本配置
    // （从 backup 恢复 settings.json + 清 backup）。
    switch_claude_provider(state, "default").await
}

/// 切换 claude current provider 并同步 settings.json。
///
/// - 切到 `default`：走用户原本配置。有 backup 则恢复 + 清 backup；无 backup 则
///   settings.json 已是用户配置，不动（透传语义）。
/// - 切到其他 provider：无 backup 则先备份当前 settings.json，再写新 provider 配置；
///   有 backup 则保留最早的原始备份，仅覆写 settings.json 为新 provider 配置。
async fn switch_claude_provider(state: &AppState, provider_id: &str) -> CliResult {
    let provider = state
        .db
        .get_provider_by_id(provider_id, AppType::Claude.as_str())
        .map_err(|e| format!("failed to load claude provider '{provider_id}': {e}"))?
        .ok_or_else(|| {
            format!("claude provider '{provider_id}' not found. Run: matpool provider list claude")
        })?;

    let has_backup = state
        .db
        .get_live_backup(AppType::Claude.as_str())
        .await
        .map(|b| b.is_some())
        .unwrap_or(false);

    if provider_id == "default" {
        if has_backup {
            state
                .proxy_service
                .restore_live_config_for_app_with_fallback(&AppType::Claude)
                .await
                .map_err(|e| format!("failed to restore Claude live config: {e}"))?;
            state
                .db
                .delete_live_backup(AppType::Claude.as_str())
                .await
                .map_err(|e| format!("failed to delete Claude live backup: {e}"))?;
            state
                .db
                .set_current_provider(AppType::Claude.as_str(), provider_id)
                .map_err(|e| format!("failed to set claude current provider: {e}"))?;
            // 恢复后 settings.json 已是用户原始配置，同步到 `default` 的 settings_config，
            // 让 `provider list` 显示与 live 一致的内容。
            sync_default_provider_from_live(state, &AppType::Claude)?;
            println!("Switched claude to default; settings.json restored to your original config.");
        } else {
            state
                .db
                .set_current_provider(AppType::Claude.as_str(), provider_id)
                .map_err(|e| format!("failed to set claude current provider: {e}"))?;
            println!(
                "Switched claude to default; settings.json unchanged (no takeover was active)."
            );
        }
    } else {
        if claude_provider_requires_proxy(&provider) {
            return Err(format!(
                "provider '{provider_id}' requires the local proxy; use the desktop app to switch"
            ));
        }
        if !has_backup {
            // 首次接管：当前 settings.json 是用户原始配置，先同步到 `default` provider
            // 的 settings_config（让 `provider list` 显示最新），再备份 + 覆写。
            sync_default_provider_from_live(state, &AppType::Claude)?;
            state
                .proxy_service
                .backup_live_config_strict(&AppType::Claude)
                .await
                .map_err(|e| format!("failed to backup Claude live config: {e}"))?;
        }
        let live_provider =
            services::matpool_inject::provider_with_injected_matpool_token(&provider)
                .unwrap_or(provider);
        services::provider::write_live_with_common_config(
            state.db.as_ref(),
            &AppType::Claude,
            &live_provider,
        )
        .map_err(|e| format!("failed to write Claude live config: {e}"))?;
        state
            .db
            .set_current_provider(AppType::Claude.as_str(), provider_id)
            .map_err(|e| format!("failed to set claude current provider: {e}"))?;
        println!("Switched claude to '{provider_id}'.");
    }

    Ok(())
}

async fn sync_claude_live_from_current_provider(db: &Database) -> CliResult {
    let Some(provider) = current_provider_for_app(db, &AppType::Claude)? else {
        return Err("Claude current provider is not configured".to_string());
    };
    if claude_provider_requires_proxy(&provider) {
        return Err("Claude current provider requires the local proxy".to_string());
    }
    let live_provider = services::matpool_inject::provider_with_injected_matpool_token(&provider)
        .unwrap_or(provider);
    services::provider::write_live_with_common_config(db, &AppType::Claude, &live_provider)
        .map_err(|e| format!("failed to write Claude live config: {e}"))?;
    Ok(())
}

async fn daemon(args: &[String]) -> CliResult {
    let command = args.first().map(String::as_str).unwrap_or("status");
    match command {
        "status" => daemon_status(),
        "run" => daemon_run_foreground().await,
        "install" => daemon_install(),
        "start" => daemon_start_service(),
        "stop" => daemon_stop_service(),
        "restart" => {
            let _ = daemon_stop_service();
            daemon_start_service()
        }
        "uninstall" => daemon_uninstall(),
        "logs" => daemon_logs(),
        unknown => Err(format!("unknown daemon command '{unknown}'")),
    }
}

async fn proxy(args: &[String]) -> CliResult {
    let command = args.first().map(String::as_str).unwrap_or("start");
    match command {
        "start" | "run" => daemon_run_foreground().await,
        "status" => daemon_status(),
        unknown => Err(format!("unknown proxy command '{unknown}'")),
    }
}

fn daemon_status() -> CliResult {
    let state = read_db_state().unwrap_or_default();
    match probe_proxy_health(state.proxy_addr())? {
        Some(addr) => println!("Matpool proxy is running at {addr}."),
        None => {
            if let Some(addr) = state.proxy_addr() {
                println!("Matpool proxy is not running at {addr}.");
            } else {
                println!("Matpool proxy is not running.");
            }
        }
    }
    Ok(())
}

async fn daemon_run_foreground() -> CliResult {
    let db = init_db()?;
    let state = AppState::new(db);
    let info = state.proxy_service.start().await?;
    #[cfg(target_os = "windows")]
    write_daemon_pid_file(std::process::id())?;
    println!(
        "Matpool proxy daemon running at {}:{}.",
        info.address, info.port
    );
    println!("Press Ctrl+C to stop.");

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("failed to wait for Ctrl+C: {e}"))?;

    println!();
    println!("Stopping Matpool proxy daemon...");
    // 前台 daemon 退出时恢复 Live 配置，和桌面 App 退出对齐。
    // stop_with_restore_keep_state 会停止代理 + 从 backup 恢复
    // settings.json/auth.json/.env，但保留 proxy_config.enabled 状态，
    // 下次桌面 App 启动时可自动重新接管。
    let stop_result = state.proxy_service.stop_with_restore_keep_state().await;
    if stop_result.is_ok() {
        println!("Matpool proxy daemon stopped; live configs restored.");
    }
    #[cfg(target_os = "windows")]
    let _ = remove_daemon_pid_file();
    stop_result
}

#[cfg(target_os = "macos")]
const DAEMON_LABEL: &str = "com.matpool.switch.daemon";

#[cfg(target_os = "windows")]
const WINDOWS_TASK_NAME: &str = r"\MatpoolSwitchDaemon";

#[cfg(target_os = "windows")]
const WINDOWS_RUN_VALUE_NAME: &str = "MatpoolSwitchDaemon";

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(target_os = "windows")]
const DETACHED_PROCESS: u32 = 0x00000008;

#[cfg(target_os = "macos")]
fn daemon_install() -> CliResult {
    let plist_path = macos_launch_agent_plist_path()?;
    let log_dir = get_app_config_dir().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| format!("failed to create log dir: {e}"))?;
    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create LaunchAgents dir: {e}"))?;
    }

    let exe = env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
    let plist = macos_launch_agent_plist(&exe, &log_dir);
    fs::write(&plist_path, plist).map_err(|e| format!("failed to write plist: {e}"))?;

    println!("Installed LaunchAgent: {}", plist_path.display());
    println!("Run: matpool daemon start");
    Ok(())
}

#[cfg(target_os = "linux")]
fn daemon_install() -> CliResult {
    let service_path = linux_systemd_service_path()?;
    if let Some(parent) = service_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create systemd user dir: {e}"))?;
    }

    let exe = env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
    let service = linux_systemd_service(&exe);
    fs::write(&service_path, service).map_err(|e| format!("failed to write service: {e}"))?;

    run_systemctl_user(&["daemon-reload"])?;
    run_systemctl_user(&["enable", linux_systemd_service_name()])?;

    println!("Installed systemd user service: {}", service_path.display());
    println!("Run: matpool daemon start");
    Ok(())
}

#[cfg(target_os = "windows")]
fn daemon_install() -> CliResult {
    let log_dir = get_app_config_dir().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| format!("failed to create log dir: {e}"))?;
    let exe = env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
    let script_path = windows_daemon_script_path();
    let script = windows_daemon_script(&exe, &log_dir);
    fs::write(&script_path, script)
        .map_err(|e| format!("failed to write Windows daemon launcher script: {e}"))?;

    let task_xml_path = windows_task_xml_path();
    let task_xml = windows_task_xml(&script_path);
    write_windows_task_xml(&task_xml_path, &task_xml)?;

    let task_xml_arg = task_xml_path.to_string_lossy().to_string();
    run_schtasks(&[
        "/Create",
        "/TN",
        WINDOWS_TASK_NAME,
        "/XML",
        &task_xml_arg,
        "/F",
    ])?;
    let _ = windows_delete_run_key();

    println!("Installed hidden Windows scheduled task: {WINDOWS_TASK_NAME}");
    println!("Run: matpool daemon start");
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn daemon_install() -> CliResult {
    Err("daemon install is currently implemented for macOS, Linux, and Windows only".to_string())
}

#[cfg(target_os = "macos")]
fn daemon_start_service() -> CliResult {
    let plist_path = macos_launch_agent_plist_path()?;
    if !plist_path.exists() {
        return Err("daemon is not installed. Run: matpool daemon install".to_string());
    }

    let target = macos_launchctl_target()?;
    let bootstrap = Command::new("launchctl")
        .args(["bootstrap", &target])
        .arg(&plist_path)
        .output()
        .map_err(|e| format!("failed to run launchctl bootstrap: {e}"))?;
    if !bootstrap.status.success() {
        let stderr = String::from_utf8_lossy(&bootstrap.stderr);
        if !stderr.contains("Bootstrap failed: 5") && !stderr.contains("already bootstrapped") {
            return Err(format!("launchctl bootstrap failed: {}", stderr.trim()));
        }
    }

    run_launchctl(&["enable", &format!("{target}/{DAEMON_LABEL}")])?;
    run_launchctl(&["kickstart", "-k", &format!("{target}/{DAEMON_LABEL}")])?;
    println!("Matpool daemon started.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn daemon_start_service() -> CliResult {
    run_systemctl_user(&["start", linux_systemd_service_name()])?;
    println!("Matpool daemon started.");
    Ok(())
}

#[cfg(target_os = "windows")]
fn daemon_start_service() -> CliResult {
    if let Some(addr) =
        probe_proxy_health(read_db_state().ok().and_then(|state| state.proxy_addr()))?
    {
        println!("Matpool daemon already running at {addr}.");
        return Ok(());
    }

    windows_spawn_daemon_detached()?;
    println!("Matpool daemon started.");
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn daemon_start_service() -> CliResult {
    Err(
        "daemon start service is currently implemented for macOS, Linux, and Windows only; use matpool daemon run"
            .to_string(),
    )
}

#[cfg(target_os = "macos")]
fn daemon_stop_service() -> CliResult {
    let target = format!("{}/{}", macos_launchctl_target()?, DAEMON_LABEL);
    let output = Command::new("launchctl")
        .args(["bootout", &target])
        .output()
        .map_err(|e| format!("failed to run launchctl bootout: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("No such process") && !stderr.contains("Could not find service") {
            return Err(format!("launchctl bootout failed: {}", stderr.trim()));
        }
    }
    println!("Matpool daemon stopped.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn daemon_stop_service() -> CliResult {
    run_systemctl_user(&["stop", linux_systemd_service_name()])?;
    std::thread::sleep(Duration::from_millis(300));

    let state = read_db_state().unwrap_or_default();
    if let Some(addr) = probe_proxy_health(state.proxy_addr())? {
        return Err(format!(
            "systemd service stopped, but another process is still listening at {addr}; find it with: ss -lntp | grep ':{}'",
            addr.port()
        ));
    }

    println!("Matpool daemon stopped.");
    Ok(())
}

#[cfg(target_os = "windows")]
fn daemon_stop_service() -> CliResult {
    let _ = Command::new("schtasks.exe")
        .args(["/End", "/TN", WINDOWS_TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run schtasks.exe /End: {e}"))?;

    let _ = windows_kill_daemon_pid()?;
    std::thread::sleep(Duration::from_millis(300));

    let state = read_db_state().unwrap_or_default();
    if let Some(addr) = probe_proxy_health(state.proxy_addr())? {
        return Err(format!("Matpool proxy is still running at {addr}."));
    }

    println!("Matpool daemon stopped.");
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn daemon_stop_service() -> CliResult {
    Err(
        "daemon stop service is currently implemented for macOS, Linux, and Windows only"
            .to_string(),
    )
}

#[cfg(target_os = "macos")]
fn daemon_uninstall() -> CliResult {
    let _ = daemon_stop_service();
    let plist_path = macos_launch_agent_plist_path()?;
    if plist_path.exists() {
        fs::remove_file(&plist_path).map_err(|e| format!("failed to remove plist: {e}"))?;
        println!("Removed LaunchAgent: {}", plist_path.display());
    } else {
        println!("LaunchAgent is not installed.");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn daemon_uninstall() -> CliResult {
    let _ = run_systemctl_user(&["disable", "--now", linux_systemd_service_name()]);
    let service_path = linux_systemd_service_path()?;
    if service_path.exists() {
        fs::remove_file(&service_path).map_err(|e| format!("failed to remove service: {e}"))?;
        println!("Removed systemd user service: {}", service_path.display());
    } else {
        println!("systemd user service is not installed.");
    }
    let _ = run_systemctl_user(&["daemon-reload"]);
    Ok(())
}

#[cfg(target_os = "windows")]
fn daemon_uninstall() -> CliResult {
    let _ = daemon_stop_service();
    windows_delete_run_key()?;
    let output = Command::new("schtasks.exe")
        .args(["/Delete", "/TN", WINDOWS_TASK_NAME, "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run schtasks.exe /Delete: {e}"))?;
    if output.status.success() {
        println!("Removed Windows scheduled task: {WINDOWS_TASK_NAME}");
        let _ = fs::remove_file(windows_task_xml_path());
        let _ = fs::remove_file(windows_daemon_script_path());
        Ok(())
    } else if !windows_task_exists() {
        println!("Windows scheduled task is not installed.");
        let _ = fs::remove_file(windows_task_xml_path());
        let _ = fs::remove_file(windows_daemon_script_path());
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = format!("{} {}", stdout.trim(), stderr.trim());
        Err(format!("schtasks /Delete failed: {}", message.trim()))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn daemon_uninstall() -> CliResult {
    Err("daemon uninstall is currently implemented for macOS, Linux, and Windows only".to_string())
}

#[cfg(not(target_os = "linux"))]
fn daemon_logs() -> CliResult {
    let log_dir = get_app_config_dir().join("logs");
    println!("stdout: {}", log_dir.join("daemon.out.log").display());
    println!("stderr: {}", log_dir.join("daemon.err.log").display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn daemon_logs() -> CliResult {
    println!(
        "journal: journalctl --user -u {}",
        linux_systemd_service_name()
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_launch_agent_plist_path() -> CliResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| "failed to resolve home directory".to_string())?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{DAEMON_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn macos_launch_agent_plist(exe: &std::path::Path, log_dir: &std::path::Path) -> String {
    let exe = xml_escape(&exe.to_string_lossy());
    let stdout = xml_escape(&log_dir.join("daemon.out.log").to_string_lossy());
    let stderr = xml_escape(&log_dir.join("daemon.err.log").to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{DAEMON_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>daemon</string>
    <string>run</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{stdout}</string>
  <key>StandardErrorPath</key>
  <string>{stderr}</string>
  <key>WorkingDirectory</key>
  <string>{}</string>
</dict>
</plist>
"#,
        xml_escape(&get_app_config_dir().to_string_lossy())
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn macos_launchctl_target() -> CliResult<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|e| format!("failed to run id -u: {e}"))?;
    if !output.status.success() {
        return Err("failed to resolve current uid".to_string());
    }
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(format!("gui/{uid}"))
}

#[cfg(target_os = "macos")]
fn run_launchctl(args: &[&str]) -> CliResult {
    let output = Command::new("launchctl")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run launchctl {}: {e}", args.join(" ")))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "launchctl {} failed: {}",
            args.join(" "),
            stderr.trim()
        ))
    }
}

#[cfg(target_os = "linux")]
fn linux_systemd_service_name() -> &'static str {
    "com.matpool.switch.daemon.service"
}

#[cfg(target_os = "linux")]
fn linux_systemd_service_path() -> CliResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| "failed to resolve home directory".to_string())?;
    Ok(home
        .join(".config")
        .join("systemd")
        .join("user")
        .join(linux_systemd_service_name()))
}

#[cfg(target_os = "linux")]
fn linux_systemd_service(exe: &std::path::Path) -> String {
    format!(
        r#"[Unit]
Description=Matpool Switch local proxy daemon
After=network-online.target

[Service]
Type=simple
ExecStart={} daemon run
Restart=always
RestartSec=3
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
"#,
        systemd_quote_path(exe)
    )
}

#[cfg(target_os = "linux")]
fn systemd_quote_path(path: &std::path::Path) -> String {
    format!("\"{}\"", systemd_unit_escape(&path.to_string_lossy()))
}

#[cfg(target_os = "linux")]
fn systemd_unit_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

#[cfg(target_os = "linux")]
fn run_systemctl_user(args: &[&str]) -> CliResult {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run systemctl --user {}: {e}", args.join(" ")))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "systemctl --user {} failed: {}",
            args.join(" "),
            stderr.trim()
        ))
    }
}

#[cfg(target_os = "windows")]
fn run_schtasks(args: &[&str]) -> CliResult {
    let output = Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run schtasks.exe {}: {e}", args.join(" ")))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "schtasks {} failed: {} {}",
            args.join(" "),
            stdout.trim(),
            stderr.trim()
        ))
    }
}

#[cfg(target_os = "windows")]
fn windows_run_key() -> CliResult<RegKey> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| format!("failed to open HKCU Run key: {e}"))?;
    Ok(key)
}

#[cfg(target_os = "windows")]
fn windows_delete_run_key() -> CliResult {
    match windows_run_key()?.delete_value(WINDOWS_RUN_VALUE_NAME) {
        Ok(()) => {
            println!(
                "Removed user login launcher: HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\\{WINDOWS_RUN_VALUE_NAME}"
            );
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("failed to delete HKCU Run key: {err}")),
    }
}

#[cfg(target_os = "windows")]
fn daemon_pid_path() -> PathBuf {
    get_app_config_dir().join("daemon.pid")
}

#[cfg(target_os = "windows")]
fn write_daemon_pid_file(pid: u32) -> CliResult {
    let config_dir = get_app_config_dir();
    fs::create_dir_all(&config_dir).map_err(|e| format!("failed to create config dir: {e}"))?;
    fs::write(daemon_pid_path(), pid.to_string())
        .map_err(|e| format!("failed to write daemon pid file: {e}"))
}

#[cfg(target_os = "windows")]
fn read_daemon_pid_file() -> CliResult<Option<u32>> {
    let path = daemon_pid_path();
    if !path.exists() {
        return Ok(None);
    }
    let value =
        fs::read_to_string(&path).map_err(|e| format!("failed to read daemon pid file: {e}"))?;
    let pid = value
        .trim()
        .parse::<u32>()
        .map_err(|e| format!("invalid daemon pid file {}: {e}", path.display()))?;
    Ok(Some(pid))
}

#[cfg(target_os = "windows")]
fn remove_daemon_pid_file() -> CliResult {
    let path = daemon_pid_path();
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("failed to remove daemon pid file: {err}")),
    }
}

#[cfg(target_os = "windows")]
fn windows_kill_daemon_pid() -> CliResult<bool> {
    let Some(pid) = read_daemon_pid_file()? else {
        return Ok(false);
    };

    if pid == std::process::id() {
        let _ = remove_daemon_pid_file();
        return Err("daemon pid file points to the current CLI process".to_string());
    }

    if !windows_pid_is_matpool(pid)? {
        let _ = remove_daemon_pid_file();
        return Ok(false);
    }

    let output = Command::new("taskkill.exe")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run taskkill.exe: {e}"))?;
    if output.status.success() {
        let _ = remove_daemon_pid_file();
        Ok(true)
    } else if !windows_pid_is_matpool(pid)? {
        let _ = remove_daemon_pid_file();
        Ok(false)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = format!("{} {}", stdout.trim(), stderr.trim());
        Err(format!("taskkill failed: {}", message.trim()))
    }
}

#[cfg(target_os = "windows")]
fn windows_pid_is_matpool(pid: u32) -> CliResult<bool> {
    let filter = format!("PID eq {pid}");
    let output = Command::new("tasklist.exe")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run tasklist.exe: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = format!("{} {}", stdout.trim(), stderr.trim());
        return Err(format!("tasklist failed: {}", message.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .to_ascii_lowercase()
        .contains("matpool.exe"))
}

#[cfg(target_os = "windows")]
fn windows_task_exists() -> bool {
    Command::new("schtasks.exe")
        .args(["/Query", "/TN", WINDOWS_TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn windows_spawn_daemon_detached() -> CliResult {
    let log_dir = get_app_config_dir().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| format!("failed to create log dir: {e}"))?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("daemon.out.log"))
        .map_err(|e| format!("failed to open daemon stdout log: {e}"))?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("daemon.err.log"))
        .map_err(|e| format!("failed to open daemon stderr log: {e}"))?;
    let exe = env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;

    Command::new(exe)
        .args(["daemon", "run"])
        .current_dir(get_app_config_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|e| format!("failed to start Matpool daemon: {e}"))?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_task_xml_path() -> PathBuf {
    get_app_config_dir().join("daemon-task.xml")
}

#[cfg(target_os = "windows")]
fn write_windows_task_xml(path: &std::path::Path, xml: &str) -> CliResult {
    let mut bytes = Vec::with_capacity(2 + xml.len() * 2);
    bytes.extend_from_slice(&[0xFF, 0xFE]);
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(path, bytes).map_err(|e| format!("failed to write Windows scheduled task XML: {e}"))
}

#[cfg(target_os = "windows")]
fn windows_daemon_script_path() -> PathBuf {
    get_app_config_dir().join("daemon-launcher.vbs")
}

#[cfg(target_os = "windows")]
fn vbs_escape(value: &str) -> String {
    value.replace('"', "\"\"")
}

#[cfg(target_os = "windows")]
fn windows_daemon_script(exe: &std::path::Path, log_dir: &std::path::Path) -> String {
    let working_dir = vbs_escape(&get_app_config_dir().to_string_lossy());
    let command = vbs_escape(&format!(
        r#"cmd.exe /D /C ""{}" daemon run >> "{}" 2>> "{}"""#,
        exe.display(),
        log_dir.join("daemon.out.log").display(),
        log_dir.join("daemon.err.log").display()
    ));

    format!(
        r#"Option Explicit
Dim shell
Set shell = CreateObject("WScript.Shell")
shell.CurrentDirectory = "{working_dir}"
shell.Run "{command}", 0, True
"#
    )
}

#[cfg(target_os = "windows")]
fn windows_task_user_id() -> Option<String> {
    let username = env::var("USERNAME")
        .ok()
        .filter(|value| !value.is_empty())?;
    match env::var("USERDOMAIN")
        .ok()
        .filter(|value| !value.is_empty())
    {
        Some(domain) => Some(format!(r"{domain}\{username}")),
        None => Some(username),
    }
}

#[cfg(target_os = "windows")]
fn windows_task_xml(script_path: &std::path::Path) -> String {
    let working_dir = xml_escape(&get_app_config_dir().to_string_lossy());
    let args = xml_escape(&format!(r#""{}""#, script_path.display()));
    let user_id = windows_task_user_id()
        .map(|value| format!("\n      <UserId>{}</UserId>", xml_escape(&value)))
        .unwrap_or_default();

    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Matpool Switch local proxy daemon</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">{user_id}
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <Enabled>true</Enabled>
    <Hidden>true</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>wscript.exe</Command>
      <Arguments>{args}</Arguments>
      <WorkingDirectory>{working_dir}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#
    )
}

async fn setup(args: &[String]) -> CliResult {
    let options = SetupOptions::parse(args)?;
    if options.dry_run {
        println!("Matpool setup plan:");
        if options.skip_login {
            println!("  - login: skipped");
        } else if options.token.is_some() {
            println!("  - login: save provided token");
        } else {
            println!("  - login: use existing token or prompt for one");
        }
        println!(
            "  - daemon: {}",
            if options.skip_daemon {
                "skipped".to_string()
            } else {
                "install and start background daemon".to_string()
            }
        );
        println!(
            "  - takeover: {}",
            if options.skip_takeover {
                "skipped".to_string()
            } else {
                format!("enable for {}", options.apps.join(","))
            }
        );
        return Ok(());
    }

    if !options.skip_login {
        if let Some(token) = options.token.as_deref() {
            matpool_keychain_set(token.to_string())?;
            println!("Matpool token saved to OS keychain.");
        } else if matpool_keychain_get()?.is_some() {
            println!("Matpool token already configured.");
        } else {
            login(&[])?;
        }
    }

    let setup_db = if options.skip_takeover {
        None
    } else {
        Some(init_db()?)
    };
    let proxy_app_keys = if let Some(db) = setup_db.as_ref() {
        proxy_takeover_apps(db, &options.apps)?
    } else {
        Vec::new()
    };
    let daemon_required =
        !options.skip_daemon && (options.skip_takeover || !proxy_app_keys.is_empty());

    if daemon_required {
        daemon_install()?;
        daemon_start_service()?;
        std::thread::sleep(Duration::from_millis(500));
    } else if !options.skip_daemon {
        println!("Matpool daemon skipped; selected takeover apps do not require the local proxy.");
    }

    if let Some(db) = setup_db {
        sync_matpool_models_best_effort(&db, &options.apps).await;
        ensure_live_configs_for_apps(&options.apps)?;
        if !proxy_app_keys.is_empty() {
            let db_state = read_db_state().unwrap_or_default();
            if probe_proxy_health(db_state.proxy_addr())?.is_none() {
                return Err(
                    "local proxy daemon is not running. Run: matpool daemon start, then retry setup"
                        .to_string(),
                );
            }
        }
        let state = AppState::new(db);
        if proxy_app_keys
            .iter()
            .any(|key| key == AppType::Claude.as_str())
        {
            prepare_claude_matpool_proxy_takeover(&state)?;
        }
        set_takeover_apps(&state, &proxy_app_keys, true).await?;
        if options.apps.contains(&AppType::Claude.as_str())
            && !proxy_app_keys.iter().any(|key| key == "claude")
        {
            enable_claude_direct_takeover(&state).await?;
        }
    }

    println!();
    println!("Matpool setup complete.");
    Ok(())
}

fn ensure_live_configs_for_apps(apps: &[&str]) -> CliResult {
    for app_key in apps {
        match *app_key {
            "claude" => ensure_claude_live_config()?,
            "codex" => ensure_codex_live_config()?,
            "gemini" => ensure_gemini_live_config()?,
            _ => {}
        }
    }
    Ok(())
}

fn ensure_claude_live_config() -> CliResult {
    let path = get_claude_settings_path();
    if !path.exists() {
        write_json_file(&path, &serde_json::json!({}))
            .map_err(|e| format!("failed to create Claude settings: {e}"))?;
        println!("Created Claude settings: {}", path.display());
    }
    Ok(())
}

fn ensure_codex_live_config() -> CliResult {
    let auth_path = get_codex_auth_path();
    if !auth_path.exists() {
        write_json_file(&auth_path, &serde_json::json!({}))
            .map_err(|e| format!("failed to create Codex auth: {e}"))?;
        println!("Created Codex auth: {}", auth_path.display());
    }

    let config_path = get_codex_config_path();
    if !config_path.exists() {
        write_text_file(&config_path, "")
            .map_err(|e| format!("failed to create Codex config: {e}"))?;
        println!("Created Codex config: {}", config_path.display());
    }
    Ok(())
}

fn ensure_gemini_live_config() -> CliResult {
    let path = get_gemini_env_path();
    if !path.exists() {
        write_text_file(&path, "").map_err(|e| format!("failed to create Gemini env: {e}"))?;
        println!("Created Gemini env: {}", path.display());
    }
    Ok(())
}

fn init_db() -> CliResult<Arc<Database>> {
    let db = Database::init().map_err(|e| e.to_string())?;
    db.init_default_official_providers()
        .map_err(|e| e.to_string())?;
    ensure_default_current_providers(&db)?;
    let db = Arc::new(db);
    // CLI 环境下也确保 `default` provider 存在。桌面 App 在 lib.rs 启动时会从
    // live settings.json 自动导入，但纯 CLI 用户（headless / SSH）可能从未启动
    // 过桌面 App，DB 里就缺 `default`，导致 `matpool provider switch claude default`
    // 报 "provider not found"。这里补上导入。
    let state = AppState::new(db.clone());
    ensure_default_provider_exists(&state)?;
    // 同步 `default` provider 的 settings_config 与 live settings.json，
    // 让 `matpool provider list` 显示最新配置（覆盖用户手动改 settings.json 的场景）。
    sync_default_provider_from_live(&state, &AppType::Claude)?;
    Ok(db)
}

/// 确保 `default` provider 存在（claude/codex/gemini）。
///
/// 跳过条件：
/// - `default` 已存在
/// - live 配置当前被代理接管（含占位符，不是用户真实配置）
/// - `import_default_config` 内部的 `has_non_official_seed_provider` 守卫
///
/// 导入失败时分情况：live 配置文件不存在视为"用户没用这个 app"，静默跳过；
/// 其他错误打 stderr 但不阻塞 CLI。
fn ensure_default_provider_exists(state: &AppState) -> CliResult<()> {
    for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
        if state
            .db
            .get_provider_by_id("default", app.as_str())
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        if state
            .proxy_service
            .detect_takeover_in_live_config_for_app(&app)
        {
            continue;
        }
        match services::provider::import_default_config(state, app.clone()) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                let missing = msg.contains("不存在")
                    || msg.contains("is missing")
                    || msg.contains("configuration is missing");
                if !missing {
                    eprintln!(
                        "[matpool] skipped importing 'default' {} provider: {msg}",
                        app.as_str()
                    );
                }
            }
        }
    }
    Ok(())
}

/// 同步 `default` provider 的 settings_config 与 live settings.json 最新内容。
///
/// 用于让 `matpool provider list` 显示最新的用户配置（否则 `default` 是一次性快照，
/// 用户手动改 settings.json 后 DB 里仍是旧值）。
///
/// 跳过条件（任何一条都不同步）：
/// - `default` provider 不存在
/// - live 配置含代理占位符（处于代理接管状态）
/// - `proxy_live_backup` 表有记录（处于直接接管状态，settings.json 是 matpool 配置而非用户原始配置）
/// - settings.json 不存在或不可读
fn sync_default_provider_from_live(state: &AppState, app_type: &AppType) -> CliResult<()> {
    if *app_type != AppType::Claude {
        return Ok(());
    }
    let Some(existing) = state
        .db
        .get_provider_by_id("default", app_type.as_str())
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };
    if state
        .proxy_service
        .detect_takeover_in_live_config_for_app(app_type)
    {
        return Ok(());
    }
    if state
        .db
        .has_live_backup_sync(app_type.as_str())
        .map_err(|e| e.to_string())?
    {
        return Ok(());
    }
    let settings_path = get_claude_settings_path();
    if !settings_path.exists() {
        return Ok(());
    }
    let mut live: serde_json::Value =
        crate::config::read_json_file(&settings_path).map_err(|e| e.to_string())?;
    let _ = services::provider::normalize_claude_models_in_value(&mut live);
    if existing.settings_config == live {
        return Ok(());
    }
    state
        .db
        .update_provider_settings_config(app_type.as_str(), "default", &live)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn ensure_provider_seeds() -> CliResult<usize> {
    let db = Database::init().map_err(|e| e.to_string())?;
    let inserted = db
        .init_default_official_providers()
        .map_err(|e| e.to_string())?;
    ensure_default_current_providers(&db)?;
    Ok(inserted)
}

async fn sync_matpool_models_best_effort(db: &Database, apps: &[&str]) {
    let Ok(app_types) = expand_app_types_from_keys(apps) else {
        return;
    };
    match services::matpool_models::sync_matpool_models_for_apps(db, &app_types).await {
        Ok(outcome) => {
            if !outcome.updated.is_empty() {
                print_model_sync_outcome(&outcome);
            }
        }
        Err(err) => {
            eprintln!("warning: failed to sync Matpool models: {err}");
        }
    }
}

async fn sync_claude_live_after_model_sync(db: Arc<Database>, apps: &[AppType]) -> CliResult {
    if !apps.iter().any(|app| matches!(app, AppType::Claude)) {
        return Ok(());
    }

    let effective_current_provider =
        settings::get_effective_current_provider(&db, &AppType::Claude)
            .map_err(|e| e.to_string())?;
    let db_current_provider = db
        .get_current_provider(AppType::Claude.as_str())
        .map_err(|e| e.to_string())?;

    if effective_current_provider.as_deref() != Some("matpool-claude")
        && db_current_provider.as_deref() != Some("matpool-claude")
    {
        return Ok(());
    }
    if db_current_provider.as_deref() == Some("matpool-claude")
        && effective_current_provider.as_deref() != Some("matpool-claude")
    {
        settings::set_current_provider(&AppType::Claude, Some("matpool-claude"))
            .map_err(|e| e.to_string())?;
    }

    let Some(provider) = db
        .get_provider_by_id("matpool-claude", AppType::Claude.as_str())
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };

    let proxy_enabled = db
        .get_proxy_config_for_app(AppType::Claude.as_str())
        .await
        .map(|config| config.enabled)
        .unwrap_or(false);

    if proxy_enabled {
        let state = AppState::new(db);
        state
            .proxy_service
            .sync_claude_live_from_provider_while_proxy_active(&provider)
            .await?;
    } else if !claude_provider_requires_proxy(&provider) {
        let live_provider =
            services::matpool_inject::provider_with_injected_matpool_token(&provider)
                .unwrap_or(provider);
        services::provider::write_live_with_common_config(
            db.as_ref(),
            &AppType::Claude,
            &live_provider,
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn sync_codex_live_after_model_sync(db: Arc<Database>, apps: &[AppType]) -> CliResult {
    if !apps.iter().any(|app| matches!(app, AppType::Codex)) {
        return Ok(());
    }

    let Some(current_provider_id) = db
        .get_current_provider(AppType::Codex.as_str())
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };
    if current_provider_id != "matpool-codex" {
        return Ok(());
    }

    let Some(provider) = db
        .get_provider_by_id(&current_provider_id, AppType::Codex.as_str())
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };

    let proxy_enabled = db
        .get_proxy_config_for_app(AppType::Codex.as_str())
        .await
        .map(|config| config.enabled)
        .unwrap_or(false);

    if proxy_enabled {
        let state = AppState::new(db);
        state
            .proxy_service
            .sync_codex_live_from_provider_while_proxy_active(&provider)
            .await?;
    } else {
        let settings = provider
            .settings_config
            .as_object()
            .ok_or_else(|| "Codex provider config must be a JSON object".to_string())?;
        let auth = settings
            .get("auth")
            .ok_or_else(|| "Codex provider config is missing auth".to_string())?;
        let config_text = settings.get("config").and_then(serde_json::Value::as_str);
        crate::codex_config::write_codex_provider_live_with_catalog(
            &provider.settings_config,
            provider.category.as_deref(),
            auth,
            config_text,
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn print_model_sync_outcome(outcome: &services::matpool_models::MatpoolModelSyncOutcome) {
    println!(
        "Matpool models synced. fetched={} chat_capable={} updated={}",
        outcome.fetched,
        outcome.chat_capable,
        if outcome.updated.is_empty() {
            "-".to_string()
        } else {
            outcome.updated.join(",")
        }
    );
    for (app, model) in &outcome.defaults {
        println!("  {app}: default_model={model}");
    }
}

fn ensure_default_current_providers(db: &Database) -> CliResult {
    for (app, provider_id) in [
        (AppType::Claude, "matpool-claude"),
        (AppType::Codex, "matpool-codex"),
        (AppType::Gemini, "matpool-gemini"),
    ] {
        let app_key = app.as_str();
        if db
            .get_current_provider(app_key)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        if db
            .get_provider_by_id(provider_id, app_key)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            db.set_current_provider(app_key, provider_id)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct ReadOnlyDbState {
    current_providers: Vec<(String, String)>,
    takeover_enabled: Vec<String>,
    direct_takeover_apps: Vec<String>,
    proxy_address: Option<String>,
    proxy_port: Option<u16>,
}

impl ReadOnlyDbState {
    fn current_provider(&self, app_key: &str) -> Option<&str> {
        self.current_providers
            .iter()
            .find_map(|(app, provider)| (app == app_key).then_some(provider.as_str()))
    }

    fn takeover_enabled(&self, app_key: &str) -> bool {
        self.takeover_enabled.iter().any(|app| app == app_key)
    }

    fn any_takeover_enabled(&self) -> bool {
        !self.takeover_enabled.is_empty()
    }

    fn local_proxy_required(&self) -> bool {
        if self.any_takeover_enabled() {
            return true;
        }
        self.current_provider(AppType::Claude.as_str()) == Some("matpool-claude")
            && self
                .direct_takeover_apps
                .iter()
                .any(|app| app == AppType::Claude.as_str())
    }

    /// 返回 takeover 状态字符串：
    /// - `"true"`: 代理模式接管（proxy_config.enabled=1）
    /// - `"direct"`: 直接模式接管（无本地代理，但 settings.json 已被覆写，backup 存在）
    /// - `"false"`: 未接管
    fn takeover_state(&self, app_key: &str) -> &'static str {
        if self.takeover_enabled(app_key) {
            "true"
        } else if self.direct_takeover_apps.iter().any(|app| app == app_key) {
            "direct"
        } else {
            "false"
        }
    }

    fn proxy_addr(&self) -> Option<SocketAddr> {
        let address = self.proxy_address.as_deref().unwrap_or("127.0.0.1");
        let port = self.proxy_port.unwrap_or(15721);
        format!("{address}:{port}").parse().ok()
    }
}

fn read_db_state() -> CliResult<ReadOnlyDbState> {
    let db_path = get_app_config_dir().join("matpool-switch.db");
    if !db_path.exists() {
        return Ok(ReadOnlyDbState::default());
    }

    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;

    let current_providers = query_current_providers(&conn)?;
    let takeover_enabled = query_takeover_enabled(&conn)?;
    let direct_takeover_apps = query_direct_takeover_apps(&conn)?;
    let (proxy_address, proxy_port) = query_proxy_listen_addr(&conn)?;

    Ok(ReadOnlyDbState {
        current_providers,
        takeover_enabled,
        direct_takeover_apps,
        proxy_address,
        proxy_port,
    })
}

fn query_current_providers(conn: &Connection) -> CliResult<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare("SELECT app_type, id FROM providers WHERE is_current = 1 ORDER BY app_type")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut providers = Vec::new();
    for row in rows {
        providers.push(row.map_err(|e| e.to_string())?);
    }
    Ok(providers)
}

fn query_takeover_enabled(conn: &Connection) -> CliResult<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT app_type FROM proxy_config WHERE enabled = 1 ORDER BY app_type")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;

    let mut apps = Vec::new();
    for row in rows {
        apps.push(row.map_err(|e| e.to_string())?);
    }
    Ok(apps)
}

/// 直接模式接管：proxy_config.enabled=0 但 proxy_live_backup 表中有记录，
/// 说明 settings.json 已被覆写为 matpool 配置（无本地代理）。
fn query_direct_takeover_apps(conn: &Connection) -> CliResult<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT app_type FROM proxy_live_backup ORDER BY app_type")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;

    let mut apps = Vec::new();
    for row in rows {
        apps.push(row.map_err(|e| e.to_string())?);
    }
    Ok(apps)
}

fn query_proxy_listen_addr(conn: &Connection) -> CliResult<(Option<String>, Option<u16>)> {
    match conn.query_row(
        "SELECT listen_address, listen_port FROM proxy_config WHERE app_type = 'claude'",
        [],
        |row| {
            let address: String = row.get(0)?;
            let port: i32 = row.get(1)?;
            Ok((Some(address), Some(port as u16)))
        },
    ) {
        Ok(value) => Ok(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok((None, None)),
        Err(err) => Err(err.to_string()),
    }
}

fn probe_proxy_health(addr: Option<SocketAddr>) -> CliResult<Option<SocketAddr>> {
    let Some(addr) = addr else {
        return Ok(None);
    };

    match TcpStream::connect_timeout(&addr, Duration::from_millis(250)) {
        Ok(_) => Ok(Some(addr)),
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::TimedOut
            ) =>
        {
            Ok(None)
        }
        Err(err) => Err(err.to_string()),
    }
}

fn parse_option_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find_map(|pair| (pair[0] == name).then(|| pair[1].clone()))
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

#[derive(Debug)]
struct SetupOptions {
    token: Option<String>,
    apps: Vec<&'static str>,
    skip_login: bool,
    skip_daemon: bool,
    skip_takeover: bool,
    dry_run: bool,
}

impl SetupOptions {
    fn parse(args: &[String]) -> CliResult<Self> {
        Ok(Self {
            token: parse_option_value(args, "--token").or_else(|| env::var("MATPOOL_TOKEN").ok()),
            apps: parse_setup_apps(parse_option_value(args, "--apps").as_deref())?,
            skip_login: has_flag(args, "--skip-login"),
            skip_daemon: has_flag(args, "--skip-daemon"),
            skip_takeover: has_flag(args, "--skip-takeover"),
            dry_run: has_flag(args, "--dry-run"),
        })
    }
}

fn parse_setup_apps(value: Option<&str>) -> CliResult<Vec<&'static str>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return expand_apps("all");
    };
    if value == "all" {
        return expand_apps("all");
    }

    let mut apps = Vec::new();
    for app in value
        .split(',')
        .map(str::trim)
        .filter(|app| !app.is_empty())
    {
        for expanded in expand_apps(app)? {
            if !apps.contains(&expanded) {
                apps.push(expanded);
            }
        }
    }
    if apps.is_empty() {
        return Err("setup apps list is empty".to_string());
    }
    Ok(apps)
}

fn expand_apps(app: &str) -> CliResult<Vec<&'static str>> {
    let normalized = app
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    let app = match normalized.as_str() {
        "app|all" => "all",
        other => other,
    };

    match app {
        "all" => Ok(vec!["claude", "codex", "gemini"]),
        "claude" => Ok(vec!["claude"]),
        "codex" => Ok(vec!["codex"]),
        "gemini" => Ok(vec!["gemini"]),
        other => Err(format!(
            "unsupported takeover app '{other}'. Expected claude, codex, gemini, or all."
        )),
    }
}

fn expand_app_types(app: &str) -> CliResult<Vec<AppType>> {
    let apps = expand_apps(app)?;
    expand_app_types_from_keys(&apps)
}

fn expand_app_types_from_keys(apps: &[&str]) -> CliResult<Vec<AppType>> {
    let mut app_types = Vec::new();
    for app in apps {
        let app_type = match *app {
            "claude" => AppType::Claude,
            "codex" => AppType::Codex,
            "gemini" => AppType::Gemini,
            other => {
                return Err(format!(
                    "unsupported takeover app '{other}'. Expected claude, codex, gemini, or all."
                ));
            }
        };
        if !app_types.contains(&app_type) {
            app_types.push(app_type);
        }
    }
    Ok(app_types)
}

fn print_path_status(label: &str, path: std::path::PathBuf) {
    println!(
        "  {:<16} {:<7} {}",
        label,
        if path.exists() { "exists" } else { "missing" },
        path.display()
    );
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use serde_json::{json, Value};

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_systemd_service_uses_backward_compatible_journal_output() {
        let service = linux_systemd_service(std::path::Path::new("/opt/Matpool Switch/matpool"));

        assert!(service.contains("ExecStart=\"/opt/Matpool Switch/matpool\" daemon run"));
        assert!(service.contains("StandardOutput=journal"));
        assert!(service.contains("StandardError=journal"));
        assert!(!service.contains("WorkingDirectory="));
        assert!(!service.contains("append:"));
    }

    #[test]
    fn parse_claude_model_slot_updates_accepts_supported_slots() {
        let args = vec![
            "--sonnet".to_string(),
            "MiMo-V2.5".to_string(),
            "--custom".to_string(),
            "GPT-5.5".to_string(),
        ];

        let parsed = parse_claude_model_slot_updates(&args).expect("parse updates");

        assert_eq!(
            parsed,
            vec![
                (
                    "sonnet",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL",
                    "MiMo-V2.5".to_string()
                ),
                (
                    "custom",
                    "ANTHROPIC_CUSTOM_MODEL_OPTION",
                    "GPT-5.5".to_string()
                ),
            ]
        );
    }

    #[test]
    fn canonicalize_claude_model_matches_case_insensitively() {
        let catalog = vec!["MiMo-V2.5".to_string(), "GLM-5.2".to_string()];

        assert_eq!(
            canonicalize_claude_model("mimo-v2.5", &catalog),
            Some("MiMo-V2.5".to_string())
        );
        assert_eq!(canonicalize_claude_model("missing", &catalog), None);
    }

    #[test]
    fn set_claude_provider_env_model_writes_model_and_display_name() {
        let mut provider = provider::Provider::with_id(
            "matpool-claude".to_string(),
            "Matpool".to_string(),
            json!({ "env": {} }),
            None,
        );

        set_claude_provider_env_model(&mut provider, "ANTHROPIC_CUSTOM_MODEL_OPTION", "GPT-5.5");

        let env = provider.settings_config["env"].as_object().expect("env");
        assert_eq!(
            env["ANTHROPIC_CUSTOM_MODEL_OPTION"],
            Value::String("GPT-5.5".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_CUSTOM_MODEL_OPTION_NAME"],
            Value::String("GPT-5.5".to_string())
        );
    }

    #[test]
    fn matpool_claude_provider_requires_proxy() {
        let provider = provider::Provider::with_id(
            "matpool-claude".to_string(),
            "Matpool".to_string(),
            json!({ "env": { "ANTHROPIC_BASE_URL": "https://token.matpool.com" } }),
            None,
        );

        assert!(claude_provider_requires_proxy(&provider));
    }

    #[test]
    fn codex_takeover_selects_matpool_provider() {
        assert_eq!(takeover_provider_id(&AppType::Codex), Some("matpool-codex"));
        assert_eq!(takeover_provider_id(&AppType::Claude), None);
        assert_eq!(takeover_provider_id(&AppType::Gemini), None);
    }
}
