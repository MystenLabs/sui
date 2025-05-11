// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::rewards_distribution_tests;

use std::unit_test::assert_eq;
use sui::address;
use sui::balance;
use sui::test_scenario;
use sui_system::governance_test_utils::{
    create_validator_for_testing,
    create_sui_system_state_for_testing
};
use sui_system::sui_system::SuiSystemState;
use sui_system::test_runner;
use sui_system::validator_builder;

const VALIDATOR_ADDR_1: address = @01;
const VALIDATOR_ADDR_2: address = @02;
const VALIDATOR_ADDR_3: address = @03;
const VALIDATOR_ADDR_4: address = @04;

const STAKER_ADDR_1: address = @42;
const STAKER_ADDR_2: address = @43;
const STAKER_ADDR_3: address = @44;
const STAKER_ADDR_4: address = @45;

const MIST_PER_SUI: u64 = 1_000_000_000;

#[test]
fun validator_rewards() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1000)
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    let opts = runner.advance_epoch_opts().computation_charge(100);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // check rewards distribution, 1:2:3:4
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 125 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 225 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_3), 325 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_4), 425 * MIST_PER_SUI);
    });

    runner.set_sender(VALIDATOR_ADDR_2).stake_with(VALIDATOR_ADDR_2, 720);

    let opts = runner.advance_epoch_opts().computation_charge(100);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // check rewards distribution, given that validator 2 has 920 SUI of stake now
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 150 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 970 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_3), 350 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_4), 450 * MIST_PER_SUI);
    });

    runner.finish();
}

#[test]
fun stake_subsidy() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1_000_000_000)
        .validators(vector[
            validator_builder::new().initial_stake(100_000_000).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200_000_000).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300_000_000).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400_000_000).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    let opts = runner.advance_epoch_opts().computation_charge(100);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 100_000_025 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 200_000_025 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_3), 300_000_025 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_4), 400_000_025 * MIST_PER_SUI);
    });

    runner.finish();
}

#[test]
fun stake_rewards() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1000)
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 200);
    runner.set_sender(STAKER_ADDR_2).stake_with(VALIDATOR_ADDR_2, 100);

    // advance epoch so that the stake is active
    runner.advance_epoch(option::none()).destroy_for_testing();

    // check the total stake amount
    runner.system_tx!(|system, _| {
        // total stake
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 300 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 300 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_3), 300 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_4), 400 * MIST_PER_SUI);
    });

    // check total stake and rewards for each validator
    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[100 * MIST_PER_SUI, 200 * MIST_PER_SUI, 300 * MIST_PER_SUI, 400 * MIST_PER_SUI],
    );

    let opts = runner.advance_epoch_opts().computation_charge(120);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // check total stake and rewards for each validator
    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[110 * MIST_PER_SUI, 220 * MIST_PER_SUI, 330 * MIST_PER_SUI, 430 * MIST_PER_SUI],
    );

    runner.set_sender(STAKER_ADDR_1).unstake(0);
    runner.set_sender(STAKER_ADDR_2).stake_with(VALIDATOR_ADDR_1, 600);

    // Each pool gets 30 SUI.
    let opts = runner.advance_epoch_opts().computation_charge(120);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[140 * MIST_PER_SUI, 240 * MIST_PER_SUI, 360 * MIST_PER_SUI, 460 * MIST_PER_SUI],
    );

    // staker 1 receives only 20 SUI of rewards, not 40 since we are using pre-epoch exchange rate.
    assert_eq!(runner.set_sender(STAKER_ADDR_1).sui_balance(), 220 * MIST_PER_SUI);

    // staker 2 receives 20 SUI of rewards.
    runner.set_sender(STAKER_ADDR_2).unstake(0);
    assert_eq!(runner.set_sender(STAKER_ADDR_2).sui_balance(), 120 * MIST_PER_SUI);

    let opts = runner.advance_epoch_opts().computation_charge(40);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // unstake 600 principal SUI
    runner.set_sender(STAKER_ADDR_2).unstake(0);

    // additional 600 SUI of principal and 46 SUI of rewards withdrawn to Coin<SUI>
    // For this stake, the staking exchange rate is 100 : 140 and the unstaking
    // exchange rate is 528 : 750 -ish so the total sui withdraw will be:
    // (600 * 100 / 140) * 750 / 528 = ~608. Together with the 120 SUI we already have,
    // that would be about 728 SUI.
    // TODO: Come up with better numbers and clean it up!
    assert_eq!(runner.set_sender(STAKER_ADDR_2).sui_balance(), 728108108107);

    runner.finish();
}

