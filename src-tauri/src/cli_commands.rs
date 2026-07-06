#[path = "commands/matpool_keychain.rs"]
mod matpool_keychain;

pub use matpool_keychain::{
    matpool_keychain_clear, matpool_keychain_get, matpool_keychain_set, read_token_from_keychain,
};

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tokio::sync::RwLock;

#[cfg(feature = "desktop")]
pub struct CodexOAuthState(
    pub Arc<RwLock<crate::proxy::providers::codex_oauth_auth::CodexOAuthManager>>,
);

#[cfg(feature = "desktop")]
pub struct CopilotAuthState(
    pub Arc<RwLock<crate::proxy::providers::copilot_auth::CopilotAuthManager>>,
);
