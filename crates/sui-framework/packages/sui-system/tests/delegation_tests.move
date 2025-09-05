// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::delegation_tests;

use std::unit_test::assert_eq;
use sui::table::Table;
use sui_system::staking_pool::{Self, StakedSui, PoolTokenExchangeRate};
use sui_system::test_runner;
use sui_system::validator_builder;
use sui_system::validator_set;

const VALIDATOR_ADDR_1: address = @1;
const VALIDATOR_ADDR_2: address = @2;

const STAKER_ADDR_1: address = @42;
const STAKER_ADDR_2: address = @43;
const STAKER_ADDR_3: address = @44;

// prettier-ignore
const NEW_VALIDATOR_ADDR: address = @0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357;
const MIST_PER_SUI: u64 = 1_000_000_000;

#[test]
// Scenario:
// 1. Stake 60 SUI to VALIDATOR_ADDR_1
// 2. Split the stake into 20 and 40
// 3. Join the 20 and 40 back together
// 4. Check that the stake is 60 again
fun split_join_staked_sui() {
    let validator = validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 60);

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

    runner.owned_tx!<StakedSui>(|stake| {
        assert_eq!(stake.amount(), 60 * MIST_PER_SUI);
        runner.keep(stake);
    });

    runner.finish();
}

#[test, expected_failure(abort_code = staking_pool::EIncompatibleStakedSui)]
fun join_different_epochs() {
    let validator = validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // stake 1
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 60);
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 60);

    // aborts trying to join stakes with different epoch activations
    runner.scenario_fn!(|scenario| {
        let staked_sui_ids = scenario.ids_for_sender<StakedSui>();
        let mut part1 = scenario.take_from_sender_by_id<StakedSui>(staked_sui_ids[0]);
        let part2 = scenario.take_from_sender_by_id<StakedSui>(staked_sui_ids[1]);

        part1.join(part2);
    });

    abort
}

#[test, expected_failure(abort_code = staking_pool::EStakedSuiBelowThreshold)]
fun split_below_threshold() {
    let validator = validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Stake 2 SUI to the validator.
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 2);

    runner.owned_tx!<StakedSui>(|mut stake| {
        stake.split_to_sender(1 * MIST_PER_SUI + 1, runner.ctx());
    });

    abort
}

#[test, expected_failure(abort_code = staking_pool::EStakedSuiBelowThreshold)]
fun split_nonentry_below_threshold() {
    let validator = validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Stake 2 SUI to the validator.
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 2);

    runner.owned_tx!<StakedSui>(|mut stake| {
        stake.split_to_sender(1 * MIST_PER_SUI + 1, runner.ctx());
    });

    abort
}

#[test]
// Scenario:
// 1. Stake 60 SUI to VALIDATOR_ADDR_1
// 2. Check that the stake is not yet added to the validator
// 3. Advance epoch
// 4. Check that the stake is added to the validator
// 5. Withdraw the stake and advance epoch
// 6. Check that the stake is not added to the validator again
fun add_remove_stake_flow() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .build();

    // Stake 60 SUI to the validator.
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 60);

    // Check that the stake is NOT yet added to the validator.
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 100 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 100 * MIST_PER_SUI);
    });

    // Advance epoch. Stake is now added to the validator.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Withdraw the stake.
    runner.set_sender(STAKER_ADDR_1);
    runner.owned_tx!<StakedSui>(|stake| {
        runner.system_tx!(|system, ctx| {
            assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 160 * MIST_PER_SUI);
            assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 100 * MIST_PER_SUI);

            system.request_withdraw_stake(stake, ctx);

            assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 160 * MIST_PER_SUI);
        });
    });

    // Advance epoch. Stake is now removed from the validator.
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 100 * MIST_PER_SUI);
    });

    runner.finish();
}

#[test]
fun remove_stake_post_active_flow_no_rewards() {
    remove_stake_post_active_flow(false)
}

#[test]
fun remove_stake_post_active_flow_with_rewards() {
    remove_stake_post_active_flow(true)
}

