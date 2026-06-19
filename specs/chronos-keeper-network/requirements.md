# Requirements Document

## Introduction

Chronos Keeper Network is a decentralized, crypto-economically secured network of Rust nodes that trustlessly automates time-sensitive funding splits for large-scale open-source ecosystems on the Drips Network. The system eliminates reliance on manual gas payments or centralized cron jobs by introducing a permissionless keeper network backed by Proof-of-Stake. It consists of two primary components: **The Anchor**, a Soroban smart contract (Rust, `no_std`) that manages keeper registry, staking, slashing, and task provisioning; and **The Engine**, an off-chain Rust node daemon using Tokio that monitors ledger state, coordinates execution, and fulfills execution tasks.

---

## Glossary

- **Anchor**: The Soroban smart contract (`the-anchor` crate) responsible for keeper registry, staking, slashing, and task provisioning on the Stellar/Soroban blockchain.
- **Engine**: The off-chain Rust daemon (`the-engine` crate) responsible for ledger tracking, event stream processing, and triggering execution of Drips distribution tasks.
- **Keeper**: A registered network participant who has staked the native token and is eligible to execute distribution tasks in exchange for micro-rewards.
- **Designated_Keeper**: The Keeper explicitly assigned to a given `ExecutionTask` for a given execution window.
- **Secondary_Keeper**: Any registered Keeper other than the `Designated_Keeper` who executes a task during the grace period after the `Designated_Keeper` has failed to act.
- **ExecutionTask**: An on-chain record defining a target Drip List, execution interval, next allowed execution ledger, micro-reward, and `Designated_Keeper`.
- **Drip_List**: A target smart contract on the Drips Network that distributes funding splits when invoked.
- **SLA**: Service Level Agreement — the obligation of the `Designated_Keeper` to execute a task within the allowed execution window.
- **Grace_Period**: A 50-ledger window following the `next_allowed_execution` ledger during which a `Secondary_Keeper` may execute a task if the `Designated_Keeper` has failed.
- **Slash_Penalty**: A 5% reduction of a Keeper's staked amount applied when the `Designated_Keeper` breaches its SLA.
- **Secondary_Reward**: 50% of the slashed amount distributed to the `Secondary_Keeper` who executed during the Grace Period.
- **Ledger**: A Stellar network block; the atomic unit of time used for scheduling and SLA enforcement.
- **Registry**: The on-chain map of all registered Keepers maintained by the Anchor contract.
- **Stake**: Native token amount locked by a Keeper as collateral to participate in the network.
- **Workspace**: The Rust Cargo workspace containing both `the-anchor` and `the-engine` crates.

---

## Requirements

### Requirement 1: Keeper Registration and Staking

**User Story:** As a node operator, I want to register as a Keeper by staking native tokens, so that I can participate in the network and earn rewards for executing Drips distribution tasks.

#### Acceptance Criteria

1. WHEN a node operator invokes `register_keeper` with a stake amount greater than or equal to `MIN_KEEPER_STAKE`, THE Anchor SHALL record the Keeper's address, stake amount, and initial metadata (last_execution_ledger = 0, total_executions = 0) in the Registry.
2. WHEN a node operator invokes `register_keeper` with a stake amount below `MIN_KEEPER_STAKE`, THE Anchor SHALL reject the registration and return an error indicating the stake is below the minimum required threshold.
3. IF a Keeper address is already present in the Registry, THEN THE Anchor SHALL reject a duplicate `register_keeper` call and return an error indicating the Keeper is already registered.
4. WHEN a read-only `get_keeper` method is invoked with a registered Keeper address, THE Anchor SHALL return the full `Keeper` struct for that address.
5. WHEN a Keeper is successfully registered, THE Anchor SHALL emit an on-chain event recording the Keeper's address and stake amount.
6. IF on-chain event emission fails after a successful registration, THE Anchor SHALL retain the registration as valid without reverting the transaction.

---

### Requirement 2: Task Provisioning

**User Story:** As a protocol administrator or ecosystem participant, I want to provision recurring execution tasks on-chain, so that Drips distribution splits are automated at precise ledger-defined intervals.

#### Acceptance Criteria

