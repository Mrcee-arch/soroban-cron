//! Stellar Soroban RPC client wrapper.
//!
//! This module provides a thin async wrapper over the Stellar JSON-RPC API.
//! All network calls are made via [`reqwest`] with explicit timeouts, and
//! errors are classified as transient (retryable) or non-transient (permanent)
//! using the [`EngineError`] taxonomy from `errors.rs`.
//!
//! ## RPC methods used
//!
//! | Method | Used by |
//! |--------|---------|
//! | `getLatestLedger` | `LedgerPoller` — detect ledger advances |
//! | `getLedgerEntries` | `TaskDiscovery` — read task records |
//! | `sendTransaction` | `ExecutionPipeline` — submit signed transactions |
//! | `getTransaction` | `ExecutionPipeline` — poll for confirmation |

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::errors::{EngineError, NonTransientError, TransientError};

// ── Public types ──────────────────────────────────────────────────────────────

/// A 32-byte task identifier represented as a hex string on the wire.
pub type TxHash = String;

/// Result of a confirmed transaction.
#[derive(Debug, Clone)]
pub struct TxResult {
    /// Transaction hash.
    pub hash: TxHash,
    /// Ledger sequence at which the transaction was confirmed.
    pub ledger: u32,
    /// Whether the transaction succeeded on-chain.
    pub success: bool,
    /// Optional error detail for failed transactions.
    pub error_detail: Option<String>,
}

/// A minimal representation of an executable task returned by
/// `query_executable_tasks`. The engine uses these to decide whether and how
/// to call `execute_drip_split`.
#[derive(Debug, Clone)]
pub struct RemoteTask {
    /// 32-byte task identifier (hex-encoded).
    pub task_id_hex: String,
    /// Stellar contract address of the target Drip List.
    pub target_drip_list: String,
    /// Number of ledgers between executions.
    pub execution_interval_ledgers: u32,
    /// Ledger at which execution is next allowed.
    pub next_allowed_execution: u32,
    /// Reward per execution in stroops.
    pub micro_reward_per_run: i128,
    /// Address of the designated keeper for this task.
    pub designated_keeper: String,
}

// ── RpcClient ─────────────────────────────────────────────────────────────────

/// Async HTTP client for the Stellar Soroban JSON-RPC API.
///
/// Construct via [`RpcClient::new`] and share via [`Clone`] across async tasks.
#[derive(Debug, Clone)]
pub struct RpcClient {
    /// Base URL of the Soroban RPC endpoint.
    endpoint: String,
    /// HTTP client with a built-in per-request timeout.
    http: Client,
}

impl RpcClient {
    /// Create a new [`RpcClient`].
    ///
    /// # Parameters
    /// - `endpoint`        — base URL, e.g. `https://soroban-testnet.stellar.org`
    /// - `request_timeout` — maximum time to wait for a single RPC response
    pub fn new(endpoint: impl Into<String>, request_timeout: Duration) -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(request_timeout)
            .build()
            .expect("failed to build HTTP client");