// Scenario:
// 1. Stake 100 SUI to VALIDATOR_ADDR_1
// 2. Advance epoch
// 3. Check that the stake is added to the validator
// 4. Remove the validator and advance epoch
// 5. Check that the stake is withdrawn immediately
// 6. Check that the validator unstakes and gets the rewards
fun remove_stake_post_active_flow(should_distribute_rewards: bool) {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .sui_supply_amount(300)
        .storage_fund_amount(100)
        .build();

    // Stake 100 SUI to the validator.
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 100);

    // Advance epoch.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Check that the stake is added to the validator.
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 200 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 100 * MIST_PER_SUI);
    });

    // Depending on the flag we configure epoch advance options
    let options = if (should_distribute_rewards) {
        option::some(runner.advance_epoch_opts().computation_charge(80))
    } else {
        option::none()
    };

    // Remove the validator and advance epoch.
    runner.set_sender(VALIDATOR_ADDR_1).remove_validator();

    // Advance epoch.
    runner.advance_epoch(options).destroy_for_testing();

    let reward_amt = if (should_distribute_rewards) 15 * MIST_PER_SUI else 0;
    let validator_reward_amt = if (should_distribute_rewards) 10 * MIST_PER_SUI else 0;

    runner.set_sender(STAKER_ADDR_1);
    runner.owned_tx!<StakedSui>(|stake| {
        assert_eq!(stake.amount(), 100 * MIST_PER_SUI);
        runner.system_tx!(|system, ctx| {
            assert!(!system.validators().is_active_validator_by_sui_address(VALIDATOR_ADDR_1));
            system.request_withdraw_stake(stake, ctx);
        });
    });

    // check that the stake is withdrawn immediately
    runner.set_sender(STAKER_ADDR_1);
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI + reward_amt);

    // check that the validator unstakes and gets the rewards
    runner.set_sender(VALIDATOR_ADDR_1).unstake(0);

    if (should_distribute_rewards) runner.unstake(0);
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI + reward_amt + validator_reward_amt);

    runner.finish();
}

#[test]
fun earns_rewards_at_last_epoch() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100),
        ])
        .sui_supply_amount(300)
        .storage_fund_amount(100)
        .build();

    // Stake 100 SUI to the validator.
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 100);

    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.set_sender(VALIDATOR_ADDR_1).remove_validator();

    // Add some rewards after the validator requests to leave. Since the validator is still active
    // this epoch, they should get the rewards from this epoch.
    let options = runner.advance_epoch_opts().computation_charge(80);
    runner.advance_epoch(option::some(options)).destroy_for_testing();

    // Each validator pool gets 30 MIST and validators shares the 20 MIST from the storage fund
    // so validator gets another 10 MIST.
    let reward_amt = 15 * MIST_PER_SUI;
    let validator_reward_amt = 10 * MIST_PER_SUI;

    runner.set_sender(STAKER_ADDR_1);
    runner.owned_tx!<StakedSui>(|stake| {
        assert_eq!(stake.amount(), 100 * MIST_PER_SUI);
        runner.system_tx!(|system, ctx| {
            // Make sure stake withdrawal happens
            system.request_withdraw_stake(stake, ctx);
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
fun add_stake_post_active_flow() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100),
        ])
        .build();

    // Stake 100 SUI to the validator.
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 100);

    // Advance epoch.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Remove the validator.
    runner.set_sender(VALIDATOR_ADDR_1).remove_validator();

    // Advance epoch again.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Make sure the validator is no longer active.
    runner.system_tx!(|system, _| {
        assert!(!system.validators().is_active_validator_by_sui_address(VALIDATOR_ADDR_1));
    });

    // Now try and stake to the old validator/staking pool. This should fail!
    runner.stake_with(VALIDATOR_ADDR_1, 60);

    abort
}

#[test]
// Scenario:
// 1. Add a validator candidate
// 2. Stake 100 SUI to the validator candidate
// 3. Advance epoch twice with some rewards
// 4. Unstake from the preactive validator. There should be no rewards earned.
fun add_preactive_remove_preactive() {
    let mut runner = test_runner::new().validators_initial_stake(100).validators_count(2).build();
    let validator = validator_builder::preset().sui_address(NEW_VALIDATOR_ADDR).build(runner.ctx());

    runner.add_validator_candidate(validator);
    runner.set_sender(STAKER_ADDR_1).stake_with(NEW_VALIDATOR_ADDR, 100);

    // Advance epoch with some rewards
    let opts = runner.advance_epoch_opts().computation_charge(400);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // And again
    let opts = runner.advance_epoch_opts().computation_charge(900);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Unstake from the preactive validator. There should be no rewards earned.
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI);

    runner.finish();
}

#[test, expected_failure(abort_code = validator_set::ENotAValidator)]
// Scenario:
// 1. Add a validator candidate
// 2. Stake 100 SUI to the validator candidate
// 3. Request to add the validator candidate to the active validator set.
// 4. Try staking to the validator candidate. This should fail because the validator candidate is pending.
fun add_preactive_remove_pending_failure() {
    let mut runner = test_runner::new().validators_initial_stake(100).validators_count(2).build();
    let validator = validator_builder::preset().sui_address(NEW_VALIDATOR_ADDR).build(runner.ctx());

    // Add the validator candidate.
    runner.add_validator_candidate(validator);

    // Stake 100 SUI to the validator candidate.
    runner.set_sender(STAKER_ADDR_1).stake_with(NEW_VALIDATOR_ADDR, 100);

    // Try adding self to the active validator set.
    runner.set_sender(NEW_VALIDATOR_ADDR).add_validator();

    // No advance epoch has happened yet.
    runner.set_sender(STAKER_ADDR_1).stake_with(NEW_VALIDATOR_ADDR, 100);

    abort
}