1. WHEN an authorized caller invokes `provision_task` with a valid `target_drip_list`, `execution_interval_ledgers`, `micro_reward_per_run`, and `designated_keeper`, THE Anchor SHALL store a new `ExecutionTask` with `next_allowed_execution` set to the current ledger plus `execution_interval_ledgers`.
2. WHEN `provision_task` is called with an unregistered `designated_keeper` address, THE Anchor SHALL reject the task and return an error message indicating the reason for rejection.
3. WHEN `provision_task` is called with an `execution_interval_ledgers` value of zero, THE Anchor SHALL reject the task and return an error message indicating the reason for rejection.
4. WHEN `provision_task` is called with a `micro_reward_per_run` value of zero or less, THE Anchor SHALL reject the task and return an error message indicating the reason for rejection.
5. THE Anchor SHALL assign a unique task identifier to each provisioned `ExecutionTask` and return that identifier to the caller.
6. WHEN a task is successfully provisioned, THE Anchor SHALL emit an on-chain event recording the task identifier, target address, interval, micro_reward_per_run, and designated keeper.
7. WHEN `provision_task` is called by an unauthorized caller, THE Anchor SHALL reject the call and return an error message indicating insufficient authorization.
8. WHEN `provision_task` is called with an unregistered or invalid `target_drip_list` address, THE Anchor SHALL reject the task and return an error message indicating the reason for rejection.

---

### Requirement 3: Designated Keeper Execution

**User Story:** As a Designated Keeper, I want to execute a Drips distribution split during my assigned execution window, so that I receive the micro-reward and my SLA record is maintained.

#### Acceptance Criteria

1. WHEN the `Designated_Keeper` invokes `execute_drip_split` for a task and the current ledger is greater than or equal to `next_allowed_execution`, AND the caller is authorized as the `Designated_Keeper` for that task, THE Anchor SHALL invoke the target `Drip_List` contract to distribute the funding split.
2. WHEN `execute_drip_split` succeeds and the contract holds sufficient balance to cover `micro_reward_per_run`, THE Anchor SHALL transfer the `micro_reward_per_run` to the `Designated_Keeper`'s address.
3. WHEN `execute_drip_split` succeeds, THE Anchor SHALL update the task's `next_allowed_execution` to the current ledger plus `execution_interval_ledgers`.
4. WHEN `execute_drip_split` succeeds, THE Anchor SHALL increment `total_executions` and update `last_execution_ledger` on the `Designated_Keeper`'s `Keeper` struct.
5. WHEN the `Designated_Keeper` invokes `execute_drip_split` and the current ledger is less than `next_allowed_execution`, THE Anchor SHALL reject the call and return a descriptive error.
6. WHEN `execute_drip_split` is called by the `Designated_Keeper` and the target `Drip_List` contract invocation fails, THE Anchor SHALL revert the entire transaction and return a descriptive error.
7. WHEN a task is executed successfully, THE Anchor SHALL emit an on-chain event recording the task identifier, executing keeper address, ledger of execution, and reward amount transferred.
8. WHEN `execute_drip_split` is called by a caller who is neither the `Designated_Keeper` nor an eligible `Secondary_Keeper`, THE Anchor SHALL reject the call and return an error indicating unauthorized execution.

---

### Requirement 4: SLA Grace Period and Secondary Keeper Execution

**User Story:** As a Secondary Keeper, I want to execute an overdue task during the grace period when the Designated Keeper has failed to act, so that I receive the secondary reward and the ecosystem continues uninterrupted.

#### Acceptance Criteria

