//! Execution pipeline: sign → submit → confirm → retry.
//!
//! [`ExecutionPipeline`] is the component that actually submits
//! `execute_drip_split` transactions to the Anchor contract.
//!
//! ## Retry policy
//!
//! Transient errors (I/O, timeout, connection reset) are retried up to
//! `max_retries` times with exponential backoff capped at `retry_backoff_max`.
//!
//! Non-transient errors (contract rejection) are logged and the task is
//! dropped from the in-flight set immediately with no retry.
//!
//! Confirmation timeout is treated as a permanent failure for the current
//! attempt: the task is removed from in-flight and a WARN is logged.  The
//! poller will re-discover the task on the next cycle if it is still executable.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use tokio::time::{sleep, timeout};
use tracing::{error, info, warn};

use crate::errors::{EngineError, NonTransientError};
use crate::in_flight::InFlightSet;
use crate::rpc_client::{RemoteTask, RpcClient, TxHash};

// ── Execution role ────────────────────────────────────────────────────────────

/// Which role this node is taking for the execution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionRole {
    /// This node is the designated keeper for the task.
    Designated,
    /// This node is stepping in as secondary keeper during the grace period.
    Secondary,
    /// The grace window has expired; any registered keeper may execute.
    PostGrace,
}

// ── ExecutionPipeline ─────────────────────────────────────────────────────────

/// Manages the full lifecycle of a single task execution attempt.
///
/// Constructed once and shared (via [`Clone`]) across async tasks.
#[derive(Clone, Debug)]
pub struct ExecutionPipeline {
    rpc: RpcClient,
    in_flight: InFlightSet,
    /// Strkey-encoded Stellar account address (G…) of the keeper.
    keeper_address: String,
    /// Raw 32-byte Ed25519 signing key bytes derived from the secret seed.
    signing_key_bytes: [u8; 32],
    anchor_contract_address: String,
    confirmation_timeout: Duration,
    retry_backoff_initial: Duration,
    retry_backoff_max: Duration,
    max_retries: u32,
    /// Unix timestamp (seconds) of the last successfully confirmed execution.
    last_confirmed_ts: Arc<AtomicI64>,
}

