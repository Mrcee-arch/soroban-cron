#![no_std]
//! # the-anchor
//!
//! Soroban smart contract for the Chronos Keeper Network.
//!
//! Manages keeper registration, staking, task provisioning, execution
//! arbitration, slashing, and reward distribution on the Stellar/Soroban
//! blockchain.
//!
//! ## Contract entry points
//!
//! | Method | Description |
//! |--------|-------------|
//! | `initialize` | One-time setup: set admin and `MIN_KEEPER_STAKE` |
//! | `register_keeper` | Register a keeper by staking native tokens |
//! | `get_keeper` | Read a keeper record by address |
//! | `provision_task` | Admin-gated: create a recurring execution task |
//! | `get_task` | Read a task record by its `TaskId` |
//! | `execute_drip_split` | Execute a Drips distribution split (window-arbitrated) |

pub mod errors;
pub mod events;
pub mod execution;
pub mod registry;
pub mod slashing;
pub mod storage_keys;
pub mod tasks;

use soroban_sdk::{contract, contractimpl, Address, Env};

use chronos_types::{ExecutionTask, Keeper, TaskId};
use errors::ContractError;
use storage_keys::DataKey;

/// The Anchor smart contract.
///
/// All public methods are exposed as Soroban contract entry points via the
/// `#[contractimpl]` macro.
#[contract]
pub struct AnchorContract;

#[contractimpl]
impl AnchorContract {
    // ── Initialisation ────────────────────────────────────────────────────────

    /// One-time initialisation: record the admin address, the native token
    /// address used for reward transfers, and the minimum keeper stake.
    ///
    /// # Errors
    /// - [`ContractError::AlreadyInitialized`] — called more than once.
    pub fn initialize(
        env: Env,
        admin: Address,
        native_token: Address,
        min_keeper_stake: i128,
    ) -> Result<(), ContractError> {
        // Guard: reject if already initialised.
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }

        // Require admin to authorise this call.
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::NativeToken, &native_token);
        env.storage()
            .instance()
            .set(&DataKey::MinKeeperStake, &min_keeper_stake);

        Ok(())
    }

    // ── Keeper Registry ───────────────────────────────────────────────────────

    /// Register a new keeper by committing a stake amount.
    ///
    /// The caller must provide `stake_amount ≥ MIN_KEEPER_STAKE` and must
    /// not already be registered.
    ///
    /// Emits: `KeeperRegistered { address, stake_amount }`
    ///
    /// # Errors
    /// - [`ContractError::StakeBelowMinimum`]
    /// - [`ContractError::KeeperAlreadyRegistered`]
    pub fn register_keeper(
        env: Env,
        keeper: Address,
        stake_amount: i128,
    ) -> Result<(), ContractError> {
        registry::register_keeper(&env, keeper, stake_amount)
    }

    /// Read-only lookup of a registered keeper by address.
    ///
    /// # Errors
    /// - [`ContractError::KeeperNotFound`]
    pub fn get_keeper(env: Env, keeper: Address) -> Result<Keeper, ContractError> {
        registry::get_keeper(&env, keeper)
    }

    // ── Task Provisioning ─────────────────────────────────────────────────────

    /// Admin-gated: provision a new recurring execution task.
    ///
    /// Returns the generated [`TaskId`] on success.
    ///
    /// Emits: `TaskProvisioned { task_id, target_drip_list, interval,
    ///                            micro_reward_per_run, designated_keeper }`
    ///
    /// # Errors
    /// - [`ContractError::UnauthorizedCaller`]
    /// - [`ContractError::InvalidInterval`]
    /// - [`ContractError::InvalidReward`]
    /// - [`ContractError::KeeperNotFound`]
    /// - [`ContractError::IneligibleKeeper`]
    pub fn provision_task(
        env: Env,
        target_drip_list: Address,
        execution_interval_ledgers: u32,
        micro_reward_per_run: i128,
        designated_keeper: Address,
    ) -> Result<TaskId, ContractError> {
        tasks::provision_task(
            &env,
            target_drip_list,
            execution_interval_ledgers,
            micro_reward_per_run,
            designated_keeper,
        )
    }

    /// Read-only lookup of a provisioned task by its [`TaskId`].
    ///
    /// # Errors
    /// - [`ContractError::TaskNotFound`]
    pub fn get_task(env: Env, task_id: TaskId) -> Result<ExecutionTask, ContractError> {
        tasks::get_task(&env, task_id)
    }

    // ── Execution ─────────────────────────────────────────────────────────────

    /// Execute a Drips distribution split for the given task.
    ///
    /// Applies the four-region window arbitration model:
    ///
    /// | Region | Condition | Allowed callers |
    /// |--------|-----------|-----------------|
    /// | Pre-window | `L < T` | None |
    /// | Designated | `L == T` | `designated_keeper` only |
    /// | Grace | `T < L ≤ T+50` | Any registered keeper except `designated_keeper` |
    /// | Post-grace | `L > T+50` | Any registered keeper |
    ///
    /// # Errors
    /// See [`execution::execute_drip_split`] for the full error table.
    pub fn execute_drip_split(
        env: Env,
        task_id: TaskId,
        caller: Address,
    ) -> Result<(), ContractError> {
        execution::execute_drip_split(&env, task_id, caller)
    }
}
