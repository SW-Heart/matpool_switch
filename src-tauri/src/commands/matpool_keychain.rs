//! Matpool Token 在 OS keychain 中的读写。
//!
//! - macOS: Keychain Access
//! - Windows: Credential Manager
//! - Linux: Secret Service (libsecret)
//!
//! Service / account 字段均使用固定字符串：用户在 keychain 工具里能搜到 "Matpool Switch"。

const SERVICE: &str = "Matpool Switch";
const ACCOUNT: &str = "matpool-token";

#[cfg(feature = "desktop")]
use keyring::Entry;

#[cfg(feature = "desktop")]
fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("keyring entry error: {e}"))
}

#[cfg(not(feature = "desktop"))]
fn token_file_path() -> std::path::PathBuf {
    crate::config::get_app_config_dir().join("matpool-token")
}

#[cfg(not(feature = "desktop"))]
fn read_token_file() -> Result<Option<String>, String> {
    let path = token_file_path();
    match std::fs::read_to_string(&path) {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("token file read failed: {err}")),
    }
}

#[cfg(not(feature = "desktop"))]
fn write_token_file(token: &str) -> Result<(), String> {
    let path = token_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("token dir create failed: {err}"))?;
    }

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map_err(|err| format!("token file open failed: {err}"))?;
        file.write_all(token.as_bytes())
            .map_err(|err| format!("token file write failed: {err}"))?;
        file.write_all(b"\n")
            .map_err(|err| format!("token file write failed: {err}"))?;
    }

    #[cfg(not(unix))]
    std::fs::write(&path, format!("{token}\n"))
        .map_err(|err| format!("token file write failed: {err}"))?;

    Ok(())
}

/// Tauri 命令：把 token 写入 OS keychain。
///
/// 空字符串等同于"清除"。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn matpool_keychain_set(token: String) -> Result<(), String> {
    let trimmed = token.trim();
    #[cfg(not(feature = "desktop"))]
    {
        if trimmed.is_empty() {
            return matpool_keychain_clear();
        }
        return write_token_file(trimmed);
    }

    #[cfg(feature = "desktop")]
    {
        let e = entry()?;
        if trimmed.is_empty() {
            // delete_credential 在条目不存在时返回 NoEntry，归一化为 Ok
            match e.delete_credential() {
                Ok(()) => Ok(()),
                Err(keyring::Error::NoEntry) => Ok(()),
                Err(err) => Err(format!("keychain delete failed: {err}")),
            }
        } else {
            e.set_password(trimmed)
                .map_err(|err| format!("keychain set failed: {err}"))
        }
    }
}

/// Tauri 命令：从 OS keychain 读 token。
///
/// 不存在时返回 `Ok(None)`，便于前端区分"没设置"和"读取失败"。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn matpool_keychain_get() -> Result<Option<String>, String> {
    #[cfg(not(feature = "desktop"))]
    {
        return read_token_file();
    }

    #[cfg(feature = "desktop")]
    {
        match entry()?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(format!("keychain get failed: {err}")),
        }
    }
}

/// Tauri 命令：清除 keychain 中的 token。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn matpool_keychain_clear() -> Result<(), String> {
    #[cfg(not(feature = "desktop"))]
    {
        match std::fs::remove_file(token_file_path()) {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(format!("token file clear failed: {err}")),
        }
    }

    #[cfg(feature = "desktop")]
    {
        match entry()?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(format!("keychain clear failed: {err}")),
        }
    }
}

/// 内部读取 helper：返回 trimmed token，不存在 / 失败 / 空串都归一化为 None。
///
/// 给 services 层（例如 switch_normal 里的 Matpool token 注入逻辑）用，
/// 与 tauri::command 解耦。失败时只 log 不抛 —— Token 注入是 best-effort，
/// 拿不到 token 也要让 switch 流程继续（用户依然能切到 Matpool，只是没 Token，
/// 切完再去 Token Wizard 补就行）。
pub fn read_token_from_keychain() -> Option<String> {
    #[cfg(not(feature = "desktop"))]
    {
        return matpool_keychain_get().ok().flatten();
    }

    #[cfg(feature = "desktop")]
    {
        match entry() {
            Ok(e) => match e.get_password() {
                Ok(s) => {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                }
                Err(keyring::Error::NoEntry) => None,
                Err(err) => {
                    log::warn!("[matpool_keychain] read failed: {err}");
                    None
                }
            },
            Err(err) => {
                log::warn!("[matpool_keychain] entry init failed: {err}");
                None
            }
        }
    }
}
