//! Engine configuration: struct definition, env-var loading, file fallback,
//! and validation.
//!
//! Priority order (highest → lowest):
//! 1. Environment variables (`KEEPER_KEYPAIR`, `RPC_ENDPOINT_URL`, …)
//! 2. TOML config file at `CONFIG_FILE` env var path (or `engine.toml` if
//!    present in the working directory)
//!
//! The engine exits with a non-zero code and a descriptive message when any
//! required field is missing or any value is out of the allowed range.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use tracing::warn;

use crate::errors::{EngineError, NonTransientError};

// ── Defaults ──────────────────────────────────────────────────────────────────

const DEFAULT_POLL_INTERVAL_SECS: u64 = 2;
const DEFAULT_TASK_QUERY_TIMEOUT_SECS: u64 = 10;
const DEFAULT_CONFIRMATION_TIMEOUT_SECS: u64 = 60;
const DEFAULT_RETRY_BACKOFF_INITIAL_SECS: u64 = 1;
const DEFAULT_RETRY_BACKOFF_MAX_SECS: u64 = 60;
const DEFAULT_MAX_RETRIES: u32 = 5;
const DEFAULT_HEALTH_PORT: u16 = 8080;

// ── Public struct ─────────────────────────────────────────────────────────────

/// Full runtime configuration for the Engine daemon.
///
/// Construct via [`EngineConfig::from_env`] which reads environment variables
/// (optionally merged with a TOML file) and validates all fields.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Base58-encoded Stellar keypair secret seed for this keeper node.
    pub keeper_keypair: String,

    /// URL of the Soroban RPC endpoint (e.g. `https://soroban-testnet.stellar.org`).
    pub rpc_endpoint_url: String,

    /// Strkey-encoded contract address of the deployed Anchor contract.
    pub anchor_contract_address: String,

    /// How often to poll for new ledgers. Must be in `[1 s, 5 s]`.
    pub poll_interval: Duration,

    /// Maximum time to wait for the task-query RPC call. Must be `≤ 10 s`.
    pub task_query_timeout: Duration,

    /// Maximum time to wait for on-chain transaction confirmation. Must be `≤ 60 s`.
    pub confirmation_timeout: Duration,

    /// Initial exponential-backoff interval for transient errors. Must be `≥ 1 s`.
    pub retry_backoff_initial: Duration,

    /// Cap for exponential backoff. Must be in `[retry_backoff_initial, 60 s]`.
    pub retry_backoff_max: Duration,

    /// Maximum submission attempts per task before giving up. Min = 1.
    pub max_retries: u32,

    /// TCP port for the health-check HTTP server. Default: 8080.
    pub health_port: u16,

    /// Optional path to a TOML configuration file.
    pub config_file_path: Option<PathBuf>,
}

// ── Internal file schema ──────────────────────────────────────────────────────

/// Deserialisation target for the optional TOML config file.
///
/// All fields are optional; env vars always take precedence.
#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    keeper_keypair: Option<String>,
    rpc_endpoint_url: Option<String>,
    anchor_contract_address: Option<String>,
    poll_interval_secs: Option<u64>,
    task_query_timeout_secs: Option<u64>,
    confirmation_timeout_secs: Option<u64>,
    retry_backoff_initial_secs: Option<u64>,
    retry_backoff_max_secs: Option<u64>,
    max_retries: Option<u32>,
    health_port: Option<u16>,
}

// ── Constructor ───────────────────────────────────────────────────────────────

