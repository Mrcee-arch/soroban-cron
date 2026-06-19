//! Task provisioning and lookup for the Anchor contract.
//!
//! This module owns all logic related to the on-chain task registry:
//! - [`provision_task`] — admin-gated creation of a new recurring execution
//!   task, including unique ID generation, validation, and event emission.
//! - [`get_task`] — read-only lookup of a provisioned task by its [`TaskId`].
//! - [`update_task`] — write an updated [`ExecutionTask`] record back to
//!   Persistent storage (used by `execution.rs` after a successful execution).

use chronos_types::{ExecutionTask, TaskId};
use soroban_sdk::{Address, Bytes, Env};

use crate::errors::ContractError;
use crate::storage_keys::DataKey;

// ── Public API ────────────────────────────────────────────────────────────────

/// Provisions a new recurring execution task on-chain.
///
/// This function is governance-gated: the stored admin address is loaded from
/// Instance storage and its authorisation is required before any mutation takes
/// place.  All parameters are validated before any state is written.
///
/// # Steps
///
/// 1. Load `DataKey::Admin` from Instance storage; call `admin.require_auth()`.
///    If no admin is stored, return [`ContractError::UnauthorizedCaller`].
/// 2. Validate `execution_interval_ledgers > 0`; return
///    [`ContractError::InvalidInterval`] if zero.
/// 3. Validate `micro_reward_per_run > 0`; return
///    [`ContractError::InvalidReward`] if not.
/// 4. Verify `designated_keeper` is registered via
///    [`crate::registry::is_registered`]; return
///    [`ContractError::KeeperNotFound`] if absent.
/// 5. Verify `designated_keeper` is eligible via
///    [`crate::registry::is_eligible`]; return
///    [`ContractError::IneligibleKeeper`] if flagged.
/// 6. `target_drip_list` is accepted as any valid [`Address`] in this version.
///    On-chain contract-existence validation requires a cross-contract call
///    and is planned for a future release.
/// 7. Generate a unique `TaskId` by:
///    - Reading and incrementing the monotonic `TaskCounter` (stored as `u32`
///      in Instance storage).
///    - Building an 8-byte buffer: `[counter_be || ledger_seq_be]`.
///    - Hashing the buffer with `env.crypto().sha256()` to produce a
///      `BytesN<32>`.
/// 8. Set `next_allowed_execution = current_ledger + execution_interval_ledgers`.
/// 9. Construct and persist the [`ExecutionTask`] in Persistent storage at
///    `DataKey::Task(task_id)`.
/// 10. Emit a `TaskProvisioned` event via [`crate::events::emit_task_provisioned`]
///     (stubbed until task 8).
/// 11. Return `Ok(task_id)`.
///
/// # Errors
///
/// - [`ContractError::UnauthorizedCaller`] — no admin stored or auth fails.
/// - [`ContractError::InvalidInterval`] — `execution_interval_ledgers == 0`.
/// - [`ContractError::InvalidReward`] — `micro_reward_per_run <= 0`.
/// - [`ContractError::KeeperNotFound`] — `designated_keeper` not in registry.
/// - [`ContractError::IneligibleKeeper`] — keeper registered but ineligible.
///
/// _Validates: Requirements 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8_
pub fn provision_task(
    env: &Env,
    target_drip_list: Address,
    execution_interval_ledgers: u32,
    micro_reward_per_run: i128,
    designated_keeper: Address,
) -> Result<TaskId, ContractError> {
    // Step 1 — load admin and require their authorisation.
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(ContractError::UnauthorizedCaller)?;
    admin.require_auth();

    // Step 2 — reject a zero execution interval.
    if execution_interval_ledgers == 0 {
        return Err(ContractError::InvalidInterval);
    }

    // Step 3 — reject a non-positive reward.
    if micro_reward_per_run <= 0 {
        return Err(ContractError::InvalidReward);
    }

    // Step 4 — ensure designated_keeper is a registered keeper.
    if !crate::registry::is_registered(env, &designated_keeper) {
        return Err(ContractError::KeeperNotFound);
    }

    // Step 5 — ensure designated_keeper is not flagged as ineligible.
    if !crate::registry::is_eligible(env, &designated_keeper) {
        return Err(ContractError::IneligibleKeeper);
    }

    // Step 6 — target_drip_list: accepted as any valid Address in MVP.
    // On-chain Drip List contract existence validation requires a cross-contract
    // call; deferred until the next version.

    // Step 7 — generate a unique TaskId via SHA-256(counter_be || ledger_be).
    let counter: u32 = env
        .storage()
        .instance()
        .get(&DataKey::TaskCounter)
        .unwrap_or(0u32);
    env.storage()
        .instance()
        .set(&DataKey::TaskCounter, &(counter + 1));

    let current_ledger: u32 = env.ledger().sequence();

    // Build an 8-byte seed: first 4 bytes = counter, last 4 bytes = ledger seq.
    let mut buf = [0u8; 8];
    buf[0..4].copy_from_slice(&counter.to_be_bytes());
    buf[4..8].copy_from_slice(&current_ledger.to_be_bytes());
    let seed = Bytes::from_array(env, &buf);
    let task_id: TaskId = env.crypto().sha256(&seed);

    // Step 8 — set the first allowed execution ledger.
    let next_allowed_execution: u32 = current_ledger + execution_interval_ledgers;

    // Step 9 — build the ExecutionTask and persist it.
    let task = ExecutionTask {
        task_id: task_id.clone(),
        target_drip_list: target_drip_list.clone(),
        execution_interval_ledgers,
        next_allowed_execution,
        micro_reward_per_run,
        designated_keeper: designated_keeper.clone(),
    };
    env.storage()
        .persistent()
        .set(&DataKey::Task(task_id.clone()), &task);

    // Step 10 — emit TaskProvisioned event.
    crate::events::emit_task_provisioned(
        env,
        &task_id,
        &target_drip_list,
        execution_interval_ledgers,
        micro_reward_per_run,
        &designated_keeper,
    );

    // Step 11 — return the generated task identifier.
    Ok(task_id)
}

/// Read-only lookup returning the full [`ExecutionTask`] struct for a given
/// [`TaskId`].
///
/// # Errors
///
/// - [`ContractError::TaskNotFound`] — no task is stored under `task_id` in
///   Persistent storage.
///
/// _Validates: Requirements 2.5_
pub fn get_task(env: &Env, task_id: TaskId) -> Result<ExecutionTask, ContractError> {
    env.storage()
        .persistent()
        .get::<DataKey, ExecutionTask>(&DataKey::Task(task_id))
        .ok_or(ContractError::TaskNotFound)
}

/// Write an updated [`ExecutionTask`] record back to Persistent storage.
///
/// Used by `execution.rs` after a successful [`crate::execution::execute_drip_split`]
/// call to advance `next_allowed_execution` to the next scheduled ledger.
///
/// The `task.task_id` field is used as the storage key; callers must ensure
/// they pass the correct task struct.
pub fn update_task(env: &Env, task: &ExecutionTask) {
    env.storage()
        .persistent()
        .set(&DataKey::Task(task.task_id.clone()), task);
}