#[test]
fun stake_tiny_rewards() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1_000_000_000)
        .validators(vector[
            validator_builder::new().initial_stake(100_000_000).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200_000_000).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300_000_000).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400_000_000).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 200000000);

    let opts = runner.advance_epoch_opts().computation_charge(150000);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // stake a small amount
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 10);
    let opts = runner.advance_epoch_opts().computation_charge(130);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // unstake the stakes
    runner.set_sender(STAKER_ADDR_1).unstake(1);

    // and advance epoch should succeed
    let opts = runner.advance_epoch_opts().computation_charge(150);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.finish();
}

#[test]
fun validator_commission() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1000)
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 100);
    runner.set_sender(STAKER_ADDR_2).stake_with(VALIDATOR_ADDR_2, 100);

    // validator 2 now has 20% commission
    // advance epoch to apply stake and update commission rate
    runner
        .set_sender(VALIDATOR_ADDR_2)
        .system_tx!(|system, ctx| system.request_set_commission_rate(20_00, ctx))
        .advance_epoch(option::none())
        .destroy_for_testing();

    // V1: 200, V2: 300, V3: 300, V4: 400
    runner.set_sender(VALIDATOR_ADDR_2).system_tx!(|system, _| {
        // check stakes
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 200 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 300 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_3), 300 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_4), 400 * MIST_PER_SUI);
    });

    let opts = runner.advance_epoch_opts().computation_charge(120);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // V1: 230, V2: 330, V3: 330, V4: 430
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 230 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 330 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_3), 330 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_4), 430 * MIST_PER_SUI);
    });

    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[STAKER_ADDR_1, STAKER_ADDR_2],
        vector[115 * MIST_PER_SUI, 108 * MIST_PER_SUI],
    );

    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[115 * MIST_PER_SUI, 222 * MIST_PER_SUI, 330 * MIST_PER_SUI, 430 * MIST_PER_SUI],
    );

    // validator 1 now has 10% commission
    runner
        .set_sender(VALIDATOR_ADDR_1)
        .system_tx!(|system, ctx| system.request_set_commission_rate(10_00, ctx))
        .advance_epoch(option::none())
        .destroy_for_testing();

    let opts = runner.advance_epoch_opts().computation_charge(240);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 290 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_2), 390 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_3), 390 * MIST_PER_SUI);
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_4), 490 * MIST_PER_SUI);
    });

    // Staker 1 rewards in the recent distribution is 0.9 x 30 = 27 SUI
    // Validator 1 rewards in the recent distribution is 60 - 27 = 33 SUI

    // Staker 2 amounts for 0.8 * 60 * (108 / 330) + 108 = 123.709 SUI
    // Validator 2 amounts for 390 - 123.709 = 266.291 SUI

    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[STAKER_ADDR_1, STAKER_ADDR_2],
        vector[142 * MIST_PER_SUI, 123709090909],
    );

    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[148 * MIST_PER_SUI, 266290909091, 390 * MIST_PER_SUI, 490 * MIST_PER_SUI],
    );

    runner.finish();
}

fun assert_stake_rewards_for_addresses(
    runner: &mut test_runner::TestRunner,
    validator_addresses: vector<address>,
    expected_amounts: vector<u64>,
) {
    validator_addresses.zip_do!(expected_amounts, |validator_address, expected_amount| {
        let sum_rewards = runner.set_sender(validator_address).staking_rewards_balance();

        assert_eq!(sum_rewards, expected_amount);
    });
}

