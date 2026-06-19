//! Core execution pipeline for the Anchor contract.
//!
//! This module implements the four-region window arbitration model described in
//! the design document (Property 5) and enforced by Requirements 3 and 4.
//!
//! ## Window regions
//!
//! Given a task with `next_allowed_execution = T` and current ledger `L`:
//!
//! | Region | Condition | Allowed callers |
//! |--------|-----------|-----------------|
//! | Pre-window  | `L < T`            | None (→ `TooEarlyToExecute`) |
//! | Designated  | `L == T`           | `designated_keeper` only |
//! | Grace       | `T < L ≤ T + 50`   | Any registered keeper except `designated_keeper` |
//! | Post-grace  | `L > T + 50`       | Any registered keeper, no slash |
//!
//! ## Atomicity
//!
//! The slash, secondary-reward transfer, Drip List invocation, keeper-record
//! update, and task-schedule advance all happen within a single Soroban
//! invocation — no partial state is visible on-chain.

use chronos_types::{ExecutionTask, TaskId};
use soroban_sdk::{token::TokenClient, Address, Env, Symbol, Vec};

use crate::errors::ContractError;
use crate::registry::{get_keeper, is_registered, update_keeper};
use crate::storage_keys::DataKey;
use crate::tasks::{get_task, update_task};

// ── Public entry point ────────────────────────────────────────────────────────