1. WHEN the current ledger exceeds `next_allowed_execution` by at least 1 ledger and by no more than 50 ledgers (the Grace Period), THE Anchor SHALL permit any registered `Secondary_Keeper` (other than the `Designated_Keeper`, and registered before `next_allowed_execution` was exceeded) to invoke `execute_drip_split` for that task.
2. WHEN a `Secondary_Keeper` successfully executes a task during the Grace Period, THE Anchor SHALL apply the `Slash_Penalty` of 5% of the `Designated_Keeper`'s pre-slash `stake_amount` (rounded down) to the `Designated_Keeper`'s on-chain stake balance.
3. IF the `Secondary_Keeper` execution does not succeed, THE Anchor SHALL not apply any `Slash_Penalty` to the `Designated_Keeper`.
4. WHEN a `Secondary_Keeper` successfully executes a task during the Grace Period, THE Anchor SHALL transfer the `Secondary_Reward` (50% of the slashed amount) to that `Secondary_Keeper`'s address; the remaining 50% of the slashed amount SHALL be retained in the contract treasury.
5. WHEN a `Secondary_Keeper` successfully executes a task during the Grace Period, THE Anchor SHALL transfer the `micro_reward_per_run` to that `Secondary_Keeper`'s address in addition to the `Secondary_Reward`.
6. WHEN a `Secondary_Keeper` successfully executes a task during the Grace Period, THE Anchor SHALL update the task record's `next_allowed_execution` to the current ledger plus `execution_interval_ledgers`, and SHALL increment `total_executions` and update `last_execution_ledger` on that `Secondary_Keeper`'s `Keeper` struct.
7. WHEN a `Secondary_Keeper` attempts `execute_drip_split` and the current ledger has not yet exceeded `next_allowed_execution`, THE Anchor SHALL reject the call and return a descriptive error.
8. WHEN the `Designated_Keeper` attempts `execute_drip_split` during the Grace Period (current ledger exceeds `next_allowed_execution` by 1–50 ledgers), THE Anchor SHALL reject the call and return a descriptive error.
9. WHEN the current ledger exceeds `next_allowed_execution` by more than 50 ledgers (Grace Period expired), THE Anchor SHALL permit the `Designated_Keeper` or any registered Keeper to execute the task without applying the `Slash_Penalty`, and SHALL emit a missed-execution event recording the task identifier and the number of ledgers elapsed.

---

### Requirement 5: Slashing and Stake Enforcement

**User Story:** As a protocol participant, I want Keepers that breach their SLA to have their stake reduced, so that economic incentives enforce reliable task execution.

#### Acceptance Criteria

1. WHEN a `Slash_Penalty` is applied, THE Anchor SHALL reduce the `Designated_Keeper`'s `stake_amount` by exactly 5% of the pre-slash value, rounded down to the nearest integer.
2. WHEN a Keeper's `stake_amount` falls below `minimum_stake_threshold` after a `Slash_Penalty`, THE Anchor SHALL mark that Keeper as ineligible for `Designated_Keeper` assignment on new tasks.
3. WHEN a Keeper is marked ineligible (stake below `minimum_stake_threshold`), THE Anchor SHALL not assign that Keeper as `Designated_Keeper` on any subsequently provisioned task.
4. WHEN a `Slash_Penalty` is applied, THE Anchor SHALL record a slash event on-chain including the affected keeper address, amount slashed, and timestamp of the event.
5. WHEN a slashed Keeper's remaining stake is greater than zero, THE Anchor SHALL retain the remaining stake in the Registry rather than confiscating it entirely.
6. WHEN a `Slash_Penalty` is attempted on a Keeper whose `stake_amount` is already zero, THE Anchor SHALL skip the penalty and record a zero-slash event without altering the Keeper's record.

---

### Requirement 6: Engine Ledger Tracking and Task Discovery

**User Story:** As a node operator running the Engine, I want the Engine daemon to continuously monitor ledger height and discover executable tasks, so that execution opportunities are never missed.

#### Acceptance Criteria

1. WHEN the Engine starts, THE Engine SHALL connect to a configured Stellar RPC endpoint and begin polling the current ledger height at a configurable interval between 1 second and 5 seconds inclusive.
2. WHEN the Engine detects that the current ledger height has advanced, THE Engine SHALL query the Anchor contract (with a timeout of no more than 10 seconds) for all `ExecutionTask` records where `next_allowed_execution` is less than or equal to the current ledger height; IF the query does not complete within 10 seconds, THE Engine SHALL log a timeout warning and retry on the next poll cycle.
3. WHEN the Engine identifies an executable task for which the running node is the `Designated_Keeper`, THE Engine SHALL initiate the execution pipeline for that task without waiting for the next poll cycle.
4. IF the Engine loses connectivity to the Stellar RPC endpoint, THEN THE Engine SHALL log the connectivity failure with a structured log entry, retry the connection with exponential backoff starting at 1 second and capped at the configurable maximum retry interval, and resume normal operation upon successful reconnection.
5. THE Engine SHALL maintain an in-memory set of task identifiers that are currently in-flight; WHEN a task is added to the in-flight set, THE Engine SHALL not initiate a duplicate execution attempt for that task until it is removed from the set.
6. WHEN a task execution attempt completes (success or failure), THE Engine SHALL remove the task identifier from the in-flight set.