#[test]
fun rewards_slashing() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1000)
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 100);
    runner.set_sender(STAKER_ADDR_2).stake_with(VALIDATOR_ADDR_2, 100);
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.set_sender(VALIDATOR_ADDR_1).report_validator(VALIDATOR_ADDR_2);
    runner.set_sender(VALIDATOR_ADDR_3).report_validator(VALIDATOR_ADDR_2);
    runner.set_sender(VALIDATOR_ADDR_4).report_validator(VALIDATOR_ADDR_2);
    runner.set_sender(VALIDATOR_ADDR_3).report_validator(VALIDATOR_ADDR_1);

    let opts = runner.advance_epoch_opts().computation_charge(3600).reward_slashing_rate(1000);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Without reward slashing, the validator's stakes should be [100+450, 200+600, 300+900, 400+900]
    // after the last epoch advancement.
    // Since 60 SUI, or 10% of validator_2's rewards (600) are slashed, she only has 800 - 60 = 740 now.
    // There are in total 90 SUI of rewards slashed (60 from the validator, and 30 from her staker)
    // so the unslashed validators each get their share of additional rewards, which is 30.
    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[565 * MIST_PER_SUI, 740 * MIST_PER_SUI, 1230 * MIST_PER_SUI, 1330 * MIST_PER_SUI],
    );

    runner.set_sender(STAKER_ADDR_1).unstake(0);
    runner.set_sender(STAKER_ADDR_2).unstake(0);

    assert_eq!(runner.set_sender(STAKER_ADDR_1).sui_balance(), 565 * MIST_PER_SUI);
    assert_eq!(runner.set_sender(STAKER_ADDR_2).sui_balance(), 370 * MIST_PER_SUI);

    runner.finish();
}

#[test]
fun entire_rewards_slashing() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1000)
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 100);
    runner.set_sender(STAKER_ADDR_2).stake_with(VALIDATOR_ADDR_2, 100);
    runner.advance_epoch(option::none()).destroy_for_testing();

    // validator_2 is reported by 3 other validators, so 75% of total stake.
    runner.set_sender(VALIDATOR_ADDR_1).report_validator(VALIDATOR_ADDR_2);
    runner.set_sender(VALIDATOR_ADDR_3).report_validator(VALIDATOR_ADDR_2);
    runner.set_sender(VALIDATOR_ADDR_4).report_validator(VALIDATOR_ADDR_2);

    // 3600 SUI of total rewards, 100% reward slashing.
    // So validator_2 is the only one whose rewards should get slashed.
    let opts = runner.advance_epoch_opts().computation_charge(3600).reward_slashing_rate(10_000);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Without reward slashing, the validator's stakes should be [100+450, 200+600, 300+900, 400+900]
    // after the last epoch advancement.
    // The entire rewards of validator 2's staking pool are slashed, which is 900 SUI.
    // so the unslashed validators each get their share of additional rewards, which is 300.
    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[
            (550 + 150) * MIST_PER_SUI,
            200 * MIST_PER_SUI,
            (1200 + 300) * MIST_PER_SUI,
            (1300 + 300) * MIST_PER_SUI,
        ],
    );

    // Unstake so we can check the stake rewards as well.
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    runner.set_sender(STAKER_ADDR_2).unstake(0);

    // Same analysis as above. Staker 1 has 150 additional SUI, and since all of staker 2's rewards are slashed she only gets back her principal.
    assert_eq!(runner.set_sender(STAKER_ADDR_1).sui_balance(), (550 + 150) * MIST_PER_SUI);
    assert_eq!(runner.set_sender(STAKER_ADDR_2).sui_balance(), 100 * MIST_PER_SUI);

    runner.finish();
}

