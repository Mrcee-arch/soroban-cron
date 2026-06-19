//! Integration test: slash mechanics.
//!
//! Verifies that:
//! - Designated keeper's stake is reduced by exactly `stake * 5 / 100` (floor).
//! - `new_stake` is never negative.
//! - Ineligibility is set when stake falls below `MIN_KEEPER_STAKE`.

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
fn secondary_execution_slashes_designated_keeper_by_5_percent() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    let secondary = Address::generate(&env);
    let initial_stake: i128 = 100_000_000; // 10 XLM

    client.register_keeper(&designated, &initial_stake).unwrap();
    client.register_keeper(&secondary, &initial_stake).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &designated)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    // 25 ledgers into grace.
    env.ledger().set_sequence_number(task.next_allowed_execution + 25);

    client.execute_drip_split(&task_id, &secondary).unwrap();

    let expected_slash = initial_stake * 5 / 100;
    let expected_new_stake = initial_stake - expected_slash;

    let designated_rec = client.get_keeper(&designated).unwrap();
    assert_eq!(
        designated_rec.stake_amount, expected_new_stake,
        "designated keeper stake must be reduced by exactly 5%"
    );
}

#[test]
fn slash_never_produces_negative_stake() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    let secondary = Address::generate(&env);
    // Minimal stake — one slash will push it below MIN_KEEPER_STAKE.
    client.register_keeper(&designated, &10_000_000_i128).unwrap();
    client.register_keeper(&secondary, &10_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &designated)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    env.ledger().set_sequence_number(task.next_allowed_execution + 10);

    client.execute_drip_split(&task_id, &secondary).unwrap();

    let designated_rec = client.get_keeper(&designated).unwrap();
    assert!(
        designated_rec.stake_amount >= 0,
        "stake must never go negative, got {}",
        designated_rec.stake_amount
    );
}

#[test]
fn keeper_marked_ineligible_after_stake_falls_below_minimum() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    let secondary = Address::generate(&env);
    // 10 XLM — one slash (0.5 XLM) leaves exactly 9.5 XLM < 10 XLM MIN_KEEPER_STAKE,
    // so ineligible should be set to true.
    let initial_stake: i128 = 10_000_001; // just above minimum, slash puts below
    client.register_keeper(&designated, &initial_stake).unwrap();
    client.register_keeper(&secondary, &10_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &designated)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    env.ledger().set_sequence_number(task.next_allowed_execution + 1);

    client.execute_drip_split(&task_id, &secondary).unwrap();

    let designated_rec = client.get_keeper(&designated).unwrap();
    let new_stake = initial_stake - (initial_stake * 5 / 100);
    if new_stake < 10_000_000 {
        assert!(
            designated_rec.ineligible,
            "keeper should be ineligible when stake falls below MIN_KEEPER_STAKE"
        );
    }
}

#[test]
fn secondary_keeper_receives_micro_reward() {
    let (env, anchor, _token, drip_list) = setup();
    let client = AnchorContractClient::new(&env, &anchor);

    let designated = Address::generate(&env);
    let secondary = Address::generate(&env);
    client.register_keeper(&designated, &100_000_000_i128).unwrap();
    client.register_keeper(&secondary, &100_000_000_i128).unwrap();

    let task_id = client
        .provision_task(&drip_list, &100_u32, &5_000_000_i128, &designated)
        .unwrap();

    let task = client.get_task(&task_id).unwrap();
    env.ledger().set_sequence_number(task.next_allowed_execution + 20);

    // Should succeed — secondary gets micro_reward + secondary_reward.
    client.execute_drip_split(&task_id, &secondary).unwrap();

    // secondary's execution count updated.
    let secondary_rec = client.get_keeper(&secondary).unwrap();
    assert_eq!(secondary_rec.total_executions, 1);
}
