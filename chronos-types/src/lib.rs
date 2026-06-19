//! # chronos-types
//!
//! Shared canonical types for the Chronos Keeper Network.
//!
//! This crate is `no_std`-compatible and can be compiled for both:
//! - **WASM / Soroban contract** targets — enable the `"contract"` feature;
//!   types receive `#[contracttype]` annotations for on-chain ABI compatibility.
//! - **Native / Engine** targets — no feature needed; standard Rust derives are
//!   used and `soroban-sdk` is not linked.
//!
//! ## Feature flags
//!
//! | Feature | Effect |
//! |---------|--------|
//! | `contract` | Links `soroban-sdk`, uses `soroban_sdk::{Address, BytesN}` as base types, and applies `#[contracttype]` to structs |
//! | *(none)* | Uses `[u8; 32]` for `TaskId` and `String` for `Address`-like types; no `soroban-sdk` dependency |

#![no_std]

// On native targets we need `String` from the standard library.
#[cfg(not(feature = "contract"))]
extern crate std;

// ── Feature-gated imports ────────────────────────────────────────────────────

#[cfg(feature = "contract")]
use soroban_sdk::{contracttype, Address, BytesN};

#[cfg(not(feature = "contract"))]
use std::string::String;

// ── Type aliases ─────────────────────────────────────────────────────────────

/// Unique identifier for a provisioned execution task (32-byte hash).
///
/// - On contract targets: `soroban_sdk::BytesN<32>`
/// - On native targets: `[u8; 32]`
#[cfg(feature = "contract")]
pub type TaskId = BytesN<32>;

/// Unique identifier for a provisioned execution task (32-byte hash).
///
/// - On contract targets: `soroban_sdk::BytesN<32>`
/// - On native targets: `[u8; 32]`
#[cfg(not(feature = "contract"))]
pub type TaskId = [u8; 32];

/// Stellar account address used to identify a keeper.
///
/// - On contract targets: `soroban_sdk::Address`
/// - On native targets: `std::string::String`
#[cfg(feature = "contract")]
pub type KeeperAddress = Address;

/// Stellar account address used to identify a keeper.
///
/// - On contract targets: `soroban_sdk::Address`
/// - On native targets: `std::string::String`
#[cfg(not(feature = "contract"))]
pub type KeeperAddress = String;

// ── Structs ───────────────────────────────────────────────────────────────────

/// On-chain record for a registered keeper.
///
/// Annotated with `#[contracttype]` when compiled with the `"contract"` feature
/// so the type is recognised by the Soroban XDR ABI encoder.
#[cfg_attr(feature = "contract", contracttype)]
#[derive(Clone, Debug, PartialEq)]
pub struct Keeper {
    /// Stellar address of the keeper.
    pub address: KeeperAddress,
    /// Amount of native token staked, in stroops.
    pub stake_amount: i128,
    /// Ledger sequence number of the most recent successful execution.
    pub last_execution_ledger: u32,
    /// Cumulative count of successful executions by this keeper.
    pub total_executions: u64,
    /// `true` when the keeper's stake has fallen below `MIN_KEEPER_STAKE`
    /// after slashing and the keeper is no longer eligible for new
    /// designated-keeper assignments.
    pub ineligible: bool,
}

/// On-chain record for a provisioned recurring execution task.
///
/// Annotated with `#[contracttype]` when compiled with the `"contract"` feature
/// so the type is recognised by the Soroban XDR ABI encoder.
#[cfg_attr(feature = "contract", contracttype)]
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionTask {
    /// Unique identifier for this task.
    pub task_id: TaskId,
    /// Address of the target Drip List contract to invoke on each execution.
    pub target_drip_list: KeeperAddress,
    /// Number of ledgers that must elapse between successive executions.
    pub execution_interval_ledgers: u32,
    /// The earliest ledger sequence at which this task may next be executed.
    pub next_allowed_execution: u32,
    /// Native-token reward paid to the executing keeper per run, in stroops.
    pub micro_reward_per_run: i128,
    /// Address of the keeper designated for primary execution of this task.
    pub designated_keeper: KeeperAddress,
}