#[test]
fun rewards_slashing_with_storage_fund() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1000)
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    let opts = runner.advance_epoch_opts().storage_charge(300);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Add a few stakes.
    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_3, 100);
    runner.set_sender(STAKER_ADDR_2).stake_with(VALIDATOR_ADDR_4, 100);
    runner.advance_epoch(option::none()).destroy_for_testing();

    // validator_4 is reported by 3 other validators, so 75% of total stake.
    runner.set_sender(VALIDATOR_ADDR_1).report_validator(VALIDATOR_ADDR_4);
    runner.set_sender(VALIDATOR_ADDR_2).report_validator(VALIDATOR_ADDR_4);
    runner.set_sender(VALIDATOR_ADDR_3).report_validator(VALIDATOR_ADDR_4);

    // 1000 SUI of storage rewards, 1500 SUI of computation rewards, 50% slashing threshold
    // and 20% slashing rate
    let opts = runner
        .advance_epoch_opts()
        .storage_charge(1000)
        .computation_charge(1500)
        .reward_slashing_rate(2000);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Each unslashed validator staking pool gets 300 SUI of computation rewards + 75 SUI of storage fund rewards +
    // 20 SUI (1/3) of validator 4's slashed computation reward and 5 SUI (1/3) of validator 4's slashed
    // storage fund reward, so in total it gets 400 SUI of rewards.
    // Validator 3 has a delegator with her so she gets 320 * 3/4 + 75 + 5 = 320 SUI of rewards.
    // Validator 4's should get 300 * 4/5 * (1 - 20%) = 192 in computation rewards and 75 * (1 - 20%) = 60 in storage rewards.
    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[500 * MIST_PER_SUI, 600 * MIST_PER_SUI, 620 * MIST_PER_SUI, 652 * MIST_PER_SUI],
    );

    // Unstake so we can check the stake rewards as well.
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    runner.set_sender(STAKER_ADDR_2).unstake(0);

    // Staker 1 gets 320 * 1/4 = 80 SUI of rewards.
    assert_eq!(runner.set_sender(STAKER_ADDR_1).sui_balance(), (100 + 80) * MIST_PER_SUI);
    // Staker 2 gets 300 * 1/5 * (1 - 20%) = 48 SUI of rewards.
    assert_eq!(runner.set_sender(STAKER_ADDR_2).sui_balance(), (100 + 48) * MIST_PER_SUI);

    runner.finish();
}

#[test]
// This test is to make sure that if everyone is slashed, our protocol works as expected without aborting
// and all rewards go to the storage fund.
fun everyone_slashed() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1000)
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    // Report all validators to validator 4.
    runner.set_sender(VALIDATOR_ADDR_1).report_validator(VALIDATOR_ADDR_4);
    runner.set_sender(VALIDATOR_ADDR_2).report_validator(VALIDATOR_ADDR_4);
    runner.set_sender(VALIDATOR_ADDR_3).report_validator(VALIDATOR_ADDR_4);

    // Report all validators to validator 3.
    runner.set_sender(VALIDATOR_ADDR_1).report_validator(VALIDATOR_ADDR_3);
    runner.set_sender(VALIDATOR_ADDR_2).report_validator(VALIDATOR_ADDR_3);
    runner.set_sender(VALIDATOR_ADDR_4).report_validator(VALIDATOR_ADDR_3);

    // Report all validators to validator 2.
    runner.set_sender(VALIDATOR_ADDR_1).report_validator(VALIDATOR_ADDR_2);
    runner.set_sender(VALIDATOR_ADDR_3).report_validator(VALIDATOR_ADDR_2);
    runner.set_sender(VALIDATOR_ADDR_4).report_validator(VALIDATOR_ADDR_2);

    // Report all validators to validator 1.
    runner.set_sender(VALIDATOR_ADDR_2).report_validator(VALIDATOR_ADDR_1);
    runner.set_sender(VALIDATOR_ADDR_3).report_validator(VALIDATOR_ADDR_1);
    runner.set_sender(VALIDATOR_ADDR_4).report_validator(VALIDATOR_ADDR_1);

    let opts = runner
        .advance_epoch_opts()
        .storage_charge(1000)
        .computation_charge(3000)
        .reward_slashing_rate(10_000);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // All validators should have 0 rewards added so their stake stays the same.
    assert_stake_rewards_for_addresses(
        &mut runner,
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4],
        vector[100 * MIST_PER_SUI, 200 * MIST_PER_SUI, 300 * MIST_PER_SUI, 400 * MIST_PER_SUI],
    );

    runner.system_tx!(|system, _| {
        // Storage fund balance should increase by 4000 SUI.
        assert_eq!(system.get_storage_fund_total_balance(), 4000 * MIST_PER_SUI);
        // The entire 1000 SUI of storage rewards should go to the object rebate portion of the storage fund.
        assert_eq!(system.get_storage_fund_object_rebates(), 1000 * MIST_PER_SUI);
    });

    runner.finish();
}

