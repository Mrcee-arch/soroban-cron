//! Integration test: designated keeper execution (happy path).
//!
//! Registers a keeper, provisions a task, advances the ledger to
//! `next_allowed_execution`, executes as the designated keeper, and asserts:
//! - `next_allowed_execution` advances by the interval.
//! - `total_executions` increments.
//! - `last_execution_ledger` is set correctly.

mod common;

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, Env};
use the_anchor::AnchorContractClient;

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let native_token = sac.address();
    let drip_list = env.register(common::MockDripList, ());
    let anchor = env.register(the_anchor::AnchorContract, ());

    AnchorContractClient::new(&env, &anchor)
        .initialize(&admin, &native_token, &10_000_000_i128)
        .unwrap();

    token::StellarAssetClient::new(&env, &native_token).mint(&anchor, &500_000_000_i128);

    (env, anchor, native_token, drip_list)
}

#[test]
fn designated_keeper_executes_and_state_updates() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let keeper = Address::generate(&env);
    client.register_keeper(&keeper, &10_000_000_i128).unwrap();

    let interval: u32 = 100;
    let task_id = client
        .provision_task(&drip_list, &interval, &5_000_000_i128, &keeper)
        .unwrap();

    let task_before = client.get_task(&task_id).unwrap();
    let target_ledger = task_before.next_allowed_execution;

    // Advance ledger to exactly `next_allowed_execution`.
    env.ledger().set_sequence_number(target_ledger);

    client.execute_drip_split(&task_id, &keeper).unwrap();

    // Schedule must advance.
    let task_after = client.get_task(&task_id).unwrap();
    assert_eq!(
        task_after.next_allowed_execution,
        target_ledger + interval,
        "next_allowed_execution should advance by the interval"
    );

    // Keeper stats must update.
    let keeper_rec = client.get_keeper(&keeper).unwrap();
    assert_eq!(keeper_rec.total_executions, 1);
    assert_eq!(keeper_rec.last_execution_ledger, target_ledger);
}

#[test]
fn execution_before_window_is_rejected() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let keeper = Address::generate(&env);
    client.register_keeper(&keeper, &10_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &keeper)
        .unwrap();

    // Ledger is still at provision-time — before next_allowed_execution.
    let err = client.execute_drip_split(&task_id, &keeper).unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("TooEarlyToExecute") || msg.contains('9'),
        "unexpected error: {msg}"
    );
}

#[test]
fn wrong_caller_in_designated_window_is_rejected() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    let other = Address::generate(&env);
    client.register_keeper(&designated, &10_000_000_i128).unwrap();
    client.register_keeper(&other, &10_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &designated)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    env.ledger().set_sequence_number(task.next_allowed_execution);

    let err = client.execute_drip_split(&task_id, &other).unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("UnauthorizedExecutor") || msg.contains("10"),
        "unexpected error: {msg}"
    );
}
