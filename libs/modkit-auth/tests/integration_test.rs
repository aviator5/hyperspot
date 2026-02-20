#![allow(clippy::unwrap_used, clippy::expect_used)]

use modkit_auth::{ClaimsError, JwksConfig};

#[test]
fn test_jwks_config_serialization_roundtrip() {
    let config = JwksConfig {
        uri: "https://auth.example.com/.well-known/jwks.json".to_owned(),
        refresh_interval_seconds: 300,
        max_backoff_seconds: 3600,
    };

    let json = serde_json::to_string_pretty(&config).unwrap();
    let deserialized: JwksConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.uri, config.uri);
    assert_eq!(
        deserialized.refresh_interval_seconds,
        config.refresh_interval_seconds
    );
    assert_eq!(deserialized.max_backoff_seconds, config.max_backoff_seconds);
}

#[test]
fn test_claims_error_types() {
    let err = ClaimsError::InvalidIssuer {
        expected: vec!["https://expected.com".to_owned()],
        actual: "https://actual.com".to_owned(),
    };
    assert_eq!(
        err.to_string(),
        "Invalid issuer: expected one of [\"https://expected.com\"], got https://actual.com"
    );

    let err = ClaimsError::Expired;
    assert_eq!(err.to_string(), "Token expired");

    let err = ClaimsError::MissingClaim("sub".to_owned());
    assert_eq!(err.to_string(), "Missing required claim: sub");

    let err = ClaimsError::UnknownKidAfterRefresh;
    assert_eq!(err.to_string(), "Unknown key ID after refresh");
}