impl ExecutionPipeline {
    /// Construct a new [`ExecutionPipeline`].
    ///
    /// `signing_key_bytes` must be the 32-byte raw Ed25519 private scalar
    /// decoded from the Stellar secret seed (see `derive_keypair` in `main.rs`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rpc: RpcClient,
        in_flight: InFlightSet,
        keeper_address: String,
        signing_key_bytes: [u8; 32],
        anchor_contract_address: String,
        confirmation_timeout: Duration,
        retry_backoff_initial: Duration,
        retry_backoff_max: Duration,
        max_retries: u32,
    ) -> Self {
        ExecutionPipeline {
            rpc,
            in_flight,
            keeper_address,
            signing_key_bytes,
            anchor_contract_address,
            confirmation_timeout,
            retry_backoff_initial,
            retry_backoff_max,
            max_retries,
            last_confirmed_ts: Arc::new(AtomicI64::new(0)),
        }
    }

    /// Unix timestamp of the last successfully confirmed execution, or `None`
    /// if no execution has been confirmed yet.
    pub fn last_confirmed_ts(&self) -> Option<i64> {
        let ts = self.last_confirmed_ts.load(Ordering::Relaxed);
        if ts == 0 { None } else { Some(ts) }
    }

    /// Return a cheap shared handle to the last-confirmed-timestamp atomic so
    /// it can be wired directly into the [`HealthServer`] state.
    pub fn last_confirmed_ts_handle(&self) -> Arc<AtomicI64> {
        Arc::clone(&self.last_confirmed_ts)
    }

    /// Dispatch an execution attempt for `task` in the given `role`.
    ///
    /// 1. Inserts the task into the in-flight set (no-op if already present).
    /// 2. Runs the retry loop.
    /// 3. Removes the task from in-flight regardless of outcome.
    pub async fn dispatch(&self, task: RemoteTask, role: ExecutionRole) {
        let task_id = parse_task_id(&task.task_id_hex);

        if !self.in_flight.insert(task_id) {
            return; // already in-flight
        }

        self.execute_with_retry(&task, role).await;

        self.in_flight.remove(&task_id);
    }

    // ── Private ───────────────────────────────────────────────────────────────

    async fn execute_with_retry(&self, task: &RemoteTask, role: ExecutionRole) {
        let mut backoff = self.retry_backoff_initial;

        for attempt in 0..=self.max_retries {
            let signed_envelope = self.build_signed_transaction(task, role);

            match self.rpc.send_transaction(&signed_envelope).await {
                Ok(hash) => {
                    match self.wait_for_confirmation(&hash).await {
                        Ok(result) => {
                            if result.success {
                                let now_ts = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .map(|d| d.as_secs() as i64)
                                    .unwrap_or(0);
                                self.last_confirmed_ts.store(now_ts, Ordering::Relaxed);

                                info!(
                                    task_id  = %task.task_id_hex,
                                    keeper   = %self.keeper_address,
                                    ledger   = result.ledger,
                                    tx_hash  = %hash,
                                    role     = ?role,
                                    "task execution confirmed"
                                );
                            } else {
                                error!(
                                    task_id = %task.task_id_hex,
                                    tx_hash = %hash,
                                    error   = ?result.error_detail,
                                    "transaction confirmed but failed on-chain"
                                );
                            }
                            return;
                        }

                        Err(EngineError::NonTransient(
                            NonTransientError::ConfirmationTimeout { .. },
                        )) => {
                            warn!(
                                task_id = %task.task_id_hex,
                                tx_hash = %hash,
                                "confirmation timeout — will not retry"
                            );
                            return;
                        }

                        Err(e) => {
                            warn!(
                                task_id = %task.task_id_hex,
                                error   = %e,
                                "confirmation error"
                            );
                            return;
                        }
                    }
                }

                Err(EngineError::Transient(ref e)) => {
                    if attempt == self.max_retries {
                        error!(
                            task_id = %task.task_id_hex,
                            error   = %e,
                            attempt,
                            "max retries reached — dropping task"
                        );
                        return;
                    }
                    warn!(
                        task_id      = %task.task_id_hex,
                        error        = %e,
                        attempt,
                        backoff_secs = backoff.as_secs(),
                        "transient error — retrying"
                    );
                    sleep(backoff).await;
                    backoff = (backoff * 2).min(self.retry_backoff_max);
                }

                Err(EngineError::NonTransient(ref e)) => {
                    error!(
                        task_id = %task.task_id_hex,
                        error   = %e,
                        "non-transient error — dropping task immediately"
                    );
                    return;
                }
            }
        }
    }

    /// Wait up to `confirmation_timeout` for the transaction to appear on-chain.
    async fn wait_for_confirmation(
        &self,
        hash: &TxHash,
    ) -> Result<crate::rpc_client::TxResult, EngineError> {
        let poll_interval = Duration::from_secs(2);

        let result = timeout(self.confirmation_timeout, async {
            loop {
                match self.rpc.get_transaction(hash).await {
                    Ok(tx) if tx.success || tx.error_detail.is_some() => return Ok(tx),
                    Ok(_) => sleep(poll_interval).await,
                    Err(e) => return Err(e),
                }
            }
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_elapsed) => Err(NonTransientError::ConfirmationTimeout {
                elapsed: self.confirmation_timeout,
            }
            .into()),
        }
    }

    /// Build and sign the `execute_drip_split` transaction envelope.
    ///
    /// Signs a canonical message with the keeper's Ed25519 key and returns a
    /// hex-encoded envelope string.  In a full deployment this is replaced with
    /// a proper Stellar XDR `TransactionEnvelope` (using the `stellar-xdr`
    /// crate), but the signing primitive and key derivation are production-correct
    /// here — the signature is a real Ed25519 signature over a deterministic
    /// message that encodes the contract, task, keeper, and role.
    fn build_signed_transaction(&self, task: &RemoteTask, role: ExecutionRole) -> String {
        let signing_key = SigningKey::from_bytes(&self.signing_key_bytes);

        // Canonical message: all fields that uniquely identify this invocation.
        let msg = format!(
            "execute_drip_split:contract={}:task={}:keeper={}:role={:?}",
            self.anchor_contract_address,
            task.task_id_hex,
            self.keeper_address,
            role
        );

        let signature = signing_key.sign(msg.as_bytes());
        let sig_hex = hex::encode(signature.to_bytes());

        // Encode as a signed envelope string. Replace with a full Stellar XDR
        // TransactionEnvelope once the stellar-xdr crate is integrated.
        format!(
            "SIGNED:contract={}:task={}:keeper={}:role={:?}:sig={}",
            self.anchor_contract_address,
            task.task_id_hex,
            self.keeper_address,
            role,
            sig_hex
        )
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_task_id(hex_str: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    let bytes = hex::decode(hex_str).unwrap_or_default();
    let len = bytes.len().min(32);
    out[..len].copy_from_slice(&bytes[..len]);
    out
}
