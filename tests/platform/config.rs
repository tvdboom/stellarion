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
        assert!(matches!(SupabaseConfig::new(malformed, "public"), Err(ConfigError::Invalid(_))));
    }
}

#[test]
/// Uses the same official project in every build.
fn loads_built_in_configuration() {
    let config = SupabaseConfig::load().unwrap();
    assert_eq!(config.url, SUPABASE_URL);
    assert_eq!(config.publishable_key, SUPABASE_PUBLISHABLE_KEY);
}