#[test]
fun mul_rewards_withdraws_at_same_epoch() {
    let mut runner = test_runner::new()
        .sui_supply_amount(1000)
        .validators(vector[
            validator_builder::new().initial_stake(100).sui_address(VALIDATOR_ADDR_1),
            validator_builder::new().initial_stake(200).sui_address(VALIDATOR_ADDR_2),
            validator_builder::new().initial_stake(300).sui_address(VALIDATOR_ADDR_3),
            validator_builder::new().initial_stake(400).sui_address(VALIDATOR_ADDR_4),
        ])
        .build();

    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 220);

    let opts = runner.advance_epoch_opts().computation_charge(40);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.set_sender(STAKER_ADDR_2).stake_with(VALIDATOR_ADDR_1, 480);

    // Staker 1 gets 2/3 * 1/4 * 120 = 20 SUI here.
    let opts = runner.advance_epoch_opts().computation_charge(120);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.set_sender(STAKER_ADDR_1).stake_with(VALIDATOR_ADDR_1, 130);
    runner.set_sender(STAKER_ADDR_3).stake_with(VALIDATOR_ADDR_1, 390);

    // Staker 1 gets 20 SUI here and staker 2 gets 40 SUI here.
    let opts = runner.advance_epoch_opts().computation_charge(280);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.set_sender(STAKER_ADDR_3).stake_with(VALIDATOR_ADDR_1, 280);
    runner.set_sender(STAKER_ADDR_4).stake_with(VALIDATOR_ADDR_1, 1400);

    // Staker 1 gets 30 SUI, staker 2 gets 40 SUI and staker 3 gets 30 SUI.
    let opts = runner.advance_epoch_opts().computation_charge(440);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Check that we have the right amount of SUI in the staking pool.
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 140 * 23 * MIST_PER_SUI);
    });

    // Withdraw all stakes at once.
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    runner.set_sender(STAKER_ADDR_1).unstake(0);
    runner.set_sender(STAKER_ADDR_2).unstake(0);
    runner.set_sender(STAKER_ADDR_3).unstake(0);
    runner.set_sender(STAKER_ADDR_3).unstake(0);
    runner.set_sender(STAKER_ADDR_4).unstake(0);

    // staker 1's first stake was active for 3 epochs so got 20 * 3 = 60 SUI of rewards
    // and her second stake was active for only one epoch and got 10 SUI of rewards.
    assert_eq!(
        runner.set_sender(STAKER_ADDR_1).sui_balance(),
        (220 + 130 + 20 * 3 + 10) * MIST_PER_SUI,
    );
    // staker 2's stake was active for 2 epochs so got 40 * 2 = 80 SUI of rewards
    assert_eq!(runner.set_sender(STAKER_ADDR_2).sui_balance(), (480 + 40 * 2) * MIST_PER_SUI);
    // staker 3's first stake was active for 1 epoch and got 30 SUI of rewards
    // and her second stake didn't get any rewards.
    assert_eq!(runner.set_sender(STAKER_ADDR_3).sui_balance(), (390 + 280 + 30) * MIST_PER_SUI);
    // staker 4 joined and left in an epoch where no rewards were earned so she got no rewards.
    assert_eq!(runner.set_sender(STAKER_ADDR_4).sui_balance(), 1400 * MIST_PER_SUI);

    runner.advance_epoch(option::none()).destroy_for_testing();

    // Since all the stakes are gone the pool is empty except for the validator's original stake.
    runner.system_tx!(|system, _| {
        assert_eq!(system.validator_stake_amount(VALIDATOR_ADDR_1), 140 * MIST_PER_SUI);
    });

    runner.finish();
}

