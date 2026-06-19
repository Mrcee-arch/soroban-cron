//! Storage key definitions for The Anchor contract.
//!
//! This module defines the [`DataKey`] enum, which enumerates every logical
//! key used when reading from or writing to Soroban ledger storage.
//!
//! ## Soroban storage tiers
//!
//! Soroban provides three storage tiers with different persistence and cost
//! characteristics:
//!
//! | Tier         | Survives upgrade? | Cost model                          |
//! |--------------|-------------------|-------------------------------------|
//! | **Persistent** | Yes             | Higher per-entry rent; long TTL     |
//! | **Instance**   | Yes (with contract instance) | Cheaper; shared 64 KB budget |
//! | **Temporary**  | No (auto-expires) | Cheapest; only for ephemeral data  |
//!
//! Each variant below documents which tier is used and the rationale behind
//! the choice.

use soroban_sdk::{contracttype, Address, BytesN};

/// Enumerates all storage keys used by the Anchor contract.
///
/// Variants are partitioned across Soroban's **Persistent** and **Instance**
/// storage tiers according to their access patterns and lifetime requirements.
/// No variant uses Temporary storage because all Anchor state must be durable.
#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    /// Per-keeper registry entry, keyed by the keeper's Stellar [`Address`].
    ///
    /// **Storage tier: Persistent**
    ///
    /// Rationale: Keeper records contain stake balances and execution history
    /// that must survive contract upgrades and must not expire due to inactivity.
    /// Using Persistent storage ensures that a keeper's stake is never silently
    /// lost across upgrade boundaries or long periods without transactions.
    Keeper(Address),

    /// Per-task execution record, keyed by the task's 32-byte unique identifier.
    ///
    /// **Storage tier: Persistent**
    ///
    /// Rationale: `ExecutionTask` records are long-lived by design — they
    /// represent recurring automation commitments that may span thousands of
    /// ledgers. Persistent storage guarantees they are never garbage-collected
    /// and are fully preserved across contract upgrades.
    Task(BytesN<32>),

    /// Monotonic counter used to generate unique task identifiers.
    ///
    /// **Storage tier: Instance**
    ///
    /// Rationale: `TaskCounter` is a single scalar value that is read and
    /// incremented on every `provision_task` call. It fits easily within the
    /// 64 KB Instance storage budget and benefits from Instance storage's
    /// lower rent cost relative to Persistent. Because the contract instance
    /// itself is always upgraded atomically, the counter is preserved across
    /// upgrades along with the rest of Instance state.
    TaskCounter,

    /// The contract's governance/admin address.
    ///
    /// **Storage tier: Instance**
    ///
    /// Rationale: The admin address is set once during `initialize` and rarely
    /// (if ever) changes thereafter. It is a small scalar value that fits
    /// within the Instance storage budget and is accessed on every
    /// privileged call (e.g., `provision_task`). Storing it in Instance
    /// storage minimises read costs for this frequently-checked field.
    Admin,

    /// Minimum stake required for keeper registration (in stroops).
    ///
    /// **Storage tier: Instance**
    ///
    /// Rationale: `MinKeeperStake` is a governance-settable configuration
    /// constant. Like `Admin`, it is a small value accessed on every
    /// `register_keeper` call. Instance storage keeps it cheap to read and
    /// ensures it is co-located with the contract instance for efficient
    /// access.
    MinKeeperStake,

    /// Accumulated treasury balance (in stroops) retained from slash penalties.
    ///
    /// **Storage tier: Persistent**
    ///
    /// Rationale: The treasury balance grows continuously as slashing events
    /// occur over the lifetime of the contract. It must never expire or be
    /// lost, and must survive contract upgrades. Persistent storage is the
    /// only tier that provides the necessary guarantees for a value that
    /// accumulates value over time and may not be touched for extended periods.
    TreasuryBalance,

    /// Address of the native token (Stellar's XLM token) used for reward
    /// transfers and stake accounting.
    ///
    /// **Storage tier: Instance**
    ///
    /// Rationale: The native token address is a singleton set once during
    /// `initialize` and read on every execution that involves reward transfer.
    /// Instance storage is cheapest for small, frequently-read configuration
    /// values that are co-located with the contract instance.
    NativeToken,
}
