// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::delegation_tests;

use std::unit_test::assert_eq;
use sui::table::Table;
use sui::test_scenario;
use sui_system::governance_test_utils::{
    add_validator,
    add_validator_candidate,
    advance_epoch,
    advance_epoch_with_reward_amounts,
    create_validator_for_testing,
    create_sui_system_state_for_testing,
    stake_with,
    remove_validator,
    remove_validator_candidate,
    total_sui_balance,
    unstake
};
use sui_system::staking_pool::{Self, StakedSui, PoolTokenExchangeRate};
use sui_system::sui_system::SuiSystemState;
use sui_system::test_runner;
use sui_system::validator_builder;
use sui_system::validator_set;

const VALIDATOR_ADDR_1: address = @1;
const VALIDATOR_ADDR_2: address = @2;

const STAKER_ADDR_1: address = @42;
const STAKER_ADDR_2: address = @43;
const STAKER_ADDR_3: address = @44;

const NEW_VALIDATOR_ADDR: address =
    @0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357;
// Generated with seed [0;32]
const NEW_VALIDATOR_PUBKEY: vector<u8> =
    x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
// Generated using [fn test_proof_of_possession]
const NEW_VALIDATOR_POP: vector<u8> =
    x"8b93fc1b33379e2796d361c4056f0f04ad5aea7f4a8c02eaac57340ff09b6dc158eb1945eece103319167f420daf0cb3";

const MIST_PER_SUI: u64 = 1_000_000_000;

#[test]
fun test_split_join_staked_sui() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1);
    runner.stake_with(VALIDATOR_ADDR_1, 60);

    runner.owned_tx!<StakedSui>(|mut stake| {
        stake.split_to_sender(20 * MIST_PER_SUI, runner.ctx());
        runner.keep(stake);
    });

    runner.scenario_fn!(|scenario| {
        let ids = scenario.ids_for_sender<StakedSui>();
        assert_eq!(ids.length(), 2);

        let mut stake_1 = scenario.take_from_sender_by_id<StakedSui>(ids[0]);
        let stake_2 = scenario.take_from_sender_by_id<StakedSui>(ids[1]);

        assert_eq!(stake_1.amount(), 20 * MIST_PER_SUI);
        assert_eq!(stake_2.amount(), 40 * MIST_PER_SUI);

        stake_1.join(stake_2);
        runner.keep(stake_1);
    });

    runner.finish();
}

#[test, expected_failure(abort_code = staking_pool::EIncompatibleStakedSui)]
fun test_join_different_epochs() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1); // stake 1
    runner.stake_with(VALIDATOR_ADDR_1, 60);
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.stake_with(VALIDATOR_ADDR_1, 60); // stake 2

    // aborts trying to join stakes with different epoch activations
    runner.scenario_fn!(|scenario| {
        let staked_sui_ids = scenario.ids_for_sender<StakedSui>();
        let mut part1 = scenario.take_from_sender_by_id<StakedSui>(staked_sui_ids[0]);
        let part2 = scenario.take_from_sender_by_id<StakedSui>(staked_sui_ids[1]);

        part1.join(part2);
    });

    abort // unreacheable
}

#[test, expected_failure(abort_code = staking_pool::EStakedSuiBelowThreshold)]
fun test_split_below_threshold() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1);
    runner.stake_with(VALIDATOR_ADDR_1, 2);

    runner.owned_tx!<StakedSui>(|mut stake| {
        stake.split_to_sender(1 * MIST_PER_SUI + 1, runner.ctx());
    });

    abort // unreacheable
}

#[test, expected_failure(abort_code = staking_pool::EStakedSuiBelowThreshold)]
fun test_split_nonentry_below_threshold() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1);
    runner.stake_with(VALIDATOR_ADDR_1, 2); // Stake 2 SUI

    runner.owned_tx!<StakedSui>(|mut stake| {
        stake.split_to_sender(1 * MIST_PER_SUI + 1, runner.ctx());
    });

    abort // unreacheable
}