/// Execute a Drip distribution split according to the window arbitration rules.
///
/// Requires the `caller` to have authorised this invocation via
/// `caller.require_auth()`.
///
/// # Window arbitration
///
/// 1. **Pre-window** (`current_ledger < next_allowed_execution`):
///    Returns [`ContractError::TooEarlyToExecute`].
///
/// 2. **Designated window** (`current_ledger == next_allowed_execution`):
///    Only the task's `designated_keeper` may execute.  Any other caller
///    receives [`ContractError::UnauthorizedExecutor`].
///
/// 3. **Grace period** (`0 < current_ledger - next_allowed_execution ≤ 50`):
///    Only a *secondary* keeper (registered, not the designated keeper) may
///    execute.  The designated keeper is blocked with
///    [`ContractError::GracePeriodActive`].  An unregistered caller receives
///    [`ContractError::CallerNotSecondaryEligible`].
///    On success: the designated keeper's stake is slashed 5 % (floor);
///    50 % of the slash is transferred to the secondary keeper as
///    `secondary_reward`; the remaining 50 % goes to the treasury.
///
/// 4. **Post-grace** (`current_ledger - next_allowed_execution > 50`):
///    Any registered keeper may execute without slash penalty.  Emits a
///    [`MissedExecution`] event before proceeding.
///
/// # Common success path (all regions)
///
/// - Cross-contract call to `task.target_drip_list` → `distribute_wave_splits`.
/// - Transfer `micro_reward_per_run` to `caller` from the contract's native-
///   token balance (reverts with [`ContractError::InsufficientRewardBalance`]
///   if the balance is too low).
/// - Update `task.next_allowed_execution = current_ledger + execution_interval_ledgers`.
/// - Increment `caller`'s `total_executions` and `last_execution_ledger`.
/// - Emit an execution event.
///
/// # Errors
///
/// See the window arbitration table above for region-specific errors.
/// Additional errors: [`ContractError::TaskNotFound`],
/// [`ContractError::InsufficientRewardBalance`].
///
/// _Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8,
///             4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.9_
pub fn execute_drip_split(
    env: &Env,
    task_id: TaskId,
    caller: Address,
) -> Result<(), ContractError> {
    // ── Load task ─────────────────────────────────────────────────────────────
    let task = get_task(env, task_id.clone())?;

    // ── Require caller authorisation ──────────────────────────────────────────
    caller.require_auth();

    let current_ledger = env.ledger().sequence();

    // ── Region 1: Pre-window ──────────────────────────────────────────────────
    // Requirement 3.5 / Property 5: reject calls before the window opens.
    if current_ledger < task.next_allowed_execution {
        return Err(ContractError::TooEarlyToExecute);
    }

    let ledgers_elapsed = current_ledger - task.next_allowed_execution;

    if ledgers_elapsed == 0 {
        // ── Region 2: Designated keeper window ────────────────────────────────
        // Requirement 3.1 / Property 5: exactly at `next_allowed_execution`;
        // only the designated keeper may execute.
        if caller != task.designated_keeper {
            return Err(ContractError::UnauthorizedExecutor);
        }

        // Execute via designated path (no slash).
        execute_distribution(env, &task, &caller, false)?;

    } else if ledgers_elapsed <= 50 {
        // ── Region 3: Grace period ────────────────────────────────────────────
        // Requirement 4.7 / Property 5: designated keeper is blocked.
        if caller == task.designated_keeper {
            return Err(ContractError::GracePeriodActive);
        }

        // Requirement 4.1 / Property 5: caller must be a registered keeper.
        if !is_registered(env, &caller) {
            return Err(ContractError::CallerNotSecondaryEligible);
        }

        // Requirement 4.2: apply 5 % slash to the designated keeper.
        // `apply_slash` handles the zero-stake guard and treasury accounting.
        let (slash_amount, secondary_reward) =
            crate::slashing::apply_slash(env, &task.designated_keeper)?;

        // Execute distribution; the secondary reward is transferred below.
        execute_distribution(env, &task, &caller, true)?;

        // Transfer `secondary_reward` on top of the `micro_reward_per_run` that
        // `execute_distribution` already transferred.  The secondary reward
        // comes from the slashed amount that was deducted from the designated
        // keeper's stake record; the contract holds those tokens as part of its
        // treasury/balance managed by the native token contract.
        //
        // Requirement 4.4: secondary_reward (50 % of slash) → secondary keeper.
        if secondary_reward > 0 {
            let native_token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::NativeToken)
                .expect("native token not initialised");
            let token = TokenClient::new(env, &native_token_addr);
            let contract_address = env.current_contract_address();
            let balance = token.balance(&contract_address);
            if balance < secondary_reward {
                return Err(ContractError::InsufficientRewardBalance);
            }
            token.transfer(&contract_address, &caller, &secondary_reward);
        }

        // Emit secondary execution event.
        crate::events::emit_task_executed_by_secondary(
            env,
            &task_id,
            &caller,
            &task.designated_keeper,
            current_ledger,
            slash_amount,
            secondary_reward,
            task.micro_reward_per_run,
        );

    } else {
        // ── Region 4: Post-grace open execution ───────────────────────────────
        // Requirement 4.9 / Property 5: any registered keeper, no slash.
        if !is_registered(env, &caller) {
            return Err(ContractError::UnauthorizedExecutor);
        }

        // Requirement 4.9: emit missed-execution event before proceeding.
        crate::events::emit_missed_execution(env, &task_id, ledgers_elapsed);

        execute_distribution(env, &task, &caller, false)?;
    }

    Ok(())
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Shared execution logic for all success paths.
///
/// Performs the following steps atomically within the same Soroban invocation:
///
/// 1. Cross-contract invocation of the target Drip List contract.
///    Soroban reverts the entire transaction on cross-contract failure, which
///    satisfies Requirement 3.6 automatically.
///
/// 2. Transfer `micro_reward_per_run` from the contract's native-token balance
///    to `caller`.  Reverts with [`ContractError::InsufficientRewardBalance`]
///    if the balance is insufficient (Requirement 3.2).
///
/// 3. Advance `task.next_allowed_execution = current_ledger + interval`
///    and persist the updated task (Requirement 3.3 / Property 6).
///
/// 4. Increment `caller`'s `total_executions` and update
///    `last_execution_ledger` in the keeper registry (Requirement 3.4 / 4.6).
///
/// 5. Emit a `TaskExecuted` event (Requirement 3.7) — skipped for the secondary
///    path because `execute_drip_split` emits `TaskExecutedBySecondary` there.
///
/// # Parameters
/// - `task`         — the loaded [`ExecutionTask`] record.
/// - `caller`       — the keeper executing the task.
/// - `is_secondary` — when `true`, the `TaskExecuted` event is suppressed
///                    because the caller emits `TaskExecutedBySecondary` after
///                    this function returns.
fn execute_distribution(
    env: &Env,
    task: &ExecutionTask,
    caller: &Address,
    is_secondary: bool,
) -> Result<(), ContractError> {
    let current_ledger = env.ledger().sequence();

    // ── Step 1: Cross-contract call to the target Drip List ───────────────────
    // Requirement 3.6: if the invocation panics, Soroban reverts the whole tx.
    // We call `distribute_wave_splits` with no arguments.
    env.invoke_contract::<()>(
        &task.target_drip_list,
        &Symbol::new(env, "distribute_wave_splits"),
        Vec::new(env),
    );

    // ── Step 2: Transfer micro_reward_per_run to caller ───────────────────────
    // Requirement 3.2: revert if balance is insufficient.
    let native_token_addr: Address = env
        .storage()
        .instance()
        .get(&DataKey::NativeToken)
        .expect("native token not initialised");
    let token = TokenClient::new(env, &native_token_addr);
    let contract_address = env.current_contract_address();
    let balance = token.balance(&contract_address);
    if balance < task.micro_reward_per_run {
        return Err(ContractError::InsufficientRewardBalance);
    }
    token.transfer(&contract_address, caller, &task.micro_reward_per_run);

    // ── Step 3: Advance task schedule ─────────────────────────────────────────
    // Requirement 3.3 / Property 6: new_next > old_next always holds because
    // execution_interval_ledgers > 0 is enforced by provision_task.
    let mut updated_task = task.clone();
    updated_task.next_allowed_execution =
        current_ledger + task.execution_interval_ledgers;
    update_task(env, &updated_task);

    // ── Step 4: Update executing keeper record ────────────────────────────────
    // Requirement 3.4 / 4.6: increment total_executions and last_execution_ledger.
    let mut keeper = get_keeper(env, caller.clone())?;
    keeper.total_executions += 1;
    keeper.last_execution_ledger = current_ledger;
    update_keeper(env, &keeper);

    // ── Step 5: Emit TaskExecuted event (designated / post-grace paths) ───────
    // Requirement 3.7: emit after confirming all state mutations succeeded.
    if !is_secondary {
        crate::events::emit_task_executed(
            env,
            &task.task_id,
            caller,
            current_ledger,
            task.micro_reward_per_run,
        );
    }

    Ok(())
}
