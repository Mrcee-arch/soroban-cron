//! Integration test: post-grace open execution (no slash).
//!
//! After the 50-ledger grace period expires:
//! - Any registered keeper (including the designated one) may execute.
//! - No slash is applied.
//! - The task schedule advances from the post-grace execution ledger.

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
fn post_grace_execution_succeeds_with_no_slash() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    let executor = Address::generate(&env);
    let initial_stake: i128 = 50_000_000;

    client.register_keeper(&designated, &initial_stake).unwrap();
    client.register_keeper(&executor, &initial_stake).unwrap();

    let interval: u32 = 100;
    let task_id = client
        .provision_task(&drip_list, &interval, &5_000_000_i128, &designated)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    // 80 ledgers past window — 30 past the grace boundary.
    let post_grace_ledger = task.next_allowed_execution + 80;
    env.ledger().set_sequence_number(post_grace_ledger);

    client
        .execute_drip_split(&task_id, &executor)
        .expect("post-grace execution must succeed");

    // Designated keeper's stake must be unchanged — no slash.
    let designated_rec = client.get_keeper(&designated).unwrap();
    assert_eq!(
        designated_rec.stake_amount, initial_stake,
        "designated keeper stake must NOT change in post-grace execution"
    );

    // Task schedule advances from the post-grace ledger.
    let task_after = client.get_task(&task_id).unwrap();
    assert_eq!(task_after.next_allowed_execution, post_grace_ledger + interval);
}

#[test]
fn designated_keeper_can_execute_post_grace() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let keeper = Address::generate(&env);
    client.register_keeper(&keeper, &50_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &keeper)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    // One ledger past the grace boundary (51 > 50).
    env.ledger().set_sequence_number(task.next_allowed_execution + 51);

    client
        .execute_drip_split(&task_id, &keeper)
        .expect("designated keeper must be allowed post-grace");
}

#[test]
fn unregistered_caller_blocked_post_grace() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    client.register_keeper(&designated, &50_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &designated)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    env.ledger().set_sequence_number(task.next_allowed_execution + 60);

    let stranger = Address::generate(&env);
    let err = client.execute_drip_split(&task_id, &stranger).unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("UnauthorizedExecutor") || msg.contains("10"),
        "unregistered post-grace caller should be rejected: {msg}"
    );
}