#[test]
// Scenario:
// 1. Stake 60 SUI to VALIDATOR_ADDR_1
// 2. Check that the stake is not yet added to the validator
// 3. Advance epoch
// 4. Check that the stake is added to the validator
// 5. Withdraw the stake and advance epoch
// 6. Check that the stake is not added to the validator again
fun test_add_remove_stake_flow() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1);

    runner.stake_with(VALIDATOR_ADDR_1, 60);
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 100 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 100 * MIST_PER_SUI);
    });

    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.set_sender(STAKER_ADDR_1);
    runner.owned_tx!<StakedSui>(|stake| {
        runner.system_tx!(|system, ctx| {
            assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 160 * MIST_PER_SUI);
            assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 100 * MIST_PER_SUI);

            system.request_withdraw_stake(stake, ctx);

            assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 160 * MIST_PER_SUI);
        });
    });

    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 100 * MIST_PER_SUI);
    });

    runner.finish();
}

#[test]
fun test_remove_stake_post_active_flow_no_rewards() {
    test_remove_stake_post_active_flow(false)
}

#[test]
fun test_remove_stake_post_active_flow_with_rewards() {
    test_remove_stake_post_active_flow(true)
}

// Scenario:
// 1. Stake 100 SUI to VALIDATOR_ADDR_1
// 2. Advance epoch
// 3. Check that the stake is added to the validator
// 4. Remove the validator and advance epoch
// 5. Check that the stake is withdrawn immediately
// 6. Check that the validator unstakes and gets the rewards
fun test_remove_stake_post_active_flow(should_distribute_rewards: bool) {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .sui_supply_amount(300)
        .storage_fund_amount(100)
        .build();

    runner.set_sender(STAKER_ADDR_1);
    runner.stake_with(VALIDATOR_ADDR_1, 100);
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 200 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 100 * MIST_PER_SUI);
    });

    // depending on the flag we configure epoch advance options
    let options = if (should_distribute_rewards) {
        option::some(runner.advance_epoch_opts().storage_charge(0).computation_charge(80))
    } else {
        option::none()
    };

    runner.remove_validator(VALIDATOR_ADDR_1);
    runner.advance_epoch(options).destroy_for_testing();

    let reward_amt = if (should_distribute_rewards) 15 * MIST_PER_SUI else 0;
    let validator_reward_amt = if (should_distribute_rewards) 10 * MIST_PER_SUI else 0;

    runner.set_sender(STAKER_ADDR_1);
    runner.owned_tx!<StakedSui>(|stake| {
        assert_eq!(stake.amount(), 100 * MIST_PER_SUI);
        runner.system_tx!(|system, _| {
            assert!(!system.validators().is_active_validator_by_sui_address(VALIDATOR_ADDR_1));
            system.request_withdraw_stake(stake, runner.ctx());
        });
    });

    // check that the stake is withdrawn immediately
    runner.set_sender(STAKER_ADDR_1);
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI + reward_amt);

    // check that the validator unstakes and gets the rewards
    runner.set_sender(VALIDATOR_ADDR_1);
    runner.unstake(0);
    if (should_distribute_rewards) runner.unstake(0);
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI + reward_amt + validator_reward_amt);

    runner.finish();
}

#[test]
fun test_earns_rewards_at_last_epoch() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .sui_supply_amount(300)
        .storage_fund_amount(100)
        .build();

    runner.set_sender(STAKER_ADDR_1);
    runner.stake_with(VALIDATOR_ADDR_1, 100);

    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.remove_validator(VALIDATOR_ADDR_1);

    // Add some rewards after the validator requests to leave. Since the validator is still active
    // this epoch, they should get the rewards from this epoch.
    let options = runner.advance_epoch_opts().storage_charge(0).computation_charge(80);
    runner.advance_epoch(option::some(options)).destroy_for_testing();

    // Each validator pool gets 30 MIST and validators shares the 20 MIST from the storage fund
    // so validator gets another 10 MIST.
    let reward_amt = 15 * MIST_PER_SUI;
    let validator_reward_amt = 10 * MIST_PER_SUI;

    runner.owned_tx!<StakedSui>(|stake| {
        assert_eq!(stake.amount(), 100 * MIST_PER_SUI);
        runner.system_tx!(|system, _| {
            // Make sure stake withdrawal happens
            system.request_withdraw_stake(stake, runner.ctx());
        });
    });

    // Make sure they have all of their stake.
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI + reward_amt);

    // Validator unstakes now.
    runner.set_sender(VALIDATOR_ADDR_1);
    runner.unstake(0);
    runner.unstake(0);

    // Make sure have all of their stake. NB there is no epoch change. This is immediate.
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI + reward_amt + validator_reward_amt);

    runner.finish();
}

