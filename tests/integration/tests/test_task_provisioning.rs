//! Integration test: task provisioning.
//!
//! Deploys the Anchor contract to the Soroban sandbox, registers a keeper,
//! provisions a task, and verifies all stored fields match the inputs.

mod common;

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, Env};
use the_anchor::AnchorContractClient;

/// Deploy Anchor + fund it, return (env, anchor_id, native_token_addr, drip_list_addr).
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

    // Fund anchor with enough XLM to pay rewards.
    token::StellarAssetClient::new(&env, &native_token).mint(&anchor, &500_000_000_i128);

    (env, anchor, native_token, drip_list)
}

#[test]
fn provision_task_stores_all_fields() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let keeper = Address::generate(&env);
    client.register_keeper(&keeper, &10_000_000_i128).unwrap();

    let interval: u32 = 100;
    let reward: i128 = 5_000_000;
    let task_id = client
        .provision_task(&drip_list, &interval, &reward, &keeper)
        .expect("provision_task failed");

    let task = client.get_task(&task_id).expect("get_task failed");

    assert_eq!(task.task_id, task_id);
    assert_eq!(task.target_drip_list, drip_list);
    assert_eq!(task.execution_interval_ledgers, interval);
    assert_eq!(task.micro_reward_per_run, reward);
    assert_eq!(task.designated_keeper, keeper);

    // next_allowed_execution must equal current_ledger + interval at provision time.
    let current = env.ledger().sequence();
    assert_eq!(task.next_allowed_execution, current + interval);
}

#[test]
fn provision_task_rejects_zero_interval() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let keeper = Address::generate(&env);
    client.register_keeper(&keeper, &10_000_000_i128).unwrap();

    let err = client
        .provision_task(&drip_list, &0_u32, &5_000_000_i128, &keeper)
        .unwrap_err();

    let msg = format!("{err:?}");
    assert!(msg.contains("InvalidInterval") || msg.contains('4'), "unexpected: {msg}");
}

#[test]
fn provision_task_rejects_zero_reward() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let keeper = Address::generate(&env);
    client.register_keeper(&keeper, &10_000_000_i128).unwrap();

    let err = client
        .provision_task(&drip_list, &100_u32, &0_i128, &keeper)
        .unwrap_err();

    let msg = format!("{err:?}");
    assert!(msg.contains("InvalidReward") || msg.contains('5'), "unexpected: {msg}");
}

#[test]
fn provision_task_rejects_unregistered_keeper() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let unregistered = Address::generate(&env);
    let err = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &unregistered)
        .unwrap_err();

    let msg = format!("{err:?}");
    assert!(msg.contains("KeeperNotFound") || msg.contains('3'), "unexpected: {msg}");
}

#[test]
fn get_task_returns_not_found_for_unknown_id() {
    let (env, anchor, _token, _drip) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    // Build a dummy 32-byte ID.
    let fake_id = soroban_sdk::BytesN::from_array(&env, &[0xde; 32]);
    let err = client.get_task(&fake_id).unwrap_err();

    let msg = format!("{err:?}");
    assert!(msg.contains("TaskNotFound") || msg.contains('8'), "unexpected: {msg}");
}