---

### Requirement 7: Engine Execution Pipeline

**User Story:** As a node operator, I want the Engine to autonomously construct and submit execution transactions, so that Drips distribution splits are executed on-chain without manual intervention.

#### Acceptance Criteria

1. WHEN the Engine determines that an `ExecutionTask` is ready for execution and the node is the `Designated_Keeper` or an eligible `Secondary_Keeper` (current ledger within the 50-ledger Grace Period for secondary execution), THE Engine SHALL construct and sign the `execute_drip_split` transaction using the node's configured keypair.
2. WHEN the Engine submits an `execute_drip_split` transaction, THE Engine SHALL wait up to 60 seconds for ledger confirmation; IF confirmation is not received within 60 seconds, THE Engine SHALL log a confirmation timeout and treat the attempt as failed.
3. WHEN ledger confirmation is received, THE Engine SHALL log the resulting transaction hash and execution ledger.
4. IF an `execute_drip_split` transaction submission fails due to a transient network error (I/O error, connection timeout, or connection reset), THEN THE Engine SHALL retry submission with a configurable backoff interval between 1 and 60 seconds; WHEN the `max_retries` count is reached, THE Engine SHALL stop retrying, emit a structured log entry marking the attempt as failed, and remove the task from the execution queue.
5. IF an `execute_drip_split` transaction is rejected by the Anchor contract with a non-transient error (contract-level rejection), THEN THE Engine SHALL log the rejection reason and remove the task from the current execution queue without retrying.
6. WHEN the Engine successfully executes a task (on-ledger confirmation received), THE Engine SHALL update the local task state cache to reflect the new `next_allowed_execution` value.
7. WHEN a task execution is confirmed on-ledger, THE Engine SHALL emit a structured log entry including task identifier, keeper address, ledger height, transaction hash, and reward amount in the native token unit.

---

### Requirement 8: Engine Grace Period Monitoring

**User Story:** As a Secondary Keeper node operator, I want the Engine to detect when a Designated Keeper has missed their execution window, so that I can step in, execute the task, and receive the secondary reward.

#### Acceptance Criteria

1. WHEN the Engine detects that a task's `next_allowed_execution` has passed and the current ledger is within the 50-ledger Grace Period, THE Engine SHALL evaluate whether the running node is eligible to act as `Secondary_Keeper` — eligible means the node is registered as a Keeper AND the task has not already been marked executed for that window.
2. IF the running node is not eligible as `Secondary_Keeper`, THEN THE Engine SHALL skip the task and take no execution action for that grace period window.
3. WHEN the running node is eligible as `Secondary_Keeper` and the task is within the Grace Period, THE Engine SHALL initiate the execution pipeline for that task, including submission of the `execute_drip_split` call that triggers slashing of the `Designated_Keeper`.
4. IF the `execute_drip_split` submission fails during a Grace Period execution attempt, THE Engine SHALL log the error including task identifier, failure reason, and current ledger height, and SHALL NOT retry within the same Grace Period window.
5. WHEN the current ledger exceeds `next_allowed_execution` by more than 50 ledgers, THE Engine SHALL log a missed-execution warning including the task identifier and elapsed ledger count, and SHALL NOT attempt execution for that missed window.

---

### Requirement 9: Rust Workspace and Project Structure

**User Story:** As a contributor, I want a well-organized Rust workspace with clearly separated crates, so that the contract and engine can be developed, tested, and deployed independently.

#### Acceptance Criteria

1. THE Workspace SHALL define a root `Cargo.toml` that includes `the-anchor`, `the-engine`, and a shared types crate as member crates.
2. THE Anchor crate SHALL be compiled with `#![no_std]` and SHALL successfully produce a WASM artifact targeting `wasm32-unknown-unknown`.
3. THE Engine crate SHALL use the `tokio` async runtime and SHALL compile for at least `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` targets; IF the Engine crate is built targeting `wasm32-unknown-unknown`, THE build SHALL fail with a compile-time error.
4. THE shared types crate SHALL define the canonical `Keeper` and `ExecutionTask` structs; neither `the-anchor` nor `the-engine` SHALL define their own copy of these types.
5. THE Workspace SHALL include an integration test suite containing at least one test per scenario for: task provisioning, designated keeper execution, grace period Secondary_Keeper execution, and SLA slashing; each test SHALL assert at least one observable on-sandbox state change.

