//! Grace period monitoring and secondary-keeper dispatch.
//!
//! [`GraceMonitor`] evaluates tasks that are past their `next_allowed_execution`
//! ledger to decide whether this node should step in as a secondary keeper.
//!
//! ## Decision logic
//!
//! For a task at `next_allowed_execution = T` and current ledger `L`:
//!
//! | Condition | Action |
//! |-----------|--------|
//! | `L - T > 50` | Log missed-execution warning; no execution |
//! | `self.address == task.designated_keeper` | Skip (not eligible as secondary) |
//! | Task already in-flight | Skip |
//! | Otherwise | Dispatch via [`ExecutionPipeline`] as `Secondary` |

use tracing::{info, warn};

use crate::execution_pipeline::{ExecutionPipeline, ExecutionRole};
use crate::in_flight::InFlightSet;
use crate::rpc_client::RemoteTask;

// ── GraceMonitor ──────────────────────────────────────────────────────────────

/// Evaluates overdue tasks and dispatches secondary-keeper execution when
/// appropriate.
#[derive(Clone, Debug)]
pub struct GraceMonitor {
    /// This node's Stellar account address (strkey-encoded).
    keeper_address: String,
    /// Shared in-flight set to prevent duplicate submissions.
    in_flight: InFlightSet,
    /// Execution pipeline used to dispatch secondary execution.
    pipeline: ExecutionPipeline,
}

impl GraceMonitor {
    /// Construct a new [`GraceMonitor`].
    pub fn new(
        keeper_address: String,
        in_flight: InFlightSet,
        pipeline: ExecutionPipeline,
    ) -> Self {
        GraceMonitor {
            keeper_address,
            in_flight,
            pipeline,
        }
    }

    /// Evaluate whether this node should execute `task` as a secondary keeper.
    ///
    /// - If more than 50 ledgers have elapsed, logs a missed-execution warning
    ///   and returns without executing.
    /// - If this node is the designated keeper, skips (not eligible as secondary).
    /// - If the task is already in-flight, skips.
    /// - Otherwise, dispatches the task via the execution pipeline.
    pub async fn evaluate(&self, task: RemoteTask, current_ledger: u32) {
        let ledgers_elapsed = current_ledger.saturating_sub(task.next_allowed_execution);

        if ledgers_elapsed > 50 {
            self.log_missed(&task, current_ledger);
            return;
        }

        // Not eligible as secondary if we ARE the designated keeper.
        if self.keeper_address == task.designated_keeper {
            return;
        }

        // Skip if already processing this task.
        let task_id_bytes = parse_task_id_hex(&task.task_id_hex);
        if self.in_flight.contains(&task_id_bytes) {
            return;
        }

        info!(
            task_id = %task.task_id_hex,
            ledgers_elapsed,
            keeper = %self.keeper_address,
            "dispatching as secondary keeper"
        );

        self.pipeline.dispatch(task, ExecutionRole::Secondary).await;
    }

    /// Log a missed-execution warning without attempting execution.
    ///
    /// Called when `current_ledger > next_allowed_execution + 50` and also
    /// by `TaskDiscovery` directly for the post-grace open-execution path.
    pub fn log_missed(&self, task: &RemoteTask, current_ledger: u32) {
        let ledgers_elapsed = current_ledger.saturating_sub(task.next_allowed_execution);
        warn!(
            task_id = %task.task_id_hex,
            ledgers_elapsed,
            next_allowed = task.next_allowed_execution,
            current_ledger,
            "missed-execution: designated keeper did not execute in time"
        );
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_task_id_hex(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    let bytes = hex::decode(hex).unwrap_or_default();
    let len = bytes.len().min(32);
    out[..len].copy_from_slice(&bytes[..len]);
    out
}