#[test, expected_failure(abort_code = validator_set::ENotAValidator)]
fun test_add_stake_post_active_flow() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1);
    runner.stake_with(VALIDATOR_ADDR_1, 100);
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.remove_validator(VALIDATOR_ADDR_1);
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Make sure the validator is no longer active.
    runner.system_tx!(|system, _| {
        assert!(!system.validators().is_active_validator_by_sui_address(VALIDATOR_ADDR_1));
    });

    // Now try and stake to the old validator/staking pool. This should fail!
    runner.stake_with(VALIDATOR_ADDR_1, 60);
    runner.finish();
}

#[test]
fun test_add_preactive_remove_preactive() {
    set_up_sui_system_state();
    let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
    let scenario = &mut scenario_val;

    add_validator_candidate(
        NEW_VALIDATOR_ADDR,
        b"name5",
        b"/ip4/127.0.0.1/udp/85",
        NEW_VALIDATOR_PUBKEY,
        NEW_VALIDATOR_POP,
        scenario,
    );

    // Delegate 100 MIST to the preactive validator
    stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);

    // Advance epoch twice with some rewards
    advance_epoch_with_reward_amounts(0, 400, scenario);
    advance_epoch_with_reward_amounts(0, 900, scenario);

    // Unstake from the preactive validator. There should be no rewards earned.
    unstake(STAKER_ADDR_1, 0, scenario);
    assert_eq!(total_sui_balance(STAKER_ADDR_1, scenario), 100 * MIST_PER_SUI);

    scenario_val.end();
}

#[test]
#[expected_failure(abort_code = validator_set::ENotAValidator)]
fun test_add_preactive_remove_pending_failure() {
    set_up_sui_system_state();
    let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
    let scenario = &mut scenario_val;

    add_validator_candidate(
        NEW_VALIDATOR_ADDR,
        b"name4",
        b"/ip4/127.0.0.1/udp/84",
        NEW_VALIDATOR_PUBKEY,
        NEW_VALIDATOR_POP,
        scenario,
    );
    stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);
    add_validator(NEW_VALIDATOR_ADDR, scenario);

    // Delegate 100 SUI to the pending validator. This should fail because pending active validators don't accept
    // new stakes or withdraws.
    stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);

    scenario_val.end();
}

#[test]
fun test_add_preactive_remove_active() {
    set_up_sui_system_state_with_storage_fund();
    let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
    let scenario = &mut scenario_val;

    add_validator_candidate(
        NEW_VALIDATOR_ADDR,
        b"name3",
        b"/ip4/127.0.0.1/udp/83",
        NEW_VALIDATOR_PUBKEY,
        NEW_VALIDATOR_POP,
        scenario,
    );

    // Delegate 100 SUI to the preactive validator
    stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);
    advance_epoch_with_reward_amounts(0, 300, scenario);
    // At this point we got the following distribution of stake:
    // V1: 250, V2: 250, storage fund: 100

    stake_with(STAKER_ADDR_2, NEW_VALIDATOR_ADDR, 50, scenario);
    stake_with(STAKER_ADDR_3, NEW_VALIDATOR_ADDR, 100, scenario);

    // Now the preactive becomes active
    add_validator(NEW_VALIDATOR_ADDR, scenario);
    advance_epoch(scenario);

    // At this point we got the following distribution of stake:
    // V1: 250, V2: 250, V3: 250, storage fund: 100

    advance_epoch_with_reward_amounts(0, 85, scenario);

    // staker 1 and 3 unstake from the validator and earns about 2/5 * (85 - 10) * 1/3 = 10 SUI each.
    // Although they stake in different epochs, they earn the same rewards as long as they unstake
    // in the same epoch because the validator was preactive when they staked.
    // So they will both get slightly more than 110 SUI in total balance.
    unstake(STAKER_ADDR_1, 0, scenario);
    assert_eq!(total_sui_balance(STAKER_ADDR_1, scenario), 110002000000);
    unstake(STAKER_ADDR_3, 0, scenario);
    assert_eq!(total_sui_balance(STAKER_ADDR_3, scenario), 110002000000);

    advance_epoch_with_reward_amounts(0, 85, scenario);
    unstake(STAKER_ADDR_2, 0, scenario);
    // staker 2 earns about 5 SUI from the previous epoch and 24-ish from this one
    // so in total she has about 50 + 5 + 24 = 79 SUI.
    assert_eq!(total_sui_balance(STAKER_ADDR_2, scenario), 78862939078);

    scenario_val.end();
}

