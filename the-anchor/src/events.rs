//! On-chain event emission for the Anchor contract.
//!
//! Every public function in this module calls [`soroban_sdk::Events::publish`]
//! to record an immutable on-chain event.  Events are keyed with a short
//! symbol topic so they can be indexed by the Engine and external explorers.
//!
//! ## Event catalogue
//!
//! | Function | Symbol | Emitted by |
//! |----------|--------|-----------|
//! | [`emit_keeper_registered`] | `"kpr_reg"` | `registry.rs` |
//! | [`emit_task_provisioned`] | `"task_prov"` | `tasks.rs` |
//! | [`emit_task_executed`] | `"task_exec"` | `execution.rs` (designated / post-grace) |
//! | [`emit_task_executed_by_secondary`] | `"task_sec"` | `execution.rs` (grace period) |
//! | [`emit_slash_applied`] | `"slashed"` | `slashing.rs` |
//! | [`emit_zero_slash`] | `"zero_slsh"` | `slashing.rs` |
//! | [`emit_missed_execution`] | `"missed"` | `execution.rs` (post-grace) |

use chronos_types::TaskId;
use soroban_sdk::{symbol_short, Address, Env};

// ── Keeper events ─────────────────────────────────────────────────────────────

/// Emitted when a new keeper is successfully registered.
///
/// Topic:   `("kpr_reg",)`
/// Data:    `(keeper_address: Address, stake_amount: i128)`
///
/// # Parameters
/// - `keeper`       — address of the newly registered keeper
/// - `stake_amount` — the stake amount locked at registration (stroops)
pub fn emit_keeper_registered(env: &Env, keeper: &Address, stake_amount: i128) {
    env.events()
        .publish((symbol_short!("kpr_reg"),), (keeper.clone(), stake_amount));
}

// ── Slashing events ───────────────────────────────────────────────────────────

/// Emitted after a slash penalty is successfully applied.
///
/// Topic:   `("slashed",)`
/// Data:    `(keeper_address: Address, amount_slashed: i128, new_stake: i128)`
///
/// # Parameters
/// - `keeper`         — address of the slashed keeper
/// - `amount_slashed` — the computed slash amount (stroops)
/// - `new_stake`      — keeper's stake after the slash (stroops)
pub fn emit_slash_applied(env: &Env, keeper: &Address, amount_slashed: i128, new_stake: i128) {
    env.events().publish(
        (symbol_short!("slashed"),),
        (keeper.clone(), amount_slashed, new_stake),
    );
}

/// Emitted when a slash is attempted on a keeper whose stake is already zero.
///
/// Topic:   `("zero_slsh",)`  — note: 9-char limit of `symbol_short!`
/// Data:    `(keeper_address: Address)`
///
/// # Parameters
/// - `keeper` — address of the keeper with zero stake
pub fn emit_zero_slash(env: &Env, keeper: &Address) {
    env.events()
        .publish((symbol_short!("zero_slsh"),), (keeper.clone(),));
}

// ── Task events ───────────────────────────────────────────────────────────────

/// Emitted when a new execution task is provisioned.
///
/// Topic:   `("task_prov",)`
/// Data:    `(task_id: TaskId, target_drip_list: Address, interval: u32,
///            micro_reward_per_run: i128, designated_keeper: Address)`
///
/// # Parameters
/// - `task_id`                    — the generated 32-byte task identifier
/// - `target_drip_list`           — address of the target Drip List contract
/// - `execution_interval_ledgers` — ledger interval between allowed executions
/// - `micro_reward_per_run`       — native-token reward per execution (stroops)
/// - `designated_keeper`          — address of the designated keeper
pub fn emit_task_provisioned(
    env: &Env,
    task_id: &TaskId,
    target_drip_list: &Address,
    execution_interval_ledgers: u32,
    micro_reward_per_run: i128,
    designated_keeper: &Address,
) {
    env.events().publish(
        (symbol_short!("task_prov"),),
        (
            task_id.clone(),
            target_drip_list.clone(),
            execution_interval_ledgers,
            micro_reward_per_run,
            designated_keeper.clone(),
        ),
    );
}

// ── Execution events ──────────────────────────────────────────────────────────

/// Emitted when a designated keeper or post-grace executor successfully
/// completes a task execution.
///
/// Topic:   `("task_exec",)`
/// Data:    `(task_id: TaskId, keeper: Address, ledger: u32, reward: i128)`
///
/// # Parameters
/// - `task_id` — the task that was executed
/// - `keeper`  — address of the executing keeper
/// - `ledger`  — ledger sequence at which execution occurred
/// - `reward`  — micro-reward amount transferred to the keeper (stroops)
pub fn emit_task_executed(env: &Env, task_id: &TaskId, keeper: &Address, ledger: u32, reward: i128) {
    env.events().publish(
        (symbol_short!("task_exec"),),
        (task_id.clone(), keeper.clone(), ledger, reward),
    );
}

/// Emitted when a secondary keeper executes during the grace period, triggering
/// a slash on the designated keeper.
///
/// Topic:   `("task_sec",)`
/// Data:    `(task_id, secondary_keeper, designated_keeper, ledger,
///            slash_amount, secondary_reward, micro_reward)`
///
/// # Parameters
/// - `task_id`           — the task that was executed
/// - `secondary_keeper`  — address of the keeper who stepped in
/// - `designated_keeper` — address of the keeper who missed their window
/// - `ledger`            — ledger sequence at which execution occurred
/// - `slash_amount`      — amount slashed from the designated keeper (stroops)
/// - `secondary_reward`  — portion of the slash forwarded to the secondary keeper (stroops)
/// - `micro_reward`      — base micro-reward transferred to the secondary keeper (stroops)
#[allow(clippy::too_many_arguments)]
pub fn emit_task_executed_by_secondary(
    env: &Env,
    task_id: &TaskId,
    secondary_keeper: &Address,
    designated_keeper: &Address,
    ledger: u32,
    slash_amount: i128,
    secondary_reward: i128,
    micro_reward: i128,
) {
    env.events().publish(
        (symbol_short!("task_sec"),),
        (
            task_id.clone(),
            secondary_keeper.clone(),
            designated_keeper.clone(),
            ledger,
            slash_amount,
            secondary_reward,
            micro_reward,
        ),
    );
}

/// Emitted on the post-grace open-execution path, recording that the designated
/// keeper missed their window entirely.
///
/// Topic:   `("missed",)`
/// Data:    `(task_id: TaskId, ledgers_elapsed: u32)`
///
/// # Parameters
/// - `task_id`         — the task whose window was missed
/// - `ledgers_elapsed` — number of ledgers elapsed since `next_allowed_execution`
pub fn emit_missed_execution(env: &Env, task_id: &TaskId, ledgers_elapsed: u32) {
    env.events()
        .publish((symbol_short!("missed"),), (task_id.clone(), ledgers_elapsed));
}
