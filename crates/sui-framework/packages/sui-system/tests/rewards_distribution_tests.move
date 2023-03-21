// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::rewards_distribution_tests {
    use sui::test_scenario::{Self, Scenario};
    use sui_system::sui_system::{Self, SuiSystemState};

    use sui_system::validator_cap::UnverifiedValidatorOperationCap;
    use sui_system::governance_test_utils::{
        Self,
        advance_epoch,
        advance_epoch_with_reward_amounts,
        advance_epoch_with_reward_amounts_and_slashing_rates,
        assert_validator_total_stake_amounts,
        assert_validator_non_self_stake_amounts,
        assert_validator_self_stake_amounts,
        create_validator_for_testing,
        create_sui_system_state_for_testing,
        stake_with,
        total_sui_balance, unstake
    };
    use sui::test_utils::assert_eq;

    const VALIDATOR_ADDR_1: address = @0x1;
    const VALIDATOR_ADDR_2: address = @0x2;
    const VALIDATOR_ADDR_3: address = @0x3;
    const VALIDATOR_ADDR_4: address = @0x4;

    const STAKER_ADDR_1: address = @0x42;
    const STAKER_ADDR_2: address = @0x43;
    const STAKER_ADDR_3: address = @0x44;
    const STAKER_ADDR_4: address = @0x45;

    const MIST_PER_SUI: u64 = 1_000_000_000;

    #[test]
    fun test_validator_rewards() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        // need to advance epoch so validator's staking starts counting
        governance_test_utils::advance_epoch(scenario);

        advance_epoch_with_reward_amounts(0, 100, scenario);
        assert_validator_total_stake_amounts(validator_addrs(), 
            vector[110 * MIST_PER_SUI, 220 * MIST_PER_SUI, 330 * MIST_PER_SUI, 440 * MIST_PER_SUI],
            scenario
        );

        stake_with(VALIDATOR_ADDR_2, VALIDATOR_ADDR_2, 720, scenario);

        advance_epoch_with_reward_amounts(0, 100, scenario);
        // validator 2's new stake hasn't started counting yet so she only gets 20% of the rewards.
        assert_validator_total_stake_amounts(validator_addrs(),
            vector[120 * MIST_PER_SUI, 960 * MIST_PER_SUI, 360 * MIST_PER_SUI, 480 * MIST_PER_SUI],
        scenario);

        advance_epoch_with_reward_amounts(0, 160, scenario);
        // validator 2's new stake started counting so she gets half of the rewards.
        assert_validator_total_stake_amounts(validator_addrs(), 
            vector[130 * MIST_PER_SUI, 1040 * MIST_PER_SUI, 390 * MIST_PER_SUI, 520 * MIST_PER_SUI],
            scenario
        );
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_stake_subsidy() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state_with_big_amounts(scenario);

        // need to advance epoch so validator's staking starts counting
        governance_test_utils::advance_epoch(scenario);

        advance_epoch_with_reward_amounts(0, 100, scenario);
        assert_validator_total_stake_amounts(validator_addrs(), vector[100_000_010 * MIST_PER_SUI, 200_000_020 * MIST_PER_SUI, 300_000_030 * MIST_PER_SUI, 400_000_040 * MIST_PER_SUI], scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_stake_rewards() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 200, scenario);
        stake_with(STAKER_ADDR_2, VALIDATOR_ADDR_2, 100, scenario);
        governance_test_utils::advance_epoch(scenario);

        assert_validator_total_stake_amounts(validator_addrs(), vector[300 * MIST_PER_SUI, 300 * MIST_PER_SUI, 300 * MIST_PER_SUI, 400 * MIST_PER_SUI], scenario);
        assert_validator_self_stake_amounts(validator_addrs(), vector[100 * MIST_PER_SUI, 200 * MIST_PER_SUI, 300 * MIST_PER_SUI, 400 * MIST_PER_SUI], scenario);

        // 10 SUI rewards for each 100 SUI of stake
        advance_epoch_with_reward_amounts(0, 130, scenario);
        assert_validator_self_stake_amounts(validator_addrs(), vector[110 * MIST_PER_SUI, 220 * MIST_PER_SUI, 330 * MIST_PER_SUI, 440 * MIST_PER_SUI], scenario);
        unstake(STAKER_ADDR_1, 0, scenario);
        stake_with(STAKER_ADDR_2, VALIDATOR_ADDR_1, 600, scenario);
        // 10 SUI rewards for each 110 SUI of stake
        advance_epoch_with_reward_amounts(0, 130, scenario);
        // staker 1 receives only 20 SUI of rewards, not 40 since we are using pre-epoch exchange rate.
        assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 220 * MIST_PER_SUI);
        assert_validator_self_stake_amounts(validator_addrs(), vector[140 * MIST_PER_SUI, 240 * MIST_PER_SUI, 360 * MIST_PER_SUI, 480 * MIST_PER_SUI], scenario);
        unstake(STAKER_ADDR_2, 0, scenario);
        assert_eq(total_sui_balance(STAKER_ADDR_2, scenario), 120 * MIST_PER_SUI); // 20 SUI of rewards received

        // 10 SUI rewards for each 120 SUI of stake
        advance_epoch_with_reward_amounts(0, 150, scenario);

        unstake(STAKER_ADDR_2, 0, scenario); // unstake 600 principal SUI
        // additional 600 SUI of principal and 46 SUI of rewards withdrawn to Coin<SUI>
        // For this stake, the staking exchange rate is 600 : 740 and the unstaking
        // exchange rate is 600 : 797 so the total sui withdraw will be:
        // (600 * 600 / 740) * 797 / 600 = 646.
        // TODO: Come up with better numbers and clean it up!
        assert_eq(total_sui_balance(STAKER_ADDR_2, scenario), 766391752576);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_stake_tiny_rewards() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state_with_big_amounts(scenario);

        // stake a large amount
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 200000000, scenario);

        governance_test_utils::advance_epoch(scenario);

        advance_epoch_with_reward_amounts(0, 150000, scenario);

        // stake a small amount
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 10, scenario);
        advance_epoch_with_reward_amounts(0, 130, scenario);

        // unstake the stakes
        unstake(STAKER_ADDR_1, 1, scenario);

        // and advance epoch should succeed
        advance_epoch_with_reward_amounts(0, 150, scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_validator_commission() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 100, scenario);
        stake_with(STAKER_ADDR_2, VALIDATOR_ADDR_2, 100, scenario);
        governance_test_utils::advance_epoch(scenario);
        // V1: 200, V2: 300, V3: 300, V4: 400

        set_commission_rate_and_advance_epoch(VALIDATOR_ADDR_2, 5000, scenario); // 50% commission
        advance_epoch_with_reward_amounts(0, 120, scenario);
        // V1: 220, V2: 330, V3: 330, V4: 440
        // 5 SUI, or 50 % of staker_2's rewards, goes to validator_2
        assert_validator_non_self_stake_amounts(validator_addrs(), vector[110 * MIST_PER_SUI, 105 * MIST_PER_SUI, 0, 0], scenario);
        assert_validator_self_stake_amounts(validator_addrs(), vector[110 * MIST_PER_SUI, 225 * MIST_PER_SUI, 330 * MIST_PER_SUI, 440 * MIST_PER_SUI], scenario);

        set_commission_rate_and_advance_epoch(VALIDATOR_ADDR_1, 1000, scenario); // 10% commission

        // 1320 can be nicely partitioned to double the staking pools
        advance_epoch_with_reward_amounts(0, 1320, scenario);
        // V1: 440, V2: 660, V3: 660, V4: 880
        assert_validator_total_stake_amounts(validator_addrs(), vector[440 * MIST_PER_SUI, 660 * MIST_PER_SUI, 660 * MIST_PER_SUI, 880 * MIST_PER_SUI], scenario);
        
        // Staker 1 rewards in the recent distribution is 0.9 x (220 / 2) = 99 SUI
        // Validator 1 rewards in the recent distribution is 220 - 99 = 121 SUI
        
        // Old exchange rate for validator 2: 1.0.
        // New exchange rate for validator 2: 660/300
        // Staker 2 amounts for 0.5 * (660/300 * 100 - 100) + 100 = 160 SUI
        // Validator 2 amounts for 660 - 160 = 500 SUI
        // TODO: Investigate the discrepancy
        assert_validator_non_self_stake_amounts(validator_addrs(), vector[209 * MIST_PER_SUI, 157500000001, 0, 0], scenario);
        assert_validator_self_stake_amounts(validator_addrs(), vector[231 * MIST_PER_SUI, 502499999999, 660 * MIST_PER_SUI, 880 * MIST_PER_SUI], scenario);

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_rewards_slashing() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        advance_epoch(scenario);

        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 100, scenario);
        stake_with(STAKER_ADDR_2, VALIDATOR_ADDR_2, 100, scenario);

        advance_epoch(scenario);

        // validator_2 is reported by 3 other validators, so (200 + 300 + 400) / 1200 = 75% of total stake.
        report_validator(VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, scenario);
        report_validator(VALIDATOR_ADDR_3, VALIDATOR_ADDR_2, scenario);
        report_validator(VALIDATOR_ADDR_4, VALIDATOR_ADDR_2, scenario);

        // validator_1 is reported by only 1 other validator, which is 300 / 1200 = 25% of total stake.
        report_validator(VALIDATOR_ADDR_3, VALIDATOR_ADDR_1, scenario);

        // 1200*9 SUI of total rewards, 50% threshold and 10% reward slashing.
        // So validator_2 is the only one whose rewards should get slashed.
        advance_epoch_with_reward_amounts_and_slashing_rates(
            0, 1200*9, 1000, scenario
        );

        // Without reward slashing, the validator's stakes should be [100+100*9, 200+200*9, 300+300*9, 400+400*9]
        // after the last epoch advancement.
        // Since 20*9 SUI, or 10% of validator_2's rewards (200*9) are slashed, she only has 2000 - 180 = 1820 now.
        // There are in total 30*9 SUI of rewards slashed (20*9 from the validator, and 10*9 from her staker)
        // so the unslashed validators each get their weighted share of additional rewards, which is
        // 30, 30 * 3 = 90 and 30 * 4 = 120.
        assert_validator_self_stake_amounts(validator_addrs(), vector[1030 * MIST_PER_SUI, 1820 * MIST_PER_SUI, 3090 * MIST_PER_SUI, 4120 * MIST_PER_SUI], scenario);

        // Unstake so we can check the stake rewards as well.
        unstake(STAKER_ADDR_1, 0, scenario);
        unstake(STAKER_ADDR_2, 0, scenario);

        // Same analysis as above. Delegator 1 has 3 additional SUI, and 10% of staker 2's rewards are slashed.
        assert!(total_sui_balance(STAKER_ADDR_1, scenario) == 1030 * MIST_PER_SUI, 0);
        assert!(total_sui_balance(STAKER_ADDR_2, scenario) == 910 * MIST_PER_SUI, 0);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_rewards_slashing_with_storage_fund() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        // Put 300 SUI into the storage fund.
        advance_epoch_with_reward_amounts(300, 0, scenario);

        // Add a few stakes.
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_3, 100, scenario);
        stake_with(STAKER_ADDR_2, VALIDATOR_ADDR_4, 100, scenario);
        advance_epoch(scenario);

        // validator_4 is reported by 3 other validators, so (100 + 200 + 400) / 1200 = 58% of total stake.
        report_validator(VALIDATOR_ADDR_1, VALIDATOR_ADDR_4, scenario);
        report_validator(VALIDATOR_ADDR_2, VALIDATOR_ADDR_4, scenario);
        report_validator(VALIDATOR_ADDR_3, VALIDATOR_ADDR_4, scenario);

        // 1000 SUI of storage rewards, 1500 SUI of computation rewards, 50% slashing threshold
        // and 20% slashing rate
        advance_epoch_with_reward_amounts_and_slashing_rates(
            1000, 1500, 2000, scenario
        );

        // Validator 1 gets 100 SUI of computation rewards + 75 SUI of storage fund rewards +
        // 14 SUI (1/7) of validator 4's slashed computation reward and 5 SUI (1/3) of validator 4's
        // storage fund reward, so in total it gets 194 SUI of rewards. Same analysis for other 2
        // unslashed validators too.
        // Validator 4 should get 475 SUI of rewards without slashing. 20% is slashed so she gets
        // 380 SUI of rewards.
        // TODO: Come up with better numbers and clean it up!
        assert_validator_self_stake_amounts(validator_addrs(), vector[294285714285, 508571428571, 722857142857, 780 * MIST_PER_SUI], scenario);

        // Unstake so we can check the stake rewards as well.
        unstake(STAKER_ADDR_1, 0, scenario);
        unstake(STAKER_ADDR_2, 0, scenario);

        // WHY YOU DO THIS TO ME?
        assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 214285714285);
        assert_eq(total_sui_balance(STAKER_ADDR_2, scenario), 180 * MIST_PER_SUI);

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_mul_rewards_withdraws_at_same_epoch() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 220, scenario);

        // 10 SUI rewards for each 100 SUI of stake
        advance_epoch_with_reward_amounts(0, 100, scenario);

        stake_with(STAKER_ADDR_2, VALIDATOR_ADDR_1, 480, scenario);

        // 10 SUI rewards for each 110 SUI of stake
        advance_epoch_with_reward_amounts(0, 120, scenario);

        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 130, scenario);
        stake_with(STAKER_ADDR_3, VALIDATOR_ADDR_1, 390, scenario);

        // 10 SUI rewards for each 120 SUI of stake
        advance_epoch_with_reward_amounts(0, 160, scenario);
        stake_with(STAKER_ADDR_3, VALIDATOR_ADDR_1, 280, scenario);
        stake_with(STAKER_ADDR_4, VALIDATOR_ADDR_1, 1400, scenario);

        // 10 SUI rewards for each 130 SUI of stake
        advance_epoch_with_reward_amounts(0, 200, scenario);

        test_scenario::next_tx(scenario, @0x0);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        // Check that we have the right amount of SUI in the staking pool.
        assert_eq(sui_system::validator_stake_amount(&mut system_state, VALIDATOR_ADDR_1), 140 * 23 * MIST_PER_SUI);
        test_scenario::return_shared(system_state);

        // Withdraw all stakes at once.
        unstake(STAKER_ADDR_1, 0, scenario);
        unstake(STAKER_ADDR_1, 0, scenario);
        unstake(STAKER_ADDR_2, 0, scenario);
        unstake(STAKER_ADDR_3, 0, scenario);
        unstake(STAKER_ADDR_3, 0, scenario);
        unstake(STAKER_ADDR_4, 0, scenario);

        // staker 1's first stake was active for 3 epochs so got 20 * 3 = 60 SUI of rewards
        // and her second stake was active for only one epoch and got 10 SUI of rewards.
        assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), (220 + 130 + 20 * 3 + 10) * MIST_PER_SUI);
        // staker 2's stake was active for 2 epochs so got 40 * 2 = 80 SUI of rewards
        assert_eq(total_sui_balance(STAKER_ADDR_2, scenario), (480 + 40 * 2) * MIST_PER_SUI);
        // staker 3's first stake was active for 1 epoch and got 30 SUI of rewards
        // and her second stake didn't get any rewards.
        assert_eq(total_sui_balance(STAKER_ADDR_3, scenario), (390 + 280 + 30) * MIST_PER_SUI);
        // staker 4 joined and left in an epoch where no rewards were earned so she got no rewards.
        assert_eq(total_sui_balance(STAKER_ADDR_4, scenario), 1400 * MIST_PER_SUI);

        advance_epoch_with_reward_amounts(0, 0, scenario);

        test_scenario::next_tx(scenario, @0x0);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        // Since all the stakes are gone the pool is empty except for the validator's original stake.
        assert_eq(sui_system::validator_stake_amount(&mut system_state, VALIDATOR_ADDR_1), 140 * MIST_PER_SUI);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario_val);
    }

    fun set_up_sui_system_state(scenario: &mut Scenario) {
        let ctx = test_scenario::ctx(scenario);

        let validators = vector[
            create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_2, 200, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_3, 300, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_4, 400, ctx),
        ];
        create_sui_system_state_for_testing(validators, 1000, 0, ctx);
    }

    fun set_up_sui_system_state_with_big_amounts(scenario: &mut Scenario) {
        let ctx = test_scenario::ctx(scenario);

        let validators = vector[
            create_validator_for_testing(VALIDATOR_ADDR_1, 100000000, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_2, 200000000, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_3, 300000000, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_4, 400000000, ctx),
        ];
        create_sui_system_state_for_testing(validators, 1000000000, 0, ctx);
    }

    fun validator_addrs() : vector<address> {
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4]
    }

    fun set_commission_rate_and_advance_epoch(addr: address, commission_rate: u64, scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, addr);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let ctx = test_scenario::ctx(scenario);
        sui_system::request_set_commission_rate(&mut system_state, commission_rate, ctx);
        test_scenario::return_shared(system_state);
        governance_test_utils::advance_epoch(scenario);
    }

    fun report_validator(reporter: address, reportee: address, scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, reporter);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let cap = test_scenario::take_from_sender<UnverifiedValidatorOperationCap>(scenario);
        sui_system::report_validator(&mut system_state, &cap, reportee);
        test_scenario::return_to_sender(scenario, cap);
        test_scenario::return_shared(system_state);
    }
}