#[test]
fun test_add_preactive_remove_post_active() {
    set_up_sui_system_state();
    let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
    let scenario = &mut scenario_val;

    add_validator_candidate(
        NEW_VALIDATOR_ADDR,
        b"name1",
        b"/ip4/127.0.0.1/udp/81",
        NEW_VALIDATOR_PUBKEY,
        NEW_VALIDATOR_POP,
        scenario,
    );

    // Delegate 100 SUI to the preactive validator
    stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);

    // Now the preactive becomes active
    add_validator(NEW_VALIDATOR_ADDR, scenario);
    advance_epoch(scenario);

    // staker 1 earns a bit greater than 30 SUI here. A bit greater because the new validator's voting power
    // is slightly greater than 1/3 of the total voting power.
    advance_epoch_with_reward_amounts(0, 90, scenario);

    // And now the validator leaves the validator set.
    remove_validator(NEW_VALIDATOR_ADDR, scenario);

    advance_epoch(scenario);

    unstake(STAKER_ADDR_1, 0, scenario);
    assert_eq!(total_sui_balance(STAKER_ADDR_1, scenario), 130006000000);

    scenario_val.end();
}

#[test]
fun test_add_preactive_candidate_drop_out() {
    set_up_sui_system_state();
    let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
    let scenario = &mut scenario_val;

    add_validator_candidate(
        NEW_VALIDATOR_ADDR,
        b"name2",
        b"/ip4/127.0.0.1/udp/82",
        NEW_VALIDATOR_PUBKEY,
        NEW_VALIDATOR_POP,
        scenario,
    );

    // Delegate 100 MIST to the preactive validator
    stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);

    // Advance epoch and give out some rewards. The candidate should get nothing, of course.
    advance_epoch_with_reward_amounts(0, 800, scenario);

    // Now the candidate leaves.
    remove_validator_candidate(NEW_VALIDATOR_ADDR, scenario);

    // Advance epoch a few times.
    advance_epoch(scenario);
    advance_epoch(scenario);
    advance_epoch(scenario);

    // Unstake now and the staker should get no rewards.
    unstake(STAKER_ADDR_1, 0, scenario);
    assert_eq!(total_sui_balance(STAKER_ADDR_1, scenario), 100 * MIST_PER_SUI);

    scenario_val.end();
}

#[test]
fun test_staking_pool_exchange_rate_getter() {
    set_up_sui_system_state();
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;
    stake_with(@0x42, @0x2, 100, scenario); // stakes 100 SUI with 0x2
    scenario.next_tx(@0x42);
    let staked_sui = scenario.take_from_address<StakedSui>(@0x42);
    let pool_id = staked_sui.pool_id();
    test_scenario::return_to_address(@0x42, staked_sui);
    advance_epoch(scenario); // advances epoch to effectuate the stake
    // Each staking pool gets 10 SUI of rewards.
    advance_epoch_with_reward_amounts(0, 20, scenario);
    let mut system_state = scenario.take_shared<SuiSystemState>();
    let rates = system_state.pool_exchange_rates(&pool_id);
    assert_eq!(rates.length(), 3);
    assert_exchange_rate_eq(rates, 0, 0, 0); // no tokens at epoch 0
    assert_exchange_rate_eq(rates, 1, 200, 200); // 200 SUI of self + delegate stake at epoch 1
    assert_exchange_rate_eq(rates, 2, 210, 200); // 10 SUI of rewards at epoch 2
    test_scenario::return_shared(system_state);
    scenario_val.end();
}

fun assert_exchange_rate_eq(
    rates: &Table<u64, PoolTokenExchangeRate>,
    epoch: u64,
    sui_amount: u64,
    pool_token_amount: u64,
) {
    let rate = &rates[epoch];
    assert_eq!(rate.sui_amount(), sui_amount * MIST_PER_SUI);
    assert_eq!(rate.pool_token_amount(), pool_token_amount * MIST_PER_SUI);
}

fun set_up_sui_system_state() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;
    let ctx = scenario.ctx();

    let validators = vector[
        create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx),
        create_validator_for_testing(VALIDATOR_ADDR_2, 100, ctx),
    ];
    create_sui_system_state_for_testing(validators, 0, 0, ctx);
    scenario_val.end();
}

fun set_up_sui_system_state_with_storage_fund() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;
    let ctx = scenario.ctx();

    let validators = vector[
        create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx),
        create_validator_for_testing(VALIDATOR_ADDR_2, 100, ctx),
    ];
    create_sui_system_state_for_testing(validators, 300, 100, ctx);
    scenario_val.end();
}
