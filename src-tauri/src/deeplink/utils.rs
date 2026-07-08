//! Deep link utility functions
//!
//! Common helpers for URL validation, Base64 decoding, etc.

use crate::error::AppError;
use base64::prelude::*;
use url::Url;

/// Redact secrets from a deep link before logging it.
pub fn redact_deeplink_url_for_log(url_str: &str) -> String {
    let Ok(mut url) = Url::parse(url_str) else {
        return url_str.to_string();
    };

    let query_pairs = url
        .query_pairs()
        .map(|(key, value)| {
            let value = value.to_string();
            if is_sensitive_query_key(&key) || looks_like_bearer_secret(&value) {
                (key.to_string(), "<redacted>".to_string())
            } else {
                (key.to_string(), value)
            }
        })
        .collect::<Vec<_>>();

    if url.query().is_some() {
        url.query_pairs_mut().clear().extend_pairs(query_pairs);
    }

    url.to_string()
}

fn is_sensitive_query_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "apikey"
            | "api_key"
            | "api-key"
            | "key"
            | "token"
            | "access_token"
            | "refresh_token"
            | "id_token"
            | "auth_token"
            | "authorization"
            | "client_secret"
            | "secret"
            | "password"
            | "credential"
            | "credentials"
    )
}

fn looks_like_bearer_secret(value: &str) -> bool {
    value
        .trim_start()
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
}

/// Validate that a string is a valid HTTP(S) URL
pub fn validate_url(url_str: &str, field_name: &str) -> Result<(), AppError> {
    let url = Url::parse(url_str)
        .map_err(|e| AppError::InvalidInput(format!("Invalid URL for '{field_name}': {e}")))?;

    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::InvalidInput(format!(
            "Invalid URL scheme for '{field_name}': must be http or https, got '{scheme}'"
        )));
    }

    Ok(())
}

/// Decode a Base64 parameter from deep link URL
///
/// This function handles common issues with Base64 in URLs:
/// - `+` being decoded as space
/// - Missing padding `=`
/// - Both standard and URL-safe Base64 variants
pub fn decode_base64_param(field: &str, raw: &str) -> Result<Vec<u8>, AppError> {
    let mut candidates: Vec<String> = Vec::new();
    // Keep spaces (to restore `+`), but remove newlines
    let trimmed = raw.trim_matches(|c| c == '\r' || c == '\n');

    // First try restoring spaces to "+"
    if trimmed.contains(' ') {
        let replaced = trimmed.replace(' ', "+");
        if !replaced.is_empty() && !candidates.contains(&replaced) {
            candidates.push(replaced);
        }
    }

    // Original value
    if !trimmed.is_empty() && !candidates.contains(&trimmed.to_string()) {
        candidates.push(trimmed.to_string());
    }

    // Add padding variants
    let existing = candidates.clone();
    for candidate in existing {
        let mut padded = candidate.clone();
        let remainder = padded.len() % 4;
        if remainder != 0 {
            padded.extend(std::iter::repeat_n('=', 4 - remainder));
        }
        if !candidates.contains(&padded) {
            candidates.push(padded);
        }
    }

    let mut last_error: Option<String> = None;
    for candidate in candidates {
        for engine in [
            &BASE64_STANDARD,
            &BASE64_STANDARD_NO_PAD,
            &BASE64_URL_SAFE,
            &BASE64_URL_SAFE_NO_PAD,
        ] {
            match engine.decode(&candidate) {
                Ok(bytes) => return Ok(bytes),
                Err(err) => last_error = Some(err.to_string()),
            }
        }
    }

    Err(AppError::InvalidInput(format!(
        "{field} 参数 Base64 解码失败：{}。请确认链接参数已用 Base64 编码并经过 URL 转义（尤其是将 '+' 编码为 %2B，或使用 URL-safe Base64）。",
        last_error.unwrap_or_else(|| "未知错误".to_string())
    )))
}

/// Infer homepage URL from API endpoint
///
/// Examples:
/// - https://api.anthropic.com/v1 → https://anthropic.com
/// - https://api.openai.com/v1 → https://openai.com
/// - https://api-test.company.com/v1 → https://company.com
pub fn infer_homepage_from_endpoint(endpoint: &str) -> Option<String> {
    let url = Url::parse(endpoint).ok()?;
    let host = url.host_str()?;

    // Remove common API prefixes
    let clean_host = host
        .strip_prefix("api.")
        .or_else(|| host.strip_prefix("api-"))
        .unwrap_or(host);

    Some(format!("https://{clean_host}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_deeplink_url_for_log_masks_api_key_query() {
        let url = redact_deeplink_url_for_log(
            "matpool://v1/import?resource=provider&app=claude&apiKey=sk-secret&name=Test",
        );

        assert!(url.contains("apiKey=%3Credacted%3E"));
        assert!(url.contains("name=Test"));
        assert!(!url.contains("sk-secret"));
    }

    #[test]
    fn redact_deeplink_url_for_log_masks_bearer_query_value() {
        let url = redact_deeplink_url_for_log(
            "matpool://v1/import?resource=provider&token=Bearer%20sk-secret&name=Test",
        );

        assert!(url.contains("token=%3Credacted%3E"));
        assert!(!url.contains("sk-secret"));
    }
}
