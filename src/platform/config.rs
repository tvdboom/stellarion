//! Injectable Supabase configuration for native development and itch.io WebAssembly builds.

use base64::{engine::general_purpose::URL_SAFE_PAD_INDIFFERENT, Engine as _};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Public browser-safe values required by Supabase clients.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SupabaseConfig {
    /// HTTPS base URL of the Supabase project.
    pub url: String,
    /// Publishable or legacy anonymous public key; never a secret/service-role key.
    pub publishable_key: String,
}

impl SupabaseConfig {
    /// Creates and validates injected configuration.
    pub fn new(
        url: impl Into<String>,
        publishable_key: impl Into<String>,
    ) -> Result<Self, ConfigError> {
        let config = Self {
            url: url.into().trim().trim_end_matches('/').to_string(),
            publishable_key: publishable_key.into().trim().to_string(),
        };
        config.validate()?;
        Ok(config)
    }

    /// Loads environment/file configuration using the current platform strategy.
    pub async fn load() -> Result<Self, ConfigError> {
        load_platform_config().await
    }

    /// Validates URL shape and rejects modern or legacy server-only credentials.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() || self.publishable_key.is_empty() {
            return Err(ConfigError::Missing);
        }
        if self.url != self.url.trim().trim_end_matches('/')
            || self.publishable_key != self.publishable_key.trim()
        {
            return Err(ConfigError::Invalid(
                "Supabase configuration must be normalized through SupabaseConfig::new".to_string(),
            ));
        }
        if self.url.len() > 2_048 || self.publishable_key.len() > 4_096 {
            return Err(ConfigError::Invalid(
                "Supabase URL or publishable key exceeds the supported length".to_string(),
            ));
        }
        let parsed = reqwest::Url::parse(&self.url)
            .map_err(|error| ConfigError::Invalid(format!("SUPABASE_URL is malformed: {error}")))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| ConfigError::Invalid("SUPABASE_URL has no host".to_string()))?;
        let is_local = matches!(host, "localhost" | "127.0.0.1" | "::1");
        if parsed.scheme() != "https" && !(parsed.scheme() == "http" && is_local) {
            return Err(ConfigError::Invalid(
                "SUPABASE_URL must use HTTPS outside local development".to_string(),
            ));
        }
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !matches!(parsed.path(), "" | "/")
        {
            return Err(ConfigError::Invalid(
                "SUPABASE_URL must be a project origin without credentials, path, query, or fragment"
                    .to_string(),
            ));
        }
        if is_server_only_key(&self.publishable_key) {
            return Err(ConfigError::SecretKeyRejected);
        }
        Ok(())
    }

    /// Returns an absolute API URL below the configured project root.
    pub fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.url.trim_end_matches('/'), path.trim_start_matches('/'))
    }
}

/// Detects both modern secret prefixes and the role claim in legacy JWT service keys.
fn is_server_only_key(key: &str) -> bool {
    if key.to_ascii_lowercase().starts_with("sb_secret_") {
        return true;
    }

    let mut segments = key.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (segments.next(), segments.next(), segments.next(), segments.next())
    else {
        return false;
    };
    URL_SAFE_PAD_INDIFFERENT
        .decode(payload)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|claims| {
            claims
                .get("role")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|role| role.eq_ignore_ascii_case("service_role"))
        })
}

/// Configuration loading or validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    /// Neither environment nor the public JSON file supplied both values.
    #[error("Supabase configuration is missing")]
    Missing,
    /// Configuration file or network request failed.
    #[error("failed to load Supabase configuration: {0}")]
    Load(String),
    /// A supplied value is malformed.
    #[error("invalid Supabase configuration: {0}")]
    Invalid(String),
    /// A server-only credential was supplied to a client build.
    #[error("a Supabase secret/service-role key must never be shipped with Stellarion")]
    SecretKeyRejected,
}