#[test]
fun uncapped_rewards() {
    let validators = vector::tabulate!(20, |i| {
        validator_builder::new()
            .initial_stake(481 + i * 2)
            .sui_address(address::from_u256(i as u256))
    });

    let mut runner = test_runner::new().sui_supply_amount(1000).validators(validators).build();

    // Each validator's stake gets doubled.
    let opts = runner.advance_epoch_opts().computation_charge(10000);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Check that each validator has the correct amount of SUI in their stake pool.
    runner.system_tx!(|system, _| {
        20u64.do!(|i| {
            let addr = address::from_u256(i as u256);
            assert_eq!(system.validator_stake_amount(addr), (962 + i * 4) * MIST_PER_SUI);
        });
    });

    runner.finish();
}

#[test]
// TODO: come back to me once safe mode emulation is implemented
fun stake_subsidy_with_safe_mode_epoch_562_to_563() {
    set_up_sui_system_state_with_big_amounts();

    let mut test = test_scenario::begin(VALIDATOR_ADDR_1);
    let mut sui_system = test.take_shared<SuiSystemState>();
    let ctx = test.ctx();
    // mimic state during epoch 562, if we're in safe mode since the 560 -> 561 epoch change
    let start_epoch: u64 = 562;
    let start_distribution_counter = 540;
    let epoch_start_time = 100000000000;
    let epoch_duration = sui_system.inner_mut_for_testing().epoch_duration_ms();

    // increment epoch number (safe mode emulation)
    start_epoch.do!(|_| ctx.increment_epoch_number());
    sui_system.set_epoch_for_testing(start_epoch);
    sui_system.set_stake_subsidy_distribution_counter(start_distribution_counter);

    assert!(ctx.epoch() == start_epoch);
    assert!(ctx.epoch() == sui_system.epoch());
    assert!(sui_system.get_stake_subsidy_distribution_counter() == start_distribution_counter);

    // perform advance epoch
    sui_system
        .inner_mut_for_testing()
        .advance_epoch(
            start_epoch + 1,
            65,
            balance::zero(),
            balance::zero(),
            0,
            0,
            0,
            0,
            epoch_start_time,
            ctx,
        )
        .destroy_for_testing(); // balance returned from `advance_epoch`
    ctx.increment_epoch_number();

    // should distribute 3 epochs worth of subsidies: 560, 561, 562
    assert_eq!(sui_system.get_stake_subsidy_distribution_counter(), start_distribution_counter + 3);
    check_distribution_counter_invariant(&mut sui_system, ctx);

    // ensure that next epoch change only distributes one epoch's worth
    sui_system
        .inner_mut_for_testing()
        .advance_epoch(
            start_epoch + 2,
            65,
            balance::zero(),
            balance::zero(),
            0,
            0,
            0,
            0,
            epoch_start_time + epoch_duration,
            ctx,
        )
        .destroy_for_testing(); // balance returned from `advance_epoch`
    ctx.increment_epoch_number();

    // should distribute 1 epoch's worth of subsidies: 563 only
    assert_eq!(sui_system.get_stake_subsidy_distribution_counter(), start_distribution_counter + 4);
    check_distribution_counter_invariant(&mut sui_system, ctx);

    test_scenario::return_shared(sui_system);
    test.end();
}

