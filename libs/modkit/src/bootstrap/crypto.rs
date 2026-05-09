use std::sync::OnceLock;

/// Error returned when the crypto provider cannot be installed.
// `Clone` required by `OnceLock<Result<_>>` cache in `init_crypto_provider` —
// the cached result is cloned on every call.
// `PartialEq`/`Eq` used by tests asserting the cached result is stable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CryptoProviderError {
    /// Another crypto provider was already installed (FIPS mode).
    #[error("failed to install FIPS crypto provider - another provider is already installed")]
    FipsProviderConflict,
}

static INIT_RESULT: OnceLock<Result<(), CryptoProviderError>> = OnceLock::new();

/// Install the process-wide default rustls [`CryptoProvider`](rustls::crypto::CryptoProvider).
///
/// - **FIPS mode** (`fips` feature): installs the FIPS-validated AWS-LC provider
///   (`aws-lc-fips-sys`, NIST Certificate #4816).
/// - **Standard mode**: installs the `aws-lc-rs` provider explicitly. This is
///   required because both `ring` and `aws-lc-rs` are compiled into the binary
///   (ring via `aliri`/`pingora-rustls`), and rustls 0.23 panics when it cannot
///   auto-detect a single provider. Conflicts here are non-fatal: if another
///   provider was installed first, it stays active, the conflict is logged at
///   `warn!`, and `Ok(())` is returned.
///
/// This **must** be called before any TLS configuration, HTTP client, database
/// connection, or JWT operation is created.
///
/// Safe to call multiple times — only the first invocation has an effect;
/// subsequent calls return the cached first-call result.
///
/// # Errors
///
/// Returns [`CryptoProviderError::FipsProviderConflict`] if the `fips` feature is
/// enabled and another crypto provider has already been installed.
pub fn init_crypto_provider() -> Result<(), CryptoProviderError> {
    INIT_RESULT
        .get_or_init(|| {
            #[cfg(feature = "fips")]
            {
                if let Err(prev) = rustls::crypto::default_fips_provider().install_default() {
                    tracing::error!(
                        previous_provider = ?prev,
                        "FIPS crypto provider conflict: another rustls provider was already installed"
                    );
                    return Err(CryptoProviderError::FipsProviderConflict);
                }
                tracing::info!("FIPS-140-3 crypto provider installed (AWS-LC FIPS module)");
            }

            #[cfg(not(feature = "fips"))]
            {
                if let Err(prev) = rustls::crypto::aws_lc_rs::default_provider().install_default() {
                    // Non-fatal: another provider is already active, TLS still works.
                    tracing::warn!(
                        previous_provider = ?prev,
                        "aws-lc-rs crypto provider not installed: another default provider was already set"
                    );
                } else {
                    tracing::info!("aws-lc-rs crypto provider installed");
                }
            }

            Ok(())
        })
        .clone()
}