#[test]
// Scenario:
// 1. Add a validator candidate
// 2. Perform different stake and unstake transactions in different epochs
// 3. Make sure that the rewards are distributed correctly and proportionally
fun add_preactive_remove_active() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_2),
        ])
        .sui_supply_amount(300)
        .storage_fund_amount(100)
        .build();

    let validator = validator_builder::preset().sui_address(NEW_VALIDATOR_ADDR).build(runner.ctx());

    // Add the validator candidate.
    runner.add_validator_candidate(validator);

    // Stake 100 SUI to the validator candidate.
    runner.set_sender(STAKER_ADDR_1).stake_with(NEW_VALIDATOR_ADDR, 100);

    // Advance epoch twice with some rewards
    let opts = runner.advance_epoch_opts().computation_charge(300);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // At this point we got the following distribution of stake:
    // V1: 250, V2: 250, storage fund: 100
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 250 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 250 * MIST_PER_SUI);
        assert_eq!(
            system.inner_mut_for_testing().get_storage_fund_total_balance(),
            100 * MIST_PER_SUI,
        );
    });

    // Stake 50 SUI to the validator candidate from staker 2.
    runner.set_sender(STAKER_ADDR_2).stake_with(NEW_VALIDATOR_ADDR, 50);

    // Stake 100 SUI to the validator candidate from staker 3.
    runner.set_sender(STAKER_ADDR_3).stake_with(NEW_VALIDATOR_ADDR, 100);

    // Activate the validator candidate.
    runner.set_sender(NEW_VALIDATOR_ADDR).add_validator();

    // Advance epoch with no rewards.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // At this point we got the following distribution of stake:
    // V1: 250, V2: 250, V3: 250, storage fund: 100
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 250 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 250 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(NEW_VALIDATOR_ADDR), 250 * MIST_PER_SUI);
        assert_eq!(
            system.inner_mut_for_testing().get_storage_fund_total_balance(),
            100 * MIST_PER_SUI,
        );
    });

    // advance epoch with some rewards
    let opts = runner.advance_epoch_opts().computation_charge(85);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // staker 1 and 3 unstake from the validator and earns about 2/5 * (85 - 10) * 1/3 = 10 SUI each.
    // Although they stake in different epochs, they earn the same rewards as long as they unstake
    // in the same epoch because the validator was preactive when they staked.
    // So they will both get slightly more than 110 SUI in total balance.
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    assert_eq!(runner.sui_balance(), 110002000000);

    runner.set_sender(STAKER_ADDR_3).unstake(0);
    assert_eq!(runner.sui_balance(), 110002000000);

    // Advance epoch once more with some rewards
    let opts = runner.advance_epoch_opts().computation_charge(85);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // staker 2 earns about 5 SUI from the previous epoch and 24-ish from this one
    // so in total she has about 50 + 5 + 24 = 79 SUI.
    runner.set_sender(STAKER_ADDR_2).unstake(0);
    assert_eq!(runner.sui_balance(), 78862939078);

    runner.finish();
}

#[test]
fun add_preactive_remove_post_active() {
    let mut runner = test_runner::new().validators_initial_stake(100).validators_count(2).build();
    let validator = validator_builder::preset().sui_address(NEW_VALIDATOR_ADDR).build(runner.ctx());

    // Add the validator candidate.
    runner.add_validator_candidate(validator);

    // Stake 100 SUI to the validator candidate.
    runner.set_sender(STAKER_ADDR_1).stake_with(NEW_VALIDATOR_ADDR, 100);

    // Now the preactive becomes active
    runner.set_sender(NEW_VALIDATOR_ADDR).add_validator();

    // Advance epoch with no rewards.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // staker 1 earns a bit greater than 30 SUI here. A bit greater because the new validator's voting power
    // is slightly greater than 1/3 of the total voting power.
    let opts = runner.advance_epoch_opts().computation_charge(90);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // And now the validator leaves the validator set.
    runner.set_sender(NEW_VALIDATOR_ADDR).remove_validator();

    // Advance epoch with no rewards.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Unstake from the validator.
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    assert_eq!(runner.sui_balance(), 130006000000);

    runner.finish();
}

