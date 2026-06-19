# Implementation Plan: Chronos Keeper Network

## Overview

This plan converts the Chronos Keeper Network design into an ordered series of incremental coding tasks. The sequence moves from workspace scaffolding through the on-chain Anchor contract, the off-chain Engine daemon, integration tests, Docker packaging, CI/CD, and documentation. Each task builds on the previous and ends with the entire system wired together.

All Rust code uses the workspace layout defined in the design document. Property-based tests use the [`proptest`](https://github.com/proptest-rs/proptest) crate with a minimum of 512 cases per property, and are tagged with `// Feature: chronos-keeper-network, Property {N}: {property_text}`.

---

## Tasks

- [ ] 1. Scaffold the Cargo workspace and crate stubs
  - Create the root `Cargo.toml` declaring workspace members: `chronos-types`, `the-anchor`, `the-engine`, and `tests/integration`.
  - Create minimal `Cargo.toml` and `src/lib.rs` stubs for `chronos-types` and `the-anchor` (`#![no_std]`, `soroban-sdk` dependency).
  - Create minimal `Cargo.toml` and `src/main.rs` stub for `the-engine` (`tokio`, `tracing`, `axum`, `thiserror`, `serde` dependencies).
  - Create `.gitignore` (ignore `target/`, `*.wasm`, `.env`).
  - Verify `cargo check --workspace` passes on all stubs before proceeding.
  - _Requirements: 9.1, 9.2, 9.3_

- [ ] 2. Implement the `chronos-types` shared types crate
  - [ ] 2.1 Define `TaskId`, `KeeperAddress`, `Keeper`, and `ExecutionTask` types in `chronos-types/src/lib.rs`
    - Annotate with `#[contracttype]` from `soroban-sdk`.
    - Gate `soroban-sdk` behind an optional `"contract"` feature so the crate compiles for both WASM and native targets.
    - Add `#![no_std]` at the crate root.
    - _Requirements: 9.4_
  - [ ]* 2.2 Write unit tests for `chronos-types` round-trip serialization
    - Verify `Keeper` and `ExecutionTask` can be serialized and deserialized without data loss.
    - _Requirements: 9.4_

- [ ] 3. Implement `the-anchor`: `errors.rs` and `storage_keys.rs`
  - [ ] 3.1 Write `errors.rs` — define the full `ContractError` enum with `#[contracterror]`
    - Include all 18 variants from the design: `StakeBelowMinimum`, `KeeperAlreadyRegistered`, `KeeperNotFound`, `InvalidInterval`, `InvalidReward`, `UnauthorizedCaller`, `InvalidDripList`, `TaskNotFound`, `TooEarlyToExecute`, `UnauthorizedExecutor`, `DripListInvocationFailed`, `InsufficientRewardBalance`, `GracePeriodActive`, `GracePeriodExpired`, `CallerNotSecondaryEligible`, `SlashOnZeroStake`, `AlreadyInitialized`, `IneligibleKeeper`.
    - _Requirements: 1.2, 1.3, 2.2, 2.3, 2.4, 2.7, 2.8, 3.5, 3.6, 3.8, 4.7, 4.8, 5.6_
  - [ ] 3.2 Write `storage_keys.rs` — define the `DataKey` enum with `#[contracttype]`
    - Include variants: `Keeper(Address)`, `Task(BytesN<32>)`, `TaskCounter`, `Admin`, `MinKeeperStake`, `TreasuryBalance`.
    - Apply correct Soroban storage type (Persistent vs Instance) per the design's storage assignment table.
    - _Requirements: 9.1_

- [ ] 4. Implement `the-anchor`: `registry.rs` — keeper registration and lookup
  - [ ] 4.1 Implement `register_keeper` function in `registry.rs`
    - Read `MIN_KEEPER_STAKE` from Instance storage; reject if `stake_amount < min` with `StakeBelowMinimum`.
    - Reject duplicate registrations with `KeeperAlreadyRegistered`.
    - Write a new `Keeper` struct to Persistent storage (`DataKey::Keeper(addr)`) with `last_execution_ledger = 0`, `total_executions = 0`, `ineligible = false`.
    - Emit a `KeeperRegistered` event via `events.rs` (stubbed call — events module written in task 8).
    - _Requirements: 1.1, 1.2, 1.3, 1.5_
  - [ ] 4.2 Implement `get_keeper` read-only function
    - Load `DataKey::Keeper(addr)` from Persistent storage; return `KeeperNotFound` if absent.
    - _Requirements: 1.4_
  - [ ]* 4.3 Write property test for keeper registration idempotency rejection (Property 7)
    - **Property 7: Keeper registration idempotency rejection**
    - Tag: `// Feature: chronos-keeper-network, Property 7: For any registered keeper address, a second register_keeper call with any stake amount SHALL be rejected with KeeperAlreadyRegistered, and the keeper's stored stake and metadata SHALL remain unchanged.`
    - Generate arbitrary registered keeper addresses and stake amounts; call `register_keeper` twice; verify the second call returns `KeeperAlreadyRegistered` and stored state is unmodified.
    - **Validates: Requirements 1.3**

- [ ] 5. Implement `the-anchor`: `tasks.rs` — task provisioning and lookup
  - [ ] 5.1 Implement `provision_task` function in `tasks.rs`
    - Require admin auth (`DataKey::Admin`); reject non-admin callers with `UnauthorizedCaller`.
    - Validate `execution_interval_ledgers > 0`; return `InvalidInterval` if zero.
    - Validate `micro_reward_per_run > 0`; return `InvalidReward` if not.
    - Validate `designated_keeper` is present in Registry and not `ineligible`; return `KeeperNotFound` or `IneligibleKeeper`.
    - Validate `target_drip_list` address; return `InvalidDripList` if invalid.
    - Generate a unique `TaskId` by hashing the current ledger sequence plus a monotonic `TaskCounter` (Instance storage).
    - Set `next_allowed_execution = current_ledger + execution_interval_ledgers`.
    - Store the `ExecutionTask` in Persistent storage (`DataKey::Task(id)`).
    - Emit a `TaskProvisioned` event (stubbed call).
    - Return the `TaskId`.
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8_
  - [ ] 5.2 Implement `get_task` read-only function
    - Load `DataKey::Task(id)` from Persistent storage; return `TaskNotFound` if absent.
    - _Requirements: 2.5_

- [ ] 6. Implement `the-anchor`: `slashing.rs` — slash math, stake enforcement, ineligibility
  - [ ] 6.1 Implement `apply_slash` function in `slashing.rs`
    - If `stake_amount == 0`: emit a zero-slash event and return without modifying state (`SlashOnZeroStake` path).
    - Compute `slash_amount = stake * 5 / 100` using integer floor division.
    - Subtract `slash_amount` from keeper's `stake_amount` (result is non-negative by design).
    - Compute `secondary_reward = slash_amount / 2`; `treasury_portion = slash_amount - secondary_reward`.
    - Add `treasury_portion` to `DataKey::TreasuryBalance` (Persistent).
    - If `new_stake < MIN_KEEPER_STAKE`, set `keeper.ineligible = true`.
    - Write updated `Keeper` back to Persistent storage.
    - Emit a `SlashApplied` event (stubbed call).
    - Return `(slash_amount, secondary_reward)`.
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5, 5.6_
  - [ ]* 6.2 Write property test for stake floor-division invariant (Property 1)
    - **Property 1: Stake floor-division invariant**
    - Tag: `// Feature: chronos-keeper-network, Property 1: For any keeper stake amount s >= 0 and slash rate r = 5, slash = s * r / 100 (floor) <= s * r / 100.0 (float), and s - slash >= 0.`
    - Generate arbitrary `stake: i128` in `0..=i128::MAX / 100`; verify `slash <= (stake as f64 * 5.0 / 100.0) as i128` and `stake - slash >= 0`.
    - **Validates: Requirements 5.1, 5.5**
  - [ ]* 6.3 Write property test for slashed stake non-negativity (Property 2)
    - **Property 2: Slashed stake non-negativity**
    - Tag: `// Feature: chronos-keeper-network, Property 2: For any initial stake s >= 0, after N >= 0 successive slash penalties, the keeper's stake SHALL never go below zero.`
    - Generate `(stake: i128, n_slashes: u8)`; apply N slash cycles using `apply_slash` logic; assert stake is never negative after any iteration.
    - **Validates: Requirements 5.1, 5.5, 5.6**
  - [ ]* 6.4 Write property test for secondary reward conservation (Property 3)
    - **Property 3: Secondary reward conservation**
    - Tag: `// Feature: chronos-keeper-network, Property 3: For any pre-slash stake s, secondary_reward + treasury == slash_amount.`
    - Generate `stake: i128 >= 0`; compute `slash`, `secondary_reward`, `treasury`; assert `secondary_reward + treasury == slash`.
    - **Validates: Requirements 4.4, 5.1**
  - [ ]* 6.5 Write property test for zero-stake slash no-op (Property 10)
    - **Property 10: Zero-stake slash no-op**
    - Tag: `// Feature: chronos-keeper-network, Property 10: For any keeper whose stake_amount == 0, applying a slash SHALL result in slash_amount == 0, the keeper's record SHALL be unchanged, and no value SHALL be transferred.`
    - Set `stake = 0`; invoke `apply_slash`; assert `slash_amount == 0` and keeper record is unmodified.
    - **Validates: Requirements 5.6**

- [ ] 7. Implement `the-anchor`: `execution.rs` — `execute_drip_split` window arbitration
  - [ ] 7.1 Implement `execute_drip_split` function in `execution.rs`
    - Load task via `get_task`; return `TaskNotFound` if absent.
    - **Window arbitration** (per design Property 5):
      - `current_ledger < next_allowed_execution` → return `TooEarlyToExecute`.
      - `current_ledger == next_allowed_execution` and `caller != designated_keeper` → return `UnauthorizedExecutor`.
      - `current_ledger == next_allowed_execution` and `caller == designated_keeper` → designated path.
      - `next_allowed_execution < current_ledger <= next_allowed_execution + 50` and `caller == designated_keeper` → return `GracePeriodActive`.
      - `next_allowed_execution < current_ledger <= next_allowed_execution + 50` and caller is a registered non-designated keeper → secondary path (calls `apply_slash`, transfers `secondary_reward + micro_reward`).
      - `current_ledger > next_allowed_execution + 50` → emit `MissedExecution` event; any registered keeper may execute, no slash.
    - For all success paths: invoke target `Drip_List` via cross-contract call; revert with `DripListInvocationFailed` if it fails.
    - Transfer `micro_reward_per_run` to the executing keeper's address; revert with `InsufficientRewardBalance` if balance insufficient.
    - Update `task.next_allowed_execution = current_ledger + execution_interval_ledgers`.
    - Update executing keeper's `total_executions` and `last_execution_ledger`.
    - Emit appropriate events (stubbed call to `events.rs`).
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8, 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 4.9_
  - [ ]* 7.2 Write property test for task execution window arbitration (Property 5)
    - **Property 5: Task execution window arbitration**
    - Tag: `// Feature: chronos-keeper-network, Property 5: For any task with next_allowed_execution = T and current ledger L, the contract accepts or rejects calls per the four-region window rule.`
    - Generate `(next_allowed: u32, current_ledger: u32, caller_role)`; assert the contract accepts or rejects the invocation per the window rules for all four regions.
    - **Validates: Requirements 3.5, 3.8, 4.1, 4.7, 4.8, 4.9**
  - [ ]* 7.3 Write property test for task scheduling monotonicity (Property 6)
    - **Property 6: Task scheduling monotonicity**
    - Tag: `// Feature: chronos-keeper-network, Property 6: For any successfully executed task, new_next_allowed_execution > old_next_allowed_execution.`
    - Generate valid tasks and simulate execution; assert `new_next > old_next` for every success case.
    - **Validates: Requirements 3.3, 4.6**

- [ ] 8. Implement `the-anchor`: `events.rs` — all on-chain event emission
  - Replace all stubbed event calls with actual `env.events().publish(...)` implementations.
  - Implement the following events:
    - `KeeperRegistered { address, stake_amount }` — emitted by `registry.rs`.
    - `TaskProvisioned { task_id, target_drip_list, interval, micro_reward_per_run, designated_keeper }` — emitted by `tasks.rs`.
    - `TaskExecuted { task_id, keeper, ledger, reward }` — designated or post-grace path.
    - `TaskExecutedBySecondary { task_id, secondary_keeper, designated_keeper, ledger, slash_amount, secondary_reward, micro_reward }` — secondary path.
    - `SlashApplied { keeper, amount_slashed, new_stake }` — emitted by `slashing.rs`.
    - `MissedExecution { task_id, ledgers_elapsed }` — emitted on post-grace execution path.
  - _Requirements: 1.5, 1.6, 2.6, 3.7, 4.4, 5.4_

- [ ] 9. Wire `the-anchor`: `lib.rs` contract entry point
  - [ ] 9.1 Implement the `AnchorContract` struct with `#[contract]` and `#[contractimpl]` macros in `lib.rs`
    - Implement `initialize(env, admin, min_keeper_stake)`: store `DataKey::Admin` and `DataKey::MinKeeperStake` in Instance storage; return `AlreadyInitialized` if already set.
    - Delegate `register_keeper`, `get_keeper` to `registry.rs`.
    - Delegate `provision_task`, `get_task` to `tasks.rs`.
    - Delegate `execute_drip_split` to `execution.rs`.
    - Verify `cargo build --target wasm32-unknown-unknown --release -p the-anchor` produces a valid WASM artifact.
    - _Requirements: 9.2_
  - [ ]* 9.2 Write unit tests for contract boundary conditions
    - Test `initialize` idempotency (second call returns `AlreadyInitialized`).
    - Test slash math with specific known inputs/outputs.
    - Test window arbitration boundary conditions at `T-1`, `T`, `T+1`, `T+50`, `T+51`.
    - Test reward distribution exact amounts for known scenarios.
    - _Requirements: 1.1, 1.2, 3.1, 3.5, 4.1, 5.1_

- [ ] 10. Implement `the-anchor`: property-based tests for registry and ineligibility
  - [ ]* 10.1 Write property test for ineligibility threshold invariant (Property 4)
    - **Property 4: Ineligibility threshold invariant**
    - Tag: `// Feature: chronos-keeper-network, Property 4: For any sequence of slash events, once stake falls below minimum_stake_threshold, ineligible == true and the keeper is never returned as a valid designated_keeper candidate.`
    - Generate sequences of slash events applied to a keeper; once stake drops below threshold, verify `ineligible == true` and remains true for all subsequent slashes.
    - Located in `the-anchor/tests/property/registry.rs`.
    - **Validates: Requirements 5.2, 5.3**

- [ ] 11. Checkpoint — Anchor contract complete
  - Ensure `cargo test -p the-anchor` passes.
  - Ensure `cargo build --target wasm32-unknown-unknown --release -p the-anchor` succeeds.
  - Ask the user if questions arise before proceeding to the Engine.

- [ ] 12. Implement `the-engine`: `errors.rs` and `config.rs`
  - [ ] 12.1 Implement `errors.rs` — define `EngineError`, `TransientError`, `NonTransientError` enums
    - Use `thiserror::Error` derive.
    - `TransientError` variants: `Io`, `ConnectionTimeout`, `ConnectionReset`, `RpcTimeout`.
    - `NonTransientError` variants: `ContractRejection`, `ConfirmationTimeout`, `KeypairDecode`, `ConfigInvalid`.
    - _Requirements: 7.4, 7.5_
  - [ ] 12.2 Implement `config.rs` — `EngineConfig` struct and env-var/file loading with validation
    - Fields: `keeper_keypair`, `rpc_endpoint_url`, `anchor_contract_address`, `poll_interval` (1–5 s), `task_query_timeout` (≤10 s), `confirmation_timeout` (≤60 s), `retry_backoff_initial`, `retry_backoff_max` (≤60 s), `max_retries`, `health_port` (default 8080), `config_file_path`.
    - Validate all required fields; exit with non-zero code and descriptive error on missing/empty `KEEPER_KEYPAIR`, `RPC_ENDPOINT_URL`, `ANCHOR_CONTRACT_ADDRESS`.
    - Env vars take precedence over mounted config file values.
    - _Requirements: 11.2, 11.3_
  - [ ]* 12.3 Write unit tests for `config.rs` validation
    - Test missing env vars (each of the three required fields), zero intervals, out-of-range ports.
    - _Requirements: 11.3_

- [ ] 13. Implement `the-engine`: `rpc_client.rs` — Stellar RPC wrapper
  - [ ] 13.1 Implement `rpc_client.rs` — thin async wrapper over `stellar-rpc-client`
    - Expose `get_latest_ledger() -> Result<u32, TransientError>`.
    - Expose `query_executable_tasks(ledger: u32) -> Result<Vec<ExecutionTask>, EngineError>`.
    - Expose `send_transaction(signed_tx: ...) -> Result<TxHash, EngineError>`.
    - Expose `get_transaction(hash: TxHash) -> Result<TxResult, EngineError>`.
    - Map I/O errors → `TransientError`; contract rejections → `NonTransientError`.
    - _Requirements: 6.1, 6.4, 7.1, 7.2_

- [ ] 14. Implement `the-engine`: `in_flight.rs` — task deduplication set
  - [ ] 14.1 Implement `InFlightSet` as `Arc<Mutex<HashSet<TaskId>>>` in `in_flight.rs`
    - Expose `insert(task_id) -> bool` (returns false if already present).
    - Expose `remove(task_id)`.
    - Expose `contains(task_id) -> bool`.
    - Expose `len() -> usize`.
    - _Requirements: 6.5, 6.6_
  - [ ]* 14.2 Write property test for in-flight deduplication invariant (Property 8)
    - **Property 8: In-flight deduplication invariant**
    - Tag: `// Feature: chronos-keeper-network, Property 8: For any set of concurrent task execution attempts, the number of in-flight entries for a given task_id in the InFlightSet SHALL never exceed 1 at any point in time.`
    - Simulate concurrent `insert`/`remove` sequences via `tokio::spawn`; verify the set never holds duplicate entries for the same `task_id`.
    - Located in `the-engine/tests/property/in_flight.rs`.
    - **Validates: Requirements 6.5**

- [ ] 15. Implement `the-engine`: `ledger_poller.rs` — RPC poll loop
  - [ ] 15.1 Implement `LedgerPoller` struct in `ledger_poller.rs`
    - Holds `last_known_ledger: Arc<AtomicU32>` and `poll_interval`.
    - `run(tx: broadcast::Sender<LedgerEvent>)` async method: sleep → `rpc.get_latest_ledger()` → if advanced, update `last_known_ledger`, send `LedgerEvent::Advance(seq)` on broadcast channel; on transient error, log WARN and apply exponential backoff (1 s initial, capped at `retry_backoff_max`).
    - Expose `last_known() -> u32` for health-check reads.
    - _Requirements: 6.1, 6.4_

- [ ] 16. Implement `the-engine`: `task_discovery.rs` — task query with 10 s timeout and dispatch
  - [ ] 16.1 Implement `TaskDiscovery` struct in `task_discovery.rs`
    - Subscribe to `LedgerPoller`'s broadcast channel.
    - On each `LedgerEvent::Advance(seq)`: wrap RPC task query in `tokio::time::timeout(10s)`; on timeout log WARN and skip cycle.
    - Update `LocalTaskCache` (`HashMap<TaskId, ExecutionTask>`) on successful query.
    - Call `classify_and_dispatch(task, seq)` for each discovered task not already in `InFlightSet`.
    - `classify_and_dispatch` logic (per design pseudocode):
      - `designated_keeper == self.address && seq == task.next_allowed_execution` → `ExecutionPipeline::dispatch(task, Designated)`.
      - `seq > next_allowed && seq <= next_allowed + 50 && designated != self.address` → `GraceMonitor::evaluate(task, seq)`.
      - `seq > next_allowed + 50` → `GraceMonitor::log_missed(task, seq)`.
    - _Requirements: 6.2, 6.3, 6.5_

- [ ] 17. Implement `the-engine`: `execution_pipeline.rs` — sign, submit, confirm, retry
  - [ ] 17.1 Implement `ExecutionPipeline` struct in `execution_pipeline.rs`
    - `dispatch(task, role)`: insert task into `InFlightSet`, call `execute_with_retry`, remove from `InFlightSet` regardless of outcome.
    - `execute_with_retry` loop (`0..=max_retries`):
      - Build and sign `execute_drip_split` transaction with `keeper_keypair`.
      - `rpc.send_transaction(signed)`:
        - `Ok(hash)` → `wait_for_confirmation(hash, 60s)`:
          - `Ok(result)` → log INFO (`task_id`, `keeper`, `ledger`, `tx_hash`, `reward`); update `LocalTaskCache` and `last_confirmed_ts`; return.
          - `Err(ConfirmationTimeout)` → log WARN; break (no retry).
        - `Err(Transient(e))` → if `attempt == max_retries`: log ERROR "max retries reached", break; else sleep `min(initial * 2^attempt, max_backoff)`.
        - `Err(NonTransient(e))` → log ERROR with reason; break immediately.
    - Expose `last_confirmed_ts() -> Option<i64>` for health-check reads.
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7_
  - [ ]* 17.2 Write property test for engine retry bounded termination (Property 9)
    - **Property 9: Engine retry bounded termination**
    - Tag: `// Feature: chronos-keeper-network, Property 9: For any sequence of transient errors for a given task, the Engine SHALL attempt at most max_retries + 1 submissions and SHALL eventually terminate the retry loop, removing the task from the in-flight set regardless of outcome.`
    - Inject sequences of `max_retries`-worth of `TransientError` responses into a mock RPC; verify the pipeline terminates after exactly `max_retries + 1` attempts and `InFlightSet` is empty afterward.
    - Located in `the-engine/tests/property/pipeline.rs`.
    - **Validates: Requirements 7.4**

- [ ] 18. Implement `the-engine`: `grace_monitor.rs` — secondary keeper detection and dispatch
  - [ ] 18.1 Implement `GraceMonitor` struct in `grace_monitor.rs`
    - `evaluate(task, current_ledger)`:
      - Compute `ledgers_elapsed = current_ledger - task.next_allowed_execution`.
      - If `ledgers_elapsed > 50`: call `log_missed(task, current_ledger)`; return.
      - If `self.address == task.designated_keeper`: return (not eligible as secondary).
      - If `!keeper_registry.is_registered(self.address)`: return.
      - If `in_flight_set.contains(task.id)`: return.
      - Call `ExecutionPipeline::dispatch(task, Secondary)`.
    - `log_missed(task, seq)`: log WARN "missed-execution" with `task_id` and `ledgers_elapsed`; do not attempt execution.
    - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5_

- [ ] 19. Implement `the-engine`: `health_server.rs` — Axum health endpoint
  - [ ] 19.1 Implement `HealthServer` using Axum in `health_server.rs`
    - `GET /health` handler: read `LedgerPoller::last_known()`, `ExecutionPipeline::last_confirmed_ts()`, `InFlightSet::len()`; return `HealthResponse` as JSON with `200 OK`, `Content-Type: application/json`.
    - `HealthResponse.last_execution_timestamp` is `Option<i64>`; return `null` (JSON) when no successful execution has occurred.
    - Bind to `0.0.0.0:{health_port}` from `EngineConfig`.
    - _Requirements: 11.4_

- [ ] 20. Implement `the-engine`: `main.rs` — runtime wiring and signal handling
  - [ ] 20.1 Wire all components together in `main.rs`
    - Load `EngineConfig` (env vars / config file); exit non-zero on validation failure with descriptive message.
    - Decode `keeper_keypair` using `stellar-strkey`; exit non-zero on decode failure.
    - Construct `RpcClient`, `InFlightSet`, `LocalTaskCache`.
    - Construct and spawn `LedgerPoller`, `TaskDiscovery`, `ExecutionPipeline`, `GraceMonitor`, `HealthServer` as separate Tokio tasks.
    - Handle `SIGTERM` and `SIGINT` (Ctrl+C): gracefully shut down all tasks.
    - _Requirements: 9.3, 11.2, 11.3_

- [ ] 21. Checkpoint — Engine daemon complete
  - Ensure `cargo test -p the-engine` passes.
  - Ensure `cargo build --release -p the-engine` succeeds for the native target.
  - Ask the user if questions arise before proceeding to integration tests.

- [ ] 22. Write integration tests against the Soroban sandbox
  - [ ] 22.1 Implement `tests/integration/test_task_provisioning.rs`
    - Deploy Anchor to Soroban sandbox; register a keeper; call `provision_task`; call `get_task`; assert all stored fields match inputs.
    - _Requirements: 9.5, 2.1, 2.5_
  - [ ] 22.2 Implement `tests/integration/test_designated_execution.rs`
    - Register keeper; provision task; advance sandbox ledger to `next_allowed_execution`; call `execute_drip_split` as designated keeper; assert `next_allowed_execution` updated and `total_executions` incremented.
    - _Requirements: 9.5, 3.1, 3.3, 3.4_
  - [ ] 22.3 Implement `tests/integration/test_grace_period.rs`
    - Register two keepers; provision task; advance sandbox ledger past `next_allowed_execution` by 1–50; call `execute_drip_split` as secondary keeper; assert `next_allowed_execution` updated, secondary reward and `micro_reward_per_run` transferred to secondary keeper.
    - _Requirements: 9.5, 4.1, 4.4, 4.5, 4.6_
  - [ ] 22.4 Implement `tests/integration/test_slashing.rs`
    - Register two keepers; provision task; advance sandbox ledger into grace period; secondary keeper executes; assert designated keeper's stake reduced by exactly `stake * 5 / 100` (floor), `SlashApplied` event emitted, treasury balance increased by `slash_amount / 2`.
    - _Requirements: 9.5, 5.1, 5.4, 5.5_
  - [ ] 22.5 Implement `tests/integration/test_post_grace.rs`
    - Register keeper; provision task; advance sandbox ledger by >50 past `next_allowed_execution`; execute task; assert no slash applied, `MissedExecution` event emitted, `TaskExecuted` event emitted.
    - _Requirements: 9.5, 4.9_

- [ ] 23. Checkpoint — All tests pass
  - Ensure `cargo test --workspace` passes.
  - Ensure all five integration test scenarios pass against the Soroban sandbox.
  - Ask the user if questions arise before proceeding to packaging.

- [ ] 24. Write the Dockerfile — multi-stage Engine image
  - [ ] 24.1 Create `Dockerfile` at workspace root using the multi-stage pattern from the design
    - Stage 1 (`builder`): `FROM rust:1.78-slim AS builder`; copy workspace; `cargo build --release -p the-engine --target x86_64-unknown-linux-gnu`.
    - Stage 2 (runtime): `FROM debian:bookworm-slim`; install `ca-certificates`; copy binary from builder; `EXPOSE 8080`; `ENTRYPOINT ["/usr/local/bin/the-engine"]`.
    - Final stage MUST NOT contain the Rust toolchain or build artifacts.
    - _Requirements: 11.1_

- [ ] 25. Write GitHub Actions CI/CD pipeline
  - [ ] 25.1 Create `.github/workflows/ci.yml`
    - Trigger on `pull_request` → `main` and `push` → `main`.
    - Job `check`: `cargo fmt --check`; `cargo clippy -- -D warnings`.
    - Job `test`: `cargo test --workspace`; run integration tests against the Soroban sandbox.
    - Job `wasm`: `cargo build --target wasm32-unknown-unknown --release -p the-anchor`; upload WASM artifact (all branches).
    - Job `docker` (main branch only): `docker build` multi-stage; `docker push` to `${{ secrets.REGISTRY_URL }}`; fail build if either step fails.
    - _Requirements: 10.1, 10.2, 10.3, 10.4, 10.5, 10.6, 10.7, 10.8_

- [ ] 26. Write documentation
  - [ ] 26.1 Write `README.md` at workspace root
    - Sections: project vision, architecture overview, crate structure, prerequisites, build steps, test execution, run steps for local development and node operation.
    - _Requirements: 12.1_
  - [ ] 26.2 Add Rustdoc comments to all public types, methods, and error variants in `the-anchor`
    - Cover all public items in `lib.rs`, `registry.rs`, `tasks.rs`, `execution.rs`, `slashing.rs`, `events.rs`, `errors.rs`, `storage_keys.rs`.
    - _Requirements: 12.2_
  - [ ] 26.3 Add Rustdoc comments to all public types and configuration parameters in `the-engine`
    - Cover all public items in `config.rs`, `errors.rs`, `health_server.rs`, and all exported structs/functions.
    - _Requirements: 12.3_
  - [ ] 26.4 Write `docs/node-operator-guide.md`
    - Cover: Docker deployment, environment variable configuration, health-check endpoint, available metrics, troubleshooting (startup failures, connectivity issues, SLA violation events).
    - _Requirements: 12.4_
  - [ ] 26.5 Write `docs/protocol-spec.md`
    - Cover: SLA rules, slashing parameters, grace period behavior, reward distribution formulas with all inputs/outputs/units defined.
    - _Requirements: 12.5_

---

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP; property tests are especially valuable for the slashing math.
- Each task references specific requirements from `requirements.md` for full traceability.
- The Anchor contract must pass `cargo build --target wasm32-unknown-unknown --release` before proceeding to the Engine tasks.
- All property-based tests use `proptest` with `proptest!(cases = 512)` and are tagged with the canonical `// Feature: chronos-keeper-network, Property {N}: ...` format.
- The `chronos-types` crate must compile for both `wasm32-unknown-unknown` (contract) and native (engine) targets — the `soroban-sdk` dependency must be feature-gated.
- All Engine log entries include structured fields: `task_id`, `keeper_address`, `ledger`, `role`, `tx_hash`, `reward_amount`, `error`, `attempt` (as applicable).
- Checkpoints at tasks 11 and 21 are gates before moving to the next major layer.

---

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["2.1", "3.1", "3.2"] },
    { "id": 1, "tasks": ["2.2", "4.1", "4.2"] },
    { "id": 2, "tasks": ["4.3", "5.1", "5.2"] },
    { "id": 3, "tasks": ["6.1"] },
    { "id": 4, "tasks": ["6.2", "6.3", "6.4", "6.5", "7.1"] },
    { "id": 5, "tasks": ["7.2", "7.3", "8.1 (events)"] },
    { "id": 6, "tasks": ["9.1"] },
    { "id": 7, "tasks": ["9.2", "10.1"] },
    { "id": 8, "tasks": ["12.1", "12.2"] },
    { "id": 9, "tasks": ["12.3", "13.1"] },
    { "id": 10, "tasks": ["14.1"] },
    { "id": 11, "tasks": ["14.2", "15.1"] },
    { "id": 12, "tasks": ["16.1"] },
    { "id": 13, "tasks": ["17.1"] },
    { "id": 14, "tasks": ["17.2", "18.1"] },
    { "id": 15, "tasks": ["19.1"] },
    { "id": 16, "tasks": ["20.1"] },
    { "id": 17, "tasks": ["22.1", "22.2", "22.3", "22.4", "22.5"] },
    { "id": 18, "tasks": ["24.1", "25.1"] },
    { "id": 19, "tasks": ["26.1", "26.2", "26.3", "26.4", "26.5"] }
  ]
}
```
