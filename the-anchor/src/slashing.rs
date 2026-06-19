//! Slashing logic for the Anchor contract.
//!
//! This module implements the slash-first atomicity model described in the
//! design document.  All mutations — stake reduction, treasury credit, and
//! ineligibility marking — are committed in a single invocation so that no
//! partial state is ever visible on-chain.
//!
//! ## Slash formula
//!
//! ```text
//! slash_amount     = keeper.stake_amount * 5 / 100   (integer floor division)
//! new_stake        = keeper.stake_amount - slash_amount
//! secondary_reward = slash_amount / 2
//! treasury_portion = slash_amount - secondary_reward
//! ```
//!
//! Because `stake_amount` is always non-negative and Rust truncates integer
//! division toward zero for positive values, `slash_amount` is always a
//! non-negative floor result and `new_stake` is always ≥ 0.
//!
//! ## Ineligibility
//!
//! If `new_stake < MIN_KEEPER_STAKE` after the slash, the keeper is marked
//! `ineligible = true`.  Ineligible keepers cannot be assigned to new tasks.
//!
//! ## Zero-stake guard
//!
//! If `keeper.stake_amount == 0` the function emits a zero-slash event and
//! returns `Err(ContractError::SlashOnZeroStake)` without modifying any state.

use soroban_sdk::{Address, Env};

use crate::errors::ContractError;
use crate::storage_keys::DataKey;
use crate::registry::{get_keeper, update_keeper, MIN_KEEPER_STAKE};

/// Applies a slash penalty to the keeper identified by `designated_keeper_address`.
///
/// # Returns
///
/// `Ok((slash_amount, secondary_reward))` on success, where:
/// - `slash_amount`     — the amount deducted from the keeper's stake (stroops)
/// - `secondary_reward` — the portion of the slash forwarded to the secondary
///                        keeper that triggered the penalty (stroops)
///
/// # Errors
///
/// - [`ContractError::KeeperNotFound`] — if the address is not in the registry.
/// - [`ContractError::SlashOnZeroStake`] — if the keeper's `stake_amount` is
///   already zero; a zero-slash event is emitted but no state is modified.
///
/// # Panics
///
/// Does not panic under normal operation.  Division by zero is impossible
/// because the divisors (100 and 2) are compile-time constants.
///
/// # Validates
///
/// Requirements 5.1, 5.2, 5.3, 5.4, 5.5, 5.6
pub fn apply_slash(
    env: &Env,
    designated_keeper_address: &Address,
) -> Result<(i128, i128), ContractError> {
    // ── 1. Load keeper ────────────────────────────────────────────────────────
    let mut keeper = get_keeper(env, designated_keeper_address.clone())?;

    // ── 2. Zero-stake guard ───────────────────────────────────────────────────
    // Requirement 5.6: A keeper with zero stake must not be further slashed.
    // We emit a zero-slash event to preserve auditability and then return an
    // error so callers can distinguish this path from a successful slash.
    if keeper.stake_amount == 0 {
        crate::events::emit_zero_slash(env, designated_keeper_address);
        return Err(ContractError::SlashOnZeroStake);
    }

    // ── 3. Slash arithmetic ───────────────────────────────────────────────────
    // Requirement 5.1: Floor division — Rust truncates toward zero for positive
    // i128, which is equivalent to floor division when both operands are ≥ 0.
    let slash_amount: i128 = keeper.stake_amount * 5 / 100;

    // Requirement 5.5: new_stake is always non-negative because slash_amount ≤
    // keeper.stake_amount (slash rate is 5 %, so slash ≤ stake for any s ≥ 0).
    let new_stake: i128 = keeper.stake_amount - slash_amount;

    // Requirement 4.4 / 5.1: Split the slashed amount between the secondary
    // keeper (caller's reward) and the contract treasury.
    let secondary_reward: i128 = slash_amount / 2;
    let treasury_portion: i128 = slash_amount - secondary_reward;

    // ── 4. Treasury accounting ────────────────────────────────────────────────
    // Requirement 5.4: Accumulate the treasury portion in Persistent storage so
    // it survives across ledger boundaries and contract upgrades.
    let current_treasury: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::TreasuryBalance)
        .unwrap_or(0);
    env.storage()
        .persistent()
        .set(&DataKey::TreasuryBalance, &(current_treasury + treasury_portion));

    // ── 5. Update keeper record ───────────────────────────────────────────────
    keeper.stake_amount = new_stake;

    // Requirement 5.2 / 5.3: If the new stake falls below the minimum threshold,
    // mark the keeper as ineligible for future designated-keeper assignments.
    if new_stake < MIN_KEEPER_STAKE {
        keeper.ineligible = true;
    }

    // Persist the updated keeper record atomically.
    update_keeper(env, &keeper);

    // ── 6. Event emission ─────────────────────────────────────────────────────
    // Requirement 5.4: Emit a SlashApplied event for off-chain indexers and
    // the Engine's event log.
    crate::events::emit_slash_applied(env, designated_keeper_address, slash_amount, new_stake);

    // ── 7. Return ─────────────────────────────────────────────────────────────
    Ok((slash_amount, secondary_reward))
}