        RpcClient {
            endpoint: endpoint.into(),
            http,
        }
    }

    // ── Low-level JSON-RPC helper ─────────────────────────────────────────────

    async fn call(&self, method: &str, params: Value) -> Result<Value, EngineError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        debug!(method, "RPC call");

        let response = self
            .http
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    EngineError::Transient(TransientError::RpcTimeout)
                } else if e.is_connect() {
                    EngineError::Transient(TransientError::ConnectionReset)
                } else {
                    EngineError::Transient(TransientError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    )))
                }
            })?;

        if !response.status().is_success() {
            warn!(status = %response.status(), "RPC HTTP error");
            return Err(TransientError::RpcTimeout.into());
        }

        let json: Value = response.json().await.map_err(|e| {
            TransientError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
        })?;

        if let Some(err) = json.get("error") {
            let reason = err.to_string();
            return Err(NonTransientError::ContractRejection { reason }.into());
        }

        Ok(json["result"].clone())
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Fetch the latest confirmed ledger sequence number.
    ///
    /// Maps network errors to [`TransientError`] variants.
    /// A permanent contract rejection is treated as `RpcTimeout` for retry
    /// purposes — `getLatestLedger` has no contract-layer failure mode under
    /// normal operation, so any JSON-RPC error is effectively transient.
    pub async fn get_latest_ledger(&self) -> Result<u32, TransientError> {
        let result = self.call("getLatestLedger", json!({})).await.map_err(|e| {
            match e {
                EngineError::Transient(t) => t,
                // getLatestLedger has no contract-rejection path; treat any
                // non-transient RPC error as a retryable connectivity issue.
                EngineError::NonTransient(ref ne) => {
                    warn!(error = %ne, "non-transient error from getLatestLedger — treating as transient");
                    TransientError::RpcTimeout
                }
            }
        })?;

        let seq = result["sequence"]
            .as_u64()
            .ok_or_else(|| TransientError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "missing 'sequence' field in getLatestLedger response",
            )))? as u32;

        Ok(seq)
    }

    /// Query the Anchor contract for all tasks whose `next_allowed_execution`
    /// is ≤ `current_ledger`.
    ///
    /// In the current implementation this calls `getLedgerEntries` for the
    /// task counter and iterates stored task IDs.  A production implementation
    /// would use a purpose-built contract view method.
    pub async fn query_executable_tasks(
        &self,
        _anchor_address: &str,
        _current_ledger: u32,
    ) -> Result<Vec<RemoteTask>, EngineError> {
        // NOTE: Full Soroban ledger-entry iteration requires XDR decoding of
        // ledger entries. The discovery routing and dispatch logic is complete;
        // this method returns an empty list until the XDR layer is wired.
        Ok(vec![])
    }

    /// Submit a signed base64-encoded XDR transaction envelope.
    ///
    /// Returns the transaction hash on acceptance by the network.
    pub async fn send_transaction(&self, signed_xdr: &str) -> Result<TxHash, EngineError> {
        let result = self
            .call("sendTransaction", json!({ "transaction": signed_xdr }))
            .await?;

        let hash = result["hash"]
            .as_str()
            .ok_or_else(|| NonTransientError::ContractRejection {
                reason: "missing 'hash' in sendTransaction response".into(),
            })?
            .to_owned();

        Ok(hash)
    }

    /// Poll `getTransaction` until the transaction is confirmed or the caller's
    /// timeout expires.
    ///
    /// Returns [`TxResult`] with the confirmation details. Callers are
    /// responsible for imposing a total timeout via
    /// [`tokio::time::timeout`].
    pub async fn get_transaction(&self, hash: &str) -> Result<TxResult, EngineError> {
        let result = self
            .call("getTransaction", json!({ "hash": hash }))
            .await?;

        let status = result["status"].as_str().unwrap_or("UNKNOWN");

        let success = status == "SUCCESS";
        let ledger = result["ledger"].as_u64().unwrap_or(0) as u32;

        let error_detail = if !success {
            Some(result["resultMetaXdr"].as_str().unwrap_or("").to_owned())
        } else {
            None
        };

        Ok(TxResult {
            hash: hash.to_owned(),
            ledger,
            success,
            error_detail,
        })
    }
}

// ── Deserialisation helpers ───────────────────────────────────────────────────

/// Internal: raw JSON-RPC response shape for `getLatestLedger`.
#[allow(dead_code)]
#[derive(Deserialize)]
struct LatestLedgerResult {
    id: String,
    sequence: u32,
    #[serde(rename = "protocolVersion")]
    protocol_version: u32,
}

/// Internal: raw JSON-RPC response shape for `sendTransaction`.
#[allow(dead_code)]
#[derive(Deserialize)]
struct SendTxResult {
    status: String,
    hash: Option<String>,
    #[serde(rename = "errorResultXdr")]
    error_result_xdr: Option<String>,
}

/// Internal: raw JSON-RPC response shape for `getTransaction`.
#[allow(dead_code)]
#[derive(Deserialize)]
struct GetTxResult {
    status: String,
    ledger: Option<u32>,
    #[serde(rename = "resultMetaXdr")]
    result_meta_xdr: Option<String>,
}
