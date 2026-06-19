//! Keeper registration and lookup for the Anchor contract.
//!
//! This module owns all logic related to the on-chain keeper registry:
//! - [`register_keeper`] — validate stake, prevent duplicates, persist a new
//!   [`Keeper`] record, and emit the `KeeperRegistered` event.
//! - [`get_keeper`] — read-only lookup of a registered keeper.
//! - [`is_registered`] — lightweight existence check (no deserialization).
//! - [`is_eligible`] — registered AND not flagged as ineligible.
//! - [`update_keeper`] — write an updated [`Keeper`] record back to Persistent
//!   storage (used by slashing and execution modules).

use chronos_types::Keeper;
use soroban_sdk::{Address, Env};

use crate::errors::ContractError;
use crate::storage_keys::DataKey;

/// Minimum stake amount (in stroops) required to register as a keeper.
///
/// Equals 1 XLM (10 000 000 stroops). This constant is the hard-coded
/// fallback; the on-chain `DataKey::MinKeeperStake` Instance value (set by
/// `initialize`) takes precedence when present.
pub const MIN_KEEPER_STAKE: i128 = 10_000_000; // 1 XLM in stroops

// ── Public API ────────────────────────────────────────────────────────────────

/// Register a new keeper by locking stake collateral in the contract.
///
/// # Steps
/// 1. Require the `keeper` address to have authorised this call.
/// 2. Read the minimum stake threshold from `DataKey::MinKeeperStake` in
///    Instance storage, falling back to [`MIN_KEEPER_STAKE`] if not set.
/// 3. Reject with [`ContractError::StakeBelowMinimum`] if
///    `stake_amount < min_stake`.
/// 4. Reject with [`ContractError::KeeperAlreadyRegistered`] if the address
///    already has an entry in Persistent storage under `DataKey::Keeper(addr)`.
/// 5. Construct a fresh [`Keeper`] record with zeroed execution metadata and
///    `ineligible = false`, then write it to Persistent storage.
/// 6. Emit a `KeeperRegistered` event (stubbed until task 8).
///
/// # Errors
/// - [`ContractError::StakeBelowMinimum`] — `stake_amount` is below the
///   minimum required threshold.
/// - [`ContractError::KeeperAlreadyRegistered`] — the address is already in
///   the registry.
///
/// _Validates: Requirements 1.1, 1.2, 1.3, 1.5_
pub fn register_keeper(
    env: &Env,
    keeper: Address,
    stake_amount: i128,
) -> Result<(), ContractError> {
    // Step 1 — require keeper to authorise this call.
    keeper.require_auth();

    // Step 2 — resolve minimum stake: prefer on-chain config, fall back to
    //          the compile-time constant.
    let min_stake: i128 = env
        .storage()
        .instance()
        .get(&DataKey::MinKeeperStake)
        .unwrap_or(MIN_KEEPER_STAKE);

    // Step 3 — reject if stake is below the minimum.
    if stake_amount < min_stake {
        return Err(ContractError::StakeBelowMinimum);
    }

    // Step 4 — reject duplicate registrations.
    let key = DataKey::Keeper(keeper.clone());
    if env.storage().persistent().has(&key) {
        return Err(ContractError::KeeperAlreadyRegistered);
    }

    // Step 5 — build the initial keeper record and persist it.
    let new_keeper = Keeper {
        address: keeper.clone(),
        stake_amount,
        last_execution_ledger: 0,
        total_executions: 0,
        ineligible: false,
    };
    env.storage().persistent().set(&key, &new_keeper);

    // Step 6 — emit KeeperRegistered event.
    crate::events::emit_keeper_registered(env, &keeper, stake_amount);

    Ok(())
}

/// Read-only lookup returning the full [`Keeper`] struct for a registered address.
///
/// # Errors
/// - [`ContractError::KeeperNotFound`] — the address has no entry in the
///   on-chain registry.
///
/// _Validates: Requirements 1.4_
pub fn get_keeper(env: &Env, keeper: Address) -> Result<Keeper, ContractError> {
    env.storage()
        .persistent()
        .get::<DataKey, Keeper>(&DataKey::Keeper(keeper))
        .ok_or(ContractError::KeeperNotFound)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Returns `true` if `keeper` has an entry in the on-chain registry.
///
/// This is a lightweight check that avoids deserialising the full [`Keeper`]
/// struct — use it when you only need to know whether the keeper exists.
pub fn is_registered(env: &Env, keeper: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Keeper(keeper.clone()))
}

/// Returns `true` if `keeper` is registered **and** not marked ineligible.
///
/// A keeper becomes ineligible when their stake falls below
/// `MIN_KEEPER_STAKE` after slashing (see `slashing.rs`). Ineligible keepers
/// cannot be assigned as `designated_keeper` on new tasks.
///
/// Returns `false` for any unregistered address.
pub fn is_eligible(env: &Env, keeper: &Address) -> bool {
    match env
        .storage()
        .persistent()
        .get::<DataKey, Keeper>(&DataKey::Keeper(keeper.clone()))
    {
        Some(k) => !k.ineligible,
        None => false,
    }
}

/// Write an updated [`Keeper`] record back to Persistent storage.
///
/// Used by `slashing.rs` and `execution.rs` after mutating keeper state
/// (e.g. applying a slash, incrementing `total_executions`).
///
/// The `keeper.address` field is used as the storage key; callers must ensure
/// they pass the correct keeper struct.
pub fn update_keeper(env: &Env, keeper: &Keeper) {
    env.storage()
        .persistent()
        .set(&DataKey::Keeper(keeper.address.clone()), keeper);
}