#[cfg(not(target_arch = "wasm32"))]
/// Loads native environment variables, then `stellarion-config.json` beside the executable or CWD.
async fn load_platform_config() -> Result<SupabaseConfig, ConfigError> {
    if let (Ok(url), Ok(key)) =
        (std::env::var("SUPABASE_URL"), std::env::var("SUPABASE_PUBLISHABLE_KEY"))
    {
        return SupabaseConfig::new(url, key);
    }

    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join("stellarion-config.json"));
        }
    }
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join("stellarion-config.json"));
    }
    for path in candidates {
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                let config: SupabaseConfig = serde_json::from_str(&contents)
                    .map_err(|error| ConfigError::Load(format!("{}: {error}", path.display())))?;
                return SupabaseConfig::new(config.url, config.publishable_key);
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => {
                return Err(ConfigError::Load(format!("{}: {error}", path.display())));
            },
        }
    }
    Err(ConfigError::Missing)
}

#[cfg(target_arch = "wasm32")]
/// Loads compile-time variables first, then fetches public `stellarion-config.json` from itch.io.
async fn load_platform_config() -> Result<SupabaseConfig, ConfigError> {
    if let (Some(url), Some(key)) =
        (option_env!("SUPABASE_URL"), option_env!("SUPABASE_PUBLISHABLE_KEY"))
    {
        return SupabaseConfig::new(url, key);
    }
    let page_url = web_sys::window()
        .ok_or_else(|| ConfigError::Load("browser window is unavailable".to_string()))?
        .location()
        .href()
        .map_err(|error| ConfigError::Load(format!("could not read the page URL: {error:?}")))?;
    let config_url =
        reqwest::Url::parse(&page_url).and_then(|url| url.join("stellarion-config.json")).map_err(
            |error| ConfigError::Load(format!("could not resolve public config URL: {error}")),
        )?;
    let response =
        reqwest::get(config_url).await.map_err(|error| ConfigError::Load(error.to_string()))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ConfigError::Missing);
    }
    let config = response
        .error_for_status()
        .map_err(|error| ConfigError::Load(error.to_string()))?
        .json::<SupabaseConfig>()
        .await
        .map_err(|error| ConfigError::Load(error.to_string()))?;
    SupabaseConfig::new(config.url, config.publishable_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Accepts public HTTPS values and rejects server credentials.
    fn validates_client_safe_configuration() {
        let config =
            SupabaseConfig::new(" https://example.supabase.co/ ", " sb_publishable_test ").unwrap();
        assert_eq!(config.url, "https://example.supabase.co");
        assert_eq!(config.publishable_key, "sb_publishable_test");
        assert_eq!(config.endpoint("/rest/v1/rpc"), "https://example.supabase.co/rest/v1/rpc");
        assert_eq!(
            SupabaseConfig::new("https://example.supabase.co", "sb_secret_never_ship"),
            Err(ConfigError::SecretKeyRejected)
        );
        let service_payload = URL_SAFE_PAD_INDIFFERENT.encode(r#"{"role":"service_role"}"#);
        assert_eq!(
            SupabaseConfig::new(
                "https://example.supabase.co",
                format!("header.{service_payload}.signature")
            ),
            Err(ConfigError::SecretKeyRejected)
        );
        assert_eq!(
            SupabaseConfig::new("https://example.supabase.co", "SB_SECRET_NEVER_SHIP"),
            Err(ConfigError::SecretKeyRejected)
        );
        assert!(matches!(
            SupabaseConfig::new("http://example.com", "public"),
            Err(ConfigError::Invalid(_))
        ));
        assert!(SupabaseConfig::new("http://localhost:54321", "public").is_ok());
        assert!(matches!(
            SupabaseConfig::new("https://example.supabase.co", "x".repeat(4_097)),
            Err(ConfigError::Invalid(_))
        ));
        for malformed in [
            "https://",
            "https://example.supabase.co/rest/v1",
            "https://user@example.supabase.co",
            "https://example.supabase.co?key=value",
            "https://example.supabase.co/#fragment",
        ] {
            assert!(matches!(
                SupabaseConfig::new(malformed, "public"),
                Err(ConfigError::Invalid(_))
            ));
        }
    }
}
