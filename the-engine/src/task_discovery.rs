//! Task discovery: subscribe to ledger events, query executable tasks, dispatch.
//!
//! [`TaskDiscovery`] listens on the [`LedgerPoller`] broadcast channel.  On
//! each `LedgerEvent::Advance` it queries the Anchor contract for tasks whose
//! `next_allowed_execution ≤ current_ledger`, then classifies each task:
//!
//! | Condition | Action |
//! |-----------|--------|
//! | Already in-flight | Skip |
//! | `designated_keeper == self.address` AND `ledger == next_allowed` | `ExecutionPipeline::Designated` |
//! | `next_allowed < ledger ≤ next_allowed + 50` AND not designated | `GraceMonitor::evaluate` |
//! | `ledger > next_allowed + 50` | `GraceMonitor::log_missed` (+ optional post-grace dispatch) |
//!
//! The RPC query is wrapped in a 10-second `tokio::time::timeout`; if the
//! timeout fires, a WARN is logged and the cycle is skipped.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::timeout;
use tracing::{info, warn};

use crate::execution_pipeline::{ExecutionPipeline, ExecutionRole};
use crate::grace_monitor::GraceMonitor;
use crate::in_flight::InFlightSet;
use crate::ledger_poller::LedgerEvent;
use crate::rpc_client::{RemoteTask, RpcClient};

/// Local cache mapping task ID hex → [`RemoteTask`].
pub type TaskCache = Arc<Mutex<HashMap<String, RemoteTask>>>;

// ── TaskDiscovery ─────────────────────────────────────────────────────────────

/// Subscribes to ledger events and dispatches executable tasks.
#[derive(Clone, Debug)]
pub struct TaskDiscovery {
    rpc: RpcClient,
    anchor_contract_address: String,
    keeper_address: String,
    in_flight: InFlightSet,
    pipeline: ExecutionPipeline,
    grace_monitor: GraceMonitor,
    task_cache: TaskCache,
    task_query_timeout: Duration,
}

impl TaskDiscovery {
    /// Construct a new [`TaskDiscovery`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rpc: RpcClient,
        anchor_contract_address: String,
        keeper_address: String,
        in_flight: InFlightSet,
        pipeline: ExecutionPipeline,
        grace_monitor: GraceMonitor,
        task_cache: TaskCache,
        task_query_timeout: Duration,
    ) -> Self {
        TaskDiscovery {
            rpc,
            anchor_contract_address,
            keeper_address,
            in_flight,
            pipeline,
            grace_monitor,
            task_cache,
            task_query_timeout,
        }
    }

    /// Run the discovery loop, consuming events from `rx`.
    ///
    /// Loops indefinitely; cancel via the task handle.
    pub async fn run(self, mut rx: broadcast::Receiver<LedgerEvent>) {
        loop {
            match rx.recv().await {
                Ok(LedgerEvent::Advance(seq)) => {
                    self.on_ledger_advance(seq).await;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "TaskDiscovery: broadcast channel lagged; some ledger events were skipped");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    warn!("TaskDiscovery: ledger broadcast channel closed; exiting loop");
                    return;
                }
            }
        }
    }

    // ── Private ───────────────────────────────────────────────────────────────

    async fn on_ledger_advance(&self, current_ledger: u32) {
        // Query with a hard timeout of `task_query_timeout`.
        let query_result = timeout(
            self.task_query_timeout,
            self.rpc
                .query_executable_tasks(&self.anchor_contract_address, current_ledger),
        )
        .await;

        let tasks = match query_result {
            Ok(Ok(tasks)) => tasks,
            Ok(Err(e)) => {
                warn!(ledger = current_ledger, error = %e, "task query RPC error; skipping cycle");
                return;
            }
            Err(_elapsed) => {
                warn!(ledger = current_ledger, "task query timed out after {:?}; skipping cycle", self.task_query_timeout);
                return;
            }
        };

        if !tasks.is_empty() {
            info!(ledger = current_ledger, count = tasks.len(), "discovered executable tasks");
        }

        // Update local task cache.
        {
            let mut cache = self.task_cache.lock().expect("task cache poisoned");
            for task in &tasks {
                cache.insert(task.task_id_hex.clone(), task.clone());
            }
        }

        for task in tasks {
            self.classify_and_dispatch(task, current_ledger).await;
        }
    }

    async fn classify_and_dispatch(&self, task: RemoteTask, current_ledger: u32) {
        // Skip tasks already being processed.
        let task_id_bytes = parse_hex(&task.task_id_hex);
        if self.in_flight.contains(&task_id_bytes) {
            return;
        }

        let next = task.next_allowed_execution;

        if current_ledger == next && task.designated_keeper == self.keeper_address {
            // Region 2: designated window — this node is the designated keeper.
            self.pipeline
                .dispatch(task, ExecutionRole::Designated)
                .await;
        } else if current_ledger > next && current_ledger <= next + 50 {
            // Region 3: grace period.
            self.grace_monitor.evaluate(task, current_ledger).await;
        } else if current_ledger > next + 50 {
            // Region 4: post-grace open execution.
            // Log missed, then any registered keeper (including this node) may execute.
            self.grace_monitor.log_missed(&task, current_ledger);
            // Attempt post-grace execution as this node.
            self.pipeline
                .dispatch(task, ExecutionRole::PostGrace)
                .await;
        }
        // Region 1 (current_ledger < next) is handled upstream — the RPC query
        // only returns tasks where next_allowed_execution <= current_ledger.
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_hex(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    let bytes = hex::decode(hex).unwrap_or_default();
    let len = bytes.len().min(32);
    out[..len].copy_from_slice(&bytes[..len]);
    out
}