#[test]
// TODO: come back to me once safe mode emulation is implemented
fun stake_subsidy_with_safe_mode_epoch_563_to_564() {
    set_up_sui_system_state_with_big_amounts();

    let mut test = test_scenario::begin(VALIDATOR_ADDR_1);
    let mut sui_system = test.take_shared<SuiSystemState>();
    let ctx = test.ctx();
    // mimic state during epoch 563, if we're in safe mode since the 560 -> 561 epoch change
    let start_epoch: u64 = 563;
    let start_distribution_counter = 540;
    let epoch_start_time = 100000000000;
    let epoch_duration = sui_system.inner_mut_for_testing().epoch_duration_ms();

    // increment epoch number (safe mode emulation)
    start_epoch.do!(|_| ctx.increment_epoch_number());
    sui_system.set_epoch_for_testing(start_epoch);
    sui_system.set_stake_subsidy_distribution_counter(start_distribution_counter);

    assert!(ctx.epoch() == start_epoch);
    assert!(ctx.epoch() == sui_system.epoch());
    assert!(sui_system.get_stake_subsidy_distribution_counter() == start_distribution_counter);

    // perform advance epoch
    sui_system
        .inner_mut_for_testing()
        .advance_epoch(
            start_epoch + 1,
            65,
            balance::zero(),
            balance::zero(),
            0,
            0,
            0,
            0,
            epoch_start_time,
            ctx,
        )
        .destroy_for_testing(); // balance returned from `advance_epoch`
    ctx.increment_epoch_number();

    // should distribute 4 epochs worth of subsidies: 560, 561, 562, 563
    assert_eq!(sui_system.get_stake_subsidy_distribution_counter(), start_distribution_counter + 4);
    check_distribution_counter_invariant(&mut sui_system, ctx);

    // ensure that next epoch change only distributes one epoch's worth
    sui_system
        .inner_mut_for_testing()
        .advance_epoch(
            start_epoch + 2,
            65,
            balance::zero(),
            balance::zero(),
            0,
            0,
            0,
            0,
            epoch_start_time + epoch_duration,
            ctx,
        )
        .destroy_for_testing(); // balance returned from `advance_epoch`
    ctx.increment_epoch_number();

    // should distribute 1 epoch's worth of subsidies
    assert_eq!(sui_system.get_stake_subsidy_distribution_counter(), start_distribution_counter + 5);
    check_distribution_counter_invariant(&mut sui_system, ctx);

    test_scenario::return_shared(sui_system);
    test.end();
}

#[test]
// TODO: come back to me once safe mode emulation is implemented
// Test that the fix for the subsidy distribution doesn't affect testnet,
// where the distribution has no epoch delay, and the condition could result
// in arithmetic error.
fun stake_subsidy_with_safe_mode_testnet() {
    set_up_sui_system_state_with_big_amounts();

    let mut test = test_scenario::begin(VALIDATOR_ADDR_1);
    let mut sui_system = test.take_shared<SuiSystemState>();

    let ctx = test.ctx();

    // increment epoch number (safe mode emulation)
    540u64.do!(|_| ctx.increment_epoch_number());
    sui_system.set_epoch_for_testing(540);
    sui_system.set_stake_subsidy_distribution_counter(540);

    assert!(ctx.epoch() == 540);
    assert!(sui_system.get_stake_subsidy_distribution_counter() == 540);

    // perform advance epoch
    sui_system
        .inner_mut_for_testing()
        .advance_epoch(541, 65, balance::zero(), balance::zero(), 0, 0, 0, 0, 100000000000, ctx)
        .destroy_for_testing(); // balance returned from `advance_epoch`

    assert_eq!(sui_system.get_stake_subsidy_distribution_counter(), 541);

    test_scenario::return_shared(sui_system);
    test.end();
}

fun set_up_sui_system_state_with_big_amounts() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;
    let ctx = scenario.ctx();

    let validators = vector[
        create_validator_for_testing(VALIDATOR_ADDR_1, 100000000, ctx),
        create_validator_for_testing(VALIDATOR_ADDR_2, 200000000, ctx),
        create_validator_for_testing(VALIDATOR_ADDR_3, 300000000, ctx),
        create_validator_for_testing(VALIDATOR_ADDR_4, 400000000, ctx),
    ];
    create_sui_system_state_for_testing(validators, 1000000000, 0, ctx);
    scenario_val.end();
}

fun check_distribution_counter_invariant(system: &mut SuiSystemState, ctx: &TxContext) {
    assert!(ctx.epoch() == system.epoch());
    // first subsidy distribution was at epoch 20, so counter should always be ahead by 20
    assert_eq!(system.get_stake_subsidy_distribution_counter() + 20, ctx.epoch());
}
