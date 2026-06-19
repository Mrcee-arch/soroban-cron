# Protocol Specification — Chronos Keeper Network

Version: 0.1.0  
Network: Stellar / Soroban  
License: MIT

---

## 1. Overview

Chronos Keeper Network automates time-sensitive Drips funding-split
distributions on Stellar. It uses a **Proof-of-Stake keeper model**: operators
lock XLM as collateral to participate, are assigned tasks, and earn rewards for
timely execution. Missed execution windows trigger economic penalties
(slashing).

---

## 2. Actors

| Actor | Description |
|-------|-------------|
| **Admin** | Governance address that initialises the contract and provisions tasks. |
| **Keeper** | Registered node operator with staked XLM. Executes tasks and earns rewards. |
| **Designated Keeper** | The keeper explicitly assigned to a task for a given execution window. |
| **Secondary Keeper** | Any registered keeper (not the designated one) that steps in during the grace period. |
| **Treasury** | The Anchor contract itself, which accumulates the non-distributed portion of slashed stake. |

---

## 3. Staking

### 3.1 Registration

A keeper registers by calling `register_keeper(address, stake_amount)`.

**Minimum stake:**

```
MIN_KEEPER_STAKE = 10_000_000 stroops  (= 1 XLM)
```

The on-chain `DataKey::MinKeeperStake` Instance value overrides this default
when set by the admin during `initialize`.

**Conditions for acceptance:**

- `stake_amount ≥ MIN_KEEPER_STAKE`
- The address must not already be registered.

### 3.2 Ineligibility

A keeper becomes **ineligible** for new task assignments when their stake falls
below `MIN_KEEPER_STAKE` after slashing. Ineligible keepers:

- Remain registered (their stake is not confiscated).
- Cannot be assigned as `designated_keeper` on new tasks.
- Can still execute as secondary keepers during grace periods.

---

## 4. Task Provisioning

Only the admin may call `provision_task`.

### 4.1 Parameters

| Parameter | Type | Constraints | Description |
|-----------|------|-------------|-------------|
| `target_drip_list` | `Address` | Valid contract address | Target Drip List contract to invoke. |
| `execution_interval_ledgers` | `u32` | `> 0` | Number of ledgers between executions. |
| `micro_reward_per_run` | `i128` | `> 0` | XLM reward paid to the executing keeper (stroops). |
| `designated_keeper` | `Address` | Registered and eligible | The keeper assigned primary responsibility. |

### 4.2 Task ID generation

```
counter  = monotonic TaskCounter (u32, Instance storage)
seed     = counter_be_bytes (4) ++ current_ledger_be_bytes (4)
task_id  = SHA-256(seed)   →  BytesN<32>
```

### 4.3 Initial schedule

```
next_allowed_execution = current_ledger + execution_interval_ledgers
```

---

## 5. Execution Windows

For a task with `next_allowed_execution = T` and current ledger `L`:

```
Region          Condition               Allowed callers         Slash?
────────────────────────────────────────────────────────────────────────
Pre-window      L < T                   None                    No
Designated      L == T                  designated_keeper only  No
Grace           T < L ≤ T + 50         Secondary keepers only  Yes (5%)
Post-grace      L > T + 50             Any registered keeper   No
```

### 5.1 Designated window (`L == T`)

- Only the `designated_keeper` may call `execute_drip_split`.
- All other callers receive `UnauthorizedExecutor`.

### 5.2 Grace period (`T < L ≤ T + 50`)

- The `designated_keeper` is **blocked** (`GracePeriodActive`).
- Any registered keeper other than the designated keeper may execute.
- Unregistered callers receive `CallerNotSecondaryEligible`.
- A successful secondary execution triggers a slash on the designated keeper.

### 5.3 Post-grace (`L > T + 50`)

- The designated keeper's execution window is considered permanently missed.
- Any registered keeper (including the designated one) may execute.
- No slash penalty is applied.
- A `MissedExecution` event is emitted before execution proceeds.

---

## 6. Execution Success Path

Regardless of which region triggers execution, the following steps occur
atomically within a single Soroban invocation:

1. **Cross-contract call**: invoke `target_drip_list.distribute_wave_splits()`.
   If this call fails, the entire transaction reverts.

2. **Reward transfer**: transfer `micro_reward_per_run` stroops from the
   contract balance to the executing keeper's address. Reverts with
   `InsufficientRewardBalance` if the contract balance is too low.

3. **Schedule advance**:
   ```
   new_next_allowed_execution = current_ledger + execution_interval_ledgers
   ```
   This value is always strictly greater than the previous
   `next_allowed_execution` because `execution_interval_ledgers > 0`.

4. **Keeper stats update**:
   ```
   keeper.total_executions     += 1
   keeper.last_execution_ledger = current_ledger
   ```

5. **Event emission**: appropriate event (see §8).

---

## 7. Slashing

### 7.1 Slash formula

All arithmetic uses integer (floor) division.

```
slash_amount     = designated_keeper.stake_amount * 5 / 100
new_stake        = designated_keeper.stake_amount - slash_amount
secondary_reward = slash_amount / 2
treasury_portion = slash_amount - secondary_reward
```

**Conservation property:**
```
secondary_reward + treasury_portion == slash_amount   ∀ slash_amount ≥ 0
```

