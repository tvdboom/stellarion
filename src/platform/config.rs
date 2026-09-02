//! Stellarion's fixed public Supabase configuration.

use base64::{engine::general_purpose::URL_SAFE_PAD_INDIFFERENT, Engine as _};
use thiserror::Error;

/// Public Supabase project used by every Stellarion client.
pub const SUPABASE_URL: &str = "https://crzfxxyapixtfnogxhtu.supabase.co";

/// Browser-safe publishable key used by every Stellarion client.
pub const SUPABASE_PUBLISHABLE_KEY: &str = "sb_publishable_MfB5egfDId8rzjBMieLCiw_zODH410X";

/// Public browser-safe values required by Supabase clients.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupabaseConfig {
    /// HTTPS base URL of the Supabase project.
    pub url: String,
    /// Publishable or legacy anonymous public key; never a secret/service-role key.
    pub publishable_key: String,
}

impl SupabaseConfig {
    /// Creates and validates a public client configuration.
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

    /// Returns Stellarion's built-in public Supabase configuration.
    pub fn load() -> Result<Self, ConfigError> {
        Self::new(SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY)
    }

    /// Validates URL shape and rejects modern or legacy server-only credentials.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() || self.publishable_key.is_empty() {
            return Err(ConfigError::Invalid(
                "Supabase URL and publishable key must not be empty".to_string(),
            ));
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
    /// A supplied value is malformed.
    #[error("invalid Supabase configuration: {0}")]
    Invalid(String),
    /// A server-only credential was supplied to a client build.
    #[error("a Supabase secret/service-role key must never be shipped with Stellarion")]
    SecretKeyRejected,
}

#[cfg(test)]
#[path = "../../tests/platform/config.rs"]
mod tests;
