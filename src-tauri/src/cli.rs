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
use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::ExitCode;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use store::AppState;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
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
  matpool provider list [app|all]      List configured providers
  matpool provider seed                Ensure built-in Matpool/official providers exist
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
            .map(|state| state.any_takeover_enabled())
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
        println!(
            "  {:<15} current_provider={:<24} takeover={}",
            app_key,
            current,
            db_state
                .as_ref()
                .map(|state| state.takeover_enabled(app_key))
                .unwrap_or(false)
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
        unknown => {
            return Err(format!(
                "unknown provider command '{unknown}'. Usage: matpool provider list [app|all] | matpool provider seed"
            ));
        }
    }
    Ok(())
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
            print_model_sync_outcome(&outcome);
        }
        unknown => {
            return Err(format!(
                "unknown models command '{unknown}'. Usage: matpool models list | matpool models sync [app|all]"
            ));
        }
    }
    Ok(())
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

    if enabled {
        let app_keys = expand_apps(app)?;
        sync_matpool_models_best_effort(&state.db, &app_keys).await;
        ensure_live_configs_for_apps(&app_keys)?;
        let db_state = read_db_state().unwrap_or_default();
        if probe_proxy_health(db_state.proxy_addr())?.is_none() {
            return Err(
                "local proxy daemon is not running. Start it first with: matpool daemon start"
                    .to_string(),
            );
        }
    }

    set_takeover_apps(&state, &expand_apps(app)?, enabled).await?;

    if enabled {
        println!();
        println!("Note: takeover points managed tools at the local Matpool proxy.");
        println!("Keep the Matpool daemon or desktop app running while takeover is enabled.");
    }

    Ok(())
}

async fn set_takeover_apps(state: &AppState, apps: &[&str], enabled: bool) -> CliResult {
    for app_key in apps {
        state
            .proxy_service
            .set_takeover_for_app_without_managing_proxy(app_key, enabled)
            .await?;
        println!(
            "{} takeover {}.",
            app_key,
            if enabled { "enabled" } else { "disabled" }
        );
    }
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
    let stop_result = match state.proxy_service.stop().await {
        Ok(()) => {
            println!("Matpool proxy daemon stopped.");
            Ok(())
        }
        Err(err) if err.contains("未运行") || err.contains("not running") => Ok(()),
        Err(err) => Err(err),
    };
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
    let log_dir = get_app_config_dir().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| format!("failed to create log dir: {e}"))?;
    if let Some(parent) = service_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create systemd user dir: {e}"))?;
    }

    let exe = env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
    let service = linux_systemd_service(&exe, &log_dir);
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
    run_schtasks(&["/Run", "/TN", WINDOWS_TASK_NAME])?;
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
    let log_dir = get_app_config_dir().join("logs");
    println!("stdout: {}", log_dir.join("daemon.out.log").display());
    println!("stderr: {}", log_dir.join("daemon.err.log").display());
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
fn linux_systemd_service(exe: &std::path::Path, log_dir: &std::path::Path) -> String {
    let stdout = log_dir.join("daemon.out.log");
    let stderr = log_dir.join("daemon.err.log");
    format!(
        r#"[Unit]
Description=Matpool Switch local proxy daemon
After=network-online.target

[Service]
Type=simple
ExecStart={} daemon run
Restart=always
RestartSec=3
WorkingDirectory={}
StandardOutput=append:{}
StandardError=append:{}

[Install]
WantedBy=default.target
"#,
        systemd_quote_path(exe),
        systemd_quote_path(&get_app_config_dir()),
        systemd_unit_escape(&stdout.to_string_lossy()),
        systemd_unit_escape(&stderr.to_string_lossy())
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
    let username = env::var("USERNAME").ok().filter(|value| !value.is_empty())?;
    match env::var("USERDOMAIN").ok().filter(|value| !value.is_empty()) {
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

    if !options.skip_daemon {
        daemon_install()?;
        daemon_start_service()?;
        std::thread::sleep(Duration::from_millis(500));
    }

    if !options.skip_takeover {
        let db = init_db()?;
        sync_matpool_models_best_effort(&db, &options.apps).await;
        ensure_live_configs_for_apps(&options.apps)?;
        let db_state = read_db_state().unwrap_or_default();
        if probe_proxy_health(db_state.proxy_addr())?.is_none() {
            return Err(
                "local proxy daemon is not running. Run: matpool daemon start, then retry setup"
                    .to_string(),
            );
        }
        let state = AppState::new(db);
        set_takeover_apps(&state, &options.apps, true).await?;
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
    Ok(Arc::new(db))
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
    let (proxy_address, proxy_port) = query_proxy_listen_addr(&conn)?;

    Ok(ReadOnlyDbState {
        current_providers,
        takeover_enabled,
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
