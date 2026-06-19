//! Integration test: grace period secondary-keeper execution.
//!
//! Verifies that:
//! - A secondary keeper can execute during the 50-ledger grace window.
//! - The task schedule advances from the execution ledger.
//! - The designated keeper is blocked during grace.
//! - An unregistered caller is rejected.

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
fn secondary_keeper_executes_in_grace_and_schedule_advances() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    let secondary = Address::generate(&env);
    client.register_keeper(&designated, &50_000_000_i128).unwrap();
    client.register_keeper(&secondary, &50_000_000_i128).unwrap();

    let interval: u32 = 200;
    let task_id = client
        .provision_task(&drip_list, &interval, &5_000_000_i128, &designated)
        .unwrap();

    let task_before = client.get_task(&task_id).unwrap();
    // 30 ledgers into the grace period.
    let grace_ledger = task_before.next_allowed_execution + 30;
    env.ledger().set_sequence_number(grace_ledger);

    client.execute_drip_split(&task_id, &secondary).unwrap();

    let task_after = client.get_task(&task_id).unwrap();
    assert_eq!(
        task_after.next_allowed_execution,
        grace_ledger + interval,
        "schedule must advance from grace execution ledger"
    );

    let secondary_rec = client.get_keeper(&secondary).unwrap();
    assert_eq!(secondary_rec.total_executions, 1);
    assert_eq!(secondary_rec.last_execution_ledger, grace_ledger);
}

#[test]
fn designated_keeper_blocked_during_grace() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let keeper = Address::generate(&env);
    client.register_keeper(&keeper, &50_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &keeper)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    env.ledger().set_sequence_number(task.next_allowed_execution + 10);

    let err = client.execute_drip_split(&task_id, &keeper).unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("GracePeriodActive") || msg.contains("13"),
        "designated keeper should be blocked during grace: {msg}"
    );
}

#[test]
fn unregistered_caller_blocked_during_grace() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    client.register_keeper(&designated, &50_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &designated)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    env.ledger().set_sequence_number(task.next_allowed_execution + 5);

    let stranger = Address::generate(&env);
    let err = client.execute_drip_split(&task_id, &stranger).unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("CallerNotSecondaryEligible") || msg.contains("15"),
        "unregistered caller should be rejected: {msg}"
    );
}