**Non-negativity invariant:**
```
new_stake ≥ 0   ∀ stake_amount ≥ 0
```
(Because `slash_amount ≤ stake_amount` when the slash rate is 5 %.)

### 7.2 Reward distribution

| Recipient | Amount |
|-----------|--------|
| Secondary keeper | `secondary_reward` + `micro_reward_per_run` |
| Treasury | `treasury_portion` (held in `DataKey::TreasuryBalance`) |

### 7.3 Zero-stake guard

If `designated_keeper.stake_amount == 0`:
- A `ZeroSlash` event is emitted.
- No state is modified.
- The secondary execution still succeeds (the secondary keeper still receives
  `micro_reward_per_run`).

### 7.4 Ineligibility after slash

After every slash:
```
if new_stake < MIN_KEEPER_STAKE:
    keeper.ineligible = true
```

Once `ineligible = true`, the keeper is excluded from new task assignments.
The flag is never reset automatically.

---

## 8. On-chain Events

All events use `env.events().publish(topic, data)`.

| Event | Topic symbol | Data fields | Emitted by |
|-------|-------------|-------------|-----------|
| KeeperRegistered | `kpr_reg` | `(address, stake_amount)` | `register_keeper` |
| TaskProvisioned | `task_prov` | `(task_id, target_drip_list, interval, micro_reward_per_run, designated_keeper)` | `provision_task` |
| TaskExecuted | `task_exec` | `(task_id, keeper, ledger, reward)` | Designated / post-grace execution |
| TaskExecutedBySecondary | `task_sec` | `(task_id, secondary_keeper, designated_keeper, ledger, slash_amount, secondary_reward, micro_reward)` | Grace-period execution |
| SlashApplied | `slashed` | `(keeper, amount_slashed, new_stake)` | `apply_slash` |
| ZeroSlash | `zero_slsh` | `(keeper)` | `apply_slash` (zero-stake guard) |
| MissedExecution | `missed` | `(task_id, ledgers_elapsed)` | Post-grace execution |

---

## 9. Storage Layout

| Key | Type | Tier | Description |
|-----|------|------|-------------|
| `Keeper(Address)` | `Keeper` | Persistent | Per-keeper registry entry |
| `Task(BytesN<32>)` | `ExecutionTask` | Persistent | Per-task execution record |
| `TaskCounter` | `u32` | Instance | Monotonic counter for task ID generation |
| `Admin` | `Address` | Instance | Contract admin address |
| `MinKeeperStake` | `i128` | Instance | Minimum registration stake |
| `TreasuryBalance` | `i128` | Persistent | Accumulated slash treasury |
| `NativeToken` | `Address` | Instance | Native token contract address |

---

## 10. Error Codes

| Code | Name | Condition |
|------|------|-----------|
| 1 | `StakeBelowMinimum` | `stake_amount < MIN_KEEPER_STAKE` |
| 2 | `KeeperAlreadyRegistered` | Address already in registry |
| 3 | `KeeperNotFound` | Address not in registry |
| 4 | `InvalidInterval` | `execution_interval_ledgers == 0` |
| 5 | `InvalidReward` | `micro_reward_per_run ≤ 0` |
| 6 | `UnauthorizedCaller` | Caller is not admin |
| 7 | `InvalidDripList` | Invalid target address |
| 8 | `TaskNotFound` | TaskId has no entry |
| 9 | `TooEarlyToExecute` | `current_ledger < next_allowed_execution` |
| 10 | `UnauthorizedExecutor` | Caller not permitted in current window |
| 11 | `DripListInvocationFailed` | Cross-contract call reverted |
| 12 | `InsufficientRewardBalance` | Contract balance < reward |
| 13 | `GracePeriodActive` | Designated keeper blocked during grace |
| 14 | `GracePeriodExpired` | Grace window has closed |
| 15 | `CallerNotSecondaryEligible` | Unregistered caller during grace |
| 16 | `SlashOnZeroStake` | Slash attempted on zero-stake keeper |
| 17 | `AlreadyInitialized` | `initialize` called twice |
| 18 | `IneligibleKeeper` | Designated keeper is ineligible |

---

## 11. Units and Precision

| Unit | Definition |
|------|------------|
| Stroop | 1 × 10⁻⁷ XLM — the smallest XLM subdivision |
| 1 XLM | 10,000,000 stroops |
| `MIN_KEEPER_STAKE` | 10,000,000 stroops (1 XLM) |
| Slash rate | 5 % of pre-slash stake, floor integer division |
| Grace period | 50 ledgers (≈ 5 minutes at 6-second ledger close time) |

---

## 12. Security Considerations

- **Atomicity**: slash, reward transfer, drip distribution, and state updates
  all occur within a single Soroban invocation. Partial failure reverts all.
- **Auth**: `register_keeper` requires the keeper's own auth. `provision_task`
  requires admin auth. `execute_drip_split` requires the caller's auth.
- **Integer safety**: all slash arithmetic uses `i128` with floor division.
  Stake can never go negative (proven by the non-negativity invariant in §7.1).
- **Replay protection**: `next_allowed_execution` advances after every execution,
  preventing the same window from being executed twice.
- **Deduplication**: The Engine's `InFlightSet` prevents concurrent duplicate
  submissions for the same task.
