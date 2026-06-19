//! Contract-level error types for the Anchor contract.
//!
//! All variants are represented as `u32` values so that Soroban can encode
//! them in the XDR error envelope returned to callers.  The `#[contracterror]`
//! attribute derives the necessary SDK traits automatically.

use soroban_sdk::contracterror;

/// Exhaustive set of errors that the Anchor contract may return.
///
/// Every variant maps to a stable `u32` discriminant that is ABI-stable across
/// contract upgrades.  Callers (the Engine, explorers, SDKs) decode this value
/// from the Soroban transaction result to determine the failure reason.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    // ── Registration ──────────────────────────────────────────────────────────

    /// The caller supplied a `stake_amount` that is strictly less than the
    /// on-chain `MIN_KEEPER_STAKE` threshold.
    ///
    /// Triggered by: `register_keeper` when `stake_amount < MIN_KEEPER_STAKE`.
    ///
    /// Validates: Requirements 1.2
    StakeBelowMinimum = 1,

    /// A `register_keeper` call was made for an address that already exists in
    /// the Keeper registry.  Duplicate registrations are never permitted.
    ///
    /// Triggered by: `register_keeper` when the address is already stored under
    /// `DataKey::Keeper(addr)`.
    ///
    /// Validates: Requirements 1.3
    KeeperAlreadyRegistered = 2,

    /// The requested keeper address was not found in the on-chain registry.
    ///
    /// Triggered by: `get_keeper`, `provision_task` (when the `designated_keeper`
    /// argument refers to an unregistered address).
    ///
    /// Validates: Requirements 1.4, 2.2
    KeeperNotFound = 3,

    // ── Task provisioning ─────────────────────────────────────────────────────

    /// The `execution_interval_ledgers` argument supplied to `provision_task`
    /// was zero.  A recurring task must have a positive interval.
    ///
    /// Triggered by: `provision_task` when `execution_interval_ledgers == 0`.
    ///
    /// Validates: Requirements 2.3
    InvalidInterval = 4,

    /// The `micro_reward_per_run` argument supplied to `provision_task` was
    /// zero or negative.  Every execution must carry a positive incentive.
    ///
    /// Triggered by: `provision_task` when `micro_reward_per_run <= 0`.
    ///
    /// Validates: Requirements 2.4
    InvalidReward = 5,

    /// The caller of a governance-gated function (e.g. `provision_task`) is
    /// not the contract admin.
    ///
    /// Triggered by: `provision_task` when the caller's address does not match
    /// the stored `DataKey::Admin` value.
    ///
    /// Validates: Requirements 2.7
    UnauthorizedCaller = 6,

    /// The `target_drip_list` address supplied to `provision_task` is invalid
    /// or unregistered.
    ///
    /// Triggered by: `provision_task` when the target address cannot be
    /// validated as a legitimate Drip List contract.
    ///
    /// Validates: Requirements 2.8
    InvalidDripList = 7,

    /// A task lookup (`get_task`, `execute_drip_split`) referenced a `TaskId`
    /// that has no corresponding entry in Persistent storage.
    ///
    /// Triggered by: any function that calls `get_task` internally when
    /// `DataKey::Task(id)` is absent.
    ///
    /// Validates: Requirements 2.5
    TaskNotFound = 8,

    // ── Execution ─────────────────────────────────────────────────────────────

    /// `execute_drip_split` was called before the task's `next_allowed_execution`
    /// ledger has been reached (`current_ledger < next_allowed_execution`).
    ///
    /// Triggered by: `execute_drip_split` in the pre-window region.
    ///
    /// Validates: Requirements 3.5
    TooEarlyToExecute = 9,

    /// `execute_drip_split` was called by a caller who is neither the
    /// `designated_keeper` (in the execution window) nor an eligible
    /// `secondary_keeper` (in the grace period).
    ///
    /// Triggered by: `execute_drip_split` when the caller fails the
    /// window-arbitration role check.
    ///
    /// Validates: Requirements 3.8
    UnauthorizedExecutor = 10,

    /// The cross-contract invocation of the target Drip List contract failed.
    /// The entire transaction is reverted in this case.
    ///
    /// Note: In the current Soroban execution model, a failed cross-contract
    /// call panics and reverts the entire transaction at the host level before
    /// this error variant can be returned. The variant is retained for ABI
    /// stability and future use if Soroban adds recoverable cross-contract
    /// error propagation.
    ///
    /// Validates: Requirements 3.6
    DripListInvocationFailed = 11,

    /// The contract does not hold enough native-token balance to cover the
    /// `micro_reward_per_run` owed to the executing keeper.
    ///
    /// Triggered by: `execute_drip_split` when the reward transfer would
    /// exceed the contract's available treasury balance.
    ///
    /// Validates: Requirements 3.2
    InsufficientRewardBalance = 12,

    // ── Grace period ──────────────────────────────────────────────────────────

    /// The `designated_keeper` attempted to call `execute_drip_split` while
    /// the task is inside the 50-ledger grace window
    /// (`next_allowed_execution < current_ledger ≤ next_allowed_execution + 50`).
    /// During the grace period only a secondary keeper may execute.
    ///
    /// Triggered by: `execute_drip_split` when `caller == designated_keeper`
    /// and the current ledger is in the grace-period band.
    ///
    /// Validates: Requirements 4.7, 4.8
    GracePeriodActive = 13,

    /// The grace period (50 ledgers) has expired
    /// (`current_ledger > next_allowed_execution + 50`).  This variant is
    /// returned when a secondary-only code path is reached after expiry.
    ///
    /// Note: In the post-grace open-execution path the contract allows *any*
    /// registered keeper to run the task without a slash penalty; this error is
    /// therefore returned only when the call path is explicitly restricted to
    /// the grace window.
    ///
    /// Validates: Requirements 4.9
    GracePeriodExpired = 14,

    /// The caller tried to execute as a `secondary_keeper` during the grace
    /// period but is not eligible — either they are not a registered keeper or
    /// they registered after the grace window opened.
    ///
    /// Triggered by: `execute_drip_split` when the grace-period secondary path
    /// is taken but the caller fails the eligibility check.
    ///
    /// Validates: Requirements 4.1
    CallerNotSecondaryEligible = 15,

    // ── Slashing ──────────────────────────────────────────────────────────────

    /// A slash was attempted on a keeper whose `stake_amount` is already zero.
    /// The contract records a zero-slash event and takes no further action.
    ///
    /// Triggered by: `apply_slash` internally when `keeper.stake_amount == 0`.
    ///
    /// Validates: Requirements 5.6
    SlashOnZeroStake = 16,

    // ── General ───────────────────────────────────────────────────────────────

    /// `initialize` was called on a contract that has already been initialised
    /// (i.e. `DataKey::Admin` is already present in Instance storage).
    ///
    /// Triggered by: `initialize` when the contract state has already been set.
    ///
    /// Validates: (initialisation guard — not enumerated as a numbered
    /// requirement but required by the design's initialisation section)
    AlreadyInitialized = 17,

    /// The `designated_keeper` address supplied to `provision_task` refers to a
    /// keeper that is currently marked `ineligible` (stake has fallen below the
    /// minimum threshold).  Ineligible keepers cannot be assigned to new tasks.
    ///
    /// Triggered by: `provision_task` when `keeper.ineligible == true`.
    ///
    /// Validates: Requirements 5.2, 5.3
    IneligibleKeeper = 18,
}