#[test]
fun add_remove_stake_preactive_candidate() {
    let mut runner = test_runner::new().validators_initial_stake(100).validators_count(2).build();
    let validator = validator_builder::preset().sui_address(NEW_VALIDATOR_ADDR).build(runner.ctx());
    runner.add_validator_candidate(validator);

    // Stake 100 SUI to the validator candidate from each of the two stakers.
    runner.set_sender(STAKER_ADDR_1).stake_with(NEW_VALIDATOR_ADDR, 100);
    runner.set_sender(STAKER_ADDR_2).stake_with(NEW_VALIDATOR_ADDR, 100);

    // Check values for the candidate.
    runner.system_tx!(|system, _| {
        let validator = system.validators().get_candidate_validator_ref(NEW_VALIDATOR_ADDR);

        assert_eq!(validator.total_stake(), 200 * MIST_PER_SUI);
        assert_eq!(validator.pending_stake_amount(), 0);
        assert_eq!(validator.pending_stake_withdraw_amount(), 0);
    });

    // Withdraw the stake. And check that the stake is withdrawn and appears in the sender balance.
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI);

    // Advance epoch, so that the stake 2 becomes active.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Unstake and check that the stake is withdrawn immediately and appears in the sender balance.
    runner.set_sender(STAKER_ADDR_2).unstake(0);
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI);

    // Check that the stake is removed completely, and that no pending stake is present.
    runner.system_tx!(|system, _| {
        let validator = system.validators().get_candidate_validator_ref(NEW_VALIDATOR_ADDR);

        assert_eq!(validator.total_stake(), 0);
        assert_eq!(validator.pending_stake_amount(), 0);
        assert_eq!(validator.pending_stake_withdraw_amount(), 0);
    });

    runner.finish();
}

#[test]
// Scenario:
// 1. Add a validator candidate
// 2. Delegate 100 SUI to the validator candidate
// 3. Advance epoch and give out some rewards. The candidate should get nothing.
// 4. Remove the candidate
// 5. Staker unstakes and gets no rewards.
fun add_preactive_candidate_drop_out() {
    let mut runner = test_runner::new().validators_initial_stake(100).validators_count(2).build();
    let validator = validator_builder::preset().build(runner.ctx());
    let validator_address = validator.sui_address();
    runner.add_validator_candidate(validator);

    // Delegate 100 MIST to the preactive validator
    runner.set_sender(STAKER_ADDR_1).stake_with(validator_address, 100);

    // Advance epoch and give out some rewards. The candidate should get nothing, of course.
    let opts = runner.advance_epoch_opts().computation_charge(800);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Now the candidate leaves.
    runner.set_sender(validator_address).remove_validator_candidate();

    // Advance epoch a few times.
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Unstake now and the staker should get no rewards.
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    assert_eq!(runner.sui_balance(), 100 * MIST_PER_SUI);

    runner.finish();
}

#[test]
// Scenario:
// 1. Add a validator candidate
// 2. Stake 100 SUI to the validator candidate
// 3. Request removal of the validator candidate without epoch advancement
// 4. Unstake from the validator candidate
fun remove_inactive_stake_from_inactive_candidate() {
    let mut runner = test_runner::new().validators_initial_stake(100).validators_count(2).build();
    let validator = validator_builder::preset().build(runner.ctx());
    let validator_address = validator.sui_address();

    runner.add_validator_candidate(validator);

    // Stake 100 SUI to the validator candidate
    runner.set_sender(validator_address).stake_with(validator_address, 100);

    // Request removal of the validator candidate without epoch advancement
    // Candidate is immediately marked as inactive.
    runner.set_sender(validator_address).remove_validator_candidate();

    // Unstake from the validator candidate
    runner.set_sender(validator_address).unstake(0);

    // Check that the stake is withdrawn fully.
    assert_eq!(runner.set_sender(validator_address).sui_balance(), 100 * MIST_PER_SUI);

    runner.finish();
}

#[test]
/// Scenario:
/// 1. Stake 100 SUI to the validator.
/// 2. Advance epoch.
/// 3. Advance epoch with rewards.
/// 4. Check the exchange rates in the system state.
fun staking_pool_exchange_rate_getter() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(100),
        ])
        .build();

    runner.stake_with(VALIDATOR_ADDR_1, 100);

    let pool_id;

    runner.owned_tx!<StakedSui>(|stake| {
        pool_id = stake.pool_id();
        runner.keep(stake);
    });

    // Advance epoch without rewards.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Advance epoch with rewards.
    // Each staking pool (2 pools) gets 10 SUI of rewards.
    let opts = runner.advance_epoch_opts().computation_charge(20);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Check the exchange rates in the system state.
    runner.system_tx!(|system, _| {
        let rates = system.pool_exchange_rates(&pool_id);
        assert_eq!(rates.length(), 3);
        rates.assert_exchange_rate_eq(0, 0, 0); // no tokens at epoch 0
        rates.assert_exchange_rate_eq(1, 200, 200); // 200 SUI of self + delegate stake at epoch 1
        rates.assert_exchange_rate_eq(2, 210, 200); // 10 SUI of rewards at epoch 2
    });

    runner.finish();
}

// trick or treat
use fun assert_exchange_rate_eq as Table.assert_exchange_rate_eq;

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