impl EngineConfig {
    /// Build an [`EngineConfig`] from environment variables, optionally merged
    /// with a TOML file.
    ///
    /// Environment variables always override file values. Missing required
    /// fields and out-of-range values are reported via
    /// [`NonTransientError::ConfigInvalid`].
    ///
    /// # Errors
    /// Returns [`EngineError`] wrapping a [`NonTransientError::ConfigInvalid`]
    /// if any required field is absent or any value fails validation.
    pub fn from_env() -> Result<Self, EngineError> {
        // ── 1. Try to load optional config file ───────────────────────────────
        let config_file_path = std::env::var("CONFIG_FILE")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                let default = PathBuf::from("engine.toml");
                if default.exists() {
                    Some(default)
                } else {
                    None
                }
            });

        let file_cfg: FileConfig = match &config_file_path {
            Some(path) => {
                let raw = std::fs::read_to_string(path).map_err(|e| {
                    NonTransientError::ConfigInvalid {
                        field: "config_file".into(),
                        reason: e.to_string(),
                    }
                })?;
                toml::from_str(&raw).map_err(|e| NonTransientError::ConfigInvalid {
                    field: "config_file".into(),
                    reason: e.to_string(),
                })?
            }
            None => FileConfig::default(),
        };

        // ── Helper: env var overrides file value ──────────────────────────────
        let resolve_str = |env_key: &str, file_val: Option<String>| -> Option<String> {
            std::env::var(env_key).ok().or(file_val)
        };

        let resolve_u64 = |env_key: &str, file_val: Option<u64>| -> Option<u64> {
            std::env::var(env_key)
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file_val)
        };

        let resolve_u32 = |env_key: &str, file_val: Option<u32>| -> Option<u32> {
            std::env::var(env_key)
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file_val)
        };

        let resolve_u16 = |env_key: &str, file_val: Option<u16>| -> Option<u16> {
            std::env::var(env_key)
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file_val)
        };

        // ── 2. Resolve required fields ────────────────────────────────────────
        let keeper_keypair =
            resolve_str("KEEPER_KEYPAIR", file_cfg.keeper_keypair).ok_or_else(|| {
                NonTransientError::ConfigInvalid {
                    field: "KEEPER_KEYPAIR".into(),
                    reason: "required env var is missing or empty".into(),
                }
            })?;

        if keeper_keypair.trim().is_empty() {
            return Err(NonTransientError::ConfigInvalid {
                field: "KEEPER_KEYPAIR".into(),
                reason: "value must not be empty".into(),
            }
            .into());
        }

        let rpc_endpoint_url =
            resolve_str("RPC_ENDPOINT_URL", file_cfg.rpc_endpoint_url).ok_or_else(|| {
                NonTransientError::ConfigInvalid {
                    field: "RPC_ENDPOINT_URL".into(),
                    reason: "required env var is missing or empty".into(),
                }
            })?;

        if rpc_endpoint_url.trim().is_empty() {
            return Err(NonTransientError::ConfigInvalid {
                field: "RPC_ENDPOINT_URL".into(),
                reason: "value must not be empty".into(),
            }
            .into());
        }

        let anchor_contract_address =
            resolve_str("ANCHOR_CONTRACT_ADDRESS", file_cfg.anchor_contract_address)
                .ok_or_else(|| NonTransientError::ConfigInvalid {
                    field: "ANCHOR_CONTRACT_ADDRESS".into(),
                    reason: "required env var is missing or empty".into(),
                })?;

        if anchor_contract_address.trim().is_empty() {
            return Err(NonTransientError::ConfigInvalid {
                field: "ANCHOR_CONTRACT_ADDRESS".into(),
                reason: "value must not be empty".into(),
            }
            .into());
        }

        // ── 3. Resolve optional fields with defaults ──────────────────────────
        let poll_interval_secs = resolve_u64(
            "POLL_INTERVAL_SECS",
            file_cfg.poll_interval_secs,
        )
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);

        let task_query_timeout_secs = resolve_u64(
            "TASK_QUERY_TIMEOUT_SECS",
            file_cfg.task_query_timeout_secs,
        )
        .unwrap_or(DEFAULT_TASK_QUERY_TIMEOUT_SECS);

        let confirmation_timeout_secs = resolve_u64(
            "CONFIRMATION_TIMEOUT_SECS",
            file_cfg.confirmation_timeout_secs,
        )
        .unwrap_or(DEFAULT_CONFIRMATION_TIMEOUT_SECS);

        let retry_backoff_initial_secs = resolve_u64(
            "RETRY_BACKOFF_INITIAL_SECS",
            file_cfg.retry_backoff_initial_secs,
        )
        .unwrap_or(DEFAULT_RETRY_BACKOFF_INITIAL_SECS);

        let retry_backoff_max_secs = resolve_u64(
            "RETRY_BACKOFF_MAX_SECS",
            file_cfg.retry_backoff_max_secs,
        )
        .unwrap_or(DEFAULT_RETRY_BACKOFF_MAX_SECS);

        let max_retries =
            resolve_u32("MAX_RETRIES", file_cfg.max_retries).unwrap_or(DEFAULT_MAX_RETRIES);

        let health_port =
            resolve_u16("HEALTH_PORT", file_cfg.health_port).unwrap_or(DEFAULT_HEALTH_PORT);

        // ── 4. Validate ranges ────────────────────────────────────────────────
        if !(1..=5).contains(&poll_interval_secs) {
            return Err(NonTransientError::ConfigInvalid {
                field: "POLL_INTERVAL_SECS".into(),
                reason: format!("must be between 1 and 5 inclusive, got {poll_interval_secs}"),
            }
            .into());
        }

        if task_query_timeout_secs > 10 {
            return Err(NonTransientError::ConfigInvalid {
                field: "TASK_QUERY_TIMEOUT_SECS".into(),
                reason: format!(
                    "must be ≤ 10 seconds, got {task_query_timeout_secs}"
                ),
            }
            .into());
        }

        if confirmation_timeout_secs > 60 {
            return Err(NonTransientError::ConfigInvalid {
                field: "CONFIRMATION_TIMEOUT_SECS".into(),
                reason: format!(
                    "must be ≤ 60 seconds, got {confirmation_timeout_secs}"
                ),
            }
            .into());
        }

        if retry_backoff_initial_secs < 1 {
            return Err(NonTransientError::ConfigInvalid {
                field: "RETRY_BACKOFF_INITIAL_SECS".into(),
                reason: "must be ≥ 1 second".into(),
            }
            .into());
        }

        if retry_backoff_max_secs > 60 {
            return Err(NonTransientError::ConfigInvalid {
                field: "RETRY_BACKOFF_MAX_SECS".into(),
                reason: format!("must be ≤ 60 seconds, got {retry_backoff_max_secs}"),
            }
            .into());
        }

        if retry_backoff_max_secs < retry_backoff_initial_secs {
            return Err(NonTransientError::ConfigInvalid {
                field: "RETRY_BACKOFF_MAX_SECS".into(),
                reason: format!(
                    "must be ≥ RETRY_BACKOFF_INITIAL_SECS ({retry_backoff_initial_secs})"
                ),
            }
            .into());
        }

        if max_retries == 0 {
            warn!("MAX_RETRIES is 0 — tasks will not be retried on transient errors");
        }

        Ok(EngineConfig {
            keeper_keypair,
            rpc_endpoint_url,
            anchor_contract_address,
            poll_interval: Duration::from_secs(poll_interval_secs),
            task_query_timeout: Duration::from_secs(task_query_timeout_secs),
            confirmation_timeout: Duration::from_secs(confirmation_timeout_secs),
            retry_backoff_initial: Duration::from_secs(retry_backoff_initial_secs),
            retry_backoff_max: Duration::from_secs(retry_backoff_max_secs),
            max_retries,
            health_port,
            config_file_path,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // set_var / remove_var are marked unsafe in Rust ≥ 1.81 when called from
    // multi-threaded contexts. These tests each run in isolation (cargo test
    // spawns one thread per test by default) and are guarded by clear_env().
    // The allow attribute silences the deprecation lint on newer toolchains.
    #[allow(deprecated)]
    fn clear_env() {
        for key in &[
            "KEEPER_KEYPAIR",
            "RPC_ENDPOINT_URL",
            "ANCHOR_CONTRACT_ADDRESS",
            "POLL_INTERVAL_SECS",
            "TASK_QUERY_TIMEOUT_SECS",
            "CONFIRMATION_TIMEOUT_SECS",
            "RETRY_BACKOFF_INITIAL_SECS",
            "RETRY_BACKOFF_MAX_SECS",
            "MAX_RETRIES",
            "HEALTH_PORT",
            "CONFIG_FILE",
        ] {
            // SAFETY: tests run single-threaded per process; no other thread
            // reads these variables concurrently.
            unsafe { std::env::remove_var(key) };
        }
    }

    #[allow(deprecated)]
    fn set_required() {
        // SAFETY: see clear_env.
        unsafe {
            std::env::set_var("KEEPER_KEYPAIR", "SCZANGBA4XLMSEGEBHR5QKOH6X6EN73OOXNL7WI736DKCKDKB2XCJXE");
            std::env::set_var("RPC_ENDPOINT_URL", "https://soroban-testnet.stellar.org");
            std::env::set_var("ANCHOR_CONTRACT_ADDRESS", "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4");
        }
    }

    #[test]
    fn missing_keeper_keypair_returns_error() {
        clear_env();
        unsafe {
            std::env::set_var("RPC_ENDPOINT_URL", "https://example.com");
            std::env::set_var("ANCHOR_CONTRACT_ADDRESS", "CAAAA");
        }
        let err = EngineConfig::from_env().unwrap_err();
        assert!(err.to_string().contains("KEEPER_KEYPAIR"));
        clear_env();
    }

    #[test]
    fn missing_rpc_url_returns_error() {
        clear_env();
        unsafe {
            std::env::set_var("KEEPER_KEYPAIR", "SCZANGBA4XLMSEGEBHR5QKOH6X6EN73OOXNL7WI736DKCKDKB2XCJXE");
            std::env::set_var("ANCHOR_CONTRACT_ADDRESS", "CAAAA");
        }
        let err = EngineConfig::from_env().unwrap_err();
        assert!(err.to_string().contains("RPC_ENDPOINT_URL"));
        clear_env();
    }

    #[test]
    fn missing_anchor_address_returns_error() {
        clear_env();
        unsafe {
            std::env::set_var("KEEPER_KEYPAIR", "SCZANGBA4XLMSEGEBHR5QKOH6X6EN73OOXNL7WI736DKCKDKB2XCJXE");
            std::env::set_var("RPC_ENDPOINT_URL", "https://example.com");
        }
        let err = EngineConfig::from_env().unwrap_err();
        assert!(err.to_string().contains("ANCHOR_CONTRACT_ADDRESS"));
        clear_env();
    }

    #[test]
    fn poll_interval_out_of_range_returns_error() {
        clear_env();
        set_required();
        unsafe { std::env::set_var("POLL_INTERVAL_SECS", "6") };
        let err = EngineConfig::from_env().unwrap_err();
        assert!(err.to_string().contains("POLL_INTERVAL_SECS"));
        clear_env();
    }

    #[test]
    fn task_query_timeout_over_limit_returns_error() {
        clear_env();
        set_required();
        unsafe { std::env::set_var("TASK_QUERY_TIMEOUT_SECS", "11") };
        let err = EngineConfig::from_env().unwrap_err();
        assert!(err.to_string().contains("TASK_QUERY_TIMEOUT_SECS"));
        clear_env();
    }

    #[test]
    fn defaults_are_applied_when_optional_vars_absent() {
        clear_env();
        set_required();
        let cfg = EngineConfig::from_env().expect("valid config");
        assert_eq!(cfg.poll_interval, Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS));
        assert_eq!(cfg.health_port, DEFAULT_HEALTH_PORT);
        assert_eq!(cfg.max_retries, DEFAULT_MAX_RETRIES);
        clear_env();
    }

    #[test]
    fn env_var_overrides_default() {
        clear_env();
        set_required();
        unsafe { std::env::set_var("HEALTH_PORT", "9090") };
        let cfg = EngineConfig::from_env().expect("valid config");
        assert_eq!(cfg.health_port, 9090);
        clear_env();
    }
}
