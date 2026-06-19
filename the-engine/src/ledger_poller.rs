//! Ledger polling loop.
//!
//! [`LedgerPoller`] continuously polls the Soroban RPC endpoint for the latest
//! confirmed ledger sequence.  Whenever the sequence advances, it broadcasts a
//! [`LedgerEvent::Advance`] on a Tokio broadcast channel that `TaskDiscovery`
//! and `GraceMonitor` subscribe to.
//!
//! ## Backoff behaviour
//!
//! On transient RPC errors the poller applies exponential backoff starting at
//! 1 second and capped at `retry_backoff_max`.  The poll loop resumes normally
//! after a successful response.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::errors::TransientError;
use crate::rpc_client::RpcClient;

// ── Event types ───────────────────────────────────────────────────────────────

/// Events broadcast by the [`LedgerPoller`].
#[derive(Debug, Clone)]
pub enum LedgerEvent {
    /// The observed ledger sequence has advanced to the contained value.
    Advance(u32),
}

// ── LedgerPoller ──────────────────────────────────────────────────────────────

/// Polls the Stellar RPC endpoint and broadcasts [`LedgerEvent`]s.
///
/// Construct via [`LedgerPoller::new`] then call [`LedgerPoller::run`] inside
/// a `tokio::spawn` call.
#[derive(Debug, Clone)]
pub struct LedgerPoller {
    rpc: RpcClient,
    poll_interval: Duration,
    retry_backoff_initial: Duration,
    retry_backoff_max: Duration,
    last_known_ledger: Arc<AtomicU32>,
}

impl LedgerPoller {
    /// Create a new [`LedgerPoller`].
    ///
    /// # Parameters
    /// - `rpc`                   — shared RPC client
    /// - `poll_interval`         — how often to poll (1–5 s per config)
    /// - `retry_backoff_initial` — starting backoff on transient errors
    /// - `retry_backoff_max`     — backoff cap
    pub fn new(
        rpc: RpcClient,
        poll_interval: Duration,
        retry_backoff_initial: Duration,
        retry_backoff_max: Duration,
    ) -> Self {
        LedgerPoller {
            rpc,
            poll_interval,
            retry_backoff_initial,
            retry_backoff_max,
            last_known_ledger: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Return the most recently observed ledger sequence.
    ///
    /// Returns 0 before the first successful poll.
    pub fn last_known(&self) -> u32 {
        self.last_known_ledger.load(Ordering::Relaxed)
    }

    /// A cheap handle to the shared ledger counter, suitable for sharing with
    /// the `HealthServer`.
    pub fn ledger_handle(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.last_known_ledger)
    }

    /// Run the poller loop, broadcasting [`LedgerEvent::Advance`] whenever the
    /// ledger advances.
    ///
    /// This method loops indefinitely. Cancel it by dropping the task handle or
    /// using a cancellation token.
    pub async fn run(&self, tx: broadcast::Sender<LedgerEvent>) {
        let mut backoff = self.retry_backoff_initial;
        let mut consecutive_errors: u32 = 0;

        loop {
            sleep(self.poll_interval).await;

            match self.rpc.get_latest_ledger().await {
                Ok(seq) => {
                    // Reset backoff on success.
                    if consecutive_errors > 0 {
                        info!(seq, "RPC connectivity restored after transient errors");
                        backoff = self.retry_backoff_initial;
                        consecutive_errors = 0;
                    }

                    let last = self.last_known_ledger.load(Ordering::Relaxed);

                    if seq > last {
                        self.last_known_ledger.store(seq, Ordering::Relaxed);
                        // Best-effort broadcast; ignore SendError if no subscribers.
                        let _ = tx.send(LedgerEvent::Advance(seq));
                        info!(ledger = seq, "ledger advance detected");
                    }
                }

                Err(TransientError::RpcTimeout) | Err(TransientError::ConnectionReset) => {
                    consecutive_errors += 1;
                    warn!(
                        consecutive_errors,
                        backoff_secs = backoff.as_secs(),
                        "transient RPC error; backing off"
                    );
                    sleep(backoff).await;
                    backoff = (backoff * 2).min(self.retry_backoff_max);
                }

                Err(e) => {
                    consecutive_errors += 1;
                    warn!(
                        error = %e,
                        consecutive_errors,
                        backoff_secs = backoff.as_secs(),
                        "ledger poll error; backing off"
                    );
                    sleep(backoff).await;
                    backoff = (backoff * 2).min(self.retry_backoff_max);
                }
            }
        }
    }
}