---

### Requirement 10: CI/CD Pipeline

**User Story:** As a contributor, I want a GitHub Actions CI/CD pipeline, so that every pull request is automatically built, tested, and validated before merging.

#### Acceptance Criteria

1. THE CI_Pipeline SHALL trigger on every pull request targeting the main branch.
2. THE CI_Pipeline SHALL trigger on every push to the main branch.
3. THE CI_Pipeline SHALL run `cargo fmt --check` and fail the build if any formatting violations are reported.
4. THE CI_Pipeline SHALL run `cargo clippy -- -D warnings` against the Soroban contract and Engine crates and fail the build if any warnings are reported.
5. THE CI_Pipeline SHALL run the full integration test suite against the Soroban sandbox environment and fail the build if any test fails.
6. WHEN the build succeeds, THE CI_Pipeline SHALL build the Anchor contract WASM artifact; IF WASM artifact creation fails, THE CI_Pipeline SHALL fail the build with a descriptive error.
7. WHEN the WASM artifact is successfully built, THE CI_Pipeline SHALL upload the WASM artifact as a build artifact regardless of the source branch.
8. WHEN a build on the main branch succeeds, THE CI_Pipeline SHALL build the Engine Docker image and push it to the container registry identified by the `REGISTRY_URL` pipeline secret; IF the Docker build or push fails, THE CI_Pipeline SHALL fail the build with a descriptive error.

---

### Requirement 11: Docker Support for Node Operators

**User Story:** As a node operator, I want a Docker image for the Engine daemon, so that I can deploy and run a Keeper node without managing Rust toolchain dependencies manually.

#### Acceptance Criteria

1. THE Docker_Image SHALL be built using a multi-stage build where the final stage contains only the Engine binary and its runtime dependencies, excluding the Rust toolchain and build artifacts.
2. THE Docker_Image SHALL accept `KEEPER_KEYPAIR`, `RPC_ENDPOINT_URL`, and `ANCHOR_CONTRACT_ADDRESS` via environment variables or a mounted configuration file; WHEN both are provided, environment variables SHALL take precedence over mounted config file values.
3. WHEN the Docker container starts, THE Engine SHALL validate that `KEEPER_KEYPAIR`, `RPC_ENDPOINT_URL`, and `ANCHOR_CONTRACT_ADDRESS` are present and non-empty; IF any required parameter is missing or empty, THE Engine SHALL exit with a non-zero exit code and a descriptive error message identifying the missing parameter.
4. THE Docker_Image SHALL expose a health-check endpoint on a configurable port (default: 8080) that returns the current ledger height and last successful execution timestamp; IF no successful execution has yet occurred, the timestamp field SHALL return a defined sentinel value (e.g., `null` or `0`).
5. THE Docker_Image SHALL be published to a public container registry with semantic version tags and a `latest` tag on each release.

---

### Requirement 12: Documentation

**User Story:** As a developer or node operator, I want comprehensive documentation, so that I can understand the protocol, integrate with the Anchor contract, and operate an Engine node.

#### Acceptance Criteria

1. THE Workspace SHALL include a root `README.md` that describes the project vision, architecture overview, crate structure, and quick-start instructions covering prerequisites, build steps, test execution, and run steps for both local development and node operation.
2. THE Anchor crate SHALL include inline Rustdoc comments on all public types, methods, and error variants.
3. THE Engine crate SHALL include inline Rustdoc comments on all public types and configuration parameters.
4. THE Workspace SHALL include a `docs/node-operator-guide.md` that details Docker deployment, environment variable configuration, available metrics and health-check endpoints, and troubleshooting steps covering at least startup failures, connectivity issues, and SLA violation events.
5. THE Workspace SHALL include a `docs/protocol-spec.md` that documents the SLA rules, slashing parameters, grace period behavior, and reward distribution formulas with all formula inputs, outputs, and units defined.
