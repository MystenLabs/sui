// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module locked_stake::locked_stake_tests;

use locked_stake::{epoch_time_lock, locked_stake as ls};
use sui::{balance, coin, test_scenario, test_utils::{assert_eq, destroy}, vec_map};
use sui_system::{
    governance_test_utils::{advance_epoch, set_up_sui_system_state},
    sui_system::{Self, SuiSystemState}
};

const MIST_PER_SUI: u64 = 1_000_000_000;

#[test]
#[expected_failure(abort_code = epoch_time_lock::EEpochAlreadyPassed)]
fun test_incorrect_creation() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;

    set_up_sui_system_state(vector[@0x1, @0x2, @0x3]);

    // Advance epoch twice so we are now at epoch 2.
    advance_epoch(scenario);
    advance_epoch(scenario);
    let ctx = test_scenario::ctx(scenario);
    assert_eq(tx_context::epoch(ctx), 2);

    // Create a locked stake with epoch 1. Should fail here.
    let ls = ls::new(1, ctx);

    destroy(ls);
    test_scenario::end(scenario_val);
}

#[test]
fun test_deposit_stake_unstake() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;

    set_up_sui_system_state(vector[@0x1, @0x2, @0x3]);

    let mut ls = ls::new(10, test_scenario::ctx(scenario));

    // Deposit 100 SUI.
    ls::deposit_sui(&mut ls, balance::create_for_testing(100 * MIST_PER_SUI));

    assert_eq(ls::sui_balance(&ls), 100 * MIST_PER_SUI);

    test_scenario::next_tx(scenario, @0x1);
    let mut system_state = test_scenario::take_shared<SuiSystemState>(scenario);

    // Stake 10 of the 100 SUI.
    ls::stake(&mut ls, &mut system_state, 10 * MIST_PER_SUI, @0x1, test_scenario::ctx(scenario));
    test_scenario::return_shared(system_state);

    assert_eq(ls::sui_balance(&ls), 90 * MIST_PER_SUI);
    assert_eq(vec_map::size(ls::staked_sui(&ls)), 1);

    test_scenario::next_tx(scenario, @0x1);
    let mut system_state = test_scenario::take_shared<SuiSystemState>(scenario);
    let ctx = test_scenario::ctx(scenario);

    // Create a StakedSui object and add it to the LockedStake object.
    let staked_sui = sui_system::request_add_stake_non_entry(
        &mut system_state,
        coin::mint_for_testing(20 * MIST_PER_SUI, ctx),
        @0x2,
        ctx,
    );
    test_scenario::return_shared(system_state);

    ls::deposit_staked_sui(&mut ls, staked_sui);
    assert_eq(ls::sui_balance(&ls), 90 * MIST_PER_SUI);
    assert_eq(vec_map::size(ls::staked_sui(&ls)), 2);
    advance_epoch(scenario);

    test_scenario::next_tx(scenario, @0x1);
    let (staked_sui_id, _) = vec_map::get_entry_by_idx(ls::staked_sui(&ls), 0);
    let mut system_state = test_scenario::take_shared<SuiSystemState>(scenario);

    // Unstake both stake objects
    ls::unstake(&mut ls, &mut system_state, *staked_sui_id, test_scenario::ctx(scenario));
    test_scenario::return_shared(system_state);
    assert_eq(ls::sui_balance(&ls), 100 * MIST_PER_SUI);
    assert_eq(vec_map::size(ls::staked_sui(&ls)), 1);

    test_scenario::next_tx(scenario, @0x1);
    let (staked_sui_id, _) = vec_map::get_entry_by_idx(ls::staked_sui(&ls), 0);
    let mut system_state = test_scenario::take_shared<SuiSystemState>(scenario);
    ls::unstake(&mut ls, &mut system_state, *staked_sui_id, test_scenario::ctx(scenario));
    test_scenario::return_shared(system_state);
    assert_eq(ls::sui_balance(&ls), 120 * MIST_PER_SUI);
    assert_eq(vec_map::size(ls::staked_sui(&ls)), 0);

    destroy(ls);
    test_scenario::end(scenario_val);
}

#[test]
fun test_unlock_correct_epoch() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;

    set_up_sui_system_state(vector[@0x1, @0x2, @0x3]);

    let mut ls = ls::new(2, test_scenario::ctx(scenario));

    ls::deposit_sui(&mut ls, balance::create_for_testing(100 * MIST_PER_SUI));

    assert_eq(ls::sui_balance(&ls), 100 * MIST_PER_SUI);

    test_scenario::next_tx(scenario, @0x1);
    let mut system_state = test_scenario::take_shared<SuiSystemState>(scenario);
    ls::stake(&mut ls, &mut system_state, 10 * MIST_PER_SUI, @0x1, test_scenario::ctx(scenario));
    test_scenario::return_shared(system_state);

    advance_epoch(scenario);
    advance_epoch(scenario);
    advance_epoch(scenario);
    advance_epoch(scenario);

    let (staked_sui, sui_balance) = ls::unlock(ls, test_scenario::ctx(scenario));
    assert_eq(balance::value(&sui_balance), 90 * MIST_PER_SUI);
    assert_eq(vec_map::size(&staked_sui), 1);

    destroy(staked_sui);
    destroy(sui_balance);
    test_scenario::end(scenario_val);
}

#[test]
#[expected_failure(abort_code = epoch_time_lock::EEpochNotYetEnded)]
fun test_unlock_incorrect_epoch() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;

    set_up_sui_system_state(vector[@0x1, @0x2, @0x3]);

    let ls = ls::new(2, test_scenario::ctx(scenario));
    let (staked_sui, sui_balance) = ls::unlock(ls, test_scenario::ctx(scenario));
    destroy(staked_sui);
    destroy(sui_balance);
    test_scenario::end(scenario_val);
}
